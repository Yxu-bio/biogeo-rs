use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

const CAPABILITIES_FORMAT: &str = "biogeo-engine-capabilities-v1";
const COMPATIBILITY_POLICY: &str = "biogeo-compatibility-policy-v1";
const ERROR_FORMAT: &str = "biogeo-cli-error-v1";
const PROGRESS_FORMAT: &str = "biogeo-cli-progress-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostTerminalState {
    Succeeded,
    Cancelled,
    BudgetStopped,
    Rejected,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MachineError {
    pub code: String,
    pub message: String,
    pub exit_code: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProgressRecord {
    pub sequence: u64,
    pub event: String,
    pub command: String,
    pub dataset_id: Option<String>,
    pub model_id: Option<String>,
    pub completed: Option<usize>,
    pub total: Option<usize>,
    pub start: Option<usize>,
    pub starts: Option<usize>,
    pub iteration: Option<usize>,
    pub max_iterations: Option<usize>,
    pub evaluations: Option<usize>,
    pub best_log_likelihood: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct ProcessCapture {
    pub exit_code: i32,
    pub stdout: String,
    pub progress: Vec<ProgressRecord>,
    pub error: Option<MachineError>,
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Debug)]
struct ParsedStderr {
    progress: Vec<ProgressRecord>,
    error: Option<MachineError>,
    diagnostics: Vec<String>,
}

impl ProcessCapture {
    pub fn terminal_state(&self) -> HostTerminalState {
        match self.error.as_ref().map(|error| error.code.as_str()) {
            None => HostTerminalState::Succeeded,
            Some("bsm_cancelled" | "task_cancelled") => HostTerminalState::Cancelled,
            Some("bsm_time_limit" | "bsm_event_limit") => HostTerminalState::BudgetStopped,
            Some("invalid_arguments" | "invalid_input" | "configuration_error") => {
                HostTerminalState::Rejected
            }
            Some(_) => HostTerminalState::Failed,
        }
    }

    pub fn resume_allowed(&self, output_dir: &Path) -> bool {
        matches!(
            self.terminal_state(),
            HostTerminalState::Cancelled | HostTerminalState::BudgetStopped
        ) && output_dir.is_dir()
    }
}

#[derive(Clone, Debug)]
pub struct EngineHandshake {
    pub version: String,
    pub build_os: String,
    pub public_formats: BTreeSet<String>,
    pub recommended_commands: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub struct ImportedModelWorkflow {
    pub selected_model_id: String,
    pub analysis_result_dir: PathBuf,
    pub bsm_result_dir: PathBuf,
    pub bsm_format: String,
    pub bsm_samples: usize,
}

#[derive(Clone, Debug)]
pub struct ReferenceRaspHost {
    engine: PathBuf,
}

impl ReferenceRaspHost {
    pub fn new(engine: impl Into<PathBuf>) -> Self {
        Self {
            engine: engine.into(),
        }
    }

    pub fn handshake(&self, registry_path: &Path) -> Result<EngineHandshake, String> {
        let capture = self.run(&["engine-info"])?;
        if capture.terminal_state() != HostTerminalState::Succeeded {
            return Err("engine-info did not complete successfully".to_string());
        }
        if !capture.progress.is_empty() || !capture.diagnostics.is_empty() {
            return Err("engine-info emitted unexpected stderr records".to_string());
        }
        let fields = parse_key_value_record(&capture.stdout, "engine-info stdout")?;
        require_value(&fields, "format", CAPABILITIES_FORMAT)?;
        require_value(&fields, "status", "ready")?;
        require_value(&fields, "engine", "biogeo-cli")?;
        require_value(
            &fields,
            "compatibility_policy_version",
            COMPATIBILITY_POLICY,
        )?;
        require_value(
            &fields,
            "format_compatibility_policy",
            "strict_versioned_schema",
        )?;
        require_value(&fields, "unknown_format_policy", "reject")?;
        require_value(&fields, "unknown_field_policy", "reject")?;

        let public_formats = split_set(required(&fields, "public_formats")?);
        let declared_count = parse_usize(
            required(&fields, "public_format_count")?,
            "public_format_count",
        )?;
        if public_formats.len() != declared_count {
            return Err(format!(
                "engine-info declared {declared_count} formats but listed {}",
                public_formats.len()
            ));
        }
        let registry_formats = parse_schema_registry(registry_path)?;
        if public_formats != registry_formats {
            return Err("engine-info public_formats do not match schemas/registry.tsv".to_string());
        }

        let recommended_commands = split_set(required(&fields, "recommended_commands")?);
        for command in [
            "engine-info",
            "analysis-plan",
            "analysis-run",
            "analysis-workflow",
            "model-workflow-plan",
            "model-workflow",
            "bsm-inspect",
        ] {
            if !recommended_commands.contains(command) {
                return Err(format!("engine-info omitted required command {command:?}"));
            }
        }

        Ok(EngineHandshake {
            version: decode_field(required(&fields, "engine_version")?)?,
            build_os: decode_field(required(&fields, "build_os")?)?,
            public_formats,
            recommended_commands,
        })
    }

    pub fn run(&self, args: &[&str]) -> Result<ProcessCapture, String> {
        let output = Command::new(&self.engine)
            .args(["--error-format", "tsv", "--progress-format", "tsv"])
            .args(args)
            .output()
            .map_err(|error| format!("failed to start {}: {error}", self.engine.display()))?;
        parse_process_output(output)
    }

    pub fn run_with_stdin(&self, args: &[&str], input: &str) -> Result<ProcessCapture, String> {
        let mut child = Command::new(&self.engine)
            .args(["--error-format", "tsv", "--progress-format", "tsv"])
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("failed to start {}: {error}", self.engine.display()))?;
        child
            .stdin
            .take()
            .ok_or_else(|| "child stdin was not piped".to_string())?
            .write_all(input.as_bytes())
            .map_err(|error| format!("failed to write child stdin: {error}"))?;
        let output = child
            .wait_with_output()
            .map_err(|error| format!("failed to wait for child process: {error}"))?;
        parse_process_output(output)
    }

    pub fn inspect_bsm(&self, result_dir: &Path) -> Result<BTreeMap<String, String>, String> {
        let result = result_dir
            .to_str()
            .ok_or_else(|| "BSM result path is not UTF-8".to_string())?;
        let capture = self.run(&["bsm-inspect", "--bsm-result", result, "--deep"])?;
        if capture.terminal_state() != HostTerminalState::Succeeded {
            return Err(format!(
                "bsm-inspect failed: {:?}",
                capture.error.as_ref().map(|error| error.code.as_str())
            ));
        }
        let fields = parse_key_value_record(&capture.stdout, "bsm-inspect stdout")?;
        require_value(&fields, "format", "biogeo-bsm-inspection-v1")?;
        require_value(&fields, "status", "valid")?;
        require_value(&fields, "run_status", "complete")?;
        require_value(&fields, "validation", "deep")?;
        Ok(fields)
    }

    pub fn import_model_workflow(&self, root: &Path) -> Result<ImportedModelWorkflow, String> {
        let metadata = read_key_value_file(&root.join("metadata.tsv"))?;
        require_value(&metadata, "format", "biogeo-model-workflow-result-v1")?;
        let completion = read_key_value_file(&root.join("complete.tsv"))?;
        require_value(&completion, "format", "biogeo-model-workflow-completion-v1")?;
        require_value(&completion, "status", "complete")?;
        let selection = read_key_value_file(&root.join("selection.tsv"))?;
        require_value(&selection, "format", "biogeo-model-workflow-selection-v1")?;

        let identity = required(&metadata, "request_fingerprint")?;
        if required(&completion, "request_fingerprint")? != identity
            || required(&selection, "request_fingerprint")? != identity
        {
            return Err(
                "workflow identity differs across metadata, selection, and completion".to_string(),
            );
        }

        let selected_model_id = decode_field(required(&selection, "selected_model_id")?)?;
        if selected_model_id == "none" {
            return Err("test host expected a selected BSM model".to_string());
        }
        let analysis_relative = decode_field(required(&selection, "selected_analysis_result")?)?;
        let analysis_relative = portable_relative_path(&analysis_relative)?;
        if analysis_relative
            .components()
            .next()
            .map(|component| component.as_os_str())
            != Some(std::ffi::OsStr::new("model-batch"))
        {
            return Err(
                "selected analysis result is not owned by the workflow model-batch directory"
                    .to_string(),
            );
        }
        let analysis_result_dir = root.join(analysis_relative);
        let analysis_metadata = read_key_value_file(&analysis_result_dir.join("metadata.tsv"))?;
        require_value(&analysis_metadata, "format", "biogeo-analysis-result-v2")?;
        require_value(&analysis_metadata, "status", "complete")?;

        require_value(&completion, "bsm_status", "complete")?;
        let bsm_relative = decode_field(required(&completion, "bsm_result_dir")?)?;
        let bsm_result_dir = root.join(portable_relative_path(&bsm_relative)?);
        let bsm_metadata = read_key_value_file(&bsm_result_dir.join("metadata.tsv"))?;
        require_value(&bsm_metadata, "status", "complete")?;
        let bsm_format = required(&bsm_metadata, "format")?.to_string();
        let requested = parse_usize(required(&bsm_metadata, "samples")?, "samples")?;
        let completed = parse_usize(
            required(&bsm_metadata, "completed_samples")?,
            "completed_samples",
        )?;
        if requested != completed {
            return Err(format!(
                "BSM is incomplete: requested {requested}, completed {completed}"
            ));
        }

        Ok(ImportedModelWorkflow {
            selected_model_id,
            analysis_result_dir,
            bsm_result_dir,
            bsm_format,
            bsm_samples: completed,
        })
    }
}

fn parse_process_output(output: std::process::Output) -> Result<ProcessCapture, String> {
    let exit_code = output
        .status
        .code()
        .ok_or_else(|| "engine terminated without an exit code".to_string())?;
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| format!("engine stdout is not UTF-8: {error}"))?;
    let stderr = String::from_utf8(output.stderr)
        .map_err(|error| format!("engine stderr is not UTF-8: {error}"))?;
    let parsed_stderr = parse_stderr(&stderr)?;
    let progress = parsed_stderr.progress;
    let error = parsed_stderr.error;
    let diagnostics = parsed_stderr.diagnostics;
    if exit_code == 0 && error.is_some() {
        return Err("successful engine process emitted a machine error".to_string());
    }
    if exit_code != 0 && error.is_none() {
        return Err(format!(
            "engine exited with {exit_code} without a {ERROR_FORMAT} record"
        ));
    }
    if let Some(error) = &error
        && error.exit_code != exit_code
    {
        return Err(format!(
            "process exit code {exit_code} differs from machine error exit code {}",
            error.exit_code
        ));
    }
    Ok(ProcessCapture {
        exit_code,
        stdout,
        progress,
        error,
        diagnostics,
    })
}

