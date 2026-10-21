use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(tips: usize, include_null: bool, constrained: bool) -> Self {
        let root = std::env::temp_dir().join(format!(
            "biogeo-small-comparison-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let (tree, ranges) = match tips {
            3 => (
                "((a:5,b:5):5,c:10);",
                "a\t1\t0\t0\nb\t0\t1\t0\nc\t0\t0\t1\n",
            ),
            4 => (
                "(('a':5,'b':5):5,('c':5,'d':5):5);",
                "a\t1\t0\t0\nb\t1\t0\t0\nc\t0\t1\t0\nd\t0\t0\t1\n",
            ),
            5 => (
                "((a:5,b:5):5,(c:5,(d:2,e:2):3):5);",
                "a\t1\t0\t0\nb\t1\t0\t0\nc\t0\t1\t0\nd\t0\t0\t1\ne\t1\t1\t0\n",
            ),
            8 => (
                "(((a:1,b:1):1,(c:1,d:1):1):1,((e:1,f:1):1,(g:1,h:1):1):1);",
                "a\t1\t0\t0\nb\t1\t0\t0\nc\t0\t1\t0\nd\t0\t0\t1\ne\t0\t1\t0\nf\t0\t0\t1\ng\t1\t0\t0\nh\t0\t1\t0\n",
            ),
            _ => unreachable!(),
        };
        fs::write(root.join("tree.nwk"), tree).unwrap();
        fs::write(root.join("ranges.tsv"), format!("tip\tA\tB\tC\n{ranges}")).unwrap();
        let mut manifest = String::from("biogeo-model-batch-manifest-v1\nmodel_id\tparameters\n");
        use biogeo_core::BioGeoBearsPreset::*;
        for (name, preset) in [
            ("DEC", Dec),
            ("DEC+J", DecJ),
            ("DIVALIKE", DivaLike),
            ("DIVALIKE+J", DivaLikeJ),
            ("BAYAREALIKE", BayAreaLike),
            ("BAYAREALIKE+J", BayAreaLikeJ),
        ] {
            let file = format!("{name}.tsv");
            fs::write(
                root.join(&file),
                preset.parameter_table().unwrap().to_versioned_tsv(),
            )
            .unwrap();
            manifest.push_str(&format!("{name}\t{file}\n"));
        }
        fs::write(root.join("models.tsv"), manifest).unwrap();
        let mut config = String::from(
            "biogeo-model-batch-config-v1\noption\tvalue\n--tree\ttree.nwk\n--ranges\tranges.tsv\n--max-range-size\t3\n--root-prior\tflat\n--min-branch-length\t0\n--initial-step\t0.2\n--tolerance\t1e-8\n--max-iterations\t1000\n",
        );
        if include_null {
            config.push_str("--include-null-range\ttrue\n");
        }
        if constrained {
            fs::write(root.join("allowed.tsv"), "# format\tbiogeo-allowed-ranges-v1\nrange\tA\tB\tC\nC\t0\t0\t1\nB\t0\t1\t0\nA\t1\t0\t0\nB+C\t0\t1\t1\nA+C\t1\t0\t1\nA+B\t1\t1\t0\n").unwrap();
            fs::write(root.join("strata.tsv"), "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\tallowed_ranges\n10\t-\t-\t-\t-\t-\t-\tallowed.tsv\n").unwrap();
            config.push_str("--dispersal-strata\tstrata.tsv\n");
        }
        fs::write(root.join("model-config.tsv"), config).unwrap();
        Self(root)
    }

    fn request(&self, criterion: &str, selection: &str) -> PathBuf {
        let path = self.0.join(format!("workflow-{criterion}-{selection}.tsv"));
        let mut text = format!(
            "key\tvalue\nformat\tbiogeo-model-workflow-request-v1\nmodels\tmodels.tsv\nconfig\tmodel-config.tsv\ncomparison_criterion\t{criterion}\nbsm_selection\t{selection}\n"
        );
        if selection != "none" {
            text.push_str(
                "bsm_samples\t2\nbsm_seed\t91\nbsm_output_level\tsummary\nbsm_threads\t1\n",
            );
        }
        if selection == "model_id" {
            text.push_str("bsm_model_id\tDEC+J\n");
        }
        fs::write(&path, text).unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn workflow(request: &Path, output: &Path, resume: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"));
    cmd.args(["--error-format", "tsv", "model-workflow", "--request"])
        .arg(request)
        .arg("--output-dir")
        .arg(output);
    if resume {
        cmd.arg("--resume");
    }
    cmd.output().unwrap()
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn section<'a>(text: &'a str, name: &str) -> Vec<Vec<&'a str>> {
    let body = text.split_once(&format!("\n\n{name}\n")).unwrap().1;
    body.split("\n\n")
        .next()
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split('\t').collect())
        .collect()
}

