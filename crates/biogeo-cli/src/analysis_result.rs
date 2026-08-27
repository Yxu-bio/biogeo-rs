use crate::input_bundle;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const LEGACY_ANALYSIS_RESULT_FORMAT_VERSION: &str = "biogeo-analysis-result-v1";
pub const ANALYSIS_RESULT_FORMAT_VERSION: &str = "biogeo-analysis-result-v2";
pub const METADATA_FILE: &str = "metadata.tsv";
pub const INPUTS_FILE: &str = "inputs.tsv";
pub const SOURCE_PARAMETERS_FILE: &str = "source-parameters.tsv";
pub const RESOLVED_PARAMETERS_FILE: &str = "resolved-parameters.tsv";
pub const INPUT_BUNDLE_DIR: &str = "input-bundle";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub struct AnalysisInputSpec<'a> {
    pub role: &'a str,
    pub path: &'a Path,
    pub required_for_replay: bool,
}

#[derive(Clone, Debug)]
pub struct AnalysisResultWriteRequest<'a> {
    pub mode: &'a str,
    pub log_likelihood: f64,
    pub model_fingerprint: &'a str,
    pub tip_observation_model: &'a str,
    pub tree_name: Option<&'a str>,
    pub max_range_size: u8,
    pub include_null_range: bool,
    pub root_prior: &'a str,
    pub min_branch_length: f64,
    pub missing_branch_length_fill: Option<f64>,
    pub states: usize,
    pub areas: usize,
    pub tips: usize,
    pub optimization: Option<AnalysisOptimizationSummary>,
    pub source_parameters: &'a str,
    pub resolved_parameters: &'a str,
    pub inputs: Vec<AnalysisInputSpec<'a>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisOptimizationSummary {
    pub converged: bool,
    pub iterations: usize,
    pub evaluations: usize,
    pub starts: usize,
    pub converged_starts: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AnalysisInputRecord {
    pub role: String,
    pub path: PathBuf,
    pub required_for_replay: bool,
    pub bytes: u64,
    pub fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct AnalysisResultManifest {
    pub mode: String,
    pub log_likelihood: f64,
    pub model_fingerprint: String,
    pub tip_observation_model: String,
    pub tree_name: Option<String>,
    pub max_range_size: u8,
    pub include_null_range: bool,
    pub root_prior: String,
    pub min_branch_length: f64,
    pub missing_branch_length_fill: Option<f64>,
    pub states: usize,
    pub areas: usize,
    pub tips: usize,
    pub optimization: Option<AnalysisOptimizationSummary>,
    pub inputs: BTreeMap<String, AnalysisInputRecord>,
}

#[derive(Clone, Debug)]
pub struct LoadedAnalysisResult {
    pub root: PathBuf,
    pub format_version: String,
    pub manifest: AnalysisResultManifest,
    pub source_parameters: String,
    pub resolved_parameters: String,
    pub fingerprint: String,
    pub input_bundle: Option<input_bundle::LoadedInputBundle>,
}

impl LoadedAnalysisResult {
    pub fn is_portable(&self) -> bool {
        self.input_bundle.is_some()
    }

    pub fn input_path(&self, role: &str) -> Option<&Path> {
        self.manifest
            .inputs
            .get(role)
            .map(|record| record.path.as_path())
    }

    pub fn require_input_path(&self, role: &'static str) -> Result<&Path, AnalysisResultError> {
        self.input_path(role)
            .ok_or(AnalysisResultError::MissingInputRole(role))
    }

    pub fn verify_replay_inputs(&self) -> Result<(), AnalysisResultError> {
        for record in self
            .manifest
            .inputs
            .values()
            .filter(|record| record.required_for_replay)
        {
            let bytes = fs::read(&record.path).map_err(|source| AnalysisResultError::Io {
                path: record.path.clone(),
                source,
            })?;
            let actual_fingerprint = stable_fingerprint(&bytes);
            if bytes.len() as u64 != record.bytes || actual_fingerprint != record.fingerprint {
                return Err(AnalysisResultError::InputChanged {
                    role: record.role.clone(),
                    path: record.path.clone(),
                    expected_bytes: record.bytes,
                    actual_bytes: bytes.len() as u64,
                    expected_fingerprint: record.fingerprint.clone(),
                    actual_fingerprint,
                });
            }
        }
        Ok(())
    }
}

pub fn model_fingerprint(model: &biogeo_core::ModelConfig) -> String {
    stable_fingerprint(&model.stable_identity_v1())
}

pub fn migrate_analysis_result(
    source: &LoadedAnalysisResult,
    output_dir: &Path,
) -> Result<LoadedAnalysisResult, AnalysisResultError> {
    if source.format_version == ANALYSIS_RESULT_FORMAT_VERSION {
        return Err(AnalysisResultError::AlreadyCurrentFormat(
            source.format_version.clone(),
        ));
    }
    source.verify_replay_inputs()?;
    let internal_source_parameters = source.root.join(SOURCE_PARAMETERS_FILE);
    let inputs: Vec<AnalysisInputSpec<'_>> = source
        .manifest
        .inputs
        .values()
        .map(|record| AnalysisInputSpec {
            role: &record.role,
            path: if record.role == "source_parameters" {
                &internal_source_parameters
            } else {
                &record.path
            },
            required_for_replay: record.required_for_replay,
        })
        .collect();
    write_analysis_result(
        output_dir,
        &AnalysisResultWriteRequest {
            mode: &source.manifest.mode,
            log_likelihood: source.manifest.log_likelihood,
            model_fingerprint: &source.manifest.model_fingerprint,
            tip_observation_model: &source.manifest.tip_observation_model,
            tree_name: source.manifest.tree_name.as_deref(),
            max_range_size: source.manifest.max_range_size,
            include_null_range: source.manifest.include_null_range,
            root_prior: &source.manifest.root_prior,
            min_branch_length: source.manifest.min_branch_length,
            missing_branch_length_fill: source.manifest.missing_branch_length_fill,
            states: source.manifest.states,
            areas: source.manifest.areas,
            tips: source.manifest.tips,
            optimization: source.manifest.optimization,
            source_parameters: &source.source_parameters,
            resolved_parameters: &source.resolved_parameters,
            inputs,
        },
    )?;
    load_analysis_result(output_dir)
}

pub fn write_analysis_result(
    output_dir: &Path,
    request: &AnalysisResultWriteRequest<'_>,
) -> Result<(), AnalysisResultError> {
    validate_write_request(output_dir, request)?;
    if output_dir.exists() {
        return Err(AnalysisResultError::OutputExists(output_dir.to_path_buf()));
    }
    let parent = output_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| AnalysisResultError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let staging_dir = create_staging_directory(parent, output_dir)?;
    let write_result = write_analysis_result_contents(&staging_dir, request);
    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(error);
    }
    if output_dir.exists() {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(AnalysisResultError::OutputExists(output_dir.to_path_buf()));
    }
    if let Err(source) = crate::fs_retry::rename(&staging_dir, output_dir) {
        let _ = fs::remove_dir_all(&staging_dir);
        return Err(AnalysisResultError::Io {
            path: output_dir.to_path_buf(),
            source,
        });
    }
    Ok(())
}