fn parse_stderr(stderr: &str) -> Result<ParsedStderr, String> {
    let lines = stderr.lines().collect::<Vec<_>>();
    let mut progress = Vec::new();
    let mut error = None;
    let mut diagnostics = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.starts_with(&format!("{PROGRESS_FORMAT}\t")) {
            let record = parse_progress(line)?;
            let expected_sequence = progress.len() as u64 + 1;
            if record.sequence != expected_sequence {
                return Err(format!(
                    "progress sequence must be {expected_sequence}, found {}",
                    record.sequence
                ));
            }
            progress.push(record);
            index += 1;
        } else if line == format!("format\t{ERROR_FORMAT}") {
            if error.is_some() {
                return Err("stderr contains multiple machine error records".to_string());
            }
            if index + 4 > lines.len() {
                return Err("truncated machine error record".to_string());
            }
            let fields = parse_key_value_record(
                &format!("{}\n", lines[index..index + 4].join("\n")),
                "machine stderr error",
            )?;
            require_value(&fields, "format", ERROR_FORMAT)?;
            error = Some(MachineError {
                code: required(&fields, "code")?.to_string(),
                message: decode_field(required(&fields, "message")?)?,
                exit_code: required(&fields, "exit_code")?
                    .parse::<i32>()
                    .map_err(|_| "machine error exit_code is not an integer".to_string())?,
            });
            index += 4;
        } else {
            diagnostics.push(line.to_string());
            index += 1;
        }
    }
    Ok(ParsedStderr {
        progress,
        error,
        diagnostics,
    })
}