fn check_comparison(output: &Path, tips: usize) {
    let batch = output.join("model-batch");
    assert!(batch.join("complete.tsv").is_file());
    let comparison = fs::read_to_string(batch.join("comparison.tsv")).unwrap();
    let rows = section(&comparison, "model_comparison");
    assert_eq!(rows.len(), 6);
    assert!(comparison.contains("eligible_models\t6\n"), "{comparison}");
    let defined = match tips {
        3 => 0,
        4 => 3,
        5 => 6,
        _ => unreachable!(),
    };
    assert!(comparison.contains(&format!("aicc_defined_models\t{defined}\n")));
    assert!(comparison.contains(&format!(
        "aicc_eligible_models\t{}\n",
        if tips == 5 { 6 } else { 0 }
    )));
    let mut aic_sum = 0.0;
    let mut aicc_sum = 0.0;
    for row in rows {
        assert_eq!(row.len(), 16);
        let k: usize = row[4].parse().unwrap();
        let lnl: f64 = row[6].parse().unwrap();
        let aic: f64 = row[8].parse().unwrap();
        assert!((aic - (2.0 * k as f64 - 2.0 * lnl)).abs() < 1e-10);
        aic_sum += row[10].parse::<f64>().unwrap();
        if tips > k + 1 {
            let aicc: f64 = row[12].parse().unwrap();
            assert!(
                (aicc - aic - 2.0 * (k * (k + 1)) as f64 / (tips - k - 1) as f64).abs() < 1e-10
            );
        } else {
            assert_eq!(row[12], "NA");
        }
        if tips == 5 {
            aicc_sum += row[14].parse::<f64>().unwrap();
        } else {
            assert_eq!(&row[13..16], &["NA", "NA", "NA"]);
        }
    }
    assert!((aic_sum - 1.0).abs() < 1e-12);
    if tips == 5 {
        assert!((aicc_sum - 1.0).abs() < 1e-12);
    }
    let average = fs::read_to_string(batch.join("model-averaged-ancestral-ranges.tsv")).unwrap();
    assert!(average.contains("aic_models\t6\n"));
    assert!(average.contains(&format!("aicc_models\t{}\n", if tips == 5 { 6 } else { 0 })));
    assert!(average.contains(&format!("criteria\t{}\n", if tips == 5 { 2 } else { 1 })));
    for name in [
        "ancestral_state_probabilities",
        "cladogenetic_split_probabilities",
    ] {
        let mut sums = BTreeMap::new();
        let scenarios = section(&average, "split_scenarios");
        let nodes: BTreeMap<_, _> = scenarios.iter().map(|row| (row[0], row[1])).collect();
        for row in section(&average, name) {
            assert!(row[0] == "AIC" || (tips == 5 && row[0] == "AICc"));
            let node = if name == "ancestral_state_probabilities" {
                row[1]
            } else {
                nodes[row[1]]
            };
            let probability: f64 = row.last().unwrap().parse().unwrap();
            assert!(probability.is_finite() && probability >= 0.0);
            *sums.entry((row[0], node)).or_insert(0.0) += probability;
        }
        assert_eq!(sums.len(), (tips - 1) * if tips == 5 { 2 } else { 1 });
        assert!(sums.values().all(|sum| (sum - 1.0_f64).abs() < 1e-10));
    }
}

#[test]
fn four_tip_six_models_keep_aic_when_only_some_aicc_scores_exist() {
    for include in [false, true] {
        let fixture = Fixture::new(4, include, true);
        for criterion in ["aic", "aicc"] {
            let request = fixture.request(criterion, "none");
            let out = fixture.0.join(format!("result-{criterion}"));
            success(&workflow(&request, &out, false));
            check_comparison(&out, 4);
            assert!(out.join("complete.tsv").is_file());
            let comparison = fs::read(out.join("model-batch/comparison.tsv")).unwrap();
            let fit = fs::read(out.join("model-batch/models/DEC/metadata.tsv")).unwrap();
            success(&workflow(&request, &out, true));
            assert_eq!(
                fs::read(out.join("model-batch/comparison.tsv")).unwrap(),
                comparison
            );
            assert_eq!(
                fs::read(out.join("model-batch/models/DEC/metadata.tsv")).unwrap(),
                fit
            );
        }
    }
}

#[test]
fn aicc_all_missing_and_all_available_remain_valid() {
    for tips in [3, 5] {
        for include in [false, true] {
            let fixture = Fixture::new(tips, include, false);
            let out = fixture.0.join("result");
            success(&workflow(&fixture.request("aicc", "none"), &out, false));
            check_comparison(&out, tips);
        }
    }
}