fn validate_write_request(
    output_dir: &Path,
    request: &AnalysisResultWriteRequest<'_>,
) -> Result<(), AnalysisResultError> {
    if request.mode != "evaluate" && request.mode != "optimize" {
        return Err(invalid_metadata(
            output_dir,
            "mode must be evaluate or optimize",
        ));
    }
    if (request.mode == "optimize") != request.optimization.is_some() {
        return Err(invalid_metadata(
            output_dir,
            "optimization summary must be present exactly when mode is optimize",
        ));
    }
    if !request.log_likelihood.is_finite() {
        return Err(invalid_metadata(output_dir, "lnL must be finite"));
    }
    if request.model_fingerprint.len() != 16
        || !request
            .model_fingerprint
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid_metadata(
            output_dir,
            "model_fingerprint must contain 16 hexadecimal digits",
        ));
    }
    if request.tip_observation_model != "exact_ranges"
        && request.tip_observation_model != "ambiguous_ranges"
        && request.tip_observation_model != "mf_dp_fdp_detection"
    {
        return Err(invalid_metadata(
            output_dir,
            "unsupported tip_observation_model",
        ));
    }
    if request.root_prior != "flat" && request.root_prior != "equal" {
        return Err(invalid_metadata(
            output_dir,
            "root_prior must be flat or equal",
        ));
    }
    if !request.min_branch_length.is_finite() || request.min_branch_length < 0.0 {
        return Err(invalid_metadata(
            output_dir,
            "min_branch_length must be finite and non-negative",
        ));
    }
    if request
        .missing_branch_length_fill
        .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err(invalid_metadata(
            output_dir,
            "missing_branch_length_fill must be finite and non-negative",
        ));
    }
    if request.states == 0 || request.areas == 0 || request.tips == 0 {
        return Err(invalid_metadata(
            output_dir,
            "states, areas, and tips must be positive",
        ));
    }
    if request.max_range_size == 0 || usize::from(request.max_range_size) > request.areas {
        return Err(invalid_metadata(
            output_dir,
            "max_range_size must be between 1 and areas",
        ));
    }
    if let Some(optimization) = request.optimization
        && (optimization.starts == 0 || optimization.converged_starts > optimization.starts)
    {
        return Err(invalid_metadata(
            output_dir,
            "optimization start counts are inconsistent",
        ));
    }
    Ok(())
}

fn create_staging_directory(
    parent: &Path,
    output_dir: &Path,
) -> Result<PathBuf, AnalysisResultError> {
    let output_name = output_dir
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_else(|| "analysis-result".into());
    loop {
        let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(
            ".{output_name}.tmp-{}-{sequence}",
            std::process::id()
        ));
        match fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(source) => {
                return Err(AnalysisResultError::Io {
                    path: candidate,
                    source,
                });
            }
        }
    }
}

