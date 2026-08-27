use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

const REGISTRY_FORMAT: &str = "biogeo-schema-registry-v1";
const CONTRACT_FORMAT: &str = "biogeo-schema-contract-v1";
const CONTRACT_HEADER: &str = "record_kind\tlocation\tname\trequirement\tvalue_type\tconstraint";

static NEXT_TEMP_DIR: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
struct ContractRow {
    record_kind: String,
    location: String,
    name: String,
    requirement: String,
    value_type: String,
    constraint: String,
}

#[derive(Debug)]
struct Contract {
    rows: Vec<ContractRow>,
}

#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let sequence = NEXT_TEMP_DIR.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "biogeo-schema-contract-{label}-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("schema contract temp directory should be created");
        Self { path }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[test]
fn published_schemas_match_real_cli_artifacts_and_streams() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let schemas = workspace.join("schemas");
    let registry = load_registry(&schemas);
    assert_eq!(registry.len(), 32);

    let registered_files = registry.values().cloned().collect::<BTreeSet<_>>();
    let disk_files = fs::read_dir(&schemas)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".schema.tsv"))
        .collect::<BTreeSet<_>>();
    assert_eq!(disk_files, registered_files, "schema registry drifted");

    let contracts = registry
        .iter()
        .map(|(format, file)| {
            let contract = load_contract(&schemas.join(file));
            assert_contract_declares_format(&contract, format);
            (format.clone(), contract)
        })
        .collect::<BTreeMap<_, _>>();

    let engine_info = run_cli(&workspace, ["engine-info".into()]);
    assert_success(&engine_info, "engine-info");
    let engine_info_stdout = String::from_utf8(engine_info.stdout).unwrap();
    validate_key_value_text(
        contract(&contracts, "biogeo-engine-capabilities-v1"),
        "stdout",
        &engine_info_stdout,
    );
    let engine_values = parse_key_value_text(&engine_info_stdout);
    assert_eq!(
        engine_values.get("engine_version").unwrap(),
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        engine_values.get("compatibility_policy_version").unwrap(),
        "biogeo-compatibility-policy-v1"
    );
    assert_eq!(
        engine_values.get("unknown_format_policy").unwrap(),
        "reject"
    );
    assert_eq!(engine_values.get("unknown_field_policy").unwrap(), "reject");
    assert_eq!(
        engine_values
            .get("public_format_count")
            .unwrap()
            .parse::<usize>()
            .unwrap(),
        registry.len()
    );
    let advertised_formats = engine_values["public_formats"]
        .split(',')
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        advertised_formats,
        registry.keys().cloned().collect::<BTreeSet<_>>(),
        "engine-info and schema registry format sets drifted"
    );

    let temp = TempDir::new("end-to-end");
    let template_dir = temp.path.join("analysis template");
    let template = run_cli(
        &workspace,
        [
            "analysis-template".into(),
            "--preset".into(),
            "dec".into(),
            "--mode".into(),
            "optimize".into(),
            "--output-dir".into(),
            template_dir.clone().into_os_string(),
        ],
    );
    assert_success(&template, "analysis-template");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-template-v1"),
        "stdout",
        &String::from_utf8(template.stdout).unwrap(),
    );
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-request-v1"),
        "file",
        &fs::read_to_string(template_dir.join("analysis.tsv")).unwrap(),
    );

    let request_dir = temp.path.join("统一 request with spaces");
    fs::create_dir(&request_dir).unwrap();
    for name in ["analysis.tsv", "tree.nwk", "ranges.tsv", "parameters.tsv"] {
        fs::copy(
            workspace.join("examples/analysis_request").join(name),
            request_dir.join(name),
        )
        .unwrap();
    }
    let request_path = request_dir.join("analysis.tsv");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-request-v1"),
        "file",
        &fs::read_to_string(&request_path).unwrap(),
    );
    let plan = run_cli(
        &workspace,
        [
            "analysis-plan".into(),
            "--request".into(),
            request_path.clone().into_os_string(),
        ],
    );
    assert_success(&plan, "analysis-plan");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-plan-v1"),
        "stdout",
        &String::from_utf8(plan.stdout).unwrap(),
    );
    let request_result_dir = temp.path.join("统一 request result");
    let request_run = run_cli(
        &workspace,
        [
            "--progress-format".into(),
            "tsv".into(),
            "analysis-run".into(),
            "--request".into(),
            request_path.clone().into_os_string(),
            "--output-dir".into(),
            request_result_dir.clone().into_os_string(),
        ],
    );
    assert_success(&request_run, "analysis-run");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-run-v2"),
        "stdout",
        &String::from_utf8(request_run.stdout).unwrap(),
    );
    validate_rows(
        contract(&contracts, "biogeo-cli-progress-v1"),
        "stderr_line",
        &String::from_utf8(request_run.stderr).unwrap(),
    );
    validate_directory(
        contract(&contracts, "biogeo-analysis-result-v2"),
        &request_result_dir,
    );

    let workflow_dir = temp.path.join("统一 analysis workflow");
    let workflow_args = [
        "analysis-workflow".into(),
        "--request".into(),
        request_path.clone().into_os_string(),
        "--output-dir".into(),
        workflow_dir.clone().into_os_string(),
        "--bsm-samples".into(),
        "2".into(),
        "--bsm-output-level".into(),
        "summary".into(),
        "--bsm-shard-samples".into(),
        "1".into(),
        "--bsm-threads".into(),
        "1".into(),
        "--seed".into(),
        "20260821".into(),
        "--deep".into(),
    ];
    let workflow = run_cli(&workspace, workflow_args.clone());
    assert_success(&workflow, "analysis-workflow");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-workflow-v1"),
        "stdout",
        &String::from_utf8(workflow.stdout).unwrap(),
    );
    validate_directory(
        contract(&contracts, "biogeo-analysis-result-v2"),
        &workflow_dir.join("analysis-result"),
    );
    let workflow_bsm_dir = workflow_dir.join("bsm-result");
    let workflow_bsm_metadata =
        parse_key_value_text(&fs::read_to_string(workflow_bsm_dir.join("metadata.tsv")).unwrap());
    validate_directory(
        contract(&contracts, workflow_bsm_metadata.get("format").unwrap()),
        &workflow_bsm_dir,
    );
    let mut workflow_resume_args = workflow_args.to_vec();
    workflow_resume_args.push("--resume".into());
    let workflow_resume = run_cli(&workspace, workflow_resume_args);
    assert_success(&workflow_resume, "analysis-workflow --resume");
    let workflow_resume_stdout = String::from_utf8(workflow_resume.stdout).unwrap();
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-workflow-v1"),
        "stdout",
        &workflow_resume_stdout,
    );
    assert!(workflow_resume_stdout.contains("analysis_reused\ttrue\n"));
    assert!(workflow_resume_stdout.contains("bsm_resumed\ttrue\n"));

    let model_workflow_request = workspace.join("examples/model_workflow/workflow.tsv");
    validate_key_value_text(
        contract(&contracts, "biogeo-model-workflow-request-v1"),
        "file",
        &fs::read_to_string(&model_workflow_request).unwrap(),
    );
    let model_workflow_plan = run_cli(
        &workspace,
        [
            "model-workflow-plan".into(),
            "--request".into(),
            model_workflow_request.clone().into_os_string(),
        ],
    );
    assert_success(&model_workflow_plan, "model-workflow-plan");
    validate_key_value_text(
        contract(&contracts, "biogeo-model-workflow-plan-v1"),
        "stdout",
        &String::from_utf8(model_workflow_plan.stdout).unwrap(),
    );
    let model_workflow_dir = temp.path.join("缁熶竴 model workflow");
    let model_workflow_args = [
        "model-workflow".into(),
        "--request".into(),
        model_workflow_request.into_os_string(),
        "--output-dir".into(),
        model_workflow_dir.clone().into_os_string(),
    ];
    let model_workflow_run = run_cli(&workspace, model_workflow_args.clone());
    assert_success(&model_workflow_run, "model-workflow");
    validate_key_value_text(
        contract(&contracts, "biogeo-model-workflow-run-v1"),
        "stdout",
        &String::from_utf8(model_workflow_run.stdout).unwrap(),
    );
    validate_directory(
        contract(&contracts, "biogeo-model-workflow-result-v1"),
        &model_workflow_dir,
    );
    let model_workflow_bsm = model_workflow_dir.join("bsm-result");
    let model_workflow_bsm_metadata =
        parse_key_value_text(&fs::read_to_string(model_workflow_bsm.join("metadata.tsv")).unwrap());
    validate_directory(
        contract(
            &contracts,
            model_workflow_bsm_metadata.get("format").unwrap(),
        ),
        &model_workflow_bsm,
    );
    let mut model_workflow_resume_args = model_workflow_args.to_vec();
    model_workflow_resume_args.push("--resume".into());
    let model_workflow_resume = run_cli(&workspace, model_workflow_resume_args);
    assert_success(&model_workflow_resume, "model-workflow --resume");
    let model_workflow_resume_stdout = String::from_utf8(model_workflow_resume.stdout).unwrap();
    validate_key_value_text(
        contract(&contracts, "biogeo-model-workflow-run-v1"),
        "stdout",
        &model_workflow_resume_stdout,
    );
    assert!(model_workflow_resume_stdout.contains("model_batch_resumed\ttrue\n"));
    assert!(model_workflow_resume_stdout.contains("bsm_resumed\ttrue\n"));

    let result_dir = temp.path.join("fit-result");
    let optimize = run_cli(
        &workspace,
        [
            "--progress-format".into(),
            "tsv".into(),
            "model-optimize".into(),
            "--tree".into(),
            workspace.join("examples/two_tip/tree.nwk").into_os_string(),
            "--ranges".into(),
            workspace
                .join("examples/two_tip/ranges.tsv")
                .into_os_string(),
            "--parameters".into(),
            workspace
                .join("examples/parameter_tables/dec.tsv")
                .into_os_string(),
            "--max-iterations".into(),
            "2".into(),
            "--analysis-result-dir".into(),
            result_dir.clone().into_os_string(),
        ],
    );
    assert_success(&optimize, "model-optimize");
    validate_rows(
        contract(&contracts, "biogeo-cli-progress-v1"),
        "stderr_line",
        &String::from_utf8(optimize.stderr).unwrap(),
    );
    validate_directory(
        contract(&contracts, "biogeo-analysis-result-v2"),
        &result_dir,
    );
    validate_directory(
        contract(&contracts, "biogeo-input-bundle-v1"),
        &result_dir.join("input-bundle"),
    );

    for level in ["full", "compact", "summary"] {
        for shard_samples in [None, Some("1")] {
            let layout = if shard_samples.is_some() {
                "sharded"
            } else {
                "monolithic"
            };
            let bsm_dir = temp.path.join(format!("bsm-{level}-{layout}"));
            let mut args = vec![
                "model-bsm".into(),
                "--analysis-result".into(),
                result_dir.clone().into_os_string(),
                "--bsm-samples".into(),
                "1".into(),
                "--bsm-output-dir".into(),
                bsm_dir.clone().into_os_string(),
                "--bsm-output-level".into(),
                level.into(),
                "--bsm-threads".into(),
                "1".into(),
                "--seed".into(),
                "20260811".into(),
            ];
            if let Some(shard_samples) = shard_samples {
                args.extend(["--bsm-shard-samples".into(), shard_samples.into()]);
            }
            let bsm = run_cli(&workspace, args);
            assert_success(&bsm, &format!("model-bsm {level} {layout}"));
            let metadata =
                parse_key_value_text(&fs::read_to_string(bsm_dir.join("metadata.tsv")).unwrap());
            let format = metadata.get("format").unwrap();
            validate_directory(contract(&contracts, format), &bsm_dir);
            let bsm_inspect = run_cli(
                &workspace,
                [
                    "bsm-inspect".into(),
                    "--bsm-result".into(),
                    bsm_dir.clone().into_os_string(),
                    "--deep".into(),
                ],
            );
            assert_success(&bsm_inspect, &format!("bsm-inspect {level} {layout}"));
            validate_key_value_text(
                contract(&contracts, "biogeo-bsm-inspection-v1"),
                "stdout",
                &String::from_utf8(bsm_inspect.stdout).unwrap(),
            );
        }
    }

    let inspect = run_cli(
        &workspace,
        [
            "analysis-result-inspect".into(),
            "--analysis-result".into(),
            result_dir.clone().into_os_string(),
            "--replay".into(),
        ],
    );
    assert_success(&inspect, "analysis-result-inspect");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-result-inspection-v1"),
        "stdout",
        &String::from_utf8(inspect.stdout).unwrap(),
    );

    let bundle_inspect = run_cli(
        &workspace,
        [
            "input-bundle-inspect".into(),
            "--input-bundle".into(),
            result_dir.join("input-bundle").into_os_string(),
        ],
    );
    assert_success(&bundle_inspect, "input-bundle-inspect");
    validate_key_value_text(
        contract(&contracts, "biogeo-input-bundle-inspection-v1"),
        "stdout",
        &String::from_utf8(bundle_inspect.stdout).unwrap(),
    );

    let legacy_dir = temp.path.join("legacy-result");
    make_legacy_result_fixture(&result_dir, &legacy_dir);
    let migrated_dir = temp.path.join("migrated-result");
    let migration = run_cli(
        &workspace,
        [
            "analysis-result-migrate".into(),
            "--analysis-result".into(),
            legacy_dir.into_os_string(),
            "--output-dir".into(),
            migrated_dir.clone().into_os_string(),
        ],
    );
    assert_success(&migration, "analysis-result-migrate");
    validate_key_value_text(
        contract(&contracts, "biogeo-analysis-result-migration-v1"),
        "stdout",
        &String::from_utf8(migration.stdout).unwrap(),
    );
    validate_directory(
        contract(&contracts, "biogeo-analysis-result-v2"),
        &migrated_dir,
    );

    let fossil_tree = temp.path.join("fossil-source.nwk");
    let fossil_manifest = temp.path.join("fossils.tsv");
    let fossil_output = temp.path.join("fossil-placement");
    fs::write(&fossil_tree, "((A:2,B:2):1,C:3);\n").unwrap();
    fs::write(
        &fossil_manifest,
        "biogeo-fossil-placement-manifest-v1\nfossil_id\tmin_age\tmax_age\tattachment\tstem_or_crown\tclade_tips\nF\t0.5\t1.5\tdirect_ancestor\tcrown\tA,B\n",
    )
    .unwrap();
    let fossil = run_cli(
        &workspace,
        [
            "fossil-place".into(),
            "--tree".into(),
            fossil_tree.into_os_string(),
            "--manifest".into(),
            fossil_manifest.into_os_string(),
            "--output-dir".into(),
            fossil_output.clone().into_os_string(),
            "--replicates".into(),
            "2".into(),
            "--seed".into(),
            "7".into(),
        ],
    );
    assert_success(&fossil, "fossil-place");
    validate_directory(
        contract(&contracts, "biogeo-fossil-placement-set-v1"),
        &fossil_output,
    );
    assert_eq!(
        fs::read_dir(fossil_output.join("trees")).unwrap().count(),
        2
    );

    let dec_path = temp.path.join("dec.tsv");
    let decj_path = temp.path.join("decj.tsv");
    fs::copy(
        workspace.join("examples/parameter_tables/dec.tsv"),
        &dec_path,
    )
    .unwrap();
    fs::copy(
        workspace.join("examples/parameter_tables/decj.tsv"),
        &decj_path,
    )
    .unwrap();
    let models_manifest = temp.path.join("models.tsv");
    fs::write(
        &models_manifest,
        "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\nDEC+J\tdecj.tsv\n",
    )
    .unwrap();
    let batch_output = temp.path.join("model-batch");
    let batch = run_cli(
        &workspace,
        [
            "model-batch".into(),
            "--manifest".into(),
            models_manifest.into_os_string(),
            "--output-dir".into(),
            batch_output.clone().into_os_string(),
            "--tree".into(),
            workspace.join("examples/two_tip/tree.nwk").into_os_string(),
            "--ranges".into(),
            workspace
                .join("examples/two_tip/ranges.tsv")
                .into_os_string(),
            "--max-iterations".into(),
            "2".into(),
        ],
    );
    assert_success(&batch, "model-batch");
    validate_sectioned_tsv(
        contract(&contracts, "biogeo-model-comparison-v3"),
        &fs::read_to_string(batch_output.join("comparison.tsv")).unwrap(),
    );
    validate_sectioned_tsv(
        contract(&contracts, "biogeo-model-averaged-ancestral-ranges-v2"),
        &fs::read_to_string(batch_output.join("model-averaged-ancestral-ranges.tsv")).unwrap(),
    );

    let error = run_cli(
        &workspace,
        [
            "--error-format".into(),
            "tsv".into(),
            "validate-inputs".into(),
            "--tree".into(),
            "missing.nwk".into(),
        ],
    );
    assert_eq!(error.status.code(), Some(2));
    assert!(error.stdout.is_empty());
    validate_key_value_text(
        contract(&contracts, "biogeo-cli-error-v1"),
        "stderr",
        &String::from_utf8(error.stderr).unwrap(),
    );
}

