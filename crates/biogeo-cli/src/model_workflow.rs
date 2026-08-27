use crate::analysis_result;
use crate::model_batch;
use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MODEL_WORKFLOW_REQUEST_FORMAT: &str = "biogeo-model-workflow-request-v1";
pub const MODEL_WORKFLOW_PLAN_FORMAT: &str = "biogeo-model-workflow-plan-v1";
pub const MODEL_WORKFLOW_RUN_FORMAT: &str = "biogeo-model-workflow-run-v1";
pub const MODEL_WORKFLOW_RESULT_FORMAT: &str = "biogeo-model-workflow-result-v1";
pub const MODEL_WORKFLOW_SELECTION_FORMAT: &str = "biogeo-model-workflow-selection-v1";
pub const MODEL_WORKFLOW_COMPLETION_FORMAT: &str = "biogeo-model-workflow-completion-v1";

pub const SOURCE_REQUEST_FILE: &str = "source-request.tsv";
pub const SOURCE_MODELS_FILE: &str = "source-models.tsv";
pub const SOURCE_CONFIG_FILE: &str = "source-model-config.tsv";
pub const METADATA_FILE: &str = "metadata.tsv";
pub const MODEL_BATCH_DIRECTORY: &str = "model-batch";
pub const SELECTION_FILE: &str = "selection.tsv";
pub const BSM_RESULT_DIRECTORY: &str = "bsm-result";
pub const COMPLETE_FILE: &str = "complete.tsv";

const RESUMABLE_EXECUTION_KEYS: &[&str] = &[
    "bsm_threads",
    "bsm_max_in_flight",
    "bsm_max_events_total",
    "bsm_memory_budget_mb",
    "bsm_checkpoint_samples",
    "bsm_time_limit_seconds",
    "bsm_interactive",
    "bsm_deep_inspection",
];

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparisonCriterion {
    Aic,
    Aicc,
}