fn write_analysis_result_contents(
    output_dir: &Path,
    request: &AnalysisResultWriteRequest<'_>,
) -> Result<(), AnalysisResultError> {
    let source_parameters_path = output_dir.join(SOURCE_PARAMETERS_FILE);
    write_file(
        &source_parameters_path,
        request.source_parameters.as_bytes(),
    )?;
    let resolved_parameters_path = output_dir.join(RESOLVED_PARAMETERS_FILE);
    write_file(
        &resolved_parameters_path,
        request.resolved_parameters.as_bytes(),
    )?;

    let bundle_specs: Vec<input_bundle::InputBundleSpec<'_>> = request
        .inputs
        .iter()
        .map(|input| input_bundle::InputBundleSpec {
            role: input.role,
            path: input.path,
            required_for_replay: input.required_for_replay,
        })
        .collect();
    let bundle =
        input_bundle::write_input_bundle(&output_dir.join(INPUT_BUNDLE_DIR), &bundle_specs)?;
    let mut records: Vec<AnalysisInputRecord> = bundle
        .top_level_inputs
        .values()
        .map(|record| AnalysisInputRecord {
            role: record.role.clone(),
            path: PathBuf::from(INPUT_BUNDLE_DIR).join(&record.relative_path),
            required_for_replay: record.required_for_replay,
            bytes: record.bytes,
            fingerprint: record.fingerprint.clone(),
        })
        .collect();
    records.sort_by(|left, right| left.role.cmp(&right.role));

    let inputs = format_inputs(&records, true)?;
    let inputs_path = output_dir.join(INPUTS_FILE);
    write_file(&inputs_path, inputs.as_bytes())?;

    let source_fingerprint = stable_fingerprint(request.source_parameters.as_bytes());
    let resolved_fingerprint = stable_fingerprint(request.resolved_parameters.as_bytes());
    let mut metadata = format!(
        "key\tvalue\n\
format\t{}\n\
status\tcomplete\n\
mode\t{}\n\
lnL\t{}\n\
lnL_bits\t{:016x}\n\
model_identity_format\t{}\n\
model_fingerprint\t{}\n\
tip_observation_model\t{}\n\
tree_name\t{}\n\
max_range_size\t{}\n\
include_null_range\t{}\n\
root_prior\t{}\n\
min_branch_length\t{}\n\
min_branch_length_bits\t{:016x}\n\
states\t{}\n\
areas\t{}\n\
tips\t{}\n\
optimization_performed\t{}\n\
inputs_file\t{}\n\
input_path_mode\trelative\n\
input_bundle_dir\t{}\n\
input_bundle_format\t{}\n\
input_bundle_fingerprint\t{}\n\
source_parameters_file\t{}\n\
source_parameters_fingerprint\t{}\n\
resolved_parameters_file\t{}\n\
resolved_parameters_fingerprint\t{}\n",
        ANALYSIS_RESULT_FORMAT_VERSION,
        request.mode,
        request.log_likelihood,
        request.log_likelihood.to_bits(),
        biogeo_core::MODEL_IDENTITY_FORMAT_VERSION,
        request.model_fingerprint,
        request.tip_observation_model,
        request.tree_name.map(encode_field).unwrap_or_default(),
        request.max_range_size,
        request.include_null_range,
        request.root_prior,
        request.min_branch_length,
        request.min_branch_length.to_bits(),
        request.states,
        request.areas,
        request.tips,
        request.optimization.is_some(),
        INPUTS_FILE,
        INPUT_BUNDLE_DIR,
        input_bundle::INPUT_BUNDLE_FORMAT_VERSION,
        bundle.fingerprint,
        SOURCE_PARAMETERS_FILE,
        source_fingerprint,
        RESOLVED_PARAMETERS_FILE,
        resolved_fingerprint,
    );
    append_missing_branch_length_fill_metadata(&mut metadata, request.missing_branch_length_fill);
    if let Some(optimization) = request.optimization {
        metadata.push_str(&format!(
            "optimization_converged\t{}\noptimization_iterations\t{}\noptimization_evaluations\t{}\noptimization_starts\t{}\noptimization_converged_starts\t{}\n",
            optimization.converged,
            optimization.iterations,
            optimization.evaluations,
            optimization.starts,
            optimization.converged_starts,
        ));
    }
    write_file(&output_dir.join(METADATA_FILE), metadata.as_bytes())
}

