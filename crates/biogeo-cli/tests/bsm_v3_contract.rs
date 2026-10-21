use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const TABLES: [&str; 8] = [
    "node_states.tsv",
    "cladogenetic_splits.tsv",
    "branch_segments.tsv",
    "sample_event_counts.tsv",
    "sample_period_event_counts.tsv",
    "sample_state_occupancy.tsv",
    "sample_period_state_occupancy.tsv",
    "anagenetic_events.tsv",
];
struct Fixture(PathBuf);
impl Fixture {
    fn new(hook: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "biogeo-bsm-v3-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(
            root.join("tree.nwk"),
            if hook {
                "((A:0.0000001,B:2):2,(C:2,D:2):2);"
            } else {
                "((A:2,B:2):2,(C:2,D:2):2);"
            },
        )
        .unwrap();
        fs::write(
            root.join("ranges.tsv"),
            "tip\tNorth\tEast\tWest\nA\t1\t1\t0\nB\t1\t0\t0\nC\t0\t1\t0\nD\t0\t0\t1\n",
        )
        .unwrap();
        Self(root)
    }
    fn run(
        &self,
        exe: &Path,
        name: &str,
        model: &str,
        j: &str,
        level: &str,
        extra: &[&str],
    ) -> PathBuf {
        let out = self.0.join(name);
        let result = Command::new(exe)
            .arg(model)
            .arg("--tree")
            .arg(self.0.join("tree.nwk"))
            .arg("--ranges")
            .arg(self.0.join("ranges.tsv"))
            .args([
                "--d",
                "0.2",
                "--e",
                "0.1",
                "--j",
                j,
                "--max-range-size",
                "3",
                "--include-null-range",
                "--bsm-samples",
                "256",
                "--seed",
                "20260928",
                "--bsm-threads",
                "1",
                "--bsm-output-level",
                level,
                "--bsm-output-dir",
            ])
            .arg(&out)
            .args(extra)
            .output()
            .unwrap();
        success(&result);
        out
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn exe() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_biogeo-cli"))
}
fn success(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn inspect(path: &Path) -> Output {
    Command::new(exe())
        .arg("bsm-inspect")
        .arg("--bsm-result")
        .arg(path)
        .arg("--deep")
        .output()
        .unwrap()
}
fn rows(path: &Path) -> Vec<BTreeMap<String, String>> {
    let text = fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    let header = lines.next().unwrap().split('\t').collect::<Vec<_>>();
    lines
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(fields.len(), header.len());
            header
                .iter()
                .zip(fields)
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect()
        })
        .collect()
}
fn dictionary(path: &Path) -> BTreeMap<String, String> {
    rows(path)
        .into_iter()
        .map(|r| (r["key"].clone(), r["value"].clone()))
        .collect()
}
fn bundles(root: &Path) -> Vec<PathBuf> {
    if !root.join("shards").is_dir() {
        return vec![root.to_path_buf()];
    }
    let mut paths = fs::read_dir(root.join("shards"))
        .unwrap()
        .map(|r| r.unwrap().path())
        .collect::<Vec<_>>();
    paths.sort();
    paths
}
fn all_rows(root: &Path, table: &str) -> Vec<BTreeMap<String, String>> {
    bundles(root)
        .iter()
        .flat_map(|dir| rows(&dir.join(table)))
        .collect()
}
fn assert_projection(left: &Path, right: &Path) {
    let different_sparsity = dictionary(&left.join("metadata.tsv"))["sparse_occupancy"]
        != dictionary(&right.join("metadata.tsv"))["sparse_occupancy"];
    for table in TABLES {
        let mut a = all_rows(left, table);
        let mut b = all_rows(right, table);
        if different_sparsity && table.contains("occupancy") {
            a.retain(|r| r["occupancy_time"].parse::<f64>().unwrap() != 0.0);
            b.retain(|r| r["occupancy_time"].parse::<f64>().unwrap() != 0.0);
        }
        assert_eq!(a.len(), b.len(), "{table}");
        for (a, b) in a.iter().zip(b.iter()) {
            for (key, value) in a {
                if let Some(other) = b.get(key) {
                    assert_eq!(value, other, "{table}: {key}");
                }
            }
        }
    }
}
fn check_counts(root: &Path) -> BTreeSet<String> {
    let splits = all_rows(root, "cladogenetic_splits.tsv");
    let counts = all_rows(root, "sample_event_counts.tsv");
    let mut observed = vec![BTreeMap::<String, usize>::new(); counts.len()];
    let mut kinds = BTreeSet::new();
    for split in splits {
        let sample: usize = split["sample"].parse().unwrap();
        *observed[sample]
            .entry(split["event_kind"].clone())
            .or_default() += 1;
        kinds.insert(split["event_kind"].clone());
        assert_eq!(split["source_area_index"], "NA");
        if split["event_kind"] == "founder_event" {
            assert_eq!(split["source_status"], "undetermined");
            let order = &split["founder_daughter_order"];
            assert!(order == "0" || order == "1");
            let target = if order == "0" {
                "left_state_index"
            } else {
                "right_state_index"
            };
            assert_ne!(split[target], split["ancestor_state_index"]);
        } else {
            assert_eq!(split["founder_daughter_order"], "NA");
        }
    }
    for (i, row) in counts.iter().enumerate() {
        for kind in [
            "range_copying",
            "subset_sympatry",
            "vicariance",
            "founder_event",
        ] {
            assert_eq!(
                row[kind].parse::<usize>().unwrap(),
                *observed[i].get(kind).unwrap_or(&0)
            );
        }
    }
    kinds
}

