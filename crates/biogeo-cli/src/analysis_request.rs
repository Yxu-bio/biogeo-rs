use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const ANALYSIS_REQUEST_FORMAT: &str = "biogeo-analysis-request-v1";
pub const ANALYSIS_TEMPLATE_FORMAT: &str = "biogeo-analysis-template-v1";
pub const ANALYSIS_PLAN_FORMAT: &str = "biogeo-analysis-plan-v1";
pub const ANALYSIS_RUN_FORMAT: &str = "biogeo-analysis-run-v2";
pub const ANALYSIS_WORKFLOW_FORMAT: &str = "biogeo-analysis-workflow-v1";
pub const REQUEST_FILE: &str = "analysis.tsv";
pub const PARAMETERS_FILE: &str = "parameters.tsv";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisRequestMode {
    Evaluate,
    Optimize,
}

impl AnalysisRequestMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Evaluate => "evaluate",
            Self::Optimize => "optimize",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Self::Evaluate => "model-evaluate",
            Self::Optimize => "model-optimize",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedAnalysisRequest {
    pub mode: AnalysisRequestMode,
    pub command_arguments: Vec<String>,
    pub portable_paths: bool,
}

pub fn parse_analysis_request(
    input: &str,
    request_path: &Path,
) -> Result<ParsedAnalysisRequest, AnalysisRequestError> {
    let values = parse_key_value_table(input, request_path)?;
    let format = required(&values, "format", request_path)?;
    if format != ANALYSIS_REQUEST_FORMAT {
        return Err(invalid_request(
            request_path,
            format!("expected format {ANALYSIS_REQUEST_FORMAT:?}, found {format:?}"),
        ));
    }
    reject_unknown_keys(&values, request_path)?;

    let mode = match required(&values, "mode", request_path)? {
        "evaluate" => AnalysisRequestMode::Evaluate,
        "optimize" => AnalysisRequestMode::Optimize,
        value => {
            return Err(invalid_request(
                request_path,
                format!("mode must be evaluate or optimize, found {value:?}"),
            ));
        }
    };
    let observation = required(&values, "observation", request_path)?;
    if !matches!(
        observation,
        "exact_ranges" | "ambiguous_ranges" | "mf_dp_fdp_detection"
    ) {
        return Err(invalid_request(
            request_path,
            format!(
                "observation must be exact_ranges, ambiguous_ranges, or mf_dp_fdp_detection, found {observation:?}"
            ),
        ));
    }

    let mut arguments = vec![mode.command().to_string()];
    let base_dir = request_path.parent().unwrap_or_else(|| Path::new("."));
    let mut portable_paths = true;
    push_required_path(
        &values,
        "tree",
        "--tree",
        base_dir,
        request_path,
        &mut portable_paths,
        &mut arguments,
    )?;
    push_optional_value(
        &values,
        "tree_name",
        "--tree-name",
        request_path,
        &mut arguments,
    )?;
    push_required_path(
        &values,
        "parameters",
        "--parameters",
        base_dir,
        request_path,
        &mut portable_paths,
        &mut arguments,
    )?;

    match observation {
        "exact_ranges" | "ambiguous_ranges" => {
            push_required_path(
                &values,
                "ranges",
                "--ranges",
                base_dir,
                request_path,
                &mut portable_paths,
                &mut arguments,
            )?;
            reject_present(&values, "detections", observation, request_path)?;
            reject_present(&values, "controls", observation, request_path)?;
            if observation == "ambiguous_ranges" {
                arguments.push("--use-ambiguities".to_string());
            }
        }
        "mf_dp_fdp_detection" => {
            reject_present(&values, "ranges", observation, request_path)?;
            arguments.push("--use-detection-model".to_string());
            push_required_path(
                &values,
                "detections",
                "--detections",
                base_dir,
                request_path,
                &mut portable_paths,
                &mut arguments,
            )?;
            push_required_path(
                &values,
                "controls",
                "--controls",
                base_dir,
                request_path,
                &mut portable_paths,
                &mut arguments,
            )?;
        }
        _ => unreachable!("observation was validated above"),
    }

    for (key, option) in [
        ("dispersal_multipliers", "--dispersal-multipliers"),
        ("dispersal_strata", "--dispersal-strata"),
        ("distance_matrix", "--distance-matrix"),
        (
            "environment_distance_matrix",
            "--environment-distance-matrix",
        ),
        ("extirpation_multipliers", "--extirpation-multipliers"),
        ("area_sizes", "--area-sizes"),
    ] {
        push_optional_path(
            &values,
            key,
            option,
            base_dir,
            request_path,
            &mut portable_paths,
            &mut arguments,
        )?;
    }

    match required(&values, "max_range_size", request_path)? {
        "auto" => {}
        value => {
            value.parse::<u8>().map_err(|_| {
                invalid_request(
                    request_path,
                    format!(
                        "max_range_size must be auto or an unsigned 8-bit integer, found {value:?}"
                    ),
                )
            })?;
            arguments.extend(["--max-range-size".to_string(), value.to_string()]);
        }
    }
    push_optional_value(
        &values,
        "max_states",
        "--max-states",
        request_path,
        &mut arguments,
    )?;
    push_required_bool_flag(
        &values,
        "include_null_range",
        "--include-null-range",
        request_path,
        &mut arguments,
    )?;
    let root_prior = required(&values, "root_prior", request_path)?;
    if !matches!(root_prior, "flat" | "equal") {
        return Err(invalid_request(
            request_path,
            format!("root_prior must be flat or equal, found {root_prior:?}"),
        ));
    }
    arguments.extend(["--root-prior".to_string(), root_prior.to_string()]);
    push_required_value(
        &values,
        "min_branch_length",
        "--min-branch-length",
        request_path,
        &mut arguments,
    )?;
    push_optional_value(
        &values,
        "missing_branch_length_fill",
        "--fill-missing-branch-length",
        request_path,
        &mut arguments,
    )?;
    push_required_bool_flag(
        &values,
        "ancestral_probabilities",
        "--ancestral-probs",
        request_path,
        &mut arguments,
    )?;
    push_required_bool_flag(
        &values,
        "split_probabilities",
        "--split-probs",
        request_path,
        &mut arguments,
    )?;

    if mode == AnalysisRequestMode::Optimize {
        for (key, option) in [
            ("optimization_initial_step", "--initial-step"),
            ("optimization_tolerance", "--tolerance"),
            ("optimization_max_iterations", "--max-iterations"),
        ] {
            push_required_value(&values, key, option, request_path, &mut arguments)?;
        }
        if let Some(starts) = values.get("optimization_additional_starts") {
            if starts.is_empty() {
                return Err(invalid_request(
                    request_path,
                    "optimization_additional_starts must be omitted instead of left empty",
                ));
            }
            for start in starts.split(';') {
                if start.is_empty() {
                    return Err(invalid_request(
                        request_path,
                        "optimization_additional_starts contains an empty vector",
                    ));
                }
                arguments.extend(["--additional-start".to_string(), start.to_string()]);
            }
        }
    } else {
        for key in [
            "optimization_initial_step",
            "optimization_tolerance",
            "optimization_max_iterations",
            "optimization_additional_starts",
        ] {
            if values.contains_key(key) {
                return Err(invalid_request(
                    request_path,
                    format!("{key} is valid only when mode=optimize"),
                ));
            }
        }
    }

    Ok(ParsedAnalysisRequest {
        mode,
        command_arguments: arguments,
        portable_paths,
    })
}