#[cfg(test)]
pub(crate) fn write_legacy_analysis_result_for_test(
    output_dir: &Path,
    request: &AnalysisResultWriteRequest<'_>,
) -> Result<(), AnalysisResultError> {
    validate_write_request(output_dir, request)?;
    if output_dir.exists() {
        return Err(AnalysisResultError::OutputExists(output_dir.to_path_buf()));
    }
    fs::create_dir_all(output_dir).map_err(|source| AnalysisResultError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;
    write_file(
        &output_dir.join(SOURCE_PARAMETERS_FILE),
        request.source_parameters.as_bytes(),
    )?;
    write_file(
        &output_dir.join(RESOLVED_PARAMETERS_FILE),
        request.resolved_parameters.as_bytes(),
    )?;
    let mut records = Vec::with_capacity(request.inputs.len());
    for input in &request.inputs {
        let canonical_path =
            fs::canonicalize(input.path).map_err(|source| AnalysisResultError::Io {
                path: input.path.to_path_buf(),
                source,
            })?;
        let bytes = fs::read(&canonical_path).map_err(|source| AnalysisResultError::Io {
            path: canonical_path.clone(),
            source,
        })?;
        records.push(AnalysisInputRecord {
            role: input.role.to_string(),
            path: canonical_path,
            required_for_replay: input.required_for_replay,
            bytes: bytes.len() as u64,
            fingerprint: stable_fingerprint(&bytes),
        });
    }
    records.sort_by(|left, right| left.role.cmp(&right.role));
    write_file(
        &output_dir.join(INPUTS_FILE),
        format_inputs(&records, false)?.as_bytes(),
    )?;
    let source_fingerprint = stable_fingerprint(request.source_parameters.as_bytes());
    let resolved_fingerprint = stable_fingerprint(request.resolved_parameters.as_bytes());
    let mut metadata = format!(
        "key\tvalue\n\
format\t{}\n\
status\tcomplete\n\
mode\t{}\n\
lnL\t{}\n\
lnL_bits\t{:016x}\n\
model_identity_format\t{}\n\
model_fingerprint\t{}\n\
tip_observation_model\t{}\n\
tree_name\t{}\n\
max_range_size\t{}\n\
include_null_range\t{}\n\
root_prior\t{}\n\
min_branch_length\t{}\n\
min_branch_length_bits\t{:016x}\n\
states\t{}\n\
areas\t{}\n\
tips\t{}\n\
optimization_performed\t{}\n\
inputs_file\t{}\n\
source_parameters_file\t{}\n\
source_parameters_fingerprint\t{}\n\
resolved_parameters_file\t{}\n\
resolved_parameters_fingerprint\t{}\n",
        LEGACY_ANALYSIS_RESULT_FORMAT_VERSION,
        request.mode,
        request.log_likelihood,
        request.log_likelihood.to_bits(),
        biogeo_core::MODEL_IDENTITY_FORMAT_VERSION,
        request.model_fingerprint,
        request.tip_observation_model,
        request.tree_name.map(encode_field).unwrap_or_default(),
        request.max_range_size,
        request.include_null_range,
        request.root_prior,
        request.min_branch_length,
        request.min_branch_length.to_bits(),
        request.states,
        request.areas,
        request.tips,
        request.optimization.is_some(),
        INPUTS_FILE,
        SOURCE_PARAMETERS_FILE,
        source_fingerprint,
        RESOLVED_PARAMETERS_FILE,
        resolved_fingerprint,
    );
    append_missing_branch_length_fill_metadata(&mut metadata, request.missing_branch_length_fill);
    if let Some(optimization) = request.optimization {
        metadata.push_str(&format!(
            "optimization_converged\t{}\noptimization_iterations\t{}\noptimization_evaluations\t{}\noptimization_starts\t{}\noptimization_converged_starts\t{}\n",
            optimization.converged,
            optimization.iterations,
            optimization.evaluations,
            optimization.starts,
            optimization.converged_starts,
        ));
    }
    write_file(&output_dir.join(METADATA_FILE), metadata.as_bytes())
}

fn append_missing_branch_length_fill_metadata(metadata: &mut String, value: Option<f64>) {
    if let Some(value) = value {
        metadata.push_str(&format!(
            "missing_branch_length_fill\t{value}\nmissing_branch_length_fill_bits\t{:016x}\n",
            value.to_bits()
        ));
    }
}

pub fn load_analysis_result(
    output_dir: &Path,
) -> Result<LoadedAnalysisResult, AnalysisResultError> {
    let root = fs::canonicalize(output_dir).map_err(|source| AnalysisResultError::Io {
        path: output_dir.to_path_buf(),
        source,
    })?;
    let metadata_path = root.join(METADATA_FILE);
    let metadata_bytes = fs::read(&metadata_path).map_err(|source| AnalysisResultError::Io {
        path: metadata_path.clone(),
        source,
    })?;
    let metadata_text = std::str::from_utf8(&metadata_bytes)
        .map_err(|_| invalid_metadata(&metadata_path, "metadata is not UTF-8"))?;
    let metadata = parse_key_value_table(&metadata_path, metadata_text)?;

    let format_version = required(&metadata, "format", &metadata_path)?.to_string();
    if format_version != LEGACY_ANALYSIS_RESULT_FORMAT_VERSION
        && format_version != ANALYSIS_RESULT_FORMAT_VERSION
    {
        return Err(invalid_metadata(
            &metadata_path,
            format!("unsupported analysis result format {format_version:?}"),
        ));
    }
    require_value(&metadata, "status", &metadata_path, "complete")?;
    require_value(
        &metadata,
        "model_identity_format",
        &metadata_path,
        biogeo_core::MODEL_IDENTITY_FORMAT_VERSION,
    )?;
    require_value(&metadata, "inputs_file", &metadata_path, INPUTS_FILE)?;
    require_value(
        &metadata,
        "source_parameters_file",
        &metadata_path,
        SOURCE_PARAMETERS_FILE,
    )?;
    require_value(
        &metadata,
        "resolved_parameters_file",
        &metadata_path,
        RESOLVED_PARAMETERS_FILE,
    )?;

    let input_bundle = if format_version == ANALYSIS_RESULT_FORMAT_VERSION {
        require_value(&metadata, "input_path_mode", &metadata_path, "relative")?;
        require_value(
            &metadata,
            "input_bundle_dir",
            &metadata_path,
            INPUT_BUNDLE_DIR,
        )?;
        require_value(
            &metadata,
            "input_bundle_format",
            &metadata_path,
            input_bundle::INPUT_BUNDLE_FORMAT_VERSION,
        )?;
        let bundle = input_bundle::load_input_bundle(&root.join(INPUT_BUNDLE_DIR))?;
        let expected = required(&metadata, "input_bundle_fingerprint", &metadata_path)?;
        if bundle.fingerprint != expected {
            return Err(AnalysisResultError::InputBundleFingerprintChanged {
                expected: expected.to_string(),
                actual: bundle.fingerprint.clone(),
            });
        }
        Some(bundle)
    } else {
        None
    };

    let source_parameters = read_internal_file(
        &root,
        SOURCE_PARAMETERS_FILE,
        required(&metadata, "source_parameters_fingerprint", &metadata_path)?,
    )?;
    let resolved_parameters = read_internal_file(
        &root,
        RESOLVED_PARAMETERS_FILE,
        required(&metadata, "resolved_parameters_fingerprint", &metadata_path)?,
    )?;
    let inputs_path = root.join(INPUTS_FILE);
    let inputs_bytes = fs::read(&inputs_path).map_err(|source| AnalysisResultError::Io {
        path: inputs_path.clone(),
        source,
    })?;
    let inputs_text = std::str::from_utf8(&inputs_bytes)
        .map_err(|_| invalid_inputs(&inputs_path, "inputs table is not UTF-8"))?;
    let inputs = parse_inputs(&inputs_path, inputs_text, &root, &format_version)?;
    if let Some(bundle) = input_bundle.as_ref() {
        validate_bundle_inputs(&inputs_path, &inputs, bundle)?;
    }

    let log_likelihood = f64::from_bits(parse_hex_u64(
        required(&metadata, "lnL_bits", &metadata_path)?,
        "lnL_bits",
        &metadata_path,
    )?);
    if !log_likelihood.is_finite() {
        return Err(invalid_metadata(&metadata_path, "lnL_bits is not finite"));
    }
    let decimal_log_likelihood: f64 = parse_value(&metadata, "lnL", &metadata_path)?;
    if decimal_log_likelihood.to_bits() != log_likelihood.to_bits() {
        return Err(invalid_metadata(
            &metadata_path,
            "lnL and lnL_bits encode different values",
        ));
    }
    let mode = required(&metadata, "mode", &metadata_path)?.to_string();
    if mode != "evaluate" && mode != "optimize" {
        return Err(invalid_metadata(
            &metadata_path,
            "mode must be evaluate or optimize",
        ));
    }
    let optimization_performed = parse_bool(&metadata, "optimization_performed", &metadata_path)?;
    if optimization_performed != (mode == "optimize") {
        return Err(invalid_metadata(
            &metadata_path,
            "optimization_performed must agree with mode",
        ));
    }
    let optimization = optimization_performed
        .then(|| {
            Ok::<_, AnalysisResultError>(AnalysisOptimizationSummary {
                converged: parse_bool(&metadata, "optimization_converged", &metadata_path)?,
                iterations: parse_value(&metadata, "optimization_iterations", &metadata_path)?,
                evaluations: parse_value(&metadata, "optimization_evaluations", &metadata_path)?,
                starts: parse_value(&metadata, "optimization_starts", &metadata_path)?,
                converged_starts: parse_value(
                    &metadata,
                    "optimization_converged_starts",
                    &metadata_path,
                )?,
            })
        })
        .transpose()?;
    let root_prior = required(&metadata, "root_prior", &metadata_path)?.to_string();
    if root_prior != "flat" && root_prior != "equal" {
        return Err(invalid_metadata(
            &metadata_path,
            "root_prior must be flat or equal",
        ));
    }
    let tip_observation_model =
        required(&metadata, "tip_observation_model", &metadata_path)?.to_string();
    if tip_observation_model != "exact_ranges"
        && tip_observation_model != "ambiguous_ranges"
        && tip_observation_model != "mf_dp_fdp_detection"
    {
        return Err(invalid_metadata(
            &metadata_path,
            "unsupported tip_observation_model",
        ));
    }
    let tree_name = metadata
        .get("tree_name")
        .filter(|value| !value.is_empty())
        .map(|value| {
            decode_field(value).map_err(|message| {
                invalid_metadata(&metadata_path, format!("tree_name: {message}"))
            })
        })
        .transpose()?;
    let min_branch_length = match metadata.get("min_branch_length_bits") {
        Some(bits) => {
            let value = f64::from_bits(parse_hex_u64(
                bits,
                "min_branch_length_bits",
                &metadata_path,
            )?);
            if !value.is_finite() || value < 0.0 {
                return Err(invalid_metadata(
                    &metadata_path,
                    "min_branch_length_bits is not finite and non-negative",
                ));
            }
            let decimal: f64 = parse_value(&metadata, "min_branch_length", &metadata_path)?;
            if decimal.to_bits() != value.to_bits() {
                return Err(invalid_metadata(
                    &metadata_path,
                    "min_branch_length and min_branch_length_bits encode different values",
                ));
            }
            value
        }
        None => 0.0,
    };
    let missing_branch_length_fill = match metadata.get("missing_branch_length_fill_bits") {
        Some(bits) => {
            let value = f64::from_bits(parse_hex_u64(
                bits,
                "missing_branch_length_fill_bits",
                &metadata_path,
            )?);
            if !value.is_finite() || value < 0.0 {
                return Err(invalid_metadata(
                    &metadata_path,
                    "missing_branch_length_fill_bits is not finite and non-negative",
                ));
            }
            let decimal: f64 =
                parse_value(&metadata, "missing_branch_length_fill", &metadata_path)?;
            if decimal.to_bits() != value.to_bits() {
                return Err(invalid_metadata(
                    &metadata_path,
                    "missing_branch_length_fill and missing_branch_length_fill_bits encode different values",
                ));
            }
            Some(value)
        }
        None => {
            if metadata.contains_key("missing_branch_length_fill") {
                return Err(invalid_metadata(
                    &metadata_path,
                    "missing_branch_length_fill requires missing_branch_length_fill_bits",
                ));
            }
            None
        }
    };

    let manifest = AnalysisResultManifest {
        mode,
        log_likelihood,
        model_fingerprint: required(&metadata, "model_fingerprint", &metadata_path)?.to_string(),
        tip_observation_model,
        tree_name,
        max_range_size: parse_value(&metadata, "max_range_size", &metadata_path)?,
        include_null_range: parse_bool(&metadata, "include_null_range", &metadata_path)?,
        root_prior,
        min_branch_length,
        missing_branch_length_fill,
        states: parse_value(&metadata, "states", &metadata_path)?,
        areas: parse_value(&metadata, "areas", &metadata_path)?,
        tips: parse_value(&metadata, "tips", &metadata_path)?,
        optimization,
        inputs,
    };

    let mut fingerprint_input = Vec::new();
    fingerprint_input.extend_from_slice(&metadata_bytes);
    fingerprint_input.extend_from_slice(&inputs_bytes);
    fingerprint_input.extend_from_slice(source_parameters.as_bytes());
    fingerprint_input.extend_from_slice(resolved_parameters.as_bytes());
    if let Some(bundle) = input_bundle.as_ref() {
        fingerprint_input.extend_from_slice(bundle.fingerprint.as_bytes());
    }
    let fingerprint = stable_fingerprint(&fingerprint_input);

    Ok(LoadedAnalysisResult {
        root,
        format_version,
        manifest,
        source_parameters,
        resolved_parameters,
        fingerprint,
        input_bundle,
    })
}

fn format_inputs(
    records: &[AnalysisInputRecord],
    portable_paths: bool,
) -> Result<String, AnalysisResultError> {
    let mut output = String::from("role\tpath\trequired_for_replay\tbytes\tfingerprint\n");
    for record in records {
        let path = require_utf8_path(&record.path)?;
        let serialized_path = if portable_paths {
            path.replace('\\', "/")
        } else {
            path.to_string()
        };
        output.push_str(&encode_field(&record.role));
        output.push('\t');
        output.push_str(&encode_field(&serialized_path));
        output.push('\t');
        output.push_str(if record.required_for_replay {
            "true"
        } else {
            "false"
        });
        output.push('\t');
        output.push_str(&record.bytes.to_string());
        output.push('\t');
        output.push_str(&record.fingerprint);
        output.push('\n');
    }
    Ok(output)
}

fn parse_inputs(
    path: &Path,
    input: &str,
    root: &Path,
    format_version: &str,
) -> Result<BTreeMap<String, AnalysisInputRecord>, AnalysisResultError> {
    let mut lines = input.lines();
    if lines.next() != Some("role\tpath\trequired_for_replay\tbytes\tfingerprint") {
        return Err(invalid_inputs(path, "unexpected inputs header"));
    }
    let mut records = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 5 {
            return Err(invalid_inputs(
                path,
                format!("line {} must contain five fields", index + 2),
            ));
        }
        let role = decode_field(fields[0]).map_err(|message| invalid_inputs(path, message))?;
        let decoded_path =
            decode_field(fields[1]).map_err(|message| invalid_inputs(path, message))?;
        let input_path = if format_version == ANALYSIS_RESULT_FORMAT_VERSION {
            resolve_portable_input_path(root, &decoded_path, path)?
        } else {
            let legacy_path = PathBuf::from(&decoded_path);
            if !legacy_path.is_absolute() {
                return Err(invalid_inputs(
                    path,
                    "analysis result v1 input paths must be absolute",
                ));
            }
            legacy_path
        };
        let required_for_replay = match fields[2] {
            "true" => true,
            "false" => false,
            _ => return Err(invalid_inputs(path, "invalid required_for_replay value")),
        };
        let bytes = fields[3]
            .parse::<u64>()
            .map_err(|_| invalid_inputs(path, "invalid input byte length"))?;
        if fields[4].len() != 16 || !fields[4].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(invalid_inputs(path, "invalid input fingerprint"));
        }
        if records.contains_key(&role) {
            return Err(invalid_inputs(
                path,
                format!("duplicate input role {role:?}"),
            ));
        }
        records.insert(
            role.clone(),
            AnalysisInputRecord {
                role,
                path: input_path,
                required_for_replay,
                bytes,
                fingerprint: fields[4].to_ascii_lowercase(),
            },
        );
    }
    Ok(records)
}