fn parse_progress(line: &str) -> Result<ProgressRecord, String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() != 14 {
        return Err(format!(
            "progress row has {} columns instead of 14",
            fields.len()
        ));
    }
    if fields[0] != PROGRESS_FORMAT {
        return Err(format!("unexpected progress format {:?}", fields[0]));
    }
    Ok(ProgressRecord {
        sequence: fields[1]
            .parse::<u64>()
            .map_err(|_| "progress sequence is not u64".to_string())?,
        event: decode_field(fields[2])?,
        command: decode_field(fields[3])?,
        dataset_id: decode_optional(fields[4])?,
        model_id: decode_optional(fields[5])?,
        completed: parse_optional_usize(fields[6], "completed")?,
        total: parse_optional_usize(fields[7], "total")?,
        start: parse_optional_usize(fields[8], "start")?,
        starts: parse_optional_usize(fields[9], "starts")?,
        iteration: parse_optional_usize(fields[10], "iteration")?,
        max_iterations: parse_optional_usize(fields[11], "max_iterations")?,
        evaluations: parse_optional_usize(fields[12], "evaluations")?,
        best_log_likelihood: if fields[13].is_empty() {
            None
        } else {
            Some(
                fields[13]
                    .parse::<f64>()
                    .map_err(|_| "best_log_likelihood is not f64".to_string())?,
            )
        },
    })
}