#[test]
fn six_presets_v3_counts_full_compact_shards_threads_and_resume_agree() {
    let fixture = Fixture::new(false);
    let mut kinds = BTreeSet::new();
    for model in ["dec", "divalike", "bayarealike"] {
        for j in ["0", "0.5"] {
            let full = fixture.run(&exe(), &format!("{model}-{j}-full"), model, j, "full", &[]);
            let compact = fixture.run(
                &exe(),
                &format!("{model}-{j}-compact"),
                model,
                j,
                "compact",
                &["--bsm-threads", "3", "--bsm-shard-samples", "73"],
            );
            success(&inspect(&full));
            success(&inspect(&compact));
            kinds.extend(check_counts(&full));
            check_counts(&compact);
            assert_projection(&full, &compact);
            let resumed = fixture.run(
                &exe(),
                &format!("{model}-{j}-compact"),
                model,
                j,
                "compact",
                &[
                    "--bsm-threads",
                    "2",
                    "--bsm-shard-samples",
                    "73",
                    "--bsm-resume",
                ],
            );
            assert_projection(&full, &resumed);
        }
    }
    assert_eq!(
        kinds,
        [
            "range_copying",
            "subset_sympatry",
            "vicariance",
            "founder_event"
        ]
        .into_iter()
        .map(str::to_string)
        .collect()
    );
}

#[test]
fn direct_ancestors_have_no_split_and_dictionary_ages_preserve_tree_order() {
    let fixture = Fixture::new(true);
    let result = fixture.run(
        &exe(),
        "hook",
        "dec",
        "0.5",
        "compact",
        &["--min-branch-length", "0.000001"],
    );
    success(&inspect(&result));
    let nodes = rows(&result.join("nodes.tsv"));
    let ancestors = nodes
        .iter()
        .filter(|r| r["is_direct_ancestor"] == "true")
        .map(|r| r["node"].clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(ancestors.len(), 1);
    assert!(
        all_rows(&result, "cladogenetic_splits.tsv")
            .iter()
            .all(|r| !ancestors.contains(&r["node"]))
    );
    let mut orders = BTreeSet::new();
    for edge in rows(&result.join("edges.tsv")) {
        assert!(orders.insert((edge["parent"].clone(), edge["child_order"].clone())));
        let parent = edge["parent"].parse::<usize>().unwrap();
        let child = edge["child"].parse::<usize>().unwrap();
        let length = edge["length"].parse::<f64>().unwrap();
        let age = |node: usize| nodes[node]["age"].parse::<f64>().unwrap();
        assert!((age(parent) - age(child) - length).abs() < 1e-12);
    }
}

#[test]
fn deep_inspect_rejects_changed_category_totals_even_when_file_lengths_match() {
    let fixture = Fixture::new(false);
    let result = fixture.run(&exe(), "tamper", "dec", "0.5", "compact", &[]);
    success(&inspect(&result));
    let path = result.join("sample_event_counts.tsv");
    let original = fs::read_to_string(&path).unwrap();
    let mut lines: Vec<_> = original.lines().map(str::to_string).collect();
    let fields = lines[1].split('\t').map(str::to_string).collect::<Vec<_>>();
    let mut changed = fields.clone();
    let pair = (6..10)
        .flat_map(|a| (6..10).map(move |b| (a, b)))
        .find(|&(a, b)| fields[a] != fields[b])
        .unwrap();
    changed.swap(pair.0, pair.1);
    lines[1] = changed.join("\t");
    let text = lines.join("\n") + "\n";
    assert_eq!(text.len(), original.len());
    fs::write(&path, text).unwrap();
    let failed = inspect(&result);
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("event_kind counts"));
    fs::write(&path, original).unwrap();
    success(&inspect(&result));
}