fn resolve_portable_input_path(
    root: &Path,
    raw: &str,
    inputs_path: &Path,
) -> Result<PathBuf, AnalysisResultError> {
    if raw.is_empty()
        || raw.contains('\\')
        || raw.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
    {
        return Err(invalid_inputs(inputs_path, "invalid portable input path"));
    }
    let mut relative = PathBuf::new();
    for component in Path::new(raw).components() {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return Err(invalid_inputs(inputs_path, "invalid portable input path")),
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(invalid_inputs(inputs_path, "invalid portable input path"));
    }
    let candidate = root.join(relative);
    let canonical = fs::canonicalize(&candidate).map_err(|source| AnalysisResultError::Io {
        path: candidate.clone(),
        source,
    })?;
    if !canonical.starts_with(root) {
        return Err(invalid_inputs(
            inputs_path,
            "portable input path escapes the analysis result directory",
        ));
    }
    Ok(canonical)
}

fn validate_bundle_inputs(
    inputs_path: &Path,
    inputs: &BTreeMap<String, AnalysisInputRecord>,
    bundle: &input_bundle::LoadedInputBundle,
) -> Result<(), AnalysisResultError> {
    if inputs.len() != bundle.top_level_inputs.len() {
        return Err(invalid_inputs(
            inputs_path,
            "inputs.tsv and the input bundle contain different input counts",
        ));
    }
    for (role, record) in inputs {
        let bundled = bundle.top_level_inputs.get(role).ok_or_else(|| {
            invalid_inputs(
                inputs_path,
                format!("input role {role:?} is missing from the input bundle"),
            )
        })?;
        if record.path != bundled.path
            || record.required_for_replay != bundled.required_for_replay
            || record.bytes != bundled.bytes
            || record.fingerprint != bundled.fingerprint
        {
            return Err(invalid_inputs(
                inputs_path,
                format!("input role {role:?} disagrees with the input bundle manifest"),
            ));
        }
    }
    Ok(())
}