pub fn parse_key_value_record(
    input: &str,
    context: &str,
) -> Result<BTreeMap<String, String>, String> {
    let mut fields = BTreeMap::new();
    for (index, line) in input.lines().enumerate() {
        if index == 0 && line == "key\tvalue" {
            continue;
        }
        let Some((key, value)) = line.split_once('\t') else {
            return Err(format!("{context} line {} is not key<TAB>value", index + 1));
        };
        if key.is_empty() || value.contains('\t') {
            return Err(format!(
                "{context} line {} does not contain exactly two fields",
                index + 1
            ));
        }
        if fields.insert(key.to_string(), value.to_string()).is_some() {
            return Err(format!("{context} contains duplicate key {key:?}"));
        }
    }
    if fields.is_empty() {
        return Err(format!("{context} is empty"));
    }
    Ok(fields)
}

fn read_key_value_file(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let input = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    parse_key_value_record(&input, &path.display().to_string())
}

fn parse_schema_registry(path: &Path) -> Result<BTreeSet<String>, String> {
    let input = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    let mut lines = input.lines();
    if lines.next() != Some("biogeo-schema-registry-v1") {
        return Err("schema registry has an unknown format".to_string());
    }
    if lines.next() != Some("format_id\tartifact_kind\tschema_file") {
        return Err("schema registry has an invalid header".to_string());
    }
    let mut formats = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 3 || columns.iter().any(|value| value.is_empty()) {
            return Err(format!("invalid schema registry row {}", index + 3));
        }
        if !formats.insert(columns[0].to_string()) {
            return Err(format!("duplicate schema registry format {:?}", columns[0]));
        }
    }
    Ok(formats)
}

fn portable_relative_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "artifact path is not portable and relative: {value:?}"
        ));
    }
    Ok(path)
}

fn split_set(value: &str) -> BTreeSet<String> {
    value.split(',').map(str::to_string).collect()
}

fn required<'a>(fields: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, String> {
    fields
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing required key {key:?}"))
}

fn require_value(
    fields: &BTreeMap<String, String>,
    key: &str,
    expected: &str,
) -> Result<(), String> {
    let actual = required(fields, key)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "key {key:?} must be {expected:?}, found {actual:?}"
        ))
    }
}

fn parse_optional_usize(value: &str, field: &str) -> Result<Option<usize>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        parse_usize(value, field).map(Some)
    }
}

fn parse_usize(value: &str, field: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("{field} is not usize: {value:?}"))
}

fn decode_optional(value: &str) -> Result<Option<String>, String> {
    if value.is_empty() {
        Ok(None)
    } else {
        decode_field(value).map(Some)
    }
}

fn decode_field(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err(format!("truncated percent escape in {value:?}"));
        }
        let escaped = hex(bytes[index + 1])?
            .checked_mul(16)
            .and_then(|high| high.checked_add(hex(bytes[index + 2]).ok()?))
            .ok_or_else(|| format!("invalid percent escape in {value:?}"))?;
        if !matches!(escaped, b'%' | b'\t' | b'\r' | b'\n') {
            return Err(format!("unsupported percent escape %{escaped:02X}"));
        }
        decoded.push(escaped);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|error| format!("decoded field is not UTF-8: {error}"))
}

fn hex(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(format!("invalid hexadecimal digit {:?}", value as char)),
    }
}