fn contract<'a>(contracts: &'a BTreeMap<String, Contract>, format: &str) -> &'a Contract {
    contracts
        .get(format)
        .unwrap_or_else(|| panic!("missing schema contract for {format}"))
}

fn load_registry(root: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(root.join("registry.tsv")).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(REGISTRY_FORMAT));
    assert_eq!(lines.next(), Some("format_id\tartifact_kind\tschema_file"));
    let mut registry = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 3, "invalid registry line {}", index + 3);
        assert!(matches!(
            fields[1],
            "directory"
                | "key_value_file"
                | "key_value_stdout"
                | "key_value_stderr"
                | "row_stderr"
                | "sectioned_tsv"
        ));
        assert!(fields[2].ends_with(".schema.tsv"));
        assert!(
            registry
                .insert(fields[0].to_string(), fields[2].to_string())
                .is_none(),
            "duplicate registered format {}",
            fields[0]
        );
    }
    registry
}

fn load_contract(path: &Path) -> Contract {
    let text = fs::read_to_string(path).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(CONTRACT_FORMAT), "schema {path:?}");
    assert_eq!(lines.next(), Some(CONTRACT_HEADER), "schema {path:?}");
    let mut rows = Vec::new();
    let mut identities = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(
            fields.len(),
            6,
            "schema {path:?} line {} must contain six fields",
            index + 3
        );
        assert!(matches!(fields[0], "file" | "directory" | "key" | "column"));
        assert!(matches!(
            fields[4],
            "key_value"
                | "table"
                | "sectioned_tsv"
                | "key_value_sectioned_tsv"
                | "utf8_file"
                | "binary_file"
                | "directory"
                | "literal"
                | "enum"
                | "string"
                | "encoded_string"
                | "optional_encoded_string"
                | "bool"
                | "u8"
                | "u64"
                | "usize"
                | "optional_usize"
                | "f64"
                | "optional_f64"
                | "na_or_f64"
                | "na_or_u64"
                | "na_or_usize"
                | "na_or_bool"
                | "auto_or_u8"
                | "auto_or_usize"
                | "none_or_usize"
                | "hex16"
                | "hex16_or_none"
                | "hex64"
                | "portable_path"
                | "semver"
                | "unbounded_or_f64"
                | "unlimited_or_usize"
                | "unlimited_or_f64"
                | "not_computed_or_usize"
        ));
        assert!(
            fields[3] == "required" || fields[3] == "optional" || fields[3].starts_with("when:")
        );
        assert!(!fields[1].is_empty() && !fields[2].is_empty());
        assert!(
            identities.insert((fields[0], fields[1], fields[2])),
            "duplicate schema row in {path:?}: {line}"
        );
        rows.push(ContractRow {
            record_kind: fields[0].to_string(),
            location: fields[1].to_string(),
            name: fields[2].to_string(),
            requirement: fields[3].to_string(),
            value_type: fields[4].to_string(),
            constraint: fields[5].to_string(),
        });
    }
    assert!(!rows.is_empty(), "schema {path:?} is empty");
    Contract { rows }
}