fn parse_key_value_table(
    path: &Path,
    input: &str,
) -> Result<BTreeMap<String, String>, AnalysisResultError> {
    let mut lines = input.lines();
    if lines.next() != Some("key\tvalue") {
        return Err(invalid_metadata(path, "unexpected metadata header"));
    }
    let mut values = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('\t') else {
            return Err(invalid_metadata(
                path,
                format!("line {} has no tab separator", index + 2),
            ));
        };
        if key.is_empty() || values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(invalid_metadata(
                path,
                format!("line {} has an empty or duplicate key", index + 2),
            ));
        }
    }
    Ok(values)
}

fn read_internal_file(
    root: &Path,
    name: &str,
    expected_fingerprint: &str,
) -> Result<String, AnalysisResultError> {
    let path = root.join(name);
    let bytes = fs::read(&path).map_err(|source| AnalysisResultError::Io {
        path: path.clone(),
        source,
    })?;
    let actual_fingerprint = stable_fingerprint(&bytes);
    if actual_fingerprint != expected_fingerprint {
        return Err(AnalysisResultError::InternalFileChanged {
            path,
            expected_fingerprint: expected_fingerprint.to_string(),
            actual_fingerprint,
        });
    }
    String::from_utf8(bytes)
        .map_err(|_| invalid_metadata(root, format!("{name} is not valid UTF-8")))
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), AnalysisResultError> {
    fs::write(path, bytes).map_err(|source| AnalysisResultError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn require_utf8_path(path: &Path) -> Result<&str, AnalysisResultError> {
    path.to_str()
        .ok_or_else(|| AnalysisResultError::NonUtf8Path(path.to_path_buf()))
}

fn required<'a>(
    values: &'a BTreeMap<String, String>,
    key: &'static str,
    path: &Path,
) -> Result<&'a str, AnalysisResultError> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid_metadata(path, format!("missing key {key:?}")))
}

