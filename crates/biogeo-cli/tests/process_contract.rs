use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn version_and_engine_info_are_available_from_the_real_cli_process() {
    let version = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .arg("--version")
        .output()
        .expect("biogeo-cli process should start");
    assert!(version.status.success());
    assert!(version.stderr.is_empty());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!("biogeo-cli {}\n", env!("CARGO_PKG_VERSION"))
    );

    let capabilities = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .arg("engine-info")
        .output()
        .expect("biogeo-cli process should start");
    assert!(capabilities.status.success());
    assert!(capabilities.stderr.is_empty());
    let stdout = String::from_utf8(capabilities.stdout).unwrap();
    assert!(
        stdout.starts_with(
            "format\tbiogeo-engine-capabilities-v1\nstatus\tready\nengine\tbiogeo-cli\n"
        )
    );
    assert!(stdout.contains(&format!("engine_version\t{}\n", env!("CARGO_PKG_VERSION"))));
    assert!(stdout.contains("compatibility_policy_version\tbiogeo-compatibility-policy-v1\n"));
    assert!(stdout.contains("supports_subcommand_help\ttrue\n"));
}

#[test]
fn every_advertised_command_has_scoped_help_in_the_real_cli_process() {
    let executable = env!("CARGO_BIN_EXE_biogeo-cli");
    let capabilities = Command::new(executable)
        .arg("engine-info")
        .output()
        .expect("biogeo-cli process should start");
    assert!(capabilities.status.success());
    let capabilities = String::from_utf8(capabilities.stdout).unwrap();
    let mut commands = Vec::new();
    for key in ["recommended_commands", "compatibility_commands"] {
        let value = capabilities
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key}\t")))
            .unwrap_or_else(|| panic!("engine-info omitted {key}"));
        commands.extend(value.split(',').map(str::to_owned));
    }

    for command in commands {
        let output = Command::new(executable)
            .args([command.as_str(), "--help"])
            .output()
            .unwrap_or_else(|error| panic!("failed to start help for {command}: {error}"));
        assert!(output.status.success(), "help failed for {command}");
        assert!(output.stderr.is_empty(), "help wrote stderr for {command}");
        let stdout = String::from_utf8(output.stdout).unwrap();
        assert!(
            stdout.starts_with(&format!("Command: {command}\n\nUsage:\n")),
            "unexpected help header for {command}: {stdout}"
        );
        assert!(stdout.contains("\nOutput:\n"));
        assert!(stdout.contains("\nExit codes:\n"));
    }

    let bsm = Command::new(executable)
        .args(["model-bsm", "--help"])
        .output()
        .unwrap();
    let bsm = String::from_utf8(bsm.stdout).unwrap();
    assert!(bsm.contains("--analysis-result <dir>"));
    assert!(!bsm.contains("--tree <path>"));
    assert!(!bsm.contains("--d <rate>"));

    let early_help = Command::new(executable)
        .args([
            "model-evaluate",
            "--tree",
            "this-file-must-not-be-read.tree",
            "--help",
        ])
        .output()
        .unwrap();
    assert!(early_help.status.success());
    assert!(early_help.stderr.is_empty());
    assert!(
        String::from_utf8(early_help.stdout)
            .unwrap()
            .starts_with("Command: model-evaluate\n")
    );
}

#[test]
fn versioned_requests_reject_unknown_formats_and_fields() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp = std::env::temp_dir().join(format!(
        "biogeo-compatibility-contract-{}-{token}",
        std::process::id()
    ));
    fs::create_dir(&temp).unwrap();

    let unknown_format = temp.join("unknown-format.tsv");
    fs::write(
        &unknown_format,
        "key\tvalue\nformat\tbiogeo-analysis-request-v999\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--error-format", "tsv", "analysis-plan", "--request"])
        .arg(&unknown_format)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr).unwrap().contains(
        "expected format \"biogeo-analysis-request-v1\", found \"biogeo-analysis-request-v999\""
    ));

    let unknown_field = temp.join("unknown-field.tsv");
    let mut request =
        fs::read_to_string(workspace.join("examples/analysis_request/analysis.tsv")).unwrap();
    request.push_str("future_field\tnot-allowed-in-v1\n");
    fs::write(&unknown_field, request).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--error-format", "tsv", "analysis-plan", "--request"])
        .arg(&unknown_field)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("unknown request key \"future_field\"")
    );

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn tsv_error_contract_is_emitted_by_the_real_cli_process() {
    let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args([
            "--error-format",
            "tsv",
            "validate-inputs",
            "--tree",
            "missing.nwk",
        ])
        .output()
        .expect("biogeo-cli process should start");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "format\tbiogeo-cli-error-v1\n\
         code\tinvalid_arguments\n\
         message\tmissing required option --ranges\n\
         exit_code\t2\n"
    );
}