fn assert_contract_declares_format(contract: &Contract, format: &str) {
    let declarations = contract
        .rows
        .iter()
        .filter(|row| {
            (row.record_kind == "key" || row.record_kind == "column")
                && row.name == "format"
                && ((row.value_type == "literal" && row.constraint == format)
                    || (row.value_type == "enum"
                        && row
                            .constraint
                            .split('|')
                            .any(|candidate| candidate == format)))
        })
        .count();
    assert_eq!(declarations, 1, "format declaration for {format}");
}

fn validate_directory(contract: &Contract, root: &Path) {
    let metadata = root.join("metadata.tsv");
    let values = if metadata.is_file() {
        parse_key_value_text(&fs::read_to_string(&metadata).unwrap())
    } else {
        BTreeMap::new()
    };
    validate_directory_location(contract, root, ".", &values);

    let wildcard_locations = contract
        .rows
        .iter()
        .filter_map(|row| row.location.strip_suffix("/*"))
        .collect::<BTreeSet<_>>();
    for relative_parent in wildcard_locations {
        let parent = root.join(relative_parent);
        if !parent.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&parent).unwrap() {
            let entry = entry.unwrap();
            assert!(
                entry.path().is_dir(),
                "wildcard location {relative_parent} contains a non-directory entry"
            );
            validate_directory_location(
                contract,
                &entry.path(),
                &format!("{relative_parent}/*"),
                &values,
            );
        }
    }
}