#[test]
fn missing_aicc_ranking_is_a_regular_error_not_an_aic_fallback() {
    let fixture = Fixture::new(4, false, true);
    let out = fixture.0.join("automatic");
    let failed = workflow(&fixture.request("aicc", "best_by_criterion"), &out, false);
    assert_eq!(failed.status.code(), Some(2));
    let error = String::from_utf8(failed.stderr).unwrap();
    assert!(error.contains("available aicc rank"), "{error}");
    assert!(!error.contains("panicked"));
    check_comparison(&out, 4);
    assert!(!out.join("complete.tsv").exists());
    assert!(!out.join("bsm-result").exists());
    let explicit = fixture.0.join("explicit");
    success(&workflow(
        &fixture.request("aicc", "model_id"),
        &explicit,
        false,
    ));
    check_comparison(&explicit, 4);
    assert!(explicit.join("complete.tsv").is_file());
    assert!(explicit.join("bsm-result").is_dir());
}

#[test]
fn eight_tip_converged_tie_is_selected_and_included_in_six_model_comparison() {
    for include in [false, true] {
        let fixture = Fixture::new(8, include, false);
        let config = fixture.0.join("model-config.tsv");
        let text = fs::read_to_string(&config)
            .unwrap()
            .replace("--max-iterations\t1000\n", "--max-iterations\t200\n");
        fs::write(config, text).unwrap();
        let out = fixture.0.join("result");
        let request = fixture.request("aic", "none");
        success(&workflow(&request, &out, false));
        let comparison_path = out.join("model-batch/comparison.tsv");
        let comparison = fs::read_to_string(&comparison_path).unwrap();
        assert!(comparison.contains("eligible_models\t6\n"), "{comparison}");
        let rows = section(&comparison, "model_comparison");
        let decj = rows.iter().find(|row| row[0] == "DEC+J").unwrap();
        assert_eq!(decj[2], "true");
        assert_eq!(decj[7], "true");
        let fit = out.join("model-batch/models/DEC+J");
        let metadata_text = fs::read_to_string(fit.join("metadata.tsv")).unwrap();
        let metadata: BTreeMap<_, _> = metadata_text
            .lines()
            .skip(1)
            .map(|line| line.split_once('\t').unwrap())
            .collect();
        assert_eq!(metadata["optimization_converged"], "true");
        let diagnostics_path = fit.join("optimization-starts.tsv");
        let diagnostics = fs::read_to_string(&diagnostics_path).unwrap();
        let starts = section(&diagnostics, "starts");
        let selected: Vec<_> = starts.iter().filter(|row| row[2] == "true").collect();
        assert_eq!(selected.len(), 3); // One row per free parameter, one target start.
        let selected_lnl: f64 = selected[0][4].parse().unwrap();
        for row in &selected {
            assert_eq!(row[0], "target");
            assert_eq!(row[1], selected[0][1]);
            assert_eq!(row[3], "converged");
            assert_eq!(row[4], selected[0][4]);
        }
        assert_eq!(selected_lnl, metadata["lnL"].parse::<f64>().unwrap());
        assert_eq!(selected_lnl, decj[6].parse::<f64>().unwrap());
        let best_lnl = starts
            .iter()
            .filter(|row| row[0] == "target")
            .map(|row| row[4].parse::<f64>().unwrap())
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(best_lnl - selected_lnl <= 8.0 * f64::EPSILON * best_lnl.abs().max(1.0));
        if include {
            assert!((selected_lnl + 6.883_749_534_478_134).abs() < 1e-9);
        }
        let replay = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
            .args(["analysis-result-inspect", "--analysis-result"])
            .arg(&fit)
            .arg("--replay")
            .output()
            .unwrap();
        success(&replay);
        success(&workflow(&request, &out, true));
        assert_eq!(fs::read_to_string(comparison_path).unwrap(), comparison);
        assert_eq!(fs::read_to_string(diagnostics_path).unwrap(), diagnostics);
        assert_eq!(
            fs::read_to_string(fit.join("metadata.tsv")).unwrap(),
            metadata_text
        );
    }
}

