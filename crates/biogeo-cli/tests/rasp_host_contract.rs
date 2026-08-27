mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use support::rasp_host::{HostTerminalState, ReferenceRaspHost, parse_key_value_record};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn new() -> Self {
        let workspace = workspace();
        let token = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "biogeo-RASP-宿主-合同-{}-{token}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&root).unwrap();
        fs::copy(
            workspace.join("examples/two_tip/tree.nwk"),
            root.join("tree.nwk"),
        )
        .unwrap();
        fs::copy(
            workspace.join("examples/two_tip/ranges.tsv"),
            root.join("ranges.tsv"),
        )
        .unwrap();
        fs::copy(
            workspace.join("examples/parameter_tables/dec.tsv"),
            root.join("dec.tsv"),
        )
        .unwrap();
        fs::write(
            root.join("models.tsv"),
            "biogeo-model-batch-manifest-v1\n\
model_id\tparameters\n\
DEC\tdec.tsv\n",
        )
        .unwrap();
        fs::write(
            root.join("model-config.tsv"),
            "biogeo-model-batch-config-v1\n\
option\tvalue\n\
--tree\ttree.nwk\n\
--ranges\tranges.tsv\n\
--max-iterations\t200\n",
        )
        .unwrap();
        Self { root }
    }

    fn write_request(&self, name: &str, samples: usize, extra_bsm: &str) -> PathBuf {
        let path = self.root.join(name);
        fs::write(
            &path,
            format!(
                "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tmodel-config.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tmodel_id\n\
bsm_model_id\tDEC\n\
bsm_samples\t{samples}\n\
bsm_output_level\tsummary\n\
bsm_threads\t1\n\
{extra_bsm}\
bsm_seed\t20260822\n"
            ),
        )
        .unwrap();
        path
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn workspace() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn host() -> ReferenceRaspHost {
    ReferenceRaspHost::new(
        std::env::var_os("BIOGEO_RASP_HOST_ENGINE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_biogeo-cli"))),
    )
}

#[cfg(windows)]
#[allow(clippy::permissions_set_readonly_false)]
fn set_readonly(path: &Path, readonly: bool) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions).unwrap();
}

fn schema_registry() -> PathBuf {
    std::env::var_os("BIOGEO_RASP_HOST_SCHEMA_REGISTRY")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace().join("schemas/registry.tsv"))
}

fn run_workflow(
    host: &ReferenceRaspHost,
    request: &Path,
    output: &Path,
    resume: bool,
) -> support::rasp_host::ProcessCapture {
    let request = request.to_str().unwrap();
    let output = output.to_str().unwrap();
    let mut args = vec![
        "model-workflow",
        "--request",
        request,
        "--output-dir",
        output,
    ];
    if resume {
        args.push("--resume");
    }
    host.run(&args).unwrap()
}