fn validate_directory_location(
    contract: &Contract,
    root: &Path,
    location: &str,
    values: &BTreeMap<String, String>,
) {
    let entries = fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {root:?}: {error}"))
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    let rows = contract
        .rows
        .iter()
        .filter(|row| {
            row.location == location && matches!(row.record_kind.as_str(), "file" | "directory")
        })
        .collect::<Vec<_>>();
    let declared = rows
        .iter()
        .map(|row| row.name.clone())
        .collect::<BTreeSet<_>>();
    assert!(
        entries.is_subset(&declared),
        "undeclared entries in {root:?}: {entries:?}"
    );

    for row in rows {
        let path = root.join(&row.name);
        let required = requirement_is_active(&row.requirement, values);
        if !path.exists() {
            assert!(!required, "missing required entry {path:?}");
            continue;
        }
        assert!(
            required || row.requirement == "optional",
            "inactive conditional entry must be absent: {path:?}"
        );
        match (row.record_kind.as_str(), row.value_type.as_str()) {
            ("directory", "directory") => assert!(path.is_dir(), "missing directory {path:?}"),
            ("file", "utf8_file") => {
                assert!(path.is_file(), "missing file {path:?}");
                fs::read_to_string(&path).unwrap();
            }
            ("file", "binary_file") => {
                assert!(path.is_file(), "missing file {path:?}");
                assert!(
                    fs::metadata(&path).unwrap().len() > 0,
                    "empty file {path:?}"
                );
            }
            ("file", "key_value") => {
                assert!(path.is_file(), "missing file {path:?}");
                let text = fs::read_to_string(&path).unwrap();
                validate_key_value_text(contract, &row.name, &text);
            }
            ("file", "table") => {
                assert!(path.is_file(), "missing file {path:?}");
                let text = fs::read_to_string(&path).unwrap();
                validate_table(contract, &row.name, &text);
            }
            ("file", "sectioned_tsv") => {
                assert!(path.is_file(), "missing file {path:?}");
                let text = fs::read_to_string(&path).unwrap();
                validate_sectioned_tsv_at(contract, Some(&row.name), &text);
            }
            ("file", "key_value_sectioned_tsv") => {
                assert!(path.is_file(), "missing file {path:?}");
                let text = fs::read_to_string(&path).unwrap();
                validate_key_value_sectioned_tsv(contract, &row.name, &text);
            }
            _ => panic!("unsupported directory schema row: {row:?}"),
        }
    }
}

