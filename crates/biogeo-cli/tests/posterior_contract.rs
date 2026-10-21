use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const TREE: &str = "(('A+B':0.3,'C D':0.3)'inner+label':0.7,'E%F':1.0)root;";
const HOOK_TREE: &str = "(('A+B':0.0000001,'C D':0.5)'inner+label':0.5,'E%F':1.0)root;";
const RANGES: &str = "tip\tNorth+East\tWest%\nA+B\t1\t0\nC D\t1\t0\nE%F\t0\t1\n";

struct Fixture(PathBuf);
impl Fixture {
    fn new(preset: &str, tree: &str, include: bool, flags: (bool, bool), mode: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "biogeo-posterior-{}-{} 空格",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("tree.nwk"), tree).unwrap();
        fs::write(root.join("ranges.tsv"), RANGES).unwrap();
        let output = cli(&["parameter-template", "--preset", preset]);
        success(&output);
        let source = String::from_utf8(output.stdout).unwrap();
        let parameters = if mode == "evaluate" {
            let table = biogeo_core::parse_parameter_table(&source).unwrap();
            let resolved = table.resolve_initial().unwrap();
            let mut fixed = table;
            for (name, value) in resolved.iter() {
                fixed = fixed.with_fixed(name, value).unwrap();
            }
            fixed.to_versioned_tsv()
        } else {
            source
        };
        fs::write(root.join("parameters.tsv"), parameters).unwrap();
        let mut request = format!(
            "key\tvalue\nformat\tbiogeo-analysis-request-v1\nmode\t{mode}\nobservation\texact_ranges\ntree\ttree.nwk\nranges\tranges.tsv\nparameters\tparameters.tsv\nmax_range_size\t2\ninclude_null_range\t{include}\nroot_prior\tflat\nmin_branch_length\t{}\nancestral_probabilities\t{}\nsplit_probabilities\t{}\n",
            if tree == HOOK_TREE { "0.000001" } else { "0" },
            flags.0,
            flags.1
        );
        if mode == "optimize" {
            request.push_str("optimization_initial_step\t0.5\noptimization_tolerance\t1e-8\noptimization_max_iterations\t100\n");
        }
        fs::write(root.join("request.tsv"), request).unwrap();
        Self(root)
    }
    fn run(&self) -> PathBuf {
        let result = self.0.join("result");
        let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
            .arg("analysis-run")
            .arg("--request")
            .arg(self.0.join("request.tsv"))
            .arg("--output-dir")
            .arg(&result)
            .output()
            .unwrap();
        success(&output);
        result
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(args)
        .output()
        .unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn inspect(result: &Path, replay: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"));
    cmd.arg("analysis-result-inspect")
        .arg("--analysis-result")
        .arg(result);
    if replay {
        cmd.arg("--replay");
    }
    cmd.output().unwrap()
}
fn table(path: &Path) -> Vec<Vec<String>> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split('\t').map(str::to_string).collect())
        .collect()
}
fn metadata(path: &Path) -> BTreeMap<String, String> {
    table(path)
        .into_iter()
        .map(|r| (r[0].clone(), r[1].clone()))
        .collect()
}