fn require_value(
    values: &BTreeMap<String, String>,
    key: &'static str,
    path: &Path,
    expected: &str,
) -> Result<(), AnalysisResultError> {
    let actual = required(values, key, path)?;
    if actual != expected {
        return Err(invalid_metadata(
            path,
            format!("{key} must be {expected:?}, got {actual:?}"),
        ));
    }
    Ok(())
}

fn parse_value<T: std::str::FromStr>(
    values: &BTreeMap<String, String>,
    key: &'static str,
    path: &Path,
) -> Result<T, AnalysisResultError> {
    required(values, key, path)?
        .parse()
        .map_err(|_| invalid_metadata(path, format!("invalid value for {key}")))
}

fn parse_bool(
    values: &BTreeMap<String, String>,
    key: &'static str,
    path: &Path,
) -> Result<bool, AnalysisResultError> {
    match required(values, key, path)? {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(invalid_metadata(path, format!("invalid boolean for {key}"))),
    }
}

fn parse_hex_u64(value: &str, key: &str, path: &Path) -> Result<u64, AnalysisResultError> {
    u64::from_str_radix(value, 16)
        .map_err(|_| invalid_metadata(path, format!("invalid hexadecimal value for {key}")))
}

pub(crate) fn encode_field(value: &str) -> String {
    let mut encoded = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'%' | b'\t' | b'\r' | b'\n' => {
                encoded.push(b'%');
                encoded.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            _ => encoded.push(byte),
        }
    }
    String::from_utf8(encoded).expect("encoding a UTF-8 field preserves UTF-8")
}

pub(crate) fn decode_field(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return Err("truncated percent escape".to_string());
            }
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .map_err(|_| "invalid percent escape".to_string())?;
            let byte =
                u8::from_str_radix(hex, 16).map_err(|_| "invalid percent escape".to_string())?;
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| "decoded field is not UTF-8".to_string())
}