fn validate_key_value_text(contract: &Contract, location: &str, text: &str) {
    let values = parse_key_value_text(text);
    let rows = contract
        .rows
        .iter()
        .filter(|row| row.record_kind == "key" && row.location == location)
        .collect::<Vec<_>>();
    assert!(!rows.is_empty(), "no key contract for {location}");
    let declared = rows
        .iter()
        .map(|row| row.name.as_str())
        .collect::<BTreeSet<_>>();
    let actual = values.keys().map(String::as_str).collect::<BTreeSet<_>>();
    assert!(
        actual.is_subset(&declared),
        "undeclared keys at {location}: {actual:?}"
    );
    for row in rows {
        let required = requirement_is_active(&row.requirement, &values);
        match values.get(&row.name) {
            Some(value) => {
                assert!(
                    required || row.requirement == "optional",
                    "conditional key {} must be absent at {location}",
                    row.name
                );
                validate_value(row, value);
            }
            None => assert!(!required, "missing required key {} at {location}", row.name),
        }
    }
}

fn parse_key_value_text(text: &str) -> BTreeMap<String, String> {
    let mut lines = text.lines();
    let first = lines.next().expect("key/value artifact must not be empty");
    let mut values = BTreeMap::new();
    let records = if first == "key\tvalue" {
        lines.collect::<Vec<_>>()
    } else {
        std::iter::once(first).chain(lines).collect::<Vec<_>>()
    };
    for line in records {
        assert!(!line.is_empty(), "empty key/value record");
        let (key, value) = line
            .split_once('\t')
            .unwrap_or_else(|| panic!("key/value line lacks a tab: {line:?}"));
        assert!(!key.is_empty());
        assert!(
            values.insert(key.to_string(), value.to_string()).is_none(),
            "duplicate key {key}"
        );
    }
    values
}