#[test]
fn reference_host_negotiates_capabilities_and_classifies_machine_errors() {
    let workspace = workspace();
    let host = host();
    let registry = schema_registry();
    let handshake = host.handshake(&registry).unwrap();
    assert_eq!(handshake.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(handshake.build_os, std::env::consts::OS);
    assert_eq!(handshake.public_formats.len(), 32);
    assert!(handshake.recommended_commands.contains("model-workflow"));

    let failure = host
        .run(&["validate-inputs", "--tree", "missing.nwk"])
        .unwrap();
    assert_eq!(failure.exit_code, 2);
    assert_eq!(failure.terminal_state(), HostTerminalState::Rejected);
    let error = failure.error.unwrap();
    assert_eq!(error.code, "invalid_arguments");
    assert_eq!(error.exit_code, 2);
    assert!(error.message.contains("missing required option --ranges"));

    let error_schema = fs::read_to_string(
        registry
            .parent()
            .unwrap()
            .join("biogeo-cli-error-v1.schema.tsv"),
    )
    .unwrap();
    assert!(error_schema.contains("analysis_workflow_error"));
    assert!(error_schema.contains("fossil_placement_error"));

    let project = TempProject::new();
    let existing_output = project.root.join("existing analysis workflow");
    fs::create_dir(&existing_output).unwrap();
    let analysis_request = workspace.join("examples/analysis_request/analysis.tsv");
    let analysis_failure = host
        .run(&[
            "analysis-workflow",
            "--request",
            analysis_request.to_str().unwrap(),
            "--output-dir",
            existing_output.to_str().unwrap(),
            "--bsm-samples",
            "1",
        ])
        .unwrap();
    assert_eq!(
        analysis_failure.error.as_ref().unwrap().code,
        "analysis_workflow_error"
    );
    assert_eq!(analysis_failure.terminal_state(), HostTerminalState::Failed);

    let invalid_fossils = project.root.join("invalid-fossils.tsv");
    fs::write(
        &invalid_fossils,
        "biogeo-fossil-placement-manifest-v999\n\
fossil_id\tmin_age\tmax_age\tattachment\tstem_or_crown\tclade_tips\n",
    )
    .unwrap();
    let fossil_failure = host
        .run(&[
            "fossil-place",
            "--tree",
            project.root.join("tree.nwk").to_str().unwrap(),
            "--manifest",
            invalid_fossils.to_str().unwrap(),
            "--output-dir",
            project.root.join("fossil-output").to_str().unwrap(),
        ])
        .unwrap();
    assert_eq!(
        fossil_failure.error.as_ref().unwrap().code,
        "fossil_placement_error"
    );
    assert_eq!(fossil_failure.terminal_state(), HostTerminalState::Failed);
}

#[test]
fn reference_host_plans_runs_tracks_progress_and_imports_a_workflow() {
    let project = TempProject::new();
    let host = host();
    let request = project.write_request(
        "workflow.tsv",
        4,
        "bsm_shard_samples\t2\nbsm_deep_inspection\ttrue\n",
    );
    let request_str = request.to_str().unwrap();

    let plan = host
        .run(&["model-workflow-plan", "--request", request_str])
        .unwrap();
    assert_eq!(plan.terminal_state(), HostTerminalState::Succeeded);
    let plan_fields = parse_key_value_record(&plan.stdout, "model-workflow plan").unwrap();
    assert_eq!(
        plan_fields.get("format").map(String::as_str),
        Some("biogeo-model-workflow-plan-v1")
    );
    assert_eq!(plan_fields.get("status").map(String::as_str), Some("valid"));

    let output = project.root.join("successful workflow result");
    let completed = run_workflow(&host, &request, &output, false);
    assert_eq!(completed.exit_code, 0);
    assert_eq!(completed.terminal_state(), HostTerminalState::Succeeded);
    assert!(completed.error.is_none());
    assert!(completed.diagnostics.is_empty());
    assert!(
        completed
            .progress
            .iter()
            .any(|record| record.event == "task_started" && record.command == "model-batch")
    );
    assert!(
        completed
            .progress
            .iter()
            .any(|record| record.event == "task_completed" && record.command == "model-batch")
    );
    let run_fields = parse_key_value_record(&completed.stdout, "model-workflow run").unwrap();
    assert_eq!(
        run_fields.get("format").map(String::as_str),
        Some("biogeo-model-workflow-run-v1")
    );
    assert_eq!(
        run_fields.get("status").map(String::as_str),
        Some("complete")
    );

    let imported = host.import_model_workflow(&output).unwrap();
    assert_eq!(imported.selected_model_id, "DEC");
    assert!(imported.analysis_result_dir.is_dir());
    assert!(imported.bsm_result_dir.is_dir());
    assert_eq!(imported.bsm_format, "biogeo-bsm-summary-sharded-tsv-v2");
    assert_eq!(imported.bsm_samples, 4);
    let inspection = host.inspect_bsm(&imported.bsm_result_dir).unwrap();
    assert_eq!(
        inspection.get("diagnostic_violations").map(String::as_str),
        Some("0")
    );

    let resumed = run_workflow(&host, &request, &output, true);
    assert_eq!(resumed.terminal_state(), HostTerminalState::Succeeded);
    assert!(resumed.stdout.contains("model_batch_resumed\ttrue\n"));
    assert!(resumed.stdout.contains("bsm_resumed\ttrue\n"));
}

#[test]
fn reference_host_maps_time_budget_stop_and_resumes_with_a_new_budget() {
    let project = TempProject::new();
    let host = host();
    let request = project.write_request(
        "timed-workflow.tsv",
        8,
        "bsm_time_limit_seconds\t0\nbsm_deep_inspection\ttrue\n",
    );
    let output = project.root.join("timed workflow result");

    let stopped = run_workflow(&host, &request, &output, false);
    assert_eq!(stopped.exit_code, 124);
    assert_eq!(stopped.terminal_state(), HostTerminalState::BudgetStopped);
    assert!(stopped.resume_allowed(&output));
    assert_eq!(stopped.error.as_ref().unwrap().code, "bsm_time_limit");
    let bsm_metadata = fs::read_to_string(output.join("bsm-result/metadata.tsv")).unwrap();
    assert!(bsm_metadata.contains("status\ttime_limit\n"));
    assert!(!output.join("complete.tsv").exists());

    let revised = fs::read_to_string(&request)
        .unwrap()
        .replace("bsm_time_limit_seconds\t0", "bsm_time_limit_seconds\t60");
    fs::write(&request, revised).unwrap();
    let resumed = run_workflow(&host, &request, &output, true);
    assert_eq!(resumed.terminal_state(), HostTerminalState::Succeeded);
    assert!(resumed.stdout.contains("bsm_resumed\ttrue\n"));
    let imported = host.import_model_workflow(&output).unwrap();
    assert_eq!(imported.bsm_samples, 8);
}

#[test]
fn reference_host_cancels_bsm_through_stdin_and_resumes_without_resampling_models() {
    let project = TempProject::new();
    let host = host();
    let request = project.write_request(
        "cancel-workflow.tsv",
        4096,
        "bsm_checkpoint_samples\t16\n\
bsm_interactive\ttrue\n\
bsm_deep_inspection\tfalse\n",
    );
    let output = project.root.join("cancelled workflow result");
    let request_str = request.to_str().unwrap();
    let output_str = output.to_str().unwrap();
    let cancelled = host
        .run_with_stdin(
            &[
                "model-workflow",
                "--request",
                request_str,
                "--output-dir",
                output_str,
            ],
            "cancel\n",
        )
        .unwrap();
    assert_eq!(cancelled.exit_code, 130);
    assert_eq!(cancelled.terminal_state(), HostTerminalState::Cancelled);
    assert!(cancelled.resume_allowed(&output));
    assert_eq!(cancelled.error.as_ref().unwrap().code, "bsm_cancelled");
    assert!(
        cancelled
            .diagnostics
            .iter()
            .any(|line| line.contains("BSM interactive control enabled"))
    );
    assert!(
        cancelled
            .diagnostics
            .iter()
            .any(|line| line.contains("cancellation requested"))
    );
    assert!(output.join("model-batch/complete.tsv").is_file());
    assert!(!output.join("complete.tsv").exists());

    let revised = fs::read_to_string(&request)
        .unwrap()
        .replace("bsm_threads\t1", "bsm_threads\t2")
        .replace("bsm_interactive\ttrue", "bsm_interactive\tfalse")
        .replace("bsm_deep_inspection\tfalse", "bsm_deep_inspection\ttrue");
    fs::write(&request, revised).unwrap();
    let resumed = run_workflow(&host, &request, &output, true);
    assert_eq!(resumed.terminal_state(), HostTerminalState::Succeeded);
    assert!(resumed.stdout.contains("model_batch_resumed\ttrue\n"));
    assert!(resumed.stdout.contains("bsm_resumed\ttrue\n"));
    let imported = host.import_model_workflow(&output).unwrap();
    assert_eq!(imported.bsm_samples, 4096);
    host.inspect_bsm(&imported.bsm_result_dir).unwrap();
}

#[cfg(windows)]
#[test]
fn reference_host_replays_a_moved_project_after_read_only_sources_are_removed() {
    let project = TempProject::new();
    let source = project.root.join("源项目 with spaces");
    fs::create_dir(&source).unwrap();
    for name in [
        "tree.nwk",
        "ranges.tsv",
        "dec.tsv",
        "models.tsv",
        "model-config.tsv",
    ] {
        fs::rename(project.root.join(name), source.join(name)).unwrap();
    }
    let request = source.join("workflow.tsv");
    fs::write(
        &request,
        "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tmodel-config.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tmodel_id\n\
bsm_model_id\tDEC\n\
bsm_samples\t2\n\
bsm_output_level\tsummary\n\
bsm_threads\t1\n\
bsm_deep_inspection\ttrue\n\
bsm_seed\t20260824\n",
    )
    .unwrap();

    let source_files = [
        "tree.nwk",
        "ranges.tsv",
        "dec.tsv",
        "models.tsv",
        "model-config.tsv",
        "workflow.tsv",
    ]
    .map(|name| source.join(name));
    for path in &source_files {
        set_readonly(path, true);
    }

    let host = host();
    let original_result = project.root.join("运行结果 with spaces");
    let completed = run_workflow(&host, &request, &original_result, false);
    assert_eq!(completed.terminal_state(), HostTerminalState::Succeeded);

    for path in &source_files {
        set_readonly(path, false);
    }
    fs::remove_dir_all(&source).unwrap();

    let archive = project.root.join("另一个项目目录 with spaces");
    fs::create_dir(&archive).unwrap();
    let moved_result = archive.join("迁移后的工作流");
    fs::rename(&original_result, &moved_result).unwrap();

    let imported = host.import_model_workflow(&moved_result).unwrap();
    assert_eq!(imported.selected_model_id, "DEC");
    assert_eq!(imported.bsm_samples, 2);
    host.inspect_bsm(&imported.bsm_result_dir).unwrap();

    let analysis_result = imported.analysis_result_dir.to_str().unwrap();
    let inspection = host
        .run(&[
            "analysis-result-inspect",
            "--analysis-result",
            analysis_result,
            "--replay",
        ])
        .unwrap();
    assert_eq!(inspection.terminal_state(), HostTerminalState::Succeeded);
    let fields = parse_key_value_record(&inspection.stdout, "moved analysis result").unwrap();
    assert_eq!(fields.get("portable").map(String::as_str), Some("true"));
    assert_eq!(
        fields.get("replay_validation").map(String::as_str),
        Some("passed")
    );

    let replay_bsm = project.root.join("迁移后新生成的随机历史");
    let replay_bsm_path = replay_bsm.to_str().unwrap();
    let sampled = host
        .run(&[
            "model-bsm",
            "--analysis-result",
            analysis_result,
            "--bsm-samples",
            "3",
            "--bsm-output-dir",
            replay_bsm_path,
            "--bsm-output-level",
            "summary",
            "--bsm-threads",
            "1",
            "--seed",
            "20260824",
        ])
        .unwrap();
    assert_eq!(sampled.terminal_state(), HostTerminalState::Succeeded);
    let bsm = host.inspect_bsm(&replay_bsm).unwrap();
    assert_eq!(bsm.get("completed_samples").map(String::as_str), Some("3"));
}