fn compare_to_core(result: &Path, tree: &str, include: bool, threshold: f64) {
    let parsed = biogeo_core::parse_newick(tree)
        .unwrap()
        .with_direct_ancestor_hooks_below(threshold)
        .unwrap();
    let ranges = biogeo_core::parse_tip_ranges_table(RANGES, &parsed).unwrap();
    let states = biogeo_core::StateSpace::new(2, 2, include).unwrap();
    let parameters = biogeo_core::parse_parameter_table(
        &fs::read_to_string(result.join("resolved-parameters.tsv")).unwrap(),
    )
    .unwrap();
    let model = biogeo_core::ModelConfig::from_biogeobears_core_parameters(
        &parameters.resolve_initial().unwrap(),
    )
    .unwrap();
    let pruning = biogeo_core::run_fixed_model_likelihoods(
        &parsed.tree,
        &states,
        &ranges.tip_likelihoods(&states).unwrap(),
        &model,
        biogeo_core::RootPrior::Flat,
    )
    .unwrap();
    let expected = biogeo_core::model_node_state_posteriors(
        &parsed.tree,
        &states,
        &pruning,
        &model,
        biogeo_core::RootPrior::Flat,
    )
    .unwrap();
    let root = result.join("posteriors");
    for row in table(&root.join("nodes.tsv")) {
        let node = row[0].parse::<usize>().unwrap();
        let actual_label = row[2].replace("%25", "%");
        assert_eq!(actual_label, parsed.node_label(node).unwrap_or(""));
        assert_eq!(row[4], (node == parsed.tree.root()).to_string());
    }
    for row in table(&root.join("edges.tsv")) {
        let edge = &parsed.tree.edges()[row[0].parse::<usize>().unwrap()];
        assert_eq!(row[1].parse::<usize>().unwrap(), edge.parent);
        assert_eq!(row[2].parse::<usize>().unwrap(), edge.child);
        assert_eq!(row[3].parse::<f64>().unwrap(), edge.length);
    }
    let rows = table(&root.join("node_posteriors.tsv"));
    assert_eq!(
        rows.len(),
        parsed.tree.postorder_internal_nodes().len() * states.len()
    );
    for row in &rows {
        let node: usize = row[0].parse().unwrap();
        let state: usize = row[1].parse().unwrap();
        let actual: f64 = row[2].parse().unwrap();
        let expected = expected
            .iter()
            .find(|p| p.node == node)
            .unwrap()
            .probabilities[state];
        assert!(
            (actual - expected).abs() < 1e-12,
            "node={node}, state={state}"
        );
    }
    let expected = biogeo_core::model_split_scenario_posteriors(
        &parsed.tree,
        &states,
        &pruning,
        &model,
        biogeo_core::RootPrior::Flat,
    )
    .unwrap();
    let rows = table(&root.join("split_posteriors.tsv"));
    assert_eq!(rows.len(), expected.len());
    for row in &rows {
        let node = row[0].parse::<usize>().unwrap();
        let children = parsed.tree.children(node).unwrap();
        assert_eq!(row[1].parse::<usize>().unwrap(), children[0].node);
        assert_eq!(row[2].parse::<usize>().unwrap(), children[1].node);
        let ancestor = row[3].parse::<usize>().unwrap();
        let left = row[4].parse::<usize>().unwrap();
        let right = row[5].parse::<usize>().unwrap();
        let expected = expected
            .iter()
            .find(|p| (p.node, p.ancestor, p.left, p.right) == (node, ancestor, left, right))
            .unwrap();
        assert!((row[7].parse::<f64>().unwrap() - expected.probability).abs() < 1e-12);
    }
}

#[test]
fn six_presets_both_null_modes_export_exact_core_probabilities_and_mapping() {
    for preset in [
        "dec",
        "dec+j",
        "divalike",
        "divalike+j",
        "bayarealike",
        "bayarealike+j",
    ] {
        for include in [false, true] {
            let fixture = Fixture::new(preset, TREE, include, (true, true), "evaluate");
            let result = fixture.run();
            success(&inspect(&result, true));
            compare_to_core(&result, TREE, include, 0.0);
            let nodes = table(&result.join("posteriors/nodes.tsv"));
            let labels = nodes
                .iter()
                .filter(|r| r[1] == "tip")
                .map(|r| r[2].as_str())
                .collect::<Vec<_>>();
            assert!(labels.contains(&"A+B"));
            assert!(labels.contains(&"C D"));
            assert!(labels.contains(&"E%25F"));
            let meta = metadata(&result.join("posteriors/metadata.tsv"));
            assert_eq!(meta["posterior_location"], "branch_top_at_node");
            assert_eq!(meta["include_null_range"], include.to_string());
            assert_eq!(meta["analysis_result_format"], "biogeo-analysis-result-v3");
        }
    }
}

#[test]
fn optimized_posteriors_use_final_parameters_and_survive_relocation() {
    let fixture = Fixture::new("dec+j", TREE, true, (true, true), "optimize");
    let result = fixture.run();
    compare_to_core(&result, TREE, true, 0.0);
    let moved = fixture.0.join("moved result");
    fs::rename(result, &moved).unwrap();
    fs::remove_file(fixture.0.join("tree.nwk")).unwrap();
    fs::remove_file(fixture.0.join("ranges.tsv")).unwrap();
    success(&inspect(&moved, true));
}

