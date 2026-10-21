use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
fn command(name: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"));
    cmd.arg(name);
    cmd
}
fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "biogeo-sources-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let example =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/bsm_source_attribution");
        for file in [
            "tree.nwk",
            "ranges.tsv",
            "parameters.tsv",
            "request.tsv",
            "young.tsv",
            "old.tsv",
            "strata.tsv",
        ] {
            fs::copy(example.join(file), root.join(file)).unwrap();
        }
        Self(root)
    }
    fn edit(&self, file: &str, old: &str, new: &str) {
        let path = self.0.join(file);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains(old), "missing edit target {old}");
        fs::write(path, text.replace(old, new)).unwrap();
    }
    fn fit(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        success(
            command("analysis-run")
                .arg("--request")
                .arg(self.0.join("request.tsv"))
                .arg("--output-dir")
                .arg(&path)
                .output()
                .unwrap(),
        );
        path
    }
    fn bsm(&self, fit: &Path, name: &str, level: &str, samples: &str, shard: bool) -> PathBuf {
        let path = self.0.join(name);
        let mut cmd = command("model-bsm");
        cmd.arg("--analysis-result")
            .arg(fit)
            .args([
                "--bsm-samples",
                samples,
                "--seed",
                "1234",
                "--bsm-output-level",
                level,
                "--bsm-threads",
                if shard { "3" } else { "1" },
                "--bsm-output-dir",
            ])
            .arg(&path);
        if shard {
            cmd.args(["--bsm-shard-samples", "73"]);
        }
        success(cmd.output().unwrap());
        path
    }
    fn assign(
        &self,
        fit: &Path,
        bsm: &Path,
        name: &str,
        seed: &str,
        threads: &str,
        resume: bool,
    ) -> Output {
        let mut cmd = command("bsm-assign-sources");
        cmd.arg("--analysis-result")
            .arg(fit)
            .arg("--bsm-result")
            .arg(bsm)
            .arg("--output-dir")
            .arg(self.0.join(name))
            .args(["--seed", seed, "--threads", threads]);
        if resume {
            cmd.arg("--resume");
        }
        cmd.output().unwrap()
    }
    fn inspect(&self, fit: &Path, bsm: &Path, name: &str, deep: bool) -> Output {
        let mut cmd = command("bsm-sources-inspect");
        cmd.arg("--analysis-result")
            .arg(fit)
            .arg("--bsm-result")
            .arg(bsm)
            .arg("--source-result")
            .arg(self.0.join(name));
        if deep {
            cmd.arg("--deep");
        }
        cmd.output().unwrap()
    }
    fn probabilities(
        &self,
        fit: &Path,
        bsm: &Path,
        name: &str,
        threads: &str,
        resume: bool,
    ) -> Output {
        let mut cmd = command("bsm-assign-sources");
        cmd.arg("--analysis-result")
            .arg(fit)
            .arg("--bsm-result")
            .arg(bsm)
            .arg("--output-dir")
            .arg(self.0.join(name))
            .args(["--allocation-mode", "probabilities", "--threads", threads]);
        if resume {
            cmd.arg("--resume");
        }
        cmd.output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                out.insert(
                    path.strip_prefix(root).unwrap().to_owned(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn source_rows(root: &Path) -> Vec<BTreeMap<String, String>> {
    chunk_rows(root, "sources-")
}
fn chunk_rows(root: &Path, prefix: &str) -> Vec<BTreeMap<String, String>> {
    let mut paths: Vec<_> = fs::read_dir(root.join("shards"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.file_name().unwrap().to_string_lossy().starts_with(prefix))
        .collect();
    paths.sort();
    paths
        .into_iter()
        .flat_map(|path| {
            let text = fs::read_to_string(path).unwrap();
            let mut lines = text.lines();
            let header: Vec<_> = lines.next().unwrap().split('\t').collect();
            lines
                .map(|line| {
                    header
                        .iter()
                        .zip(line.split('\t'))
                        .map(|(k, v)| (k.to_string(), v.to_owned()))
                        .collect()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

fn hash(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

#[test]
fn deep_inspection_recomputes_assignments_even_with_consistent_checksums() {
    let f = Fixture::new();
    let fit = f.fit("fit");
    let bsm = f.bsm(&fit, "bsm", "compact", "32", false);
    let before = snapshot(&bsm);
    assert!(
        !f.assign(&fit, &bsm, "bsm/nested", "2", "1", false)
            .status
            .success()
    );
    assert_eq!(before, snapshot(&bsm));
    success(f.assign(&fit, &bsm, "sources", "2", "1", false));
    let out = f.0.join("sources");
    let path = out.join("shards/sources-00000000.tsv");
    let original = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
    let mut row: Vec<String> = lines[1].split('\t').map(str::to_owned).collect();
    row[1].push_str("-wrong");
    lines[1] = row.join("\t");
    let changed = lines.join("\n") + "\n";
    fs::write(&path, &changed).unwrap();
    let files = format!(
        "chunk\tfile\trows\tbytes\tfingerprint\n0\tshards/sources-00000000.tsv\t{}\t{}\t{}\n",
        lines.len() - 1,
        changed.len(),
        hash(changed.as_bytes())
    );
    let old_files = fs::read(out.join("files.tsv")).unwrap();
    let complete = fs::read_to_string(out.join("complete.tsv"))
        .unwrap()
        .replace(&hash(&old_files), &hash(files.as_bytes()));
    fs::write(out.join("files.tsv"), files).unwrap();
    fs::write(out.join("complete.tsv"), complete).unwrap();
    success(f.inspect(&fit, &bsm, "sources", false));
    assert!(!f.inspect(&fit, &bsm, "sources", true).status.success());
}

#[test]
fn single_tip_history_produces_a_valid_empty_source_artifact() {
    let f = Fixture::new();
    fs::write(f.0.join("tree.nwk"), "tA;").unwrap();
    fs::write(f.0.join("ranges.tsv"), "tip\tA\tB\tC\ntA\t1\t0\t0\n").unwrap();
    let fit = f.fit("fit");
    let bsm = f.bsm(&fit, "bsm", "compact", "4", true);
    success(f.assign(&fit, &bsm, "sources", "2", "1", false));
    assert!(source_rows(&f.0.join("sources")).is_empty());
    success(f.inspect(&fit, &bsm, "sources", true));
    success(f.probabilities(&fit, &bsm, "probabilities", "1", false));
    assert!(chunk_rows(&f.0.join("probabilities"), "route-candidates-").is_empty());
    success(f.inspect(&fit, &bsm, "probabilities", true));
}

#[test]
fn joint_probabilities_match_period_weights_and_preserve_histories_across_layouts() {
    let f = Fixture::new();
    f.edit("parameters.tsv", "a\tfixed\t0\t", "a\tfixed\t0.5\t");
    f.edit(
        "parameters.tsv",
        "mx01j\tderived\t\t0.0001\t0.9999\tlogit\tmx01",
        "mx01j\tfixed\t0.9999\t0.0001\t0.9999\tlogit\t",
    );
    f.edit("request.tsv", "max_range_size\t3", "max_range_size\t4");
    fs::write(
        f.0.join("ranges.tsv"),
        "tip\tA\tB\tC\tD\ntA\t1\t1\t0\t0\ntB\t1\t0\t0\t0\ntC\t0\t0\t1\t1\ntD\t0\t0\t0\t1\n",
    )
    .unwrap();
    let matrices = [
        [
            [1., 1., 1., 2.],
            [1., 1., 6., 1.],
            [3., 4., 1., 0.],
            [5., 0., 1., 1.],
        ],
        [
            [1., 1., 6., 1.],
            [1., 1., 1., 2.],
            [4., 3., 1., 0.],
            [0., 5., 1., 1.],
        ],
    ];
    for (name, matrix) in ["young.tsv", "old.tsv"].iter().zip(&matrices) {
        let mut text = "from\tA\tB\tC\tD\n".to_string();
        for (area, row) in ["A", "B", "C", "D"].iter().zip(matrix) {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                area, row[0], row[1], row[2], row[3]
            ));
        }
        fs::write(f.0.join(name), text).unwrap();
    }
    let fit = f.fit("fit");
    let fit_before = snapshot(&fit);
    let bsm = f.bsm(&fit, "full", "full", "512", false);
    let before = snapshot(&bsm);
    success(f.probabilities(&fit, &bsm, "one", "1", false));
    success(f.probabilities(&fit, &bsm, "three", "3", false));
    assert_eq!(snapshot(&f.0.join("one")), snapshot(&f.0.join("three")));
    let events = source_rows(&f.0.join("one"));
    let routes = chunk_rows(&f.0.join("one"), "route-candidates-");
    let dictionary = fs::read_to_string(bsm.join("states.tsv")).unwrap();
    let states: BTreeMap<usize, u64> = dictionary
        .lines()
        .skip(1)
        .map(|line| {
            let f: Vec<_> = line.split('\t').collect();
            (f[0].parse().unwrap(), f[1].parse().unwrap())
        })
        .collect();
    let by_event: BTreeMap<_, _> = events.iter().map(|e| (e["event_id"].as_str(), e)).collect();
    let mut sums = BTreeMap::<String, f64>::new();
    let mut multi = BTreeSet::new();
    let mut periods = BTreeSet::new();
    let mut exact_example = 0;
    for row in &routes {
        let event = by_event[row["event_id"].as_str()];
        assert_eq!(event["sample"], row["sample"]);
        let src = states[&event["source_state_index"].parse::<usize>().unwrap()];
        let dst = states[&event["target_state_index"].parse::<usize>().unwrap()];
        let dst = if event["event_kind"] == "range_expansion" {
            dst & !src
        } else {
            dst
        };
        let q: usize = event["q_index"].parse().unwrap();
        let s: usize = row["source_area_index"].parse().unwrap();
        let t: usize = row["target_area_index"].parse().unwrap();
        assert_ne!(src & (1 << s), 0);
        assert_ne!(dst & (1 << t), 0);
        let p: f64 = row["conditional_probability"].parse().unwrap();
        let mut total = 0.;
        for (a, weights) in matrices[q].iter().enumerate() {
            for (b, weight) in weights.iter().enumerate() {
                if src & (1 << a) != 0 && dst & (1 << b) != 0 {
                    total += weight;
                }
            }
        }
        let expected = if event["event_kind"] == "range_switching" {
            1.
        } else {
            matrices[q][s][t] / total
        };
        assert!(p > 0. && (p - expected).abs() < 1e-14);
        *sums.entry(row["event_id"].clone()).or_default() += p;
        if event["event_kind"] == "founder_event" && dst.count_ones() > 1 {
            multi.insert(row["event_id"].clone());
            periods.insert(q);
            if src == 3 && dst == 12 && q == 0 {
                exact_example += 1;
            }
        }
    }
    assert!(multi.len() > 10);
    assert_eq!(periods.len(), 2);
    assert!(exact_example > 0);
    assert!(sums.values().all(|p| (p - 1.).abs() < 1e-12));
    assert!(
        events
            .iter()
            .all(|e| e["reason"] != "unsupported_multi_area_founder_target")
    );
    assert!(
        events
            .iter()
            .any(|e| e["assignment_status"] == "not_applicable")
    );
    assert!(
        events
            .iter()
            .any(|e| e["assignment_status"] == "state_transition")
    );
    for (name, level, shard) in [
        ("compact", "compact", false),
        ("compact-sharded", "compact", true),
        ("full-sharded", "full", true),
    ] {
        let history = f.bsm(&fit, name, level, "512", shard);
        let out = format!("p-{name}");
        success(f.probabilities(&fit, &history, &out, "3", false));
        assert_eq!(events, source_rows(&f.0.join(&out)));
        assert_eq!(routes, chunk_rows(&f.0.join(&out), "route-candidates-"));
        success(f.inspect(&fit, &history, &out, true));
    }
    assert_eq!(before, snapshot(&bsm));
    assert_eq!(fit_before, snapshot(&fit));
    let output_before = snapshot(&f.0.join("one"));
    fs::remove_file(f.0.join("one/complete.tsv")).unwrap();
    fs::remove_file(f.0.join("one/shards/route-candidates-00000001.tsv")).unwrap();
    success(f.probabilities(&fit, &bsm, "one", "3", true));
    assert_eq!(output_before, snapshot(&f.0.join("one")));
    success(f.inspect(&fit, &bsm, "one", true));
    // Rebalance two positive probabilities and update checksums: shallow checks
    // accept a distribution, while deep checks must reject the wrong weights.
    let out = f.0.join("one");
    let path = out.join("shards/route-candidates-00000000.tsv");
    let original = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<String> = original.lines().map(str::to_owned).collect();
    let i = (1..lines.len() - 1)
        .find(|i| {
            let a: Vec<_> = lines[*i].split('\t').collect();
            let b: Vec<_> = lines[*i + 1].split('\t').collect();
            a[1] == b[1] && a[4] != b[4]
        })
        .unwrap();
    let mut a: Vec<String> = lines[i].split('\t').map(str::to_owned).collect();
    let mut b: Vec<String> = lines[i + 1].split('\t').map(str::to_owned).collect();
    std::mem::swap(&mut a[4], &mut b[4]);
    lines[i] = a.join("\t");
    lines[i + 1] = b.join("\t");
    let changed = lines.join("\n") + "\n";
    fs::write(&path, &changed).unwrap();
    let old_files = fs::read_to_string(out.join("route-files.tsv")).unwrap();
    let mut manifest: Vec<String> = old_files.lines().map(str::to_owned).collect();
    manifest[1] = format!(
        "0\tshards/route-candidates-00000000.tsv\t{}\t{}\t{}",
        lines.len() - 1,
        changed.len(),
        hash(changed.as_bytes())
    );
    let files = manifest.join("\n") + "\n";
    let complete = fs::read_to_string(out.join("complete.tsv"))
        .unwrap()
        .replace(&hash(old_files.as_bytes()), &hash(files.as_bytes()));
    fs::write(out.join("route-files.tsv"), files).unwrap();
    fs::write(out.join("complete.tsv"), complete).unwrap();
    success(f.inspect(&fit, &bsm, "one", false));
    assert!(!f.inspect(&fit, &bsm, "one", true).status.success());
}

#[test]
fn weighted_sources_preserve_histories_across_threads_layouts_shards_seed_and_resume() {
    let f = Fixture::new();
    let fit = f.fit("fit");
    let fit_before = snapshot(&fit);
    let full = f.bsm(&fit, "full", "full", "1024", false);
    let before = snapshot(&full);
    success(f.assign(&fit, &full, "one", "29", "1", false));
    success(f.assign(&fit, &full, "three", "29", "3", false));
    assert_eq!(snapshot(&f.0.join("one")), snapshot(&f.0.join("three")));
    success(f.inspect(&fit, &full, "one", false));
    success(f.inspect(&fit, &full, "one", true));
    let expected = source_rows(&f.0.join("one"));
    let mut ids = BTreeSet::new();
    let mut weighted = 0;
    let mut periods = BTreeSet::new();
    let mut boundary = 0;
    for row in &expected {
        assert!(ids.insert(row["event_id"].clone()));
        if row["event_kind"] == "founder_event"
            && row["age_before_present"].parse::<f64>().unwrap() == 2.0
        {
            assert_eq!(row["q_index"], "0");
            boundary += 1;
        }
        if row["source_state_index"] == "4" && row["target_area_index"] == "2" {
            let q: usize = row["q_index"].parse().unwrap();
            let source: usize = row["source_area_index"].parse().unwrap();
            let p: f64 = row["conditional_probability"].parse().unwrap();
            let expected = if (q == 0 && source == 0) || (q == 1 && source == 1) {
                0.9
            } else {
                0.1
            };
            assert!((p - expected).abs() < 1e-12);
            periods.insert(q);
            weighted += 1;
        }
    }
    assert!(weighted > 100);
    assert_eq!(periods.len(), 2);
    assert!(boundary > 0);
    for (name, level, shard) in [
        ("compact", "compact", false),
        ("sharded", "compact", true),
        ("full_sharded", "full", true),
    ] {
        let bsm = f.bsm(&fit, name, level, "1024", shard);
        let out = format!("sources-{name}");
        success(f.assign(&fit, &bsm, &out, "29", "2", false));
        assert_eq!(expected, source_rows(&f.0.join(out)));
    }
    success(f.assign(&fit, &full, "other-seed", "30", "2", false));
    assert_ne!(expected, source_rows(&f.0.join("other-seed")));
    assert_eq!(before, snapshot(&full));
    assert_eq!(fit_before, snapshot(&fit));
    let out = f.0.join("three");
    let original = snapshot(&out);
    let first = out.join("shards/sources-00000000.tsv");
    let first_modified = fs::metadata(&first).unwrap().modified().unwrap();
    let mut shards: Vec<_> = fs::read_dir(out.join("shards"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    shards.sort();
    assert!(shards.len() > 1);
    fs::remove_file(shards.last().unwrap()).unwrap();
    fs::remove_file(out.join("files.tsv")).unwrap();
    fs::remove_file(out.join("complete.tsv")).unwrap();
    success(f.assign(&fit, &full, "three", "29", "4", true));
    assert_eq!(original, snapshot(&out));
    assert_eq!(
        first_modified,
        fs::metadata(&first).unwrap().modified().unwrap()
    );
    assert!(
        !f.assign(&fit, &full, "three", "30", "1", true)
            .status
            .success()
    );
    fs::write(
        &first,
        fs::read_to_string(&first)
            .unwrap()
            .replace("weighted_sampled", "bogus_sampled"),
    )
    .unwrap();
    assert!(!f.inspect(&fit, &full, "three", true).status.success());
}

#[test]
fn rejects_summary_wrong_fit_and_dictionary_mismatch() {
    let f = Fixture::new();
    let fit = f.fit("fit");
    let summary = f.bsm(&fit, "summary", "summary", "8", false);
    assert!(
        !f.assign(&fit, &summary, "invalid", "1", "1", false)
            .status
            .success()
    );
    let bsm = f.bsm(&fit, "bsm", "compact", "8", false);
    f.edit("parameters.tsv", "d\tfixed\t0.2", "d\tfixed\t0.3");
    let other = f.fit("other");
    assert!(
        !f.assign(&other, &bsm, "invalid", "1", "1", false)
            .status
            .success()
    );
    let nodes = bsm.join("nodes.tsv");
    let text = fs::read_to_string(&nodes).unwrap();
    fs::write(nodes, text.replace("tA", "tZ")).unwrap();
    assert!(
        !f.assign(&fit, &bsm, "invalid", "1", "1", false)
            .status
            .success()
    );
    assert!(!f.0.join("invalid").exists());
}

#[test]
fn switching_and_multi_area_founder_are_explicit_and_hooks_have_no_fake_splits() {
    let f = Fixture::new();
    f.edit("parameters.tsv", "a\tfixed\t0\t", "a\tfixed\t0.5\t");
    f.edit(
        "parameters.tsv",
        "mx01j\tderived\t\t0.0001\t0.9999\tlogit\tmx01",
        "mx01j\tfixed\t0.9999\t0.0001\t0.9999\tlogit\t",
    );
    f.edit("tree.nwk", "tA:2", "tA:0.0000001");
    f.edit(
        "request.tsv",
        "min_branch_length\t0",
        "min_branch_length\t0.000001",
    );
    f.edit("request.tsv", "max_range_size\t3", "max_range_size\t4");
    fs::write(
        f.0.join("ranges.tsv"),
        "tip\tA\tB\tC\tD\ntA\t1\t1\t0\t0\ntB\t1\t0\t0\t0\ntC\t0\t0\t1\t0\ntD\t0\t0\t0\t1\n",
    )
    .unwrap();
    for name in ["young.tsv", "old.tsv"] {
        fs::write(
            f.0.join(name),
            "from\tA\tB\tC\tD\nA\t1\t1\t1\t1\nB\t1\t1\t1\t1\nC\t1\t1\t1\t1\nD\t1\t1\t1\t1\n",
        )
        .unwrap();
    }
    let fit = f.fit("fit");
    let bsm = f.bsm(&fit, "bsm", "compact", "512", true);
    success(f.assign(&fit, &bsm, "sources", "29", "3", false));
    let rows = source_rows(&f.0.join("sources"));
    let mut switched = 0;
    let mut unsupported = 0;
    let tree = biogeo_core::parse_newick(&fs::read_to_string(f.0.join("tree.nwk")).unwrap())
        .unwrap()
        .with_direct_ancestor_hooks_below(0.000001)
        .unwrap();
    for row in rows {
        if row["event_scope"] == "cladogenetic" {
            assert!(
                !tree
                    .tree
                    .is_direct_ancestor_node(row["node"].parse().unwrap())
            );
        }
        if row["event_kind"] == "range_switching" {
            assert_eq!(row["assignment_status"], "state_transition");
            assert_ne!(row["source_area_index"], "NA");
            switched += 1;
        }
        if row["reason"] == "unsupported_multi_area_founder_target" {
            assert_eq!(row["assignment_status"], "undetermined");
            assert_eq!(row["source_area_index"], "NA");
            assert_eq!(row["target_area_index"], "NA");
            assert_ne!(row["target_state_index"], "NA");
            unsupported += 1;
        }
    }
    assert!(switched > 0);
    assert!(unsupported > 0);
    success(f.inspect(&fit, &bsm, "sources", true));
}

#[test]
fn all_six_presets_and_both_null_policies_accept_saved_histories() {
    for preset in [
        "dec",
        "dec+j",
        "divalike",
        "divalike+j",
        "bayarealike",
        "bayarealike+j",
    ] {
        for include in [false, true] {
            let f = Fixture::new();
            let template = success(
                command("parameter-template")
                    .args(["--preset", preset])
                    .output()
                    .unwrap(),
            );
            let mut parameters = biogeo_core::parse_parameter_table(&template)
                .unwrap()
                .with_fixed("d", 0.2)
                .unwrap()
                .with_fixed("e", 0.1)
                .unwrap();
            if preset.ends_with("+j") {
                parameters = parameters.with_fixed("j", 0.5).unwrap();
            }
            let values = parameters.resolve_initial().unwrap();
            for (name, value) in values.iter() {
                parameters = parameters.with_fixed(name, value).unwrap();
            }
            fs::write(f.0.join("parameters.tsv"), parameters.to_versioned_tsv()).unwrap();
            if !include {
                f.edit(
                    "request.tsv",
                    "include_null_range\ttrue",
                    "include_null_range\tfalse",
                );
            }
            let fit = f.fit("fit");
            let bsm = f.bsm(&fit, "bsm", "compact", "24", false);
            success(f.assign(&fit, &bsm, "sources", "17", "1", false));
            success(f.inspect(&fit, &bsm, "sources", true));
        }
    }
}

#[test]
fn uses_fitted_distance_environment_manual_exponents_and_zero_candidates() {
    let f = Fixture::new();
    f.edit("request.tsv", "dispersal_strata\tstrata.tsv", "dispersal_multipliers\tyoung.tsv\ndistance_matrix\tdistance.tsv\nenvironment_distance_matrix\tenvironment.tsv");
    f.edit("parameters.tsv", "w\tfixed\t1\t", "w\tfixed\t2\t");
    f.edit("parameters.tsv", "x\tfixed\t0\t", "x\tfixed\t-1\t");
    f.edit("parameters.tsv", "n\tfixed\t0\t", "n\tfixed\t1\t");
    fs::write(
        f.0.join("distance.tsv"),
        "from\tA\tB\tC\nA\t1\t1\t3\nB\t1\t1\t1\nC\t1\t1\t1\n",
    )
    .unwrap();
    fs::write(
        f.0.join("environment.tsv"),
        "from\tA\tB\tC\nA\t1\t1\t2\nB\t1\t1\t3\nC\t1\t1\t1\n",
    )
    .unwrap();
    let fit = f.fit("fit");
    let bsm = f.bsm(&fit, "bsm", "compact", "512", false);
    success(f.assign(&fit, &bsm, "sources", "92", "3", false));
    let mut tested = 0;
    for row in source_rows(&f.0.join("sources")) {
        if row["source_state_index"] == "4" && row["target_area_index"] == "2" {
            let expected = if row["source_area_index"] == "0" {
                18.0 / 19.0
            } else {
                1.0 / 19.0
            };
            assert!(
                (row["conditional_probability"].parse::<f64>().unwrap() - expected).abs() < 1e-12
            );
            tested += 1;
        }
    }
    assert!(tested > 10);
    let z = Fixture::new();
    z.edit("young.tsv", "B\t3\t1\t1", "B\t3\t1\t0");
    z.edit("old.tsv", "A\t1\t2\t1", "A\t1\t2\t0");
    let fit = z.fit("fit");
    let bsm = z.bsm(&fit, "bsm", "compact", "512", false);
    success(z.assign(&fit, &bsm, "sources", "92", "3", false));
    let mut tested = 0;
    for row in source_rows(&z.0.join("sources")) {
        if row["source_state_index"] == "4" && row["target_area_index"] == "2" {
            assert_eq!(row["assignment_status"], "unique_candidate");
            assert_eq!(
                row["source_area_index"],
                if row["q_index"] == "0" { "0" } else { "1" }
            );
            tested += 1;
        }
    }
    assert!(tested > 10);
}
