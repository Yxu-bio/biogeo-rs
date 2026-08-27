use crate::analysis_result;
use crate::model_batch;
use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const DATASET_BATCH_MANIFEST_FORMAT: &str = "biogeo-dataset-batch-manifest-v1";
pub const MODEL_BATCH_CONFIG_FORMAT: &str = "biogeo-model-batch-config-v1";
pub const DATASET_BATCH_RESULT_FORMAT: &str = "biogeo-dataset-batch-result-v1";
pub const DATASET_BATCH_ATTEMPT_FORMAT: &str = "biogeo-dataset-batch-attempt-v2";

const SOURCE_MANIFEST_FILE: &str = "source-manifest.tsv";
const RUN_FILE: &str = "run.tsv";
const JOBS_FILE: &str = "jobs.tsv";
const COMPLETE_FILE: &str = "complete.tsv";
const DATASETS_DIRECTORY: &str = "datasets";
const ATTEMPTS_DIRECTORY: &str = "attempts";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetBatchEntry {
    pub dataset_id: String,
    pub models_manifest_path: PathBuf,
    pub config_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PreparedDatasetBatchJob {
    pub dataset_id: String,
    pub models_manifest_path: PathBuf,
    pub models_manifest_fingerprint: String,
    pub config_path: PathBuf,
    pub config_fingerprint: String,
    pub result_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DatasetBatchJobOutcome {
    Complete,
    Failed { code: String, message: String },
    Cancelled { code: String, message: String },
    NotStarted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatasetBatchJobReport {
    pub dataset_id: String,
    pub outcome: DatasetBatchJobOutcome,
}

impl DatasetBatchJobReport {
    pub fn complete(job: &PreparedDatasetBatchJob) -> Self {
        Self {
            dataset_id: job.dataset_id.clone(),
            outcome: DatasetBatchJobOutcome::Complete,
        }
    }

    pub fn failed(job: &PreparedDatasetBatchJob, code: &str, message: String) -> Self {
        Self {
            dataset_id: job.dataset_id.clone(),
            outcome: DatasetBatchJobOutcome::Failed {
                code: code.to_string(),
                message,
            },
        }
    }

    pub fn cancelled(job: &PreparedDatasetBatchJob, code: &str, message: String) -> Self {
        Self {
            dataset_id: job.dataset_id.clone(),
            outcome: DatasetBatchJobOutcome::Cancelled {
                code: code.to_string(),
                message,
            },
        }
    }

    pub fn not_started(job: &PreparedDatasetBatchJob) -> Self {
        Self {
            dataset_id: job.dataset_id.clone(),
            outcome: DatasetBatchJobOutcome::NotStarted,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DatasetBatchWorkspace {
    output_dir: PathBuf,
    jobs: Vec<PreparedDatasetBatchJob>,
}

impl DatasetBatchWorkspace {
    pub fn jobs(&self) -> &[PreparedDatasetBatchJob] {
        &self.jobs
    }

    pub fn record_attempt(
        &self,
        reports: &[DatasetBatchJobReport],
    ) -> Result<PathBuf, DatasetBatchError> {
        if reports.len() != self.jobs.len()
            || reports
                .iter()
                .zip(&self.jobs)
                .any(|(report, job)| report.dataset_id != job.dataset_id)
        {
            return Err(DatasetBatchError::AttemptJobMismatch);
        }
        let attempts_dir = self.output_dir.join(ATTEMPTS_DIRECTORY);
        fs::create_dir_all(&attempts_dir).map_err(|source| DatasetBatchError::Io {
            path: attempts_dir.clone(),
            source,
        })?;
        let attempt_index = next_attempt_index(&attempts_dir)?;
        let path = attempts_dir.join(format!("attempt-{attempt_index:06}.tsv"));
        write_new(&path, format_attempt(reports).as_bytes())?;
        Ok(path)
    }

    pub fn finalize(&self) -> Result<String, DatasetBatchError> {
        let completion = format!(
            "key\tvalue\nformat\t{}\nstatus\tcomplete\ndatasets\t{}\ndatasets_directory\t{}\n",
            DATASET_BATCH_RESULT_FORMAT,
            self.jobs.len(),
            DATASETS_DIRECTORY,
        );
        write_once_or_verify(&self.output_dir.join(COMPLETE_FILE), completion.as_bytes())?;
        Ok(completion)
    }
}

pub fn parse_dataset_batch_manifest(
    input: &str,
    manifest_path: &Path,
) -> Result<Vec<DatasetBatchEntry>, DatasetBatchError> {
    let mut lines = content_lines(input);
    let (format_line, format) = lines
        .next()
        .ok_or_else(|| invalid_manifest(manifest_path, 1, "manifest is empty"))?;
    let format = format.trim_start_matches('\u{feff}');
    if format != DATASET_BATCH_MANIFEST_FORMAT {
        return Err(invalid_manifest(
            manifest_path,
            format_line,
            format!("expected format {DATASET_BATCH_MANIFEST_FORMAT:?}, found {format:?}"),
        ));
    }
    let (header_line, header) = lines.next().ok_or_else(|| {
        invalid_manifest(
            manifest_path,
            format_line + 1,
            "missing dataset_id/models/config header",
        )
    })?;
    if header != "dataset_id\tmodels\tconfig" {
        return Err(invalid_manifest(
            manifest_path,
            header_line,
            "header must be exactly: dataset_id<TAB>models<TAB>config",
        ));
    }

    let base_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for (line_number, line) in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 3 {
            return Err(invalid_manifest(
                manifest_path,
                line_number,
                format!("expected 3 tab-separated columns, found {}", fields.len()),
            ));
        }
        let dataset_id = fields[0].trim();
        if !model_batch::is_portable_id(dataset_id) {
            return Err(invalid_manifest(
                manifest_path,
                line_number,
                format!(
                    "invalid dataset_id {dataset_id:?}; use a portable ASCII identifier without path separators, a trailing period, or a Windows reserved name"
                ),
            ));
        }
        if !seen.insert(dataset_id.to_ascii_lowercase()) {
            return Err(invalid_manifest(
                manifest_path,
                line_number,
                format!(
                    "duplicate dataset_id {dataset_id:?}; identifiers are compared case-insensitively"
                ),
            ));
        }
        let models_manifest_path =
            resolve_required_path(fields[1], base_dir, manifest_path, line_number, "models")?;
        let config_path =
            resolve_required_path(fields[2], base_dir, manifest_path, line_number, "config")?;
        entries.push(DatasetBatchEntry {
            dataset_id: dataset_id.to_string(),
            models_manifest_path,
            config_path,
        });
    }
    if entries.is_empty() {
        return Err(invalid_manifest(
            manifest_path,
            header_line,
            "manifest contains no dataset rows",
        ));
    }
    Ok(entries)
}

pub fn parse_model_batch_config(
    input: &str,
    config_path: &Path,
) -> Result<Vec<String>, DatasetBatchError> {
    let mut lines = content_lines(input);
    let (format_line, format) = lines
        .next()
        .ok_or_else(|| invalid_config(config_path, 1, "config is empty"))?;
    let format = format.trim_start_matches('\u{feff}');
    if format != MODEL_BATCH_CONFIG_FORMAT {
        return Err(invalid_config(
            config_path,
            format_line,
            format!("expected format {MODEL_BATCH_CONFIG_FORMAT:?}, found {format:?}"),
        ));
    }
    let (header_line, header) = lines.next().ok_or_else(|| {
        invalid_config(config_path, format_line + 1, "missing option/value header")
    })?;
    if header != "option\tvalue" {
        return Err(invalid_config(
            config_path,
            header_line,
            "header must be exactly: option<TAB>value",
        ));
    }

    let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen = HashSet::new();
    let mut arguments = Vec::new();
    for (line_number, line) in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 2 {
            return Err(invalid_config(
                config_path,
                line_number,
                format!("expected 2 tab-separated columns, found {}", fields.len()),
            ));
        }
        let option = fields[0].trim();
        let value = fields[1].trim();
        let kind = config_option_kind(option).ok_or_else(|| {
            invalid_config(
                config_path,
                line_number,
                format!("unsupported model-batch option {option:?}"),
            )
        })?;
        if kind == ConfigOptionKind::Managed {
            return Err(invalid_config(
                config_path,
                line_number,
                format!("option {option} is managed by dataset-batch"),
            ));
        }
        if option != "--additional-start" && !seen.insert(option.to_string()) {
            return Err(invalid_config(
                config_path,
                line_number,
                format!("option {option} was provided more than once"),
            ));
        }
        match kind {
            ConfigOptionKind::Flag => match value {
                "true" => arguments.push(option.to_string()),
                "false" => {}
                _ => {
                    return Err(invalid_config(
                        config_path,
                        line_number,
                        format!("flag {option} requires true or false"),
                    ));
                }
            },
            ConfigOptionKind::Path => {
                if value.is_empty() {
                    return Err(invalid_config(
                        config_path,
                        line_number,
                        format!("path value for {option} is empty"),
                    ));
                }
                let path = PathBuf::from(value);
                let path = if path.is_absolute() {
                    path
                } else {
                    base_dir.join(path)
                };
                let path = path
                    .to_str()
                    .ok_or_else(|| DatasetBatchError::NonUtf8Path(path.clone()))?;
                arguments.extend([option.to_string(), path.to_string()]);
            }
            ConfigOptionKind::Value => {
                if value.is_empty() {
                    return Err(invalid_config(
                        config_path,
                        line_number,
                        format!("value for {option} is empty"),
                    ));
                }
                arguments.extend([option.to_string(), value.to_string()]);
            }
            ConfigOptionKind::Managed => unreachable!("managed options return above"),
        }
    }
    Ok(arguments)
}

pub fn prepare_dataset_batch_workspace(
    manifest_path: &Path,
    manifest_input: &str,
    output_dir: &Path,
    resume: bool,
) -> Result<DatasetBatchWorkspace, DatasetBatchError> {
    let entries = parse_dataset_batch_manifest(manifest_input, manifest_path)?;
    let jobs = prepare_jobs(entries, output_dir)?;
    let manifest_fingerprint = analysis_result::stable_fingerprint(manifest_input.as_bytes());
    let run = format!(
        "key\tvalue\nformat\t{}\nmanifest_format\t{}\nmanifest_fingerprint\t{}\ndatasets\t{}\nsource_manifest_file\t{}\njobs_file\t{}\n",
        DATASET_BATCH_RESULT_FORMAT,
        DATASET_BATCH_MANIFEST_FORMAT,
        manifest_fingerprint,
        jobs.len(),
        SOURCE_MANIFEST_FILE,
        JOBS_FILE,
    );
    let job_table = format_jobs(&jobs)?;

    if resume {
        if !output_dir.is_dir() {
            return Err(DatasetBatchError::MissingOutputDirectory(
                output_dir.to_path_buf(),
            ));
        }
        verify_file(&output_dir.join(RUN_FILE), run.as_bytes())?;
        verify_file(
            &output_dir.join(SOURCE_MANIFEST_FILE),
            manifest_input.as_bytes(),
        )?;
        verify_file(&output_dir.join(JOBS_FILE), job_table.as_bytes())?;
        if !output_dir.join(DATASETS_DIRECTORY).is_dir() {
            return Err(DatasetBatchError::MissingDatasetsDirectory(
                output_dir.join(DATASETS_DIRECTORY),
            ));
        }
    } else {
        initialize_workspace(
            output_dir,
            manifest_input.as_bytes(),
            run.as_bytes(),
            job_table.as_bytes(),
        )?;
    }
    Ok(DatasetBatchWorkspace {
        output_dir: output_dir.to_path_buf(),
        jobs,
    })
}

fn prepare_jobs(
    entries: Vec<DatasetBatchEntry>,
    output_dir: &Path,
) -> Result<Vec<PreparedDatasetBatchJob>, DatasetBatchError> {
    entries
        .into_iter()
        .map(|entry| {
            let models_manifest_path = canonical_file(&entry.models_manifest_path)?;
            let models_manifest =
                fs::read(&models_manifest_path).map_err(|source| DatasetBatchError::Io {
                    path: models_manifest_path.clone(),
                    source,
                })?;
            let config_path = canonical_file(&entry.config_path)?;
            let config = fs::read(&config_path).map_err(|source| DatasetBatchError::Io {
                path: config_path.clone(),
                source,
            })?;
            Ok(PreparedDatasetBatchJob {
                result_path: output_dir.join(DATASETS_DIRECTORY).join(&entry.dataset_id),
                dataset_id: entry.dataset_id,
                models_manifest_path,
                models_manifest_fingerprint: analysis_result::stable_fingerprint(&models_manifest),
                config_path,
                config_fingerprint: analysis_result::stable_fingerprint(&config),
            })
        })
        .collect()
}

fn canonical_file(path: &Path) -> Result<PathBuf, DatasetBatchError> {
    fs::canonicalize(path).map_err(|source| DatasetBatchError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn initialize_workspace(
    output_dir: &Path,
    manifest: &[u8],
    run: &[u8],
    jobs: &[u8],
) -> Result<(), DatasetBatchError> {
    if output_dir.exists() {
        return Err(DatasetBatchError::OutputDirectoryExists(
            output_dir.to_path_buf(),
        ));
    }
    let parent = output_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| DatasetBatchError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let file_name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("dataset-batch");
    let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let staging = parent.join(format!(
        ".{file_name}.staging-{}-{sequence}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(DatasetBatchError::OutputDirectoryExists(staging));
    }
    fs::create_dir(&staging).map_err(|source| DatasetBatchError::Io {
        path: staging.clone(),
        source,
    })?;
    let initialized = (|| {
        for directory in [DATASETS_DIRECTORY, ATTEMPTS_DIRECTORY] {
            fs::create_dir(staging.join(directory)).map_err(|source| DatasetBatchError::Io {
                path: staging.join(directory),
                source,
            })?;
        }
        write_new(&staging.join(SOURCE_MANIFEST_FILE), manifest)?;
        write_new(&staging.join(RUN_FILE), run)?;
        write_new(&staging.join(JOBS_FILE), jobs)?;
        crate::fs_retry::rename(&staging, output_dir).map_err(|source| DatasetBatchError::Io {
            path: output_dir.to_path_buf(),
            source,
        })?;
        Ok(())
    })();
    if initialized.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    initialized
}

fn format_jobs(jobs: &[PreparedDatasetBatchJob]) -> Result<String, DatasetBatchError> {
    let mut output = String::from(
        "dataset_id\tmodels_manifest\tmodels_manifest_fingerprint\tconfig\tconfig_fingerprint\tresult\n",
    );
    for job in jobs {
        let models = checked_path(&job.models_manifest_path)?;
        let config = checked_path(&job.config_path)?;
        output.push_str(&job.dataset_id);
        output.push('\t');
        output.push_str(models);
        output.push('\t');
        output.push_str(&job.models_manifest_fingerprint);
        output.push('\t');
        output.push_str(config);
        output.push('\t');
        output.push_str(&job.config_fingerprint);
        output.push('\t');
        output.push_str(DATASETS_DIRECTORY);
        output.push('/');
        output.push_str(&job.dataset_id);
        output.push('\n');
    }
    Ok(output)
}

fn format_attempt(reports: &[DatasetBatchJobReport]) -> String {
    let complete = reports
        .iter()
        .filter(|report| matches!(report.outcome, DatasetBatchJobOutcome::Complete))
        .count();
    let failed = reports
        .iter()
        .filter(|report| matches!(report.outcome, DatasetBatchJobOutcome::Failed { .. }))
        .count();
    let cancelled = reports
        .iter()
        .filter(|report| matches!(report.outcome, DatasetBatchJobOutcome::Cancelled { .. }))
        .count();
    let not_started = reports
        .iter()
        .filter(|report| matches!(report.outcome, DatasetBatchJobOutcome::NotStarted))
        .count();
    let status = if cancelled > 0 || not_started > 0 {
        "cancelled"
    } else if failed > 0 {
        "failed"
    } else {
        "complete"
    };
    let mut output = format!(
        "format\t{}\nstatus\t{}\ndatasets\t{}\ncomplete_datasets\t{}\nfailed_datasets\t{}\ncancelled_datasets\t{}\nnot_started_datasets\t{}\n\ndataset_jobs\ndataset_id\tstatus\tresult\tcomparison\terror_code\tmessage\n",
        DATASET_BATCH_ATTEMPT_FORMAT,
        status,
        reports.len(),
        complete,
        failed,
        cancelled,
        not_started,
    );
    for report in reports {
        output.push_str(&report.dataset_id);
        match &report.outcome {
            DatasetBatchJobOutcome::Complete => {
                output.push_str("\tcomplete\t");
                push_dataset_path(&mut output, &report.dataset_id);
                output.push('\t');
                push_dataset_path(&mut output, &report.dataset_id);
                output.push_str("/comparison.tsv\tNA\tNA\n");
            }
            DatasetBatchJobOutcome::Failed { code, message } => {
                output.push_str("\tfailed\t");
                push_dataset_path(&mut output, &report.dataset_id);
                output.push_str("\tNA\t");
                output.push_str(&encode_field(code));
                output.push('\t');
                output.push_str(&encode_field(message));
                output.push('\n');
            }
            DatasetBatchJobOutcome::Cancelled { code, message } => {
                output.push_str("\tcancelled\t");
                push_dataset_path(&mut output, &report.dataset_id);
                output.push_str("\tNA\t");
                output.push_str(&encode_field(code));
                output.push('\t');
                output.push_str(&encode_field(message));
                output.push('\n');
            }
            DatasetBatchJobOutcome::NotStarted => {
                output.push_str("\tnot_started\t");
                push_dataset_path(&mut output, &report.dataset_id);
                output.push_str("\tNA\tNA\tNA\n");
            }
        }
    }
    output
}

fn push_dataset_path(output: &mut String, dataset_id: &str) {
    output.push_str(DATASETS_DIRECTORY);
    output.push('/');
    output.push_str(dataset_id);
}

fn next_attempt_index(attempts_dir: &Path) -> Result<usize, DatasetBatchError> {
    let mut maximum = 0_usize;
    for entry in fs::read_dir(attempts_dir).map_err(|source| DatasetBatchError::Io {
        path: attempts_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| DatasetBatchError::Io {
            path: attempts_dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        let Some(number) = name
            .to_str()
            .and_then(|name| name.strip_prefix("attempt-"))
            .and_then(|value| value.strip_suffix(".tsv"))
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        maximum = maximum.max(number);
    }
    maximum
        .checked_add(1)
        .ok_or(DatasetBatchError::AttemptIndexOverflow)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ConfigOptionKind {
    Flag,
    Path,
    Value,
    Managed,
}

fn config_option_kind(option: &str) -> Option<ConfigOptionKind> {
    match option {
        "--use-ambiguities" | "--use-detection-model" | "--include-null-range" => {
            Some(ConfigOptionKind::Flag)
        }
        "--tree"
        | "--ranges"
        | "--detections"
        | "--controls"
        | "--dispersal-multipliers"
        | "--dispersal-strata"
        | "--distance-matrix"
        | "--environment-distance-matrix"
        | "--extirpation-multipliers"
        | "--area-sizes" => Some(ConfigOptionKind::Path),
        "--tree-name"
        | "--min-branch-length"
        | "--fill-missing-branch-length"
        | "--max-range-size"
        | "--max-states"
        | "--root-prior"
        | "--initial-step"
        | "--tolerance"
        | "--max-iterations"
        | "--additional-start" => Some(ConfigOptionKind::Value),
        "--manifest"
        | "--output-dir"
        | "--resume"
        | "--parameters"
        | "--analysis-result-dir"
        | "--ancestral-probs"
        | "--split-probs"
        | "--error-format"
        | "--progress-format" => Some(ConfigOptionKind::Managed),
        _ => None,
    }
}

fn content_lines(input: &str) -> impl Iterator<Item = (usize, &str)> {
    input.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, trimmed))
    })
}

fn resolve_required_path(
    value: &str,
    base_dir: &Path,
    manifest_path: &Path,
    line: usize,
    column: &str,
) -> Result<PathBuf, DatasetBatchError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid_manifest(
            manifest_path,
            line,
            format!("{column} path is empty"),
        ));
    }
    let path = PathBuf::from(value);
    Ok(if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    })
}

fn checked_path(path: &Path) -> Result<&str, DatasetBatchError> {
    let value = path
        .to_str()
        .ok_or_else(|| DatasetBatchError::NonUtf8Path(path.to_path_buf()))?;
    if value.contains(['\t', '\r', '\n']) {
        return Err(DatasetBatchError::UnsafePath(path.to_path_buf()));
    }
    Ok(value)
}

fn encode_field(value: &str) -> String {
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
    String::from_utf8(encoded).expect("encoding a UTF-8 dataset-batch field preserves UTF-8")
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), DatasetBatchError> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| DatasetBatchError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(bytes)
        .map_err(|source| DatasetBatchError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| DatasetBatchError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_once_or_verify(path: &Path, bytes: &[u8]) -> Result<(), DatasetBatchError> {
    if path.exists() {
        return verify_file(path, bytes);
    }
    let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{sequence}", std::process::id()));
    write_new(&temporary, bytes)?;
    match crate::fs_retry::rename(&temporary, path) {
        Ok(()) => Ok(()),
        Err(_source) if path.exists() => {
            let _ = fs::remove_file(&temporary);
            verify_file(path, bytes)
        }
        Err(source) => {
            let _ = fs::remove_file(&temporary);
            Err(DatasetBatchError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    }
}

fn verify_file(path: &Path, expected: &[u8]) -> Result<(), DatasetBatchError> {
    let actual = fs::read(path).map_err(|source| DatasetBatchError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if actual != expected {
        return Err(DatasetBatchError::ResumeIdentityMismatch {
            path: path.to_path_buf(),
            expected: analysis_result::stable_fingerprint(expected),
            actual: analysis_result::stable_fingerprint(&actual),
        });
    }
    Ok(())
}

fn invalid_manifest(path: &Path, line: usize, message: impl Into<String>) -> DatasetBatchError {
    DatasetBatchError::InvalidManifest {
        path: path.to_path_buf(),
        line,
        message: message.into(),
    }
}

fn invalid_config(path: &Path, line: usize, message: impl Into<String>) -> DatasetBatchError {
    DatasetBatchError::InvalidConfig {
        path: path.to_path_buf(),
        line,
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum DatasetBatchError {
    InvalidManifest {
        path: PathBuf,
        line: usize,
        message: String,
    },
    InvalidConfig {
        path: PathBuf,
        line: usize,
        message: String,
    },
    OutputDirectoryExists(PathBuf),
    MissingOutputDirectory(PathBuf),
    MissingDatasetsDirectory(PathBuf),
    ResumeIdentityMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    AttemptJobMismatch,
    AttemptIndexOverflow,
    NonUtf8Path(PathBuf),
    UnsafePath(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for DatasetBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest {
                path,
                line,
                message,
            } => write!(
                f,
                "invalid dataset-batch manifest {} at line {line}: {message}",
                path.display()
            ),
            Self::InvalidConfig {
                path,
                line,
                message,
            } => write!(
                f,
                "invalid model-batch config {} at line {line}: {message}",
                path.display()
            ),
            Self::OutputDirectoryExists(path) => write!(
                f,
                "dataset-batch output directory already exists; use --resume only for the same task: {}",
                path.display()
            ),
            Self::MissingOutputDirectory(path) => write!(
                f,
                "cannot resume because dataset-batch output directory does not exist: {}",
                path.display()
            ),
            Self::MissingDatasetsDirectory(path) => write!(
                f,
                "dataset-batch datasets directory is missing: {}",
                path.display()
            ),
            Self::ResumeIdentityMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "dataset-batch resume identity differs at {} (expected {expected}, found {actual})",
                path.display()
            ),
            Self::AttemptJobMismatch => {
                write!(
                    f,
                    "dataset-batch attempt rows do not match the prepared jobs"
                )
            }
            Self::AttemptIndexOverflow => {
                write!(f, "dataset-batch attempt index overflowed usize")
            }
            Self::NonUtf8Path(path) => write!(
                f,
                "dataset-batch path is not valid UTF-8: {}",
                path.display()
            ),
            Self::UnsafePath(path) => write!(
                f,
                "dataset-batch path contains a tab or newline and cannot enter TSV output: {}",
                path.display()
            ),
            Self::Io { path, source } => {
                write!(
                    f,
                    "dataset-batch I/O failed for {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for DatasetBatchError {
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
    fn parses_relative_dataset_paths_and_rejects_duplicate_ids() {
        let path = Path::new("study/datasets.tsv");
        let entries = parse_dataset_batch_manifest(
            "biogeo-dataset-batch-manifest-v1\ndataset_id\tmodels\tconfig\nIslandA\tmodels.tsv\tconfigs/a.tsv\n",
            path,
        )
        .unwrap();
        assert_eq!(
            entries[0].models_manifest_path,
            Path::new("study/models.tsv")
        );
        assert_eq!(entries[0].config_path, Path::new("study/configs/a.tsv"));

        assert!(matches!(
            parse_dataset_batch_manifest(
                "biogeo-dataset-batch-manifest-v1\ndataset_id\tmodels\tconfig\nIslandA\ta.tsv\ta.cfg\nislanda\tb.tsv\tb.cfg\n",
                path,
            ),
            Err(DatasetBatchError::InvalidManifest { .. })
        ));
    }

    #[test]
    fn cancelled_attempt_distinguishes_cancelled_and_not_started_datasets() {
        let jobs = [test_job("StudyA"), test_job("StudyB"), test_job("StudyC")];
        let reports = [
            DatasetBatchJobReport::complete(&jobs[0]),
            DatasetBatchJobReport::cancelled(&jobs[1], "task_cancelled", "stopped".to_string()),
            DatasetBatchJobReport::not_started(&jobs[2]),
        ];

        let output = format_attempt(&reports);
        assert!(output.starts_with("format\tbiogeo-dataset-batch-attempt-v2\n"));
        assert!(output.contains("status\tcancelled\n"));
        assert!(output.contains("complete_datasets\t1\n"));
        assert!(output.contains("cancelled_datasets\t1\n"));
        assert!(output.contains("not_started_datasets\t1\n"));
        assert!(
            output.contains("StudyB\tcancelled\tdatasets/StudyB\tNA\ttask_cancelled\tstopped\n")
        );
        assert!(output.contains("StudyC\tnot_started\tdatasets/StudyC\tNA\tNA\tNA\n"));
    }

    fn test_job(dataset_id: &str) -> PreparedDatasetBatchJob {
        PreparedDatasetBatchJob {
            dataset_id: dataset_id.to_string(),
            models_manifest_path: PathBuf::from("models.tsv"),
            models_manifest_fingerprint: "models".to_string(),
            config_path: PathBuf::from("config.tsv"),
            config_fingerprint: "config".to_string(),
            result_path: PathBuf::from("datasets").join(dataset_id),
        }
    }

    #[test]
    fn model_batch_config_resolves_paths_flags_and_repeated_starts() {
        let arguments = parse_model_batch_config(
            "biogeo-model-batch-config-v1\noption\tvalue\n--tree\t../trees/a.nwk\n--ranges\tranges.tsv\n--use-ambiguities\tfalse\n--include-null-range\ttrue\n--additional-start\t0.1,0.2\n--additional-start\t0.2,0.1\n",
            Path::new("study/configs/a.tsv"),
        )
        .unwrap();
        let tree_path = Path::new("study/configs")
            .join("../trees/a.nwk")
            .to_string_lossy()
            .into_owned();
        let ranges_path = Path::new("study/configs")
            .join("ranges.tsv")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            arguments,
            vec![
                "--tree".to_string(),
                tree_path,
                "--ranges".to_string(),
                ranges_path,
                "--include-null-range".to_string(),
                "--additional-start".to_string(),
                "0.1,0.2".to_string(),
                "--additional-start".to_string(),
                "0.2,0.1".to_string(),
            ]
        );

        assert!(matches!(
            parse_model_batch_config(
                "biogeo-model-batch-config-v1\noption\tvalue\n--output-dir\tbad\n",
                Path::new("a.tsv"),
            ),
            Err(DatasetBatchError::InvalidConfig { .. })
        ));
    }
}