fn requirement_is_active(requirement: &str, values: &BTreeMap<String, String>) -> bool {
    match requirement {
        "required" => true,
        "optional" => false,
        conditional => {
            let expression = conditional.strip_prefix("when:").unwrap();
            let (key, expected) = expression.split_once('=').unwrap();
            values.get(key).is_some_and(|actual| actual == expected)
        }
    }
}

fn validate_table(contract: &Contract, location: &str, text: &str) {
    let columns = contract
        .rows
        .iter()
        .filter(|row| row.record_kind == "column" && row.location == location)
        .collect::<Vec<_>>();
    assert!(!columns.is_empty(), "no column contract for {location}");
    let mut lines = text.lines();
    let header = lines.next().expect("table must have a header");
    assert_eq!(
        header,
        columns
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>()
            .join("\t"),
        "header drift at {location}"
    );
    for line in lines.filter(|line| !line.is_empty()) {
        validate_columns(&columns, line, location);
    }
}

fn validate_rows(contract: &Contract, location: &str, text: &str) {
    let columns = contract
        .rows
        .iter()
        .filter(|row| row.record_kind == "column" && row.location == location)
        .collect::<Vec<_>>();
    assert!(!columns.is_empty(), "no row contract for {location}");
    let lines = text.lines().collect::<Vec<_>>();
    assert!(!lines.is_empty(), "no rows emitted at {location}");
    for line in lines {
        validate_columns(&columns, line, location);
    }
}