#[test]
fn tsv_progress_contract_is_emitted_by_the_real_cli_process() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--progress-format", "tsv", "model-optimize", "--tree"])
        .arg(workspace.join("examples/two_tip/tree.nwk"))
        .arg("--ranges")
        .arg(workspace.join("examples/two_tip/ranges.tsv"))
        .arg("--parameters")
        .arg(workspace.join("examples/parameter_tables/dec.tsv"))
        .args(["--max-iterations", "2"])
        .output()
        .expect("biogeo-cli process should start");

    assert!(output.status.success());
    assert!(!output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    let lines = stderr.lines().collect::<Vec<_>>();
    assert!(lines.len() >= 4, "progress output was: {stderr}");
    for (index, line) in lines.iter().enumerate() {
        let fields = line.split('\t').collect::<Vec<_>>();
        assert_eq!(fields.len(), 14, "unexpected progress row: {line}");
        assert_eq!(fields[0], "biogeo-cli-progress-v1");
        assert_eq!(fields[1].parse::<usize>().unwrap(), index + 1);
        assert_eq!(fields[3], "model-optimize");
    }
    assert_eq!(
        lines.first().unwrap().split('\t').nth(2),
        Some("task_started")
    );
    assert_eq!(
        lines.last().unwrap().split('\t').nth(2),
        Some("task_completed")
    );
}

#[test]
fn special_tree_inputs_are_explicit_portable_and_replayable() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp = std::env::temp_dir().join(format!(
        "biogeo-special-tree-contract-{}-{token}",
        std::process::id()
    ));
    fs::create_dir(&temp).unwrap();

    let tree = temp.join("tree.nwk");
    let nexus = temp.join("quoted tree.nex");
    let ranges = temp.join("ranges.tsv");
    let parameters = temp.join("parameters.tsv");
    let request = temp.join("analysis.tsv");
    let result = temp.join("result");
    fs::write(&tree, "('Taxon A','O''Brien');\n").unwrap();
    fs::write(
        &nexus,
        "\u{feff}#nExUs\nBEGIN TAXA; DIMENSIONS NTAX=2; END;\n\
         begin trees;\n\
         [producer[metadata]] translate 1 'Taxon A', 2 'O''Brien';\n\
         tree * 'analysis tree' = [&R] (1,2);\n\
         endblock;\n",
    )
    .unwrap();
    fs::write(
        &ranges,
        "tip\tArea A\tArea B\nTaxon A\t1\t0\nO'Brien\t0\t1\n",
    )
    .unwrap();
    let parameter_text =
        fs::read_to_string(workspace.join("examples/parameter_tables/dec.tsv")).unwrap();
    fs::write(
        &parameters,
        parameter_text
            .replace("d\tfree\t", "d\tfixed\t")
            .replace("e\tfree\t", "e\tfixed\t"),
    )
    .unwrap();

    let strict = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--error-format", "tsv", "convert-tree", "--tree"])
        .arg(&tree)
        .output()
        .unwrap();
    assert_eq!(strict.status.code(), Some(2));
    assert!(strict.stdout.is_empty());
    let strict_error = String::from_utf8(strict.stderr).unwrap();
    assert!(strict_error.contains("code\tinvalid_input\n"));
    assert!(strict_error.contains("branch length is required"));

    let newick = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["convert-tree", "--tree"])
        .arg(&tree)
        .args(["--fill-missing-branch-length", "0.25"])
        .output()
        .unwrap();
    assert!(newick.status.success());
    assert!(newick.stderr.is_empty());
    assert_eq!(
        String::from_utf8(newick.stdout).unwrap(),
        "('Taxon A':0.25,'O''Brien':0.25);\n"
    );

    let nexus = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["convert-tree", "--tree"])
        .arg(&nexus)
        .args([
            "--tree-name",
            "analysis tree",
            "--fill-missing-branch-length",
            "0.25",
        ])
        .output()
        .unwrap();
    assert!(nexus.status.success());
    assert!(nexus.stderr.is_empty());
    assert_eq!(
        String::from_utf8(nexus.stdout).unwrap(),
        "('Taxon A':0.25,'O''Brien':0.25);\n"
    );

    fs::write(
        &request,
        "key\tvalue\n\
         format\tbiogeo-analysis-request-v1\n\
         mode\tevaluate\n\
         tree\ttree.nwk\n\
         observation\texact_ranges\n\
         ranges\tranges.tsv\n\
         parameters\tparameters.tsv\n\
         max_range_size\t2\n\
         include_null_range\tfalse\n\
         root_prior\tflat\n\
         min_branch_length\t0\n\
         missing_branch_length_fill\t0.25\n\
         ancestral_probabilities\tfalse\n\
         split_probabilities\tfalse\n",
    )
    .unwrap();
    let run = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["analysis-run", "--request"])
        .arg(&request)
        .arg("--output-dir")
        .arg(&result)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(run.stderr.is_empty());
    assert!(
        String::from_utf8(run.stdout)
            .unwrap()
            .contains("status\tcomplete\n")
    );

    fs::remove_file(&tree).unwrap();
    fs::remove_file(&ranges).unwrap();
    fs::remove_file(&parameters).unwrap();
    fs::remove_file(&request).unwrap();
    let replay = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["analysis-result-inspect", "--analysis-result"])
        .arg(&result)
        .arg("--replay")
        .output()
        .unwrap();
    assert!(
        replay.status.success(),
        "{}",
        String::from_utf8_lossy(&replay.stderr)
    );
    assert!(
        String::from_utf8(replay.stdout)
            .unwrap()
            .contains("replay_validation\tpassed\n")
    );

    let polytomy = temp.join("polytomy.nwk");
    fs::write(&polytomy, "('Taxon A':1,'O''Brien':1,C:1);\n").unwrap();
    let polytomy_ranges = temp.join("polytomy-ranges.tsv");
    fs::write(
        &polytomy_ranges,
        "tip\tArea A\tArea B\nTaxon A\t1\t0\nO'Brien\t0\t1\nC\t1\t0\n",
    )
    .unwrap();
    let rejected_polytomy = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--error-format", "tsv", "validate-inputs", "--tree"])
        .arg(&polytomy)
        .arg("--ranges")
        .arg(&polytomy_ranges)
        .output()
        .unwrap();
    assert_eq!(rejected_polytomy.status.code(), Some(2));
    let polytomy_error = String::from_utf8(rejected_polytomy.stderr).unwrap();
    assert!(polytomy_error.contains("code\tconfiguration_error\n"));
    assert!(polytomy_error.contains("tree is not binary"));

    let unrooted = temp.join("unrooted.nex");
    fs::write(
        &unrooted,
        "#NEXUS\nBEGIN TREES; UTREE bad=('Taxon A':1,'O''Brien':1); END;\n",
    )
    .unwrap();
    let rejected_unrooted = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--error-format", "tsv", "convert-tree", "--tree"])
        .arg(&unrooted)
        .output()
        .unwrap();
    assert_eq!(rejected_unrooted.status.code(), Some(2));
    let unrooted_error = String::from_utf8(rejected_unrooted.stderr).unwrap();
    assert!(unrooted_error.contains("code\tinvalid_input\n"));
    assert!(unrooted_error.contains("UTREE statement"));
    assert!(unrooted_error.contains("requires a rooted tree"));

    fs::remove_dir_all(temp).unwrap();
}