pub fn stable_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn invalid_metadata(path: &Path, message: impl Into<String>) -> AnalysisResultError {
    AnalysisResultError::InvalidMetadata {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn invalid_inputs(path: &Path, message: impl Into<String>) -> AnalysisResultError {
    AnalysisResultError::InvalidInputs {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum AnalysisResultError {
    OutputExists(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    NonUtf8Path(PathBuf),
    AlreadyCurrentFormat(String),
    MissingInputRole(&'static str),
    InvalidMetadata {
        path: PathBuf,
        message: String,
    },
    InvalidInputs {
        path: PathBuf,
        message: String,
    },
    InternalFileChanged {
        path: PathBuf,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
    InputChanged {
        role: String,
        path: PathBuf,
        expected_bytes: u64,
        actual_bytes: u64,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
    InputBundleFingerprintChanged {
        expected: String,
        actual: String,
    },
    InputBundle(Box<input_bundle::InputBundleError>),
}

impl fmt::Display for AnalysisResultError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputExists(path) => {
                write!(
                    formatter,
                    "analysis result directory already exists: {}",
                    path.display()
                )
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "analysis result I/O failed for {}: {source}",
                    path.display()
                )
            }
            Self::NonUtf8Path(path) => write!(
                formatter,
                "analysis result v1 requires UTF-8 input paths, got {}",
                path.display()
            ),
            Self::AlreadyCurrentFormat(format) => write!(
                formatter,
                "analysis result already uses the current format {format}"
            ),
            Self::MissingInputRole(role) => {
                write!(
                    formatter,
                    "analysis result is missing required input role {role:?}"
                )
            }
            Self::InvalidMetadata { path, message } => {
                write!(
                    formatter,
                    "invalid analysis metadata {}: {message}",
                    path.display()
                )
            }
            Self::InvalidInputs { path, message } => {
                write!(
                    formatter,
                    "invalid analysis inputs {}: {message}",
                    path.display()
                )
            }
            Self::InternalFileChanged {
                path,
                expected_fingerprint,
                actual_fingerprint,
            } => write!(
                formatter,
                "analysis result file {} failed its fingerprint check: expected {expected_fingerprint}, got {actual_fingerprint}",
                path.display()
            ),
            Self::InputChanged {
                role,
                path,
                expected_bytes,
                actual_bytes,
                expected_fingerprint,
                actual_fingerprint,
            } => write!(
                formatter,
                "analysis input {role:?} changed at {}: expected {expected_bytes} bytes/{expected_fingerprint}, got {actual_bytes} bytes/{actual_fingerprint}",
                path.display()
            ),
            Self::InputBundleFingerprintChanged { expected, actual } => write!(
                formatter,
                "analysis result input bundle fingerprint changed: expected {expected}, got {actual}"
            ),
            Self::InputBundle(source) => write!(formatter, "input bundle failed: {source}"),
        }
    }
}

impl Error for AnalysisResultError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::InputBundle(source) => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<input_bundle::InputBundleError> for AnalysisResultError {
    fn from(value: input_bundle::InputBundleError) -> Self {
        Self::InputBundle(Box::new(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "biogeo-analysis-result-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn result_round_trip_and_required_input_check() {
        let root = temp_dir("roundtrip");
        let input_dir = root.with_extension("inputs");
        fs::create_dir_all(&input_dir).unwrap();
        let tree = input_dir.join("tree.nwk");
        fs::write(&tree, "(A:1,B:1);\n").unwrap();
        let result_dir = root.join("fit");
        let log_likelihood = f64::from_bits(0xbed4_f8bf_d7be_fbdb);
        let min_branch_length = 1.234_567_890_123_456_7e-20_f64;
        let missing_branch_length_fill = 9.876_543_210_987_654e-21_f64;
        write_analysis_result(
            &result_dir,
            &AnalysisResultWriteRequest {
                mode: "optimize",
                log_likelihood,
                model_fingerprint: "0123456789abcdef",
                tip_observation_model: "exact_ranges",
                tree_name: Some("tree\t一"),
                max_range_size: 2,
                include_null_range: true,
                root_prior: "flat",
                min_branch_length,
                missing_branch_length_fill: Some(missing_branch_length_fill),
                states: 4,
                areas: 2,
                tips: 2,
                optimization: Some(AnalysisOptimizationSummary {
                    converged: true,
                    iterations: 12,
                    evaluations: 24,
                    starts: 2,
                    converged_starts: 2,
                }),
                source_parameters: "source\n",
                resolved_parameters: "resolved\n",
                inputs: vec![AnalysisInputSpec {
                    role: "tree",
                    path: &tree,
                    required_for_replay: true,
                }],
            },
        )
        .unwrap();

        let loaded = load_analysis_result(&result_dir).unwrap();
        assert_eq!(loaded.format_version, ANALYSIS_RESULT_FORMAT_VERSION);
        assert!(loaded.is_portable());
        assert_eq!(
            loaded.manifest.log_likelihood.to_bits(),
            log_likelihood.to_bits()
        );
        assert_eq!(
            loaded.manifest.min_branch_length.to_bits(),
            min_branch_length.to_bits()
        );
        assert_eq!(
            loaded.manifest.missing_branch_length_fill.map(f64::to_bits),
            Some(missing_branch_length_fill.to_bits())
        );
        assert_eq!(loaded.source_parameters, "source\n");
        assert_eq!(loaded.resolved_parameters, "resolved\n");
        assert_eq!(loaded.manifest.tree_name.as_deref(), Some("tree\t一"));
        assert_eq!(
            loaded.manifest.optimization,
            Some(AnalysisOptimizationSummary {
                converged: true,
                iterations: 12,
                evaluations: 24,
                starts: 2,
                converged_starts: 2,
            })
        );
        loaded.verify_replay_inputs().unwrap();

        fs::write(&tree, "(A:2,B:2);\n").unwrap();
        loaded.verify_replay_inputs().unwrap();
        fs::remove_dir_all(input_dir).unwrap();
        loaded.verify_replay_inputs().unwrap();
        let bundled_tree = loaded.require_input_path("tree").unwrap().to_path_buf();
        fs::write(&bundled_tree, "(A:3,B:3);\n").unwrap();
        assert!(matches!(
            loaded.verify_replay_inputs(),
            Err(AnalysisResultError::InputChanged { role, .. }) if role == "tree"
        ));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn percent_encoding_round_trips_utf8_and_delimiters() {
        let original = "C:\\资料\\a%b\tline\n.tsv";
        assert_eq!(decode_field(&encode_field(original)).unwrap(), original);
    }

    #[test]
    fn legacy_v1_migrates_to_a_relocatable_v2_result() {
        let root = temp_dir("migration");
        let source_dir = root.join("source");
        fs::create_dir_all(&source_dir).unwrap();
        let tree = source_dir.join("tree.nwk");
        fs::write(&tree, "(A:1,B:1);\n").unwrap();
        let legacy_dir = root.join("legacy");
        write_legacy_analysis_result_for_test(
            &legacy_dir,
            &AnalysisResultWriteRequest {
                mode: "evaluate",
                log_likelihood: -1.25,
                model_fingerprint: "0123456789abcdef",
                tip_observation_model: "exact_ranges",
                tree_name: None,
                max_range_size: 1,
                include_null_range: false,
                root_prior: "flat",
                min_branch_length: 0.0,
                missing_branch_length_fill: None,
                states: 1,
                areas: 1,
                tips: 2,
                optimization: None,
                source_parameters: "source\n",
                resolved_parameters: "resolved\n",
                inputs: vec![AnalysisInputSpec {
                    role: "tree",
                    path: &tree,
                    required_for_replay: true,
                }],
            },
        )
        .unwrap();
        let legacy = load_analysis_result(&legacy_dir).unwrap();
        assert_eq!(legacy.format_version, LEGACY_ANALYSIS_RESULT_FORMAT_VERSION);
        assert!(!legacy.is_portable());

        let portable_dir = root.join("portable");
        let portable = migrate_analysis_result(&legacy, &portable_dir).unwrap();
        assert_eq!(portable.format_version, ANALYSIS_RESULT_FORMAT_VERSION);
        assert!(portable.is_portable());
        fs::remove_dir_all(source_dir).unwrap();
        portable.verify_replay_inputs().unwrap();

        let relocated_dir = root.join("relocated");
        fs::rename(&portable_dir, &relocated_dir).unwrap();
        let relocated = load_analysis_result(&relocated_dir).unwrap();
        relocated.verify_replay_inputs().unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_write_removes_the_staging_directory() {
        let root = temp_dir("atomic-failure");
        let result_dir = root.join("fit");
        let missing_input = root.join("missing.nwk");
        let error = write_analysis_result(
            &result_dir,
            &AnalysisResultWriteRequest {
                mode: "evaluate",
                log_likelihood: -1.0,
                model_fingerprint: "0123456789abcdef",
                tip_observation_model: "exact_ranges",
                tree_name: None,
                max_range_size: 1,
                include_null_range: false,
                root_prior: "flat",
                min_branch_length: 0.0,
                missing_branch_length_fill: None,
                states: 1,
                areas: 1,
                tips: 1,
                optimization: None,
                source_parameters: "source\n",
                resolved_parameters: "resolved\n",
                inputs: vec![AnalysisInputSpec {
                    role: "tree",
                    path: &missing_input,
                    required_for_replay: true,
                }],
            },
        )
        .unwrap_err();

        assert!(matches!(
            error,
            AnalysisResultError::InputBundle(source)
                if matches!(
                    source.as_ref(),
                    input_bundle::InputBundleError::Io { path, .. } if path == &missing_input
                )
        ));
        assert!(!result_dir.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        fs::remove_dir(root).unwrap();
    }
}