#[test]
fn dispersal_source_is_not_invented_for_multiarea_ancestors() {
    let fixture = Fixture::new(false);
    let result = fixture.run(&exe(), "source", "dec", "0.5", "full", &[]);
    let states = rows(&result.join("states.tsv"));
    let events = all_rows(&result, "anagenetic_events.tsv");
    let mut widespread = 0;
    for row in events {
        if row["event_kind"] == "range_expansion" {
            assert_eq!(row["source_status"], "undetermined");
            assert_eq!(row["source_area_index"], "NA");
            assert_eq!(row["target_area_index"], row["area_index"]);
            let state = row["from_state_index"].parse::<usize>().unwrap();
            if states[state]["range_bits"]
                .parse::<u64>()
                .unwrap()
                .count_ones()
                > 1
            {
                widespread += 1;
            }
        }
    }
    assert!(widespread > 0);
    assert_eq!(
        dictionary(&result.join("metadata.tsv"))["dispersal_source_sampling"],
        "not_sampled"
    );
}

#[test]
fn saved_fit_ids_switching_and_multiarea_founder_targets_are_explicit() {
    let fixture = Fixture::new(false);
    fs::write(fixture.0.join("ranges.tsv"),
        "tip\tNorth\tEast\tWest\tSouth\nA\t1\t1\t0\t0\nB\t1\t0\t0\t0\nC\t0\t0\t1\t1\nD\t0\t0\t0\t1\n").unwrap();
    let template = Command::new(exe())
        .args(["parameter-template", "--preset", "dec+j"])
        .output()
        .unwrap();
    success(&template);
    let mut table =
        biogeo_core::parse_parameter_table(&String::from_utf8(template.stdout).unwrap()).unwrap();
    let resolved = table.resolve_initial().unwrap();
    for (name, value) in resolved.iter() {
        table = table.with_fixed(name, value).unwrap();
    }
    for (name, value) in [
        ("d", 0.2),
        ("e", 0.1),
        ("a", 0.4),
        ("j", 0.8),
        ("mx01j", 0.5),
    ] {
        table = table.with_fixed(name, value).unwrap();
    }
    fs::write(fixture.0.join("parameters.tsv"), table.to_versioned_tsv()).unwrap();
    fs::write(fixture.0.join("request.tsv"),
        "key\tvalue\nformat\tbiogeo-analysis-request-v1\nmode\tevaluate\nobservation\texact_ranges\ntree\ttree.nwk\nranges\tranges.tsv\nparameters\tparameters.tsv\nmax_range_size\t3\ninclude_null_range\ttrue\nroot_prior\tflat\nmin_branch_length\t0\nancestral_probabilities\ttrue\nsplit_probabilities\ttrue\n").unwrap();
    let fit = fixture.0.join("fit");
    success(
        &Command::new(exe())
            .arg("analysis-run")
            .arg("--request")
            .arg(fixture.0.join("request.tsv"))
            .arg("--output-dir")
            .arg(&fit)
            .output()
            .unwrap(),
    );
    let result = fixture.0.join("histories");
    success(
        &Command::new(exe())
            .arg("model-bsm")
            .arg("--analysis-result")
            .arg(&fit)
            .args([
                "--bsm-samples",
                "512",
                "--seed",
                "20260928",
                "--bsm-output-level",
                "compact",
                "--bsm-output-dir",
            ])
            .arg(&result)
            .output()
            .unwrap(),
    );
    success(&inspect(&result));
    check_counts(&result);
    let meta = dictionary(&result.join("metadata.tsv"));
    let posterior_meta = dictionary(&fit.join("posteriors/metadata.tsv"));
    assert_eq!(
        meta["analysis_result_fingerprint"],
        posterior_meta["analysis_result_fingerprint"]
    );
    let nodes = rows(&result.join("nodes.tsv"));
    let posterior_nodes = rows(&fit.join("posteriors/nodes.tsv"));
    for (a, b) in nodes.iter().zip(&posterior_nodes) {
        assert_eq!(a["node"], b["node_id"]);
        assert!(
            (a["age"].parse::<f64>().unwrap() - b["age"].parse::<f64>().unwrap()).abs() < 1e-12
        );
    }
    let edges = rows(&result.join("edges.tsv"));
    let posterior_edges = rows(&fit.join("posteriors/edges.tsv"));
    for (a, b) in edges.iter().zip(&posterior_edges) {
        assert_eq!(a["parent"], b["parent_id"]);
        assert_eq!(a["child"], b["child_id"]);
        assert_eq!(a["child_order"], b["child_order"]);
    }
    let states = rows(&result.join("states.tsv"));
    let mut switches = 0;
    for row in all_rows(&result, "anagenetic_events.tsv") {
        if row["event_kind"] == "range_switching" {
            switches += 1;
            assert_eq!(row["source_status"], "state_transition");
            let from = row["from_state_index"].parse::<usize>().unwrap();
            let bits = states[from]["range_bits"].parse::<u64>().unwrap();
            assert_eq!(row["source_area_index"], bits.trailing_zeros().to_string());
        }
    }
    assert!(switches > 0);
    let mut multiarea_targets = 0;
    for row in all_rows(&result, "cladogenetic_splits.tsv") {
        if row["event_kind"] == "founder_event" {
            let key = if row["founder_daughter_order"] == "0" {
                "left_state_index"
            } else {
                "right_state_index"
            };
            let target = row[key].parse::<usize>().unwrap();
            if states[target]["range_bits"]
                .parse::<u64>()
                .unwrap()
                .count_ones()
                > 1
            {
                multiarea_targets += 1;
            }
        }
    }
    assert!(multiarea_targets > 0);
}