#[test]
fn batch_reuses_matching_no_j_fits_but_standalone_j_refits_them() {
    let fixture = Fixture::new(8, true, false);
    let config = fixture.0.join("model-config.tsv");
    fs::write(
        &config,
        fs::read_to_string(&config)
            .unwrap()
            .replace("--max-iterations\t1000\n", "--max-iterations\t200\n"),
    )
    .unwrap();
    let request = fixture.request("aic", "none");
    let batch = fixture.0.join("batch");
    success(&workflow(&request, &batch, false));
    for (reduced, full) in [
        ("DEC", "DEC+J"),
        ("DIVALIKE", "DIVALIKE+J"),
        ("BAYAREALIKE", "BAYAREALIKE+J"),
    ] {
        let reduced_fit = batch.join("model-batch/models").join(reduced);
        let full_fit = batch.join("model-batch/models").join(full);
        let reduced_parameters =
            fs::read_to_string(reduced_fit.join("resolved-parameters.tsv")).unwrap();
        let value = |name: &str| -> f64 {
            reduced_parameters
                .lines()
                .find(|line| line.starts_with(&format!("{name}\tfixed\t")))
                .unwrap()
                .split('\t')
                .nth(2)
                .unwrap()
                .parse()
                .unwrap()
        };
        let diagnostics = fs::read_to_string(full_fit.join("optimization-starts.tsv")).unwrap();
        let rows = section(&diagnostics, "starts");
        assert!(
            rows.iter().all(|row| row[0] == "target"),
            "{full}: {diagnostics}"
        );
        assert!(rows.iter().any(|row| row[1] == "1"));
        for name in ["d", "e"] {
            assert!(
                rows.iter().any(|row| row[0] == "target"
                    && row[1] == "2"
                    && row[8] == name
                    && row[9].parse::<f64>().unwrap() == value(name)),
                "{full} did not inherit {name} from {reduced}"
            );
        }
        assert!(
            rows.iter().any(|row| row[1] == "3"),
            "{full}: {diagnostics}"
        );
    }
    let comparison = fs::read(batch.join("model-batch/comparison.tsv")).unwrap();
    success(&workflow(&request, &batch, true));
    assert_eq!(
        fs::read(batch.join("model-batch/comparison.tsv")).unwrap(),
        comparison
    );

    fs::write(
        fixture.0.join("models.tsv"),
        "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC+J\tDEC+J.tsv\n",
    )
    .unwrap();
    let standalone = fixture.0.join("standalone");
    success(&workflow(&request, &standalone, false));
    let standalone_fit = standalone.join("model-batch/models/DEC+J");
    let standalone_diagnostics =
        fs::read_to_string(standalone_fit.join("optimization-starts.tsv")).unwrap();
    let standalone_rows = section(&standalone_diagnostics, "starts");
    assert!(standalone_rows.iter().any(|row| row[0] == "nested_j_zero"));
    assert_eq!(
        fs::read(batch.join("model-batch/models/DEC+J/resolved-parameters.tsv")).unwrap(),
        fs::read(standalone_fit.join("resolved-parameters.tsv")).unwrap(),
    );
    let batch_meta =
        fs::read_to_string(batch.join("model-batch/models/DEC+J/metadata.tsv")).unwrap();
    let standalone_meta = fs::read_to_string(standalone_fit.join("metadata.tsv")).unwrap();
    let field = |text: &str, name: &str| -> usize {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{name}\t")))
            .unwrap()
            .parse()
            .unwrap()
    };
    let nested_evaluations: usize = standalone_rows
        .iter()
        .find(|row| row[0] == "nested_j_zero")
        .unwrap()[6]
        .parse()
        .unwrap();
    assert_eq!(
        field(&standalone_meta, "optimization_evaluations")
            - field(&batch_meta, "optimization_evaluations"),
        nested_evaluations,
    );
}

#[test]
fn batch_refits_j_zero_when_no_matching_earlier_model_exists() {
    for mismatch in [false, true] {
        let fixture = Fixture::new(8, false, false);
        if mismatch {
            let path = fixture.0.join("DEC.tsv");
            let source = fs::read_to_string(&path).unwrap();
            fs::write(path, source.replace("e\tfree\t0.01\t", "e\tfree\t0.02\t")).unwrap();
        }
        let entries = if mismatch {
            "DEC\tDEC.tsv\nDEC+J\tDEC+J.tsv\n"
        } else {
            "DEC+J\tDEC+J.tsv\nDEC\tDEC.tsv\n"
        };
        fs::write(
            fixture.0.join("models.tsv"),
            format!("biogeo-model-batch-manifest-v1\nmodel_id\tparameters\n{entries}"),
        )
        .unwrap();
        let output = fixture.0.join("result");
        success(&workflow(&fixture.request("aic", "none"), &output, false));
        let diagnostics =
            fs::read_to_string(output.join("model-batch/models/DEC+J/optimization-starts.tsv"))
                .unwrap();
        assert!(
            section(&diagnostics, "starts")
                .iter()
                .any(|row| row[0] == "nested_j_zero")
        );
    }
}