#[test]
fn state_limit_rejects_combinatorial_growth_before_analysis() {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let token = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let temp = std::env::temp_dir().join(format!(
        "biogeo-state-limit-contract-{}-{token}",
        std::process::id()
    ));
    fs::create_dir(&temp).unwrap();
    fs::write(temp.join("tree.nwk"), "(A:1,B:1);\n").unwrap();
    let area_names = (1..=40)
        .map(|index| format!("Area{index:02}"))
        .collect::<Vec<_>>();
    let mut ranges = format!("tip\t{}\n", area_names.join("\t"));
    ranges.push_str(&format!("A\t1\t{}\n", vec!["0"; 39].join("\t")));
    ranges.push_str(&format!("B\t0\t1\t{}\n", vec!["0"; 38].join("\t")));
    fs::write(temp.join("ranges.tsv"), ranges).unwrap();
    let parameter_text =
        fs::read_to_string(workspace.join("examples/parameter_tables/dec.tsv")).unwrap();
    fs::write(
        temp.join("parameters.tsv"),
        parameter_text
            .replace("d\tfree\t", "d\tfixed\t")
            .replace("e\tfree\t", "e\tfixed\t"),
    )
    .unwrap();
    fs::write(
        temp.join("analysis.tsv"),
        "key\tvalue\n\
         format\tbiogeo-analysis-request-v1\n\
         mode\tevaluate\n\
         tree\ttree.nwk\n\
         observation\texact_ranges\n\
         ranges\tranges.tsv\n\
         parameters\tparameters.tsv\n\
         max_range_size\t10\n\
         max_states\t1000000\n\
         include_null_range\ttrue\n\
         root_prior\tflat\n\
         min_branch_length\t0\n\
         ancestral_probabilities\tfalse\n\
         split_probabilities\tfalse\n",
    )
    .unwrap();

    let expected = biogeo_core::StateSpace::estimated_state_count(40, 10, true).unwrap();
    assert!(expected > 1_000_000);
    let output = Command::new(env!("CARGO_BIN_EXE_biogeo-cli"))
        .args(["--error-format", "tsv", "analysis-plan", "--request"])
        .arg(temp.join("analysis.tsv"))
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("code\tresource_limit\n"));
    assert!(error.contains(&format!("estimated state space has {expected} states")));
    assert!(error.contains("exceeding --max-states 1000000"));

    fs::remove_dir_all(temp).unwrap();
}