fn validate_sectioned_tsv(contract: &Contract, text: &str) {
    validate_sectioned_tsv_at(contract, None, text);
}

fn validate_sectioned_tsv_at(contract: &Contract, namespace: Option<&str>, text: &str) {
    let location = |section: &str| match namespace {
        Some(namespace) => format!("{namespace}:{section}"),
        None => section.to_string(),
    };
    let mut blocks = text.split("\n\n");
    let preamble = blocks.next().expect("sectioned TSV must have a preamble");
    validate_key_value_text(contract, &location("preamble"), preamble);
    let mut actual_sections = BTreeSet::new();
    for block in blocks.filter(|block| !block.trim().is_empty()) {
        let (section, table) = block
            .split_once('\n')
            .unwrap_or_else(|| panic!("section lacks table content: {block:?}"));
        assert!(
            actual_sections.insert(section.to_string()),
            "duplicate section {section}"
        );
        validate_table(contract, &location(section), table);
    }
    let prefix = namespace.map(|namespace| format!("{namespace}:"));
    let declared_sections = contract
        .rows
        .iter()
        .filter(|row| row.record_kind == "column")
        .filter_map(|row| match &prefix {
            Some(prefix) => row.location.strip_prefix(prefix).map(str::to_string),
            None if !row.location.contains(':') => Some(row.location.clone()),
            None => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(actual_sections, declared_sections, "section set drifted");
}

fn validate_key_value_sectioned_tsv(contract: &Contract, namespace: &str, text: &str) {
    let prefix = format!("{namespace}:");
    let sections = contract
        .rows
        .iter()
        .filter(|row| row.record_kind == "column")
        .filter_map(|row| row.location.strip_prefix(&prefix))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        sections.len(),
        1,
        "key/value sectioned TSV currently requires exactly one table"
    );
    let section = *sections.iter().next().unwrap();
    let marker = format!("\n{section}\n");
    let (preamble, table) = text
        .split_once(&marker)
        .unwrap_or_else(|| panic!("missing section marker {section:?} in {namespace}"));
    validate_key_value_text(contract, &format!("{namespace}:preamble"), preamble);
    validate_table(contract, &format!("{namespace}:{section}"), table);
}

fn validate_columns(columns: &[&ContractRow], line: &str, location: &str) {
    let fields = line.split('\t').collect::<Vec<_>>();
    assert_eq!(
        fields.len(),
        columns.len(),
        "column count drift at {location}: {line}"
    );
    for (row, value) in columns.iter().zip(fields) {
        validate_value(row, value);
    }
}

fn validate_value(row: &ContractRow, value: &str) {
    let valid = match row.value_type.as_str() {
        "literal" => value == row.constraint,
        "enum" => row
            .constraint
            .split('|')
            .any(|candidate| candidate == value),
        "string" => !value.is_empty(),
        "encoded_string" => valid_encoded_string(value),
        "optional_encoded_string" => value.is_empty() || valid_encoded_string(value),
        "bool" => matches!(value, "true" | "false"),
        "u8" => value.parse::<u8>().is_ok(),
        "u64" => value.parse::<u64>().is_ok(),
        "usize" => value.parse::<usize>().is_ok(),
        "optional_usize" => value.is_empty() || value.parse::<usize>().is_ok(),
        "f64" => value.parse::<f64>().is_ok_and(f64::is_finite),
        "optional_f64" => value.is_empty() || value.parse::<f64>().is_ok_and(f64::is_finite),
        "na_or_f64" => value == "NA" || value.parse::<f64>().is_ok_and(f64::is_finite),
        "na_or_u64" => value == "NA" || value.parse::<u64>().is_ok(),
        "na_or_usize" => value == "NA" || value.parse::<usize>().is_ok(),
        "na_or_bool" => matches!(value, "NA" | "true" | "false"),
        "auto_or_u8" => value == "auto" || value.parse::<u8>().is_ok(),
        "auto_or_usize" => value == "auto" || value.parse::<usize>().is_ok(),
        "none_or_usize" => value == "none" || value.parse::<usize>().is_ok(),
        "hex16" => valid_hex16(value),
        "hex16_or_none" => value == "none" || valid_hex16(value),
        "hex64" => value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "portable_path" => valid_portable_path(value),
        "semver" => {
            let parts = value.split('.').collect::<Vec<_>>();
            parts.len() == 3
                && parts
                    .iter()
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        }
        "unbounded_or_f64" => {
            value == "unbounded" || value.parse::<f64>().is_ok_and(f64::is_finite)
        }
        "unlimited_or_usize" => value == "unlimited" || value.parse::<usize>().is_ok(),
        "unlimited_or_f64" => {
            value == "unlimited" || value.parse::<f64>().is_ok_and(f64::is_finite)
        }
        "not_computed_or_usize" => value == "not_computed" || value.parse::<usize>().is_ok(),
        other => panic!("unsupported contract value type {other:?}"),
    };
    assert!(
        valid,
        "invalid {} value {:?} for {} at {}",
        row.value_type, value, row.name, row.location
    );
}

fn valid_hex16(value: &str) -> bool {
    value.len() == 16 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn valid_encoded_string(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn valid_portable_path(value: &str) -> bool {
    !value.is_empty()
        && !value.contains('\\')
        && value.split('/').all(|part| {
            !part.is_empty()
                && !matches!(part, "." | "..")
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
}

fn run_cli<I>(workspace: &Path, args: I) -> Output
where
    I: IntoIterator<Item = std::ffi::OsString>,
{
    Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("biogeo-cli process should start")
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn make_legacy_result_fixture(current: &Path, legacy: &Path) {
    copy_directory(current, legacy);

    let metadata_path = legacy.join("metadata.tsv");
    let metadata = fs::read_to_string(&metadata_path).unwrap();
    let mut rewritten_metadata = String::new();
    for line in metadata.lines() {
        let key = line.split_once('\t').map(|(key, _)| key).unwrap_or(line);
        if matches!(
            key,
            "input_path_mode"
                | "input_bundle_dir"
                | "input_bundle_format"
                | "input_bundle_fingerprint"
        ) {
            continue;
        }
        if key == "format" {
            rewritten_metadata.push_str("format\tbiogeo-analysis-result-v1\n");
        } else {
            rewritten_metadata.push_str(line);
            rewritten_metadata.push('\n');
        }
    }
    fs::write(metadata_path, rewritten_metadata).unwrap();

    let inputs_path = legacy.join("inputs.tsv");
    let inputs = fs::read_to_string(&inputs_path).unwrap();
    let mut lines = inputs.lines();
    let mut rewritten_inputs = format!("{}\n", lines.next().unwrap());
    for line in lines.filter(|line| !line.is_empty()) {
        let mut fields = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        let absolute = fs::canonicalize(legacy.join(&fields[1])).unwrap();
        fields[1] = encode_field(&absolute.to_string_lossy());
        rewritten_inputs.push_str(&fields.join("\t"));
        rewritten_inputs.push('\n');
    }
    fs::write(inputs_path, rewritten_inputs).unwrap();
}

fn copy_directory(source: &Path, target: &Path) {
    fs::create_dir(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target_path = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_directory(&entry.path(), &target_path);
        } else {
            fs::copy(entry.path(), target_path).unwrap();
        }
    }
}

fn encode_field(value: &str) -> String {
    let mut output = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'%' | b'\t' | b'\r' | b'\n' => {
                output.push(b'%');
                output.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            _ => output.push(byte),
        }
    }
    String::from_utf8(output).expect("field encoding preserves UTF-8")
}