#[test]
fn direct_ancestor_and_non_extant_tip_markers_do_not_create_fake_split_events() {
    let fixture = Fixture::new("dec", HOOK_TREE, true, (true, true), "evaluate");
    let result = fixture.run();
    success(&inspect(&result, true));
    compare_to_core(&result, HOOK_TREE, true, 1e-6);
    let nodes = table(&result.join("posteriors/nodes.tsv"));
    assert!(
        nodes
            .iter()
            .any(|r| r[2] == "A+B" && r[6] == "direct_ancestor_tip")
    );
    let direct = nodes.iter().find(|r| r[5] == "true").unwrap();
    assert!(
        !table(&result.join("posteriors/split_posteriors.tsv"))
            .iter()
            .any(|r| r[0] == direct[0])
    );
    let tree = "(('A+B':0.2,'C D':0.5):0.5,'E%F':1.0);";
    let fossil = Fixture::new("dec", tree, true, (true, false), "evaluate");
    let result = fossil.run();
    success(&inspect(&result, true));
    assert!(
        table(&result.join("posteriors/nodes.tsv"))
            .iter()
            .any(|r| r[2] == "A+B" && r[6] == "non_extant_tip")
    );
}

#[test]
fn flags_keep_absent_tables_distinct_from_zero_probabilities() {
    for (ancestral, splits) in [(false, false), (true, false), (false, true)] {
        let fixture = Fixture::new("dec", TREE, false, (ancestral, splits), "evaluate");
        let result = fixture.run();
        assert_eq!(result.join("posteriors").exists(), ancestral || splits);
        assert_eq!(
            result.join("posteriors/node_posteriors.tsv").exists(),
            ancestral
        );
        assert_eq!(
            result.join("posteriors/split_posteriors.tsv").exists(),
            splits
        );
        success(&inspect(&result, true));
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}
fn refresh_checksum(root: &Path, name: &str) {
    let bytes = fs::read(root.join(name)).unwrap();
    let mut rows = table(&root.join("files.tsv"));
    let record = rows.iter_mut().find(|r| r[0] == name).unwrap();
    record[1] = bytes.len().to_string();
    record[2] = fingerprint(&bytes);
    let manifest = format!(
        "file\tbytes\tfingerprint\n{}\n",
        rows.iter()
            .map(|r| r.join("\t"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fs::write(root.join("files.tsv"), &manifest).unwrap();
    let mut meta = metadata(&root.join("metadata.tsv"));
    meta.insert("files_fingerprint".into(), fingerprint(manifest.as_bytes()));
    let text = format!(
        "key\tvalue\n{}\n",
        meta.iter()
            .map(|(k, v)| format!("{k}\t{v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    fs::write(root.join("metadata.tsv"), text).unwrap();
}

#[test]
fn inspect_rejects_missing_corrupt_and_non_normalized_posteriors() {
    let fixture = Fixture::new("dec", TREE, true, (true, true), "evaluate");
    let result = fixture.run();
    let root = result.join("posteriors");
    let probabilities = root.join("node_posteriors.tsv");
    let original = fs::read_to_string(&probabilities).unwrap();
    fs::remove_file(&probabilities).unwrap();
    assert!(!inspect(&result, false).status.success());
    fs::write(&probabilities, format!("{original}\n")).unwrap();
    assert!(!inspect(&result, false).status.success());
    let mut data = original.lines().map(str::to_string).collect::<Vec<_>>();
    let mut first = data[1].split('\t').map(str::to_string).collect::<Vec<_>>();
    first[2] = "0.5".into();
    data[1] = first.join("\t");
    fs::write(&probabilities, format!("{}\n", data.join("\n"))).unwrap();
    refresh_checksum(&root, "node_posteriors.tsv");
    let output = inspect(&result, false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("sum to one"));
    fs::remove_dir_all(&root).unwrap();
    assert!(!inspect(&result, false).status.success());
}

#[test]
fn portable_v2_results_remain_readable_and_migrate_without_inventing_posteriors() {
    let fixture = Fixture::new("dec", TREE, false, (false, false), "evaluate");
    let result = fixture.run();
    let path = result.join("metadata.tsv");
    let text = fs::read_to_string(&path)
        .unwrap()
        .replace("biogeo-analysis-result-v3", "biogeo-analysis-result-v2")
        .replace("posteriors_available\tfalse\n", "");
    fs::write(path, text).unwrap();
    success(&inspect(&result, true));
    let migrated = fixture.0.join("migrated");
    let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .arg("analysis-result-migrate")
        .arg("--analysis-result")
        .arg(&result)
        .arg("--output-dir")
        .arg(&migrated)
        .output()
        .unwrap();
    success(&output);
    success(&inspect(&migrated, true));
    assert!(!migrated.join("posteriors").exists());
}

#[test]
fn named_nexus_translate_preserves_original_tip_mapping() {
    let fixture = Fixture::new("dec", TREE, true, (true, true), "evaluate");
    fs::write(fixture.0.join("tree.nwk"), "#NEXUS\nBegin trees;\nTranslate 1 'A+B', 2 'C D', 3 'E%F';\nTree other = (1:1,(2:0.3,3:0.3):0.7);\nTree selected = ((1:0.3,2:0.3)'inner+label':0.7,3:1)root;\nEnd;\n").unwrap();
    let request_path = fixture.0.join("request.tsv");
    let mut request = fs::read_to_string(&request_path).unwrap();
    request.push_str("tree_name\tselected\n");
    fs::write(request_path, request).unwrap();
    let result = fixture.run();
    success(&inspect(&result, true));
    compare_to_core(&result, TREE, true, 0.0);
}

#[test]
fn optimization_diagnostics_are_saved_replayed_and_integrity_checked() {
    for include in [false, true] {
        let fixture = Fixture::new("bayarealike+j", TREE, include, (true, true), "optimize");
        let result = fixture.run();
        success(&inspect(&result, true));
        let path = result.join("optimization-starts.tsv");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("format\tbiogeo-optimization-starts-v1\n"));
        let rows: Vec<_> = text
            .lines()
            .skip(4)
            .map(|row| row.split('\t').collect::<Vec<_>>())
            .collect();
        assert!(rows.iter().any(|row| row[0] == "nested_j_zero"));
        let selected: Vec<_> = rows.iter().filter(|row| row[2] == "true").collect();
        assert_eq!(selected.len(), 3);
        let meta = metadata(&result.join("metadata.tsv"));
        for row in selected {
            assert_eq!(row[0], "target");
            assert!(
                (row[4].parse::<f64>().unwrap() - meta["lnL"].parse::<f64>().unwrap()).abs()
                    < 1e-12
            );
            assert!(matches!(row[3], "converged" | "iteration_limit"));
            assert!(row[9].parse::<f64>().unwrap().is_finite());
            assert!(row[10].parse::<f64>().unwrap().is_finite());
        }
        fs::write(path, text.replace("target", "edited")).unwrap();
        assert!(!inspect(&result, false).status.success());
    }
}

#[test]
fn inspect_rejects_wrong_child_order_and_posteriors_from_another_fit() {
    let fixture = Fixture::new("dec", TREE, true, (true, true), "evaluate");
    let result = fixture.run();
    let root = result.join("posteriors");
    let edges_path = root.join("edges.tsv");
    let original = fs::read_to_string(&edges_path).unwrap();
    let mut lines = original.lines().map(str::to_string).collect::<Vec<_>>();
    let mut edge = lines[1].split('\t').map(str::to_string).collect::<Vec<_>>();
    edge[4] = "99".into();
    lines[1] = edge.join("\t");
    fs::write(&edges_path, format!("{}\n", lines.join("\n"))).unwrap();
    refresh_checksum(&root, "edges.tsv");
    let output = inspect(&result, false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("child order"));
    fs::write(&edges_path, original).unwrap();
    refresh_checksum(&root, "edges.tsv");
    success(&inspect(&result, false));
    let other = Fixture::new("dec", TREE, false, (true, true), "evaluate");
    let other_result = other.run();
    let other_root = other_result.join("posteriors");
    for entry in fs::read_dir(&root).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), other_root.join(entry.file_name())).unwrap();
    }
    let output = inspect(&other_result, false);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("analysis_result_fingerprint"));
}