#[test]
fn v3_inspection_rejects_inconsistent_node_age_and_child_order() {
    let fixture = Fixture::new(false);
    let result = fixture.run(&exe(), "dictionary", "dec", "0", "summary", &[]);
    success(&inspect(&result));
    for (file, column, replacement) in [("nodes.tsv", 3, "99"), ("edges.tsv", 4, "9")] {
        let path = result.join(file);
        let original = fs::read_to_string(&path).unwrap();
        let mut lines = original.lines().map(str::to_string).collect::<Vec<_>>();
        let mut fields = lines[1].split('\t').map(str::to_string).collect::<Vec<_>>();
        fields[column] = replacement.to_string();
        lines[1] = fields.join("\t");
        fs::write(&path, lines.join("\n") + "\n").unwrap();
        assert!(!inspect(&result).status.success());
        fs::write(&path, original).unwrap();
        success(&inspect(&result));
    }
}

#[test]
fn single_tip_root_has_no_split_or_branch_event() {
    let fixture = Fixture::new(false);
    fs::write(fixture.0.join("tree.nwk"), "A;").unwrap();
    fs::write(
        fixture.0.join("ranges.tsv"),
        "tip\tNorth\tEast\tWest\nA\t1\t0\t0\n",
    )
    .unwrap();
    let result = fixture.run(&exe(), "single", "dec", "0", "compact", &[]);
    success(&inspect(&result));
    assert!(all_rows(&result, "cladogenetic_splits.tsv").is_empty());
    assert!(all_rows(&result, "anagenetic_events.tsv").is_empty());
}

#[test]
#[ignore = "requires BIOGEO_BSM_BASELINE_EXE pointing to the frozen pre-v3 executable"]
fn pre_v3_executable_same_seed_histories_are_unchanged() {
    let baseline =
        PathBuf::from(std::env::var_os("BIOGEO_BSM_BASELINE_EXE").expect("baseline exe"));
    let fixture = Fixture::new(false);
    for model in ["dec", "divalike", "bayarealike"] {
        for j in ["0", "0.5"] {
            for level in ["full", "compact"] {
                let old = fixture.run(
                    &baseline,
                    &format!("{model}-{j}-{level}-old"),
                    model,
                    j,
                    level,
                    &[],
                );
                let new = fixture.run(
                    &exe(),
                    &format!("{model}-{j}-{level}-new"),
                    model,
                    j,
                    level,
                    &[],
                );
                success(&inspect(&old));
                success(&inspect(&new));
                assert_projection(&old, &new);
                check_counts(&new);
                if model == "dec" && j == "0" && level == "compact" {
                    let files = TABLES
                        .iter()
                        .copied()
                        .chain(["metadata.tsv"])
                        .collect::<Vec<_>>();
                    let before = files
                        .iter()
                        .map(|f| fs::read(old.join(f)).unwrap())
                        .collect::<Vec<_>>();
                    let attempt = Command::new(exe())
                        .arg(model)
                        .arg("--tree")
                        .arg(fixture.0.join("tree.nwk"))
                        .arg("--ranges")
                        .arg(fixture.0.join("ranges.tsv"))
                        .args([
                            "--d",
                            "0.2",
                            "--e",
                            "0.1",
                            "--j",
                            j,
                            "--max-range-size",
                            "3",
                            "--include-null-range",
                            "--bsm-samples",
                            "256",
                            "--seed",
                            "20260928",
                            "--bsm-threads",
                            "1",
                            "--bsm-output-level",
                            level,
                            "--bsm-resume",
                            "--bsm-output-dir",
                        ])
                        .arg(&old)
                        .output()
                        .unwrap();
                    assert!(!attempt.status.success(), "v3 must not resume into v2");
                    for (file, bytes) in files.iter().zip(before) {
                        assert_eq!(fs::read(old.join(file)).unwrap(), bytes);
                    }
                }
                eprintln!("same-seed PASS: {model} j={j} {level}, 256 samples, 8 tables");
            }
        }
    }
}