pub fn format_template_request(mode: AnalysisRequestMode) -> String {
    let mut output = String::new();
    output.push_str("key\tvalue\n");
    output.push_str("format\t");
    output.push_str(ANALYSIS_REQUEST_FORMAT);
    output.push('\n');
    output.push_str("mode\t");
    output.push_str(mode.as_str());
    output.push('\n');
    output.push_str("tree\ttree.nwk\n");
    output.push_str("observation\texact_ranges\n");
    output.push_str("ranges\tranges.tsv\n");
    output.push_str("parameters\tparameters.tsv\n");
    output.push_str("max_range_size\tauto\n");
    output.push_str("include_null_range\tfalse\n");
    output.push_str("root_prior\tflat\n");
    output.push_str("min_branch_length\t0\n");
    output.push_str("ancestral_probabilities\ttrue\n");
    output.push_str("split_probabilities\ttrue\n");
    if mode == AnalysisRequestMode::Optimize {
        output.push_str("optimization_initial_step\t0.2\n");
        output.push_str("optimization_tolerance\t1e-8\n");
        output.push_str("optimization_max_iterations\t200\n");
    }
    output
}

pub fn write_template_directory(
    output_dir: &Path,
    request: &str,
    parameters: &str,
) -> Result<(), AnalysisRequestError> {
    if output_dir.exists() {
        return Err(AnalysisRequestError::OutputExists(output_dir.to_path_buf()));
    }
    let parent = output_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| AnalysisRequestError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let file_name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("analysis-template");
    let staging = parent.join(format!(
        ".{file_name}.staging-{}-{sequence}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(AnalysisRequestError::OutputExists(staging));
    }

    let result = (|| {
        fs::create_dir(&staging).map_err(|source| AnalysisRequestError::Io {
            path: staging.clone(),
            source,
        })?;
        write_file(&staging.join(REQUEST_FILE), request.as_bytes())?;
        write_file(&staging.join(PARAMETERS_FILE), parameters.as_bytes())?;
        crate::fs_retry::rename(&staging, output_dir).map_err(|source| AnalysisRequestError::Io {
            path: output_dir.to_path_buf(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), AnalysisRequestError> {
    fs::write(path, bytes).map_err(|source| AnalysisRequestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_key_value_table(
    input: &str,
    request_path: &Path,
) -> Result<BTreeMap<String, String>, AnalysisRequestError> {
    let mut lines = input.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim_end_matches('\r');
        (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, trimmed))
    });
    let (header_line, header) = lines
        .next()
        .ok_or_else(|| invalid_request(request_path, "request is empty"))?;
    let header = header.trim_start_matches('\u{feff}');
    if header != "key\tvalue" {
        return Err(invalid_request_at(
            request_path,
            header_line,
            "header must be exactly: key<TAB>value",
        ));
    }

    let mut values = BTreeMap::new();
    for (line_number, line) in lines {
        let Some((key, value)) = line.split_once('\t') else {
            return Err(invalid_request_at(
                request_path,
                line_number,
                "record must contain key<TAB>value",
            ));
        };
        if key.is_empty() || value.contains('\t') {
            return Err(invalid_request_at(
                request_path,
                line_number,
                "record must contain exactly two tab-separated fields and a non-empty key",
            ));
        }
        if values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(invalid_request_at(
                request_path,
                line_number,
                format!("duplicate key {key:?}"),
            ));
        }
    }
    Ok(values)
}

fn reject_unknown_keys(
    values: &BTreeMap<String, String>,
    request_path: &Path,
) -> Result<(), AnalysisRequestError> {
    const KEYS: &[&str] = &[
        "format",
        "mode",
        "tree",
        "tree_name",
        "observation",
        "ranges",
        "detections",
        "controls",
        "parameters",
        "max_range_size",
        "max_states",
        "include_null_range",
        "root_prior",
        "min_branch_length",
        "missing_branch_length_fill",
        "dispersal_multipliers",
        "dispersal_strata",
        "distance_matrix",
        "environment_distance_matrix",
        "extirpation_multipliers",
        "area_sizes",
        "ancestral_probabilities",
        "split_probabilities",
        "optimization_initial_step",
        "optimization_tolerance",
        "optimization_max_iterations",
        "optimization_additional_starts",
    ];
    let known = KEYS.iter().copied().collect::<HashSet<_>>();
    if let Some(key) = values.keys().find(|key| !known.contains(key.as_str())) {
        return Err(invalid_request(
            request_path,
            format!("unknown request key {key:?}"),
        ));
    }
    Ok(())
}

fn required<'a>(
    values: &'a BTreeMap<String, String>,
    key: &'static str,
    request_path: &Path,
) -> Result<&'a str, AnalysisRequestError> {
    values
        .get(key)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or_else(|| invalid_request(request_path, format!("missing non-empty key {key:?}")))
}

fn reject_present(
    values: &BTreeMap<String, String>,
    key: &'static str,
    observation: &str,
    request_path: &Path,
) -> Result<(), AnalysisRequestError> {
    if values.contains_key(key) {
        Err(invalid_request(
            request_path,
            format!("{key} is incompatible with observation={observation}"),
        ))
    } else {
        Ok(())
    }
}

fn push_required_path(
    values: &BTreeMap<String, String>,
    key: &'static str,
    option: &'static str,
    base_dir: &Path,
    request_path: &Path,
    portable_paths: &mut bool,
    arguments: &mut Vec<String>,
) -> Result<(), AnalysisRequestError> {
    let raw = required(values, key, request_path)?;
    push_path(
        raw,
        option,
        base_dir,
        request_path,
        portable_paths,
        arguments,
    )
}

fn push_optional_path(
    values: &BTreeMap<String, String>,
    key: &'static str,
    option: &'static str,
    base_dir: &Path,
    request_path: &Path,
    portable_paths: &mut bool,
    arguments: &mut Vec<String>,
) -> Result<(), AnalysisRequestError> {
    let Some(raw) = values.get(key) else {
        return Ok(());
    };
    if raw.is_empty() {
        return Err(invalid_request(
            request_path,
            format!("{key} must be omitted instead of left empty"),
        ));
    }
    push_path(
        raw,
        option,
        base_dir,
        request_path,
        portable_paths,
        arguments,
    )
}

fn push_path(
    raw: &str,
    option: &'static str,
    base_dir: &Path,
    request_path: &Path,
    portable_paths: &mut bool,
    arguments: &mut Vec<String>,
) -> Result<(), AnalysisRequestError> {
    let path = PathBuf::from(raw);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        *portable_paths = false;
    }
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    let resolved = resolved
        .to_str()
        .ok_or_else(|| AnalysisRequestError::NonUtf8Path {
            request_path: request_path.to_path_buf(),
            path: resolved.clone(),
        })?;
    arguments.extend([option.to_string(), resolved.to_string()]);
    Ok(())
}

fn push_optional_value(
    values: &BTreeMap<String, String>,
    key: &'static str,
    option: &'static str,
    request_path: &Path,
    arguments: &mut Vec<String>,
) -> Result<(), AnalysisRequestError> {
    if let Some(value) = values.get(key) {
        if value.is_empty() {
            return Err(invalid_request(
                request_path,
                format!("{key} must be omitted instead of left empty"),
            ));
        }
        arguments.extend([option.to_string(), value.to_string()]);
    }
    Ok(())
}

fn push_required_value(
    values: &BTreeMap<String, String>,
    key: &'static str,
    option: &'static str,
    request_path: &Path,
    arguments: &mut Vec<String>,
) -> Result<(), AnalysisRequestError> {
    let value = required(values, key, request_path)?;
    arguments.extend([option.to_string(), value.to_string()]);
    Ok(())
}

fn push_required_bool_flag(
    values: &BTreeMap<String, String>,
    key: &'static str,
    option: &'static str,
    request_path: &Path,
    arguments: &mut Vec<String>,
) -> Result<(), AnalysisRequestError> {
    match required(values, key, request_path)? {
        "true" => arguments.push(option.to_string()),
        "false" => {}
        value => {
            return Err(invalid_request(
                request_path,
                format!("{key} must be true or false, found {value:?}"),
            ));
        }
    }
    Ok(())
}

fn invalid_request(path: &Path, message: impl Into<String>) -> AnalysisRequestError {
    AnalysisRequestError::InvalidRequest {
        path: path.to_path_buf(),
        line: None,
        message: message.into(),
    }
}

fn invalid_request_at(
    path: &Path,
    line: usize,
    message: impl Into<String>,
) -> AnalysisRequestError {
    AnalysisRequestError::InvalidRequest {
        path: path.to_path_buf(),
        line: Some(line),
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum AnalysisRequestError {
    InvalidRequest {
        path: PathBuf,
        line: Option<usize>,
        message: String,
    },
    NonUtf8Path {
        request_path: PathBuf,
        path: PathBuf,
    },
    OutputExists(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for AnalysisRequestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest {
                path,
                line: Some(line),
                message,
            } => write!(
                formatter,
                "invalid analysis request {} on line {}: {}",
                path.display(),
                line,
                message
            ),
            Self::InvalidRequest {
                path,
                line: None,
                message,
            } => write!(
                formatter,
                "invalid analysis request {}: {}",
                path.display(),
                message
            ),
            Self::NonUtf8Path { request_path, path } => write!(
                formatter,
                "analysis request {} resolves to a non-UTF-8 path {}",
                request_path.display(),
                path.display()
            ),
            Self::OutputExists(path) => {
                write!(
                    formatter,
                    "analysis template output already exists: {}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(formatter, "I/O failed for {}: {}", path.display(), source)
            }
        }
    }
}

impl Error for AnalysisRequestError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_portable_optimization_request_into_existing_cli_arguments() {
        let input = "key\tvalue\n\
format\tbiogeo-analysis-request-v1\n\
mode\toptimize\n\
tree\tdata/tree with spaces.nwk\n\
observation\tambiguous_ranges\n\
ranges\tdata/ranges.tsv\n\
parameters\tparameters.tsv\n\
max_range_size\t5\n\
max_states\t200000\n\
include_null_range\ttrue\n\
root_prior\tequal\n\
min_branch_length\t0.000001\n\
missing_branch_length_fill\t0.25\n\
ancestral_probabilities\ttrue\n\
split_probabilities\ttrue\n\
optimization_initial_step\t0.2\n\
optimization_tolerance\t1e-8\n\
optimization_max_iterations\t50\n\
optimization_additional_starts\t0.1,0.2;0.3,0.4\n";
        let request_path = Path::new("project/analysis.tsv");
        let parsed = parse_analysis_request(input, request_path).unwrap();
        assert_eq!(parsed.mode, AnalysisRequestMode::Optimize);
        assert!(parsed.portable_paths);
        assert_eq!(parsed.command_arguments[0], "model-optimize");
        let tree_option = parsed
            .command_arguments
            .iter()
            .position(|argument| argument == "--tree")
            .unwrap();
        assert_eq!(
            PathBuf::from(&parsed.command_arguments[tree_option + 1]),
            Path::new("project")
                .join("data")
                .join("tree with spaces.nwk")
        );
        assert!(
            parsed
                .command_arguments
                .contains(&"--use-ambiguities".to_string())
        );
        let fill_option = parsed
            .command_arguments
            .iter()
            .position(|argument| argument == "--fill-missing-branch-length")
            .unwrap();
        assert_eq!(parsed.command_arguments[fill_option + 1], "0.25");
        let max_states_option = parsed
            .command_arguments
            .iter()
            .position(|argument| argument == "--max-states")
            .unwrap();
        assert_eq!(parsed.command_arguments[max_states_option + 1], "200000");
        assert_eq!(
            parsed
                .command_arguments
                .iter()
                .filter(|argument| argument.as_str() == "--additional-start")
                .count(),
            2
        );
    }

    #[test]
    fn rejects_observation_conflicts_and_evaluate_optimization_fields() {
        let conflicting = "key\tvalue\n\
format\tbiogeo-analysis-request-v1\n\
mode\tevaluate\n\
tree\ttree.nwk\n\
observation\tmf_dp_fdp_detection\n\
ranges\tranges.tsv\n\
detections\tdetections.tsv\n\
controls\tcontrols.tsv\n\
parameters\tparameters.tsv\n\
max_range_size\tauto\n\
include_null_range\tfalse\n\
root_prior\tflat\n\
min_branch_length\t0\n\
ancestral_probabilities\tfalse\n\
split_probabilities\tfalse\n";
        assert!(matches!(
            parse_analysis_request(conflicting, Path::new("analysis.tsv")),
            Err(AnalysisRequestError::InvalidRequest { .. })
        ));

        let evaluate_with_optimizer = format!(
            "{}optimization_initial_step\t0.2\n",
            format_template_request(AnalysisRequestMode::Evaluate)
        );
        assert!(matches!(
            parse_analysis_request(&evaluate_with_optimizer, Path::new("analysis.tsv")),
            Err(AnalysisRequestError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn absolute_and_parent_paths_are_valid_but_not_portable() {
        let mut input = format_template_request(AnalysisRequestMode::Optimize);
        input = input.replace("tree.nwk", "../tree.nwk");
        let parsed = parse_analysis_request(&input, Path::new("project/analysis.tsv")).unwrap();
        assert!(!parsed.portable_paths);
    }
}