impl ComparisonCriterion {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aic => "aic",
            Self::Aicc => "aicc",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BsmSelection {
    None,
    ModelId(String),
    BestByCriterion,
}

impl BsmSelection {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ModelId(_) => "model_id",
            Self::BestByCriterion => "best_by_criterion",
        }
    }

    pub fn requested_model_id(&self) -> Option<&str> {
        match self {
            Self::ModelId(model_id) => Some(model_id),
            Self::None | Self::BestByCriterion => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowBsmOutputLevel {
    Legacy,
    Full,
    Compact,
    Summary,
}

impl WorkflowBsmOutputLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Summary => "summary",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkflowBsmThreads {
    Auto,
    Fixed(usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct WorkflowBsmConfig {
    pub samples: usize,
    pub output_level: WorkflowBsmOutputLevel,
    pub threads: WorkflowBsmThreads,
    pub max_in_flight: Option<usize>,
    pub max_events_per_sample: Option<usize>,
    pub max_events_total: Option<usize>,
    pub memory_budget_mb: Option<usize>,
    pub shard_samples: Option<usize>,
    pub checkpoint_samples: Option<usize>,
    pub time_limit_seconds: Option<f64>,
    pub interactive: bool,
    pub deep_inspection: bool,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedModelWorkflowRequest {
    pub models_manifest_path: PathBuf,
    pub model_config_path: PathBuf,
    pub request_paths_portable: bool,
    pub comparison_criterion: ComparisonCriterion,
    pub bsm_selection: BsmSelection,
    pub bsm: Option<WorkflowBsmConfig>,
}

#[derive(Clone, Debug)]
pub struct LoadedModelWorkflowRequest {
    pub source_request: String,
    pub source_models: String,
    pub source_config: String,
    pub parsed: ParsedModelWorkflowRequest,
}

impl LoadedModelWorkflowRequest {
    pub fn request_fingerprint(&self) -> String {
        request_identity_fingerprint(&self.source_request)
    }

    pub fn models_fingerprint(&self) -> String {
        analysis_result::stable_fingerprint(self.source_models.as_bytes())
    }

    pub fn config_fingerprint(&self) -> String {
        analysis_result::stable_fingerprint(self.source_config.as_bytes())
    }
}

pub fn load_model_workflow_request(
    request_path: &Path,
) -> Result<LoadedModelWorkflowRequest, ModelWorkflowError> {
    let source_request = read_utf8(request_path)?;
    let parsed = parse_model_workflow_request(&source_request, request_path)?;
    let source_models = read_utf8(&parsed.models_manifest_path)?;
    let source_config = read_utf8(&parsed.model_config_path)?;
    Ok(LoadedModelWorkflowRequest {
        source_request,
        source_models,
        source_config,
        parsed,
    })
}

pub fn parse_model_workflow_request(
    input: &str,
    request_path: &Path,
) -> Result<ParsedModelWorkflowRequest, ModelWorkflowError> {
    let values = parse_key_value_table(input, request_path)?;
    let format = required(&values, "format", request_path)?;
    if format != MODEL_WORKFLOW_REQUEST_FORMAT {
        return Err(invalid_request(
            request_path,
            format!("expected format {MODEL_WORKFLOW_REQUEST_FORMAT:?}, found {format:?}"),
        ));
    }
    reject_unknown_keys(&values, request_path)?;

    let base_dir = request_path.parent().unwrap_or_else(|| Path::new("."));
    let (models_manifest_path, models_portable) =
        resolve_path(required(&values, "models", request_path)?, base_dir);
    let (model_config_path, config_portable) =
        resolve_path(required(&values, "config", request_path)?, base_dir);
    let comparison_criterion = match required(&values, "comparison_criterion", request_path)? {
        "aic" => ComparisonCriterion::Aic,
        "aicc" => ComparisonCriterion::Aicc,
        value => {
            return Err(invalid_request(
                request_path,
                format!("comparison_criterion must be aic or aicc, found {value:?}"),
            ));
        }
    };
    let selection_name = required(&values, "bsm_selection", request_path)?;
    let bsm_selection = match selection_name {
        "none" => BsmSelection::None,
        "model_id" => {
            let model_id = required(&values, "bsm_model_id", request_path)?;
            if !model_batch::is_portable_id(model_id) {
                return Err(invalid_request(
                    request_path,
                    format!("bsm_model_id {model_id:?} is not a portable model identifier"),
                ));
            }
            BsmSelection::ModelId(model_id.to_string())
        }
        "best_by_criterion" => BsmSelection::BestByCriterion,
        value => {
            return Err(invalid_request(
                request_path,
                format!(
                    "bsm_selection must be none, model_id, or best_by_criterion, found {value:?}"
                ),
            ));
        }
    };

    if selection_name != "model_id" && values.contains_key("bsm_model_id") {
        return Err(invalid_request(
            request_path,
            "bsm_model_id is valid only when bsm_selection=model_id",
        ));
    }

    let bsm = if bsm_selection == BsmSelection::None {
        if let Some(key) = values
            .keys()
            .find(|key| key.starts_with("bsm_") && key.as_str() != "bsm_selection")
        {
            return Err(invalid_request(
                request_path,
                format!("{key} must be omitted when bsm_selection=none"),
            ));
        }
        None
    } else {
        let samples = parse_required_usize(&values, "bsm_samples", request_path)?;
        if samples == 0 {
            return Err(invalid_request(
                request_path,
                "bsm_samples must be greater than zero when BSM is enabled",
            ));
        }
        let output_level = match optional(&values, "bsm_output_level", request_path)?
            .unwrap_or("compact")
        {
            "legacy" => WorkflowBsmOutputLevel::Legacy,
            "full" => WorkflowBsmOutputLevel::Full,
            "compact" => WorkflowBsmOutputLevel::Compact,
            "summary" => WorkflowBsmOutputLevel::Summary,
            value => {
                return Err(invalid_request(
                    request_path,
                    format!(
                        "bsm_output_level must be legacy, full, compact, or summary, found {value:?}"
                    ),
                ));
            }
        };
        let threads = match optional(&values, "bsm_threads", request_path)?.unwrap_or("auto") {
            "auto" => WorkflowBsmThreads::Auto,
            value => {
                let threads = parse_usize_value(value, "bsm_threads", request_path)?;
                if threads == 0 {
                    return Err(invalid_request(
                        request_path,
                        "bsm_threads must be auto or a positive integer",
                    ));
                }
                WorkflowBsmThreads::Fixed(threads)
            }
        };
        let max_in_flight =
            parse_optional_positive_usize(&values, "bsm_max_in_flight", request_path)?;
        let max_events_per_sample =
            parse_optional_usize(&values, "bsm_max_events_per_sample", request_path)?;
        let max_events_total = parse_optional_usize(&values, "bsm_max_events_total", request_path)?;
        let memory_budget_mb =
            parse_optional_positive_usize(&values, "bsm_memory_budget_mb", request_path)?;
        if memory_budget_mb.is_some() && max_events_per_sample.is_none() {
            return Err(invalid_request(
                request_path,
                "bsm_memory_budget_mb requires bsm_max_events_per_sample",
            ));
        }
        let shard_samples =
            parse_optional_positive_usize(&values, "bsm_shard_samples", request_path)?;
        let checkpoint_samples =
            parse_optional_positive_usize(&values, "bsm_checkpoint_samples", request_path)?;
        let time_limit_seconds =
            parse_optional_nonnegative_f64(&values, "bsm_time_limit_seconds", request_path)?;
        let interactive = parse_optional_bool(&values, "bsm_interactive", false, request_path)?;
        let deep_inspection =
            parse_optional_bool(&values, "bsm_deep_inspection", false, request_path)?;
        let seed = match optional(&values, "bsm_seed", request_path)? {
            Some(value) => value.parse::<u64>().map_err(|_| {
                invalid_request(
                    request_path,
                    format!("bsm_seed must be an unsigned 64-bit integer, found {value:?}"),
                )
            })?,
            None => 1,
        };
        Some(WorkflowBsmConfig {
            samples,
            output_level,
            threads,
            max_in_flight,
            max_events_per_sample,
            max_events_total,
            memory_budget_mb,
            shard_samples,
            checkpoint_samples,
            time_limit_seconds,
            interactive,
            deep_inspection,
            seed,
        })
    };

    Ok(ParsedModelWorkflowRequest {
        models_manifest_path,
        model_config_path,
        request_paths_portable: models_portable && config_portable,
        comparison_criterion,
        bsm_selection,
        bsm,
    })
}

#[derive(Clone, Debug)]
pub struct ModelWorkflowWorkspace {
    root: PathBuf,
}

impl ModelWorkflowWorkspace {
    pub fn prepare(
        loaded: &LoadedModelWorkflowRequest,
        output_dir: &Path,
        resume: bool,
    ) -> Result<Self, ModelWorkflowError> {
        let metadata = format_metadata(loaded);
        if resume {
            if !output_dir.is_dir() {
                return Err(ModelWorkflowError::MissingOutputDirectory(
                    output_dir.to_path_buf(),
                ));
            }
            validate_workspace_entries(output_dir, loaded.parsed.bsm.is_some())?;
            verify_resume_compatible_request(
                &output_dir.join(SOURCE_REQUEST_FILE),
                &loaded.source_request,
            )?;
            verify_file(
                &output_dir.join(SOURCE_MODELS_FILE),
                loaded.source_models.as_bytes(),
            )?;
            verify_file(
                &output_dir.join(SOURCE_CONFIG_FILE),
                loaded.source_config.as_bytes(),
            )?;
            verify_file(&output_dir.join(METADATA_FILE), metadata.as_bytes())?;
        } else {
            initialize_workspace(output_dir, loaded, &metadata)?;
        }
        Ok(Self {
            root: output_dir.to_path_buf(),
        })
    }

    pub fn model_batch_dir(&self) -> PathBuf {
        self.root.join(MODEL_BATCH_DIRECTORY)
    }

    pub fn bsm_result_dir(&self) -> PathBuf {
        self.root.join(BSM_RESULT_DIRECTORY)
    }

    pub fn selection_path(&self) -> PathBuf {
        self.root.join(SELECTION_FILE)
    }

    pub fn publish_selection(&self, selection: &str) -> Result<(), ModelWorkflowError> {
        write_once_or_verify(&self.selection_path(), selection.as_bytes())
    }

    pub fn publish_completion(&self, completion: &str) -> Result<(), ModelWorkflowError> {
        write_once_or_verify(&self.root.join(COMPLETE_FILE), completion.as_bytes())
    }
}

fn request_identity_fingerprint(source: &str) -> String {
    let mut identity = String::with_capacity(source.len());
    for segment in source.split_inclusive('\n') {
        let record = segment.strip_suffix('\n').unwrap_or(segment);
        let record = record.strip_suffix('\r').unwrap_or(record);
        let key = record.split_once('\t').map_or(record, |(key, _)| key);
        if !RESUMABLE_EXECUTION_KEYS.contains(&key) {
            identity.push_str(segment);
        }
    }
    analysis_result::stable_fingerprint(identity.as_bytes())
}

fn verify_resume_compatible_request(
    path: &Path,
    current_source: &str,
) -> Result<(), ModelWorkflowError> {
    let stored_source = read_utf8(path)?;
    let expected = request_identity_fingerprint(current_source);
    let actual = request_identity_fingerprint(&stored_source);
    if actual == expected {
        Ok(())
    } else {
        Err(ModelWorkflowError::IdentityMismatch {
            path: path.to_path_buf(),
            expected,
            actual,
        })
    }
}

fn format_metadata(loaded: &LoadedModelWorkflowRequest) -> String {
    let parsed = &loaded.parsed;
    format!(
        "key\tvalue\nformat\t{}\nrequest_format\t{}\nrequest_fingerprint\t{}\nmodels_manifest_format\t{}\nmodels_fingerprint\t{}\nmodel_config_format\tbiogeo-model-batch-config-v1\nconfig_fingerprint\t{}\nrequest_paths_portable\t{}\ncomparison_criterion\t{}\nbsm_enabled\t{}\nbsm_selection\t{}\nbsm_requested_model_id\t{}\nsource_request_file\t{}\nsource_models_file\t{}\nsource_model_config_file\t{}\nmodel_batch_dir\t{}\nselection_file\t{}\nbsm_result_dir\t{}\ncompletion_file\t{}\n",
        MODEL_WORKFLOW_RESULT_FORMAT,
        MODEL_WORKFLOW_REQUEST_FORMAT,
        loaded.request_fingerprint(),
        model_batch::MODEL_BATCH_MANIFEST_FORMAT,
        loaded.models_fingerprint(),
        loaded.config_fingerprint(),
        parsed.request_paths_portable,
        parsed.comparison_criterion.as_str(),
        parsed.bsm.is_some(),
        parsed.bsm_selection.as_str(),
        parsed.bsm_selection.requested_model_id().unwrap_or("none"),
        SOURCE_REQUEST_FILE,
        SOURCE_MODELS_FILE,
        SOURCE_CONFIG_FILE,
        MODEL_BATCH_DIRECTORY,
        SELECTION_FILE,
        if parsed.bsm.is_some() {
            BSM_RESULT_DIRECTORY
        } else {
            "none"
        },
        COMPLETE_FILE,
    )
}

fn initialize_workspace(
    output_dir: &Path,
    loaded: &LoadedModelWorkflowRequest,
    metadata: &str,
) -> Result<(), ModelWorkflowError> {
    if output_dir.exists() {
        return Err(ModelWorkflowError::OutputDirectoryExists(
            output_dir.to_path_buf(),
        ));
    }
    let parent = output_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| ModelWorkflowError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let file_name = output_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("model-workflow");
    let staging = parent.join(format!(
        ".{file_name}.staging-{}-{sequence}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(ModelWorkflowError::OutputDirectoryExists(staging));
    }

    let result = (|| {
        fs::create_dir(&staging).map_err(|source| ModelWorkflowError::Io {
            path: staging.clone(),
            source,
        })?;
        write_new(
            &staging.join(SOURCE_REQUEST_FILE),
            loaded.source_request.as_bytes(),
        )?;
        write_new(
            &staging.join(SOURCE_MODELS_FILE),
            loaded.source_models.as_bytes(),
        )?;
        write_new(
            &staging.join(SOURCE_CONFIG_FILE),
            loaded.source_config.as_bytes(),
        )?;
        write_new(&staging.join(METADATA_FILE), metadata.as_bytes())?;
        crate::fs_retry::rename(&staging, output_dir).map_err(|source| ModelWorkflowError::Io {
            path: output_dir.to_path_buf(),
            source,
        })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

fn validate_workspace_entries(root: &Path, bsm_enabled: bool) -> Result<(), ModelWorkflowError> {
    let allowed = [
        SOURCE_REQUEST_FILE,
        SOURCE_MODELS_FILE,
        SOURCE_CONFIG_FILE,
        METADATA_FILE,
        MODEL_BATCH_DIRECTORY,
        SELECTION_FILE,
        BSM_RESULT_DIRECTORY,
        COMPLETE_FILE,
    ]
    .into_iter()
    .collect::<HashSet<_>>();
    let mut entries = Vec::new();
    for entry in fs::read_dir(root).map_err(|source| ModelWorkflowError::Io {
        path: root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ModelWorkflowError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !allowed.contains(name.as_str()) {
            entries.push(name);
        }
    }
    if !entries.is_empty() {
        entries.sort();
        return Err(ModelWorkflowError::InvalidWorkspace {
            path: root.to_path_buf(),
            message: format!("unexpected root entries: {entries:?}"),
        });
    }
    for name in [
        SOURCE_REQUEST_FILE,
        SOURCE_MODELS_FILE,
        SOURCE_CONFIG_FILE,
        METADATA_FILE,
    ] {
        if !root.join(name).is_file() {
            return Err(ModelWorkflowError::InvalidWorkspace {
                path: root.join(name),
                message: "required workflow identity file is missing".to_string(),
            });
        }
    }
    let model_batch = root.join(MODEL_BATCH_DIRECTORY);
    let selection = root.join(SELECTION_FILE);
    let bsm_result = root.join(BSM_RESULT_DIRECTORY);
    let completion = root.join(COMPLETE_FILE);
    if model_batch.exists() && !model_batch.is_dir() {
        return Err(invalid_workspace_path(&model_batch, "expected a directory"));
    }
    if selection.exists() && !selection.is_file() {
        return Err(invalid_workspace_path(&selection, "expected a file"));
    }
    if bsm_result.exists() && !bsm_result.is_dir() {
        return Err(invalid_workspace_path(&bsm_result, "expected a directory"));
    }
    if completion.exists() && !completion.is_file() {
        return Err(invalid_workspace_path(&completion, "expected a file"));
    }
    if selection.exists() && !model_batch.is_dir() {
        return Err(invalid_workspace_path(
            root,
            "selection.tsv exists without model-batch",
        ));
    }
    if bsm_result.exists() && !selection.is_file() {
        return Err(invalid_workspace_path(
            root,
            "bsm-result exists without selection.tsv",
        ));
    }
    if !bsm_enabled && bsm_result.exists() {
        return Err(invalid_workspace_path(
            root,
            "bsm-result is forbidden because this request has bsm_selection=none",
        ));
    }
    if completion.exists() && (!model_batch.is_dir() || !selection.is_file()) {
        return Err(invalid_workspace_path(
            root,
            "complete.tsv exists before model-batch and selection.tsv",
        ));
    }
    if completion.exists() && bsm_enabled && !bsm_result.is_dir() {
        return Err(invalid_workspace_path(
            root,
            "complete.tsv exists without the requested bsm-result",
        ));
    }
    Ok(())
}

fn invalid_workspace_path(path: &Path, message: &str) -> ModelWorkflowError {
    ModelWorkflowError::InvalidWorkspace {
        path: path.to_path_buf(),
        message: message.to_string(),
    }
}

fn parse_key_value_table(
    input: &str,
    request_path: &Path,
) -> Result<BTreeMap<String, String>, ModelWorkflowError> {
    let mut lines = input.lines().enumerate().filter_map(|(index, line)| {
        let line = line.trim_end_matches('\r');
        (!line.is_empty() && !line.starts_with('#')).then_some((index + 1, line))
    });
    let (header_line, header) = lines
        .next()
        .ok_or_else(|| invalid_request(request_path, "request is empty"))?;
    if header.trim_start_matches('\u{feff}') != "key\tvalue" {
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
) -> Result<(), ModelWorkflowError> {
    const KEYS: &[&str] = &[
        "format",
        "models",
        "config",
        "comparison_criterion",
        "bsm_selection",
        "bsm_model_id",
        "bsm_samples",
        "bsm_output_level",
        "bsm_threads",
        "bsm_max_in_flight",
        "bsm_max_events_per_sample",
        "bsm_max_events_total",
        "bsm_memory_budget_mb",
        "bsm_shard_samples",
        "bsm_checkpoint_samples",
        "bsm_time_limit_seconds",
        "bsm_interactive",
        "bsm_deep_inspection",
        "bsm_seed",
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
) -> Result<&'a str, ModelWorkflowError> {
    values
        .get(key)
        .filter(|value| !value.is_empty())
        .map(String::as_str)
        .ok_or_else(|| invalid_request(request_path, format!("missing non-empty key {key:?}")))
}

fn optional<'a>(
    values: &'a BTreeMap<String, String>,
    key: &'static str,
    request_path: &Path,
) -> Result<Option<&'a str>, ModelWorkflowError> {
    values
        .get(key)
        .map(|value| {
            if value.is_empty() {
                Err(invalid_request(
                    request_path,
                    format!("{key} must be omitted instead of left empty"),
                ))
            } else {
                Ok(value.as_str())
            }
        })
        .transpose()
}

fn resolve_path(raw: &str, base_dir: &Path) -> (PathBuf, bool) {
    let path = PathBuf::from(raw);
    let portable = !path.is_absolute()
        && !path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    (resolved, portable)
}

fn parse_required_usize(
    values: &BTreeMap<String, String>,
    key: &'static str,
    request_path: &Path,
) -> Result<usize, ModelWorkflowError> {
    parse_usize_value(required(values, key, request_path)?, key, request_path)
}

fn parse_optional_usize(
    values: &BTreeMap<String, String>,
    key: &'static str,
    request_path: &Path,
) -> Result<Option<usize>, ModelWorkflowError> {
    optional(values, key, request_path)?
        .map(|value| parse_usize_value(value, key, request_path))
        .transpose()
}

fn parse_optional_positive_usize(
    values: &BTreeMap<String, String>,
    key: &'static str,
    request_path: &Path,
) -> Result<Option<usize>, ModelWorkflowError> {
    let value = parse_optional_usize(values, key, request_path)?;
    if value == Some(0) {
        return Err(invalid_request(
            request_path,
            format!("{key} must be greater than zero"),
        ));
    }
    Ok(value)
}

fn parse_usize_value(
    value: &str,
    key: &'static str,
    request_path: &Path,
) -> Result<usize, ModelWorkflowError> {
    value.parse::<usize>().map_err(|_| {
        invalid_request(
            request_path,
            format!("{key} must be a non-negative integer, found {value:?}"),
        )
    })
}

fn parse_optional_nonnegative_f64(
    values: &BTreeMap<String, String>,
    key: &'static str,
    request_path: &Path,
) -> Result<Option<f64>, ModelWorkflowError> {
    optional(values, key, request_path)?
        .map(|value| {
            let parsed = value.parse::<f64>().map_err(|_| {
                invalid_request(
                    request_path,
                    format!("{key} must be a finite non-negative number, found {value:?}"),
                )
            })?;
            if !parsed.is_finite() || parsed < 0.0 {
                return Err(invalid_request(
                    request_path,
                    format!("{key} must be a finite non-negative number, found {value:?}"),
                ));
            }
            Ok(parsed)
        })
        .transpose()
}

fn parse_optional_bool(
    values: &BTreeMap<String, String>,
    key: &'static str,
    default: bool,
    request_path: &Path,
) -> Result<bool, ModelWorkflowError> {
    match optional(values, key, request_path)? {
        None => Ok(default),
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => Err(invalid_request(
            request_path,
            format!("{key} must be true or false, found {value:?}"),
        )),
    }
}

fn read_utf8(path: &Path) -> Result<String, ModelWorkflowError> {
    fs::read_to_string(path).map_err(|source| ModelWorkflowError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ModelWorkflowError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| ModelWorkflowError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(bytes)
        .map_err(|source| ModelWorkflowError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| ModelWorkflowError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_once_or_verify(path: &Path, bytes: &[u8]) -> Result<(), ModelWorkflowError> {
    if path.exists() {
        verify_file(path, bytes)
    } else {
        write_new(path, bytes)
    }
}

fn verify_file(path: &Path, expected: &[u8]) -> Result<(), ModelWorkflowError> {
    let actual = fs::read(path).map_err(|source| ModelWorkflowError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if actual == expected {
        Ok(())
    } else {
        Err(ModelWorkflowError::IdentityMismatch {
            path: path.to_path_buf(),
            expected: analysis_result::stable_fingerprint(expected),
            actual: analysis_result::stable_fingerprint(&actual),
        })
    }
}

fn invalid_request(path: &Path, message: impl Into<String>) -> ModelWorkflowError {
    ModelWorkflowError::InvalidRequest {
        path: path.to_path_buf(),
        line: None,
        message: message.into(),
    }
}

fn invalid_request_at(path: &Path, line: usize, message: impl Into<String>) -> ModelWorkflowError {
    ModelWorkflowError::InvalidRequest {
        path: path.to_path_buf(),
        line: Some(line),
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum ModelWorkflowError {
    InvalidRequest {
        path: PathBuf,
        line: Option<usize>,
        message: String,
    },
    OutputDirectoryExists(PathBuf),
    MissingOutputDirectory(PathBuf),
    InvalidWorkspace {
        path: PathBuf,
        message: String,
    },
    IdentityMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    UnknownBsmModelId(String),
    IneligibleBsmModel(String),
    MissingBestModel(ComparisonCriterion),
    TiedBestModels {
        criterion: ComparisonCriterion,
        models: Vec<String>,
    },
    UnsafeAnalysisResultPath(String),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for ModelWorkflowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest {
                path,
                line: Some(line),
                message,
            } => write!(
                formatter,
                "invalid model-workflow request {} on line {}: {}",
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
                "invalid model-workflow request {}: {}",
                path.display(),
                message
            ),
            Self::OutputDirectoryExists(path) => write!(
                formatter,
                "model-workflow output already exists; use --resume to continue {}",
                path.display()
            ),
            Self::MissingOutputDirectory(path) => write!(
                formatter,
                "cannot resume because model-workflow output does not exist: {}",
                path.display()
            ),
            Self::InvalidWorkspace { path, message } => write!(
                formatter,
                "invalid model-workflow workspace {}: {}",
                path.display(),
                message
            ),
            Self::IdentityMismatch {
                path,
                expected,
                actual,
            } => write!(
                formatter,
                "model-workflow identity file {} does not match (expected {}, found {})",
                path.display(),
                expected,
                actual
            ),
            Self::UnknownBsmModelId(model_id) => write!(
                formatter,
                "bsm_model_id {model_id:?} is not an exact model_id in the candidate manifest"
            ),
            Self::IneligibleBsmModel(model_id) => write!(
                formatter,
                "model {model_id:?} is not eligible for BSM because optimization did not converge from any start"
            ),
            Self::MissingBestModel(criterion) => write!(
                formatter,
                "no eligible model has a defined {} score; choose an explicit eligible model_id or revise the candidate set",
                criterion.as_str()
            ),
            Self::TiedBestModels { criterion, models } => write!(
                formatter,
                "{} has multiple rank-1 models ({}); choose bsm_selection=model_id explicitly",
                criterion.as_str(),
                models.join(", ")
            ),
            Self::UnsafeAnalysisResultPath(path) => write!(
                formatter,
                "model comparison returned an unsafe analysis_result path {path:?}"
            ),
            Self::Io { path, source } => {
                write!(formatter, "I/O failed for {}: {}", path.display(), source)
            }
        }
    }
}

impl Error for ModelWorkflowError {
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
    fn parses_explicit_bsm_request_with_defaults() {
        let request = "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tconfig.tsv\n\
comparison_criterion\taicc\n\
bsm_selection\tmodel_id\n\
bsm_model_id\tDEC+J\n\
bsm_samples\t100\n";
        let parsed = parse_model_workflow_request(request, Path::new("project/workflow.tsv"))
            .expect("request should parse");
        assert!(parsed.request_paths_portable);
        assert_eq!(parsed.comparison_criterion, ComparisonCriterion::Aicc);
        assert_eq!(
            parsed.bsm_selection,
            BsmSelection::ModelId("DEC+J".to_string())
        );
        let bsm = parsed.bsm.unwrap();
        assert_eq!(bsm.samples, 100);
        assert_eq!(bsm.output_level, WorkflowBsmOutputLevel::Compact);
        assert_eq!(bsm.threads, WorkflowBsmThreads::Auto);
        assert_eq!(bsm.seed, 1);
    }

    #[test]
    fn rejects_inactive_bsm_settings_and_ambiguous_model_target() {
        let inactive = "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tconfig.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tnone\n\
bsm_samples\t10\n";
        assert!(matches!(
            parse_model_workflow_request(inactive, Path::new("workflow.tsv")),
            Err(ModelWorkflowError::InvalidRequest { .. })
        ));

        let ambiguous = "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tconfig.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tbest_by_criterion\n\
bsm_model_id\tDEC\n\
bsm_samples\t10\n";
        assert!(matches!(
            parse_model_workflow_request(ambiguous, Path::new("workflow.tsv")),
            Err(ModelWorkflowError::InvalidRequest { .. })
        ));
    }

    #[test]
    fn parent_paths_are_valid_but_mark_the_request_nonportable() {
        let request = "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\t../models.tsv\n\
config\tconfig.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tnone\n";
        let parsed =
            parse_model_workflow_request(request, Path::new("project/workflow.tsv")).unwrap();
        assert!(!parsed.request_paths_portable);
    }

    #[test]
    fn request_identity_allows_only_bsm_execution_controls_to_change() {
        let original = "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tconfig.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tmodel_id\n\
bsm_model_id\tDEC\n\
bsm_samples\t100\n\
bsm_output_level\tcompact\n\
bsm_threads\t1\n\
bsm_max_events_per_sample\t1000\n\
bsm_shard_samples\t10\n\
bsm_time_limit_seconds\t0\n\
bsm_deep_inspection\tfalse\n\
bsm_seed\t7\n";
        let changed_execution = original
            .replace("bsm_threads\t1", "bsm_threads\t16")
            .replace("bsm_time_limit_seconds\t0", "bsm_time_limit_seconds\t3600")
            .replace("bsm_deep_inspection\tfalse", "bsm_deep_inspection\ttrue");
        assert_eq!(
            request_identity_fingerprint(original),
            request_identity_fingerprint(&changed_execution)
        );

        let changed_samples = original.replace("bsm_samples\t100", "bsm_samples\t101");
        assert_ne!(
            request_identity_fingerprint(original),
            request_identity_fingerprint(&changed_samples)
        );
        let changed_layout = original.replace("bsm_shard_samples\t10", "bsm_shard_samples\t20");
        assert_ne!(
            request_identity_fingerprint(original),
            request_identity_fingerprint(&changed_layout)
        );
        assert_ne!(
            request_identity_fingerprint(original),
            request_identity_fingerprint(&format!("{original}# audit note\n"))
        );
    }
}
