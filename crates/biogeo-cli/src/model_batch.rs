use crate::analysis_result;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const MODEL_BATCH_MANIFEST_FORMAT: &str = "biogeo-model-batch-manifest-v1";
pub const MODEL_BATCH_RESULT_FORMAT: &str = "biogeo-model-batch-result-v2";
pub const MODEL_COMPARISON_FORMAT: &str = "biogeo-model-comparison-v3";
pub const MODEL_BATCH_ATTEMPT_FORMAT: &str = "biogeo-model-batch-attempt-v2";

const SOURCE_MANIFEST_FILE: &str = "source-manifest.tsv";
const RUN_FILE: &str = "run.tsv";
const JOBS_FILE: &str = "jobs.tsv";
const COMPARISON_FILE: &str = "comparison.tsv";
const MODEL_AVERAGE_FILE: &str = "model-averaged-ancestral-ranges.tsv";
const COMPLETE_FILE: &str = "complete.tsv";
const MODELS_DIRECTORY: &str = "models";
const ATTEMPTS_DIRECTORY: &str = "attempts";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelBatchEntry {
    pub model_id: String,
    pub parameters_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct PreparedModelBatchJob {
    pub model_id: String,
    pub parameters_path: PathBuf,
    pub parameters_fingerprint: String,
    pub analysis_result_path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ModelBatchJobOutcome {
    Complete,
    Failed { code: String, message: String },
    Cancelled { code: String, message: String },
    NotStarted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelBatchJobReport {
    pub model_id: String,
    pub outcome: ModelBatchJobOutcome,
}

impl ModelBatchJobReport {
    pub fn complete(job: &PreparedModelBatchJob) -> Self {
        Self {
            model_id: job.model_id.clone(),
            outcome: ModelBatchJobOutcome::Complete,
        }
    }

    pub fn failed(job: &PreparedModelBatchJob, code: &str, message: String) -> Self {
        Self {
            model_id: job.model_id.clone(),
            outcome: ModelBatchJobOutcome::Failed {
                code: code.to_string(),
                message,
            },
        }
    }

    pub fn cancelled(job: &PreparedModelBatchJob, code: &str, message: String) -> Self {
        Self {
            model_id: job.model_id.clone(),
            outcome: ModelBatchJobOutcome::Cancelled {
                code: code.to_string(),
                message,
            },
        }
    }

    pub fn not_started(job: &PreparedModelBatchJob) -> Self {
        Self {
            model_id: job.model_id.clone(),
            outcome: ModelBatchJobOutcome::NotStarted,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ModelBatchWorkspace {
    output_dir: PathBuf,
    jobs: Vec<PreparedModelBatchJob>,
}

impl ModelBatchWorkspace {
    pub fn jobs(&self) -> &[PreparedModelBatchJob] {
        &self.jobs
    }

    pub fn compare_results(&self) -> Result<ModelComparison, ModelBatchError> {
        compare_analysis_results(&self.output_dir, &self.jobs)
    }

    pub fn validate_existing_result(
        &self,
        job: &PreparedModelBatchJob,
    ) -> Result<(), ModelBatchError> {
        load_validated_result(job).map(|_| ())
    }

    pub fn record_attempt(
        &self,
        reports: &[ModelBatchJobReport],
    ) -> Result<PathBuf, ModelBatchError> {
        if reports.len() != self.jobs.len()
            || reports
                .iter()
                .zip(&self.jobs)
                .any(|(report, job)| report.model_id != job.model_id)
        {
            return Err(ModelBatchError::AttemptJobMismatch);
        }
        let attempts_dir = self.output_dir.join(ATTEMPTS_DIRECTORY);
        fs::create_dir_all(&attempts_dir).map_err(|source| ModelBatchError::Io {
            path: attempts_dir.clone(),
            source,
        })?;
        let attempt_index = next_attempt_index(&attempts_dir)?;
        let path = attempts_dir.join(format!("attempt-{attempt_index:06}.tsv"));
        write_new(&path, format_attempt(reports).as_bytes())?;
        Ok(path)
    }

    pub fn finalize(&self, comparison: &str, model_average: &str) -> Result<(), ModelBatchError> {
        write_once_or_verify(
            &self.output_dir.join(COMPARISON_FILE),
            comparison.as_bytes(),
        )?;
        write_once_or_verify(
            &self.output_dir.join(MODEL_AVERAGE_FILE),
            model_average.as_bytes(),
        )?;
        let completion = format!(
            "key\tvalue\nformat\t{}\nstatus\tcomplete\nmodels\t{}\ncomparison_file\t{}\ncomparison_fingerprint\t{}\nmodel_average_file\t{}\nmodel_average_fingerprint\t{}\n",
            MODEL_BATCH_RESULT_FORMAT,
            self.jobs.len(),
            COMPARISON_FILE,
            analysis_result::stable_fingerprint(comparison.as_bytes()),
            MODEL_AVERAGE_FILE,
            analysis_result::stable_fingerprint(model_average.as_bytes()),
        );
        write_once_or_verify(&self.output_dir.join(COMPLETE_FILE), completion.as_bytes())
    }
}

pub fn parse_model_batch_manifest(
    input: &str,
    manifest_path: &Path,
) -> Result<Vec<ModelBatchEntry>, ModelBatchError> {
    let mut lines = input.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, trimmed))
    });
    let (format_line, format) = lines
        .next()
        .ok_or_else(|| invalid_manifest(manifest_path, 1, "manifest is empty"))?;
    let format = format.trim_start_matches('\u{feff}');
    if format != MODEL_BATCH_MANIFEST_FORMAT {
        return Err(invalid_manifest(
            manifest_path,
            format_line,
            format!("expected format {MODEL_BATCH_MANIFEST_FORMAT:?}, found {format:?}"),
        ));
    }
    let (header_line, header) = lines.next().ok_or_else(|| {
        invalid_manifest(
            manifest_path,
            format_line + 1,
            "missing model_id/parameters header",
        )
    })?;
    if header != "model_id\tparameters" {
        return Err(invalid_manifest(
            manifest_path,
            header_line,
            "header must be exactly: model_id<TAB>parameters",
        ));
    }

    let base_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for (line_number, line) in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 2 {
            return Err(invalid_manifest(
                manifest_path,
                line_number,
                format!("expected 2 tab-separated columns, found {}", fields.len()),
            ));
        }
        let model_id = fields[0].trim();
        validate_model_id(manifest_path, line_number, model_id)?;
        if !seen.insert(model_id.to_ascii_lowercase()) {
            return Err(invalid_manifest(
                manifest_path,
                line_number,
                format!(
                    "duplicate model_id {model_id:?}; identifiers are compared case-insensitively for Windows portability"
                ),
            ));
        }
        let parameter_field = fields[1].trim();
        if parameter_field.is_empty() {
            return Err(invalid_manifest(
                manifest_path,
                line_number,
                "parameters path is empty",
            ));
        }
        let parameter_path = PathBuf::from(parameter_field);
        entries.push(ModelBatchEntry {
            model_id: model_id.to_string(),
            parameters_path: if parameter_path.is_absolute() {
                parameter_path
            } else {
                base_dir.join(parameter_path)
            },
        });
    }
    if entries.is_empty() {
        return Err(invalid_manifest(
            manifest_path,
            header_line,
            "manifest contains no model rows",
        ));
    }
    Ok(entries)
}

pub fn prepare_model_batch_workspace(
    manifest_path: &Path,
    manifest_input: &str,
    output_dir: &Path,
    invocation_fingerprint: &str,
    resume: bool,
) -> Result<ModelBatchWorkspace, ModelBatchError> {
    let entries = parse_model_batch_manifest(manifest_input, manifest_path)?;
    let jobs = prepare_jobs(entries, output_dir)?;
    let manifest_fingerprint = analysis_result::stable_fingerprint(manifest_input.as_bytes());
    let run = format!(
        "key\tvalue\nformat\t{}\nmanifest_format\t{}\nmanifest_fingerprint\t{}\ninvocation_fingerprint\t{}\nmodels\t{}\nsource_manifest_file\t{}\njobs_file\t{}\n",
        MODEL_BATCH_RESULT_FORMAT,
        MODEL_BATCH_MANIFEST_FORMAT,
        manifest_fingerprint,
        invocation_fingerprint,
        jobs.len(),
        SOURCE_MANIFEST_FILE,
        JOBS_FILE,
    );
    let job_table = format_jobs(&jobs)?;

    if resume {
        if !output_dir.is_dir() {
            return Err(ModelBatchError::MissingOutputDirectory(
                output_dir.to_path_buf(),
            ));
        }
        verify_file(&output_dir.join(RUN_FILE), run.as_bytes())?;
        verify_file(
            &output_dir.join(SOURCE_MANIFEST_FILE),
            manifest_input.as_bytes(),
        )?;
        verify_file(&output_dir.join(JOBS_FILE), job_table.as_bytes())?;
        if !output_dir.join(MODELS_DIRECTORY).is_dir() {
            return Err(ModelBatchError::MissingModelsDirectory(
                output_dir.join(MODELS_DIRECTORY),
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

    Ok(ModelBatchWorkspace {
        output_dir: output_dir.to_path_buf(),
        jobs,
    })
}

fn prepare_jobs(
    entries: Vec<ModelBatchEntry>,
    output_dir: &Path,
) -> Result<Vec<PreparedModelBatchJob>, ModelBatchError> {
    entries
        .into_iter()
        .map(|entry| {
            let parameters_path =
                fs::canonicalize(&entry.parameters_path).map_err(|source| ModelBatchError::Io {
                    path: entry.parameters_path.clone(),
                    source,
                })?;
            let bytes = fs::read(&parameters_path).map_err(|source| ModelBatchError::Io {
                path: parameters_path.clone(),
                source,
            })?;
            Ok(PreparedModelBatchJob {
                analysis_result_path: output_dir.join(MODELS_DIRECTORY).join(&entry.model_id),
                model_id: entry.model_id,
                parameters_path,
                parameters_fingerprint: analysis_result::stable_fingerprint(&bytes),
            })
        })
        .collect()
}

fn initialize_workspace(
    output_dir: &Path,
    manifest: &[u8],
    run: &[u8],
    jobs: &[u8],
) -> Result<(), ModelBatchError> {
    if output_dir.exists() {
        return Err(ModelBatchError::OutputDirectoryExists(
            output_dir.to_path_buf(),
        ));
    }
    let parent = output_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| ModelBatchError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let file_name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("model-batch");
    let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let staging = parent.join(format!(
        ".{file_name}.staging-{}-{sequence}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(ModelBatchError::OutputDirectoryExists(staging));
    }
    fs::create_dir(&staging).map_err(|source| ModelBatchError::Io {
        path: staging.clone(),
        source,
    })?;
    let initialized = (|| {
        fs::create_dir(staging.join(MODELS_DIRECTORY)).map_err(|source| ModelBatchError::Io {
            path: staging.join(MODELS_DIRECTORY),
            source,
        })?;
        fs::create_dir(staging.join(ATTEMPTS_DIRECTORY)).map_err(|source| ModelBatchError::Io {
            path: staging.join(ATTEMPTS_DIRECTORY),
            source,
        })?;
        write_new(&staging.join(SOURCE_MANIFEST_FILE), manifest)?;
        write_new(&staging.join(RUN_FILE), run)?;
        write_new(&staging.join(JOBS_FILE), jobs)?;
        crate::fs_retry::rename(&staging, output_dir).map_err(|source| ModelBatchError::Io {
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

fn format_jobs(jobs: &[PreparedModelBatchJob]) -> Result<String, ModelBatchError> {
    let mut output =
        String::from("model_id\tparameters\tparameters_fingerprint\tanalysis_result\n");
    for job in jobs {
        let parameters = utf8_path(&job.parameters_path)?;
        if parameters.contains(['\t', '\r', '\n']) {
            return Err(ModelBatchError::UnsafePath(job.parameters_path.clone()));
        }
        output.push_str(&job.model_id);
        output.push('\t');
        output.push_str(parameters);
        output.push('\t');
        output.push_str(&job.parameters_fingerprint);
        output.push('\t');
        output.push_str(MODELS_DIRECTORY);
        output.push('/');
        output.push_str(&job.model_id);
        output.push('\n');
    }
    Ok(output)
}

fn next_attempt_index(attempts_dir: &Path) -> Result<usize, ModelBatchError> {
    let mut maximum = 0_usize;
    for entry in fs::read_dir(attempts_dir).map_err(|source| ModelBatchError::Io {
        path: attempts_dir.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| ModelBatchError::Io {
            path: attempts_dir.to_path_buf(),
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(number) = name
            .strip_prefix("attempt-")
            .and_then(|value| value.strip_suffix(".tsv"))
            .and_then(|value| value.parse::<usize>().ok())
        else {
            continue;
        };
        maximum = maximum.max(number);
    }
    maximum
        .checked_add(1)
        .ok_or(ModelBatchError::AttemptIndexOverflow)
}

fn format_attempt(reports: &[ModelBatchJobReport]) -> String {
    let complete = reports
        .iter()
        .filter(|report| matches!(report.outcome, ModelBatchJobOutcome::Complete))
        .count();
    let failed = reports
        .iter()
        .filter(|report| matches!(report.outcome, ModelBatchJobOutcome::Failed { .. }))
        .count();
    let cancelled = reports
        .iter()
        .filter(|report| matches!(report.outcome, ModelBatchJobOutcome::Cancelled { .. }))
        .count();
    let not_started = reports
        .iter()
        .filter(|report| matches!(report.outcome, ModelBatchJobOutcome::NotStarted))
        .count();
    let status = if cancelled > 0 || not_started > 0 {
        "cancelled"
    } else if failed > 0 {
        "failed"
    } else {
        "complete"
    };
    let mut output = format!(
        "format\t{}\nstatus\t{}\nmodels\t{}\ncomplete_models\t{}\nfailed_models\t{}\ncancelled_models\t{}\nnot_started_models\t{}\n\njobs\nmodel_id\tstatus\tanalysis_result\terror_code\tmessage\n",
        MODEL_BATCH_ATTEMPT_FORMAT,
        status,
        reports.len(),
        complete,
        failed,
        cancelled,
        not_started,
    );
    for report in reports {
        output.push_str(&report.model_id);
        match &report.outcome {
            ModelBatchJobOutcome::Complete => {
                output.push_str("\tcomplete\t");
                output.push_str(MODELS_DIRECTORY);
                output.push('/');
                output.push_str(&report.model_id);
                output.push_str("\tNA\tNA\n");
            }
            ModelBatchJobOutcome::Failed { code, message } => {
                output.push_str("\tfailed\t");
                output.push_str(MODELS_DIRECTORY);
                output.push('/');
                output.push_str(&report.model_id);
                output.push('\t');
                output.push_str(&encode_field(code));
                output.push('\t');
                output.push_str(&encode_field(message));
                output.push('\n');
            }
            ModelBatchJobOutcome::Cancelled { code, message } => {
                output.push_str("\tcancelled\t");
                output.push_str(MODELS_DIRECTORY);
                output.push('/');
                output.push_str(&report.model_id);
                output.push('\t');
                output.push_str(&encode_field(code));
                output.push('\t');
                output.push_str(&encode_field(message));
                output.push('\n');
            }
            ModelBatchJobOutcome::NotStarted => {
                output.push_str("\tnot_started\t");
                output.push_str(MODELS_DIRECTORY);
                output.push('/');
                output.push_str(&report.model_id);
                output.push_str("\tNA\tNA\n");
            }
        }
    }
    output
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
    String::from_utf8(encoded).expect("encoding a UTF-8 batch field preserves UTF-8")
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DatasetIdentity {
    tree_fingerprint: String,
    tree_name: Option<String>,
    tip_observation_model: String,
    observation_fingerprints: Vec<(String, String)>,
    min_branch_length_bits: u64,
    missing_branch_length_fill_bits: Option<u64>,
    max_range_size: u8,
    include_null_range: bool,
    root_prior: String,
    states: usize,
    areas: usize,
    tips: usize,
}

impl DatasetIdentity {
    fn from_result(
        model_id: &str,
        loaded: &analysis_result::LoadedAnalysisResult,
    ) -> Result<Self, ModelBatchError> {
        let tree_fingerprint = input_fingerprint(model_id, loaded, "tree")?;
        let observation_roles: &[&str] = match loaded.manifest.tip_observation_model.as_str() {
            "exact_ranges" | "ambiguous_ranges" => &["ranges"],
            "mf_dp_fdp_detection" => &["detections", "controls"],
            value => {
                return Err(ModelBatchError::UnsupportedObservationModel {
                    model_id: model_id.to_string(),
                    value: value.to_string(),
                });
            }
        };
        let observation_fingerprints = observation_roles
            .iter()
            .map(|role| {
                Ok((
                    (*role).to_string(),
                    input_fingerprint(model_id, loaded, role)?,
                ))
            })
            .collect::<Result<Vec<_>, ModelBatchError>>()?;
        Ok(Self {
            tree_fingerprint,
            tree_name: loaded.manifest.tree_name.clone(),
            tip_observation_model: loaded.manifest.tip_observation_model.clone(),
            observation_fingerprints,
            min_branch_length_bits: loaded.manifest.min_branch_length.to_bits(),
            missing_branch_length_fill_bits: loaded
                .manifest
                .missing_branch_length_fill
                .map(f64::to_bits),
            max_range_size: loaded.manifest.max_range_size,
            include_null_range: loaded.manifest.include_null_range,
            root_prior: loaded.manifest.root_prior.clone(),
            states: loaded.manifest.states,
            areas: loaded.manifest.areas,
            tips: loaded.manifest.tips,
        })
    }
}

fn input_fingerprint(
    model_id: &str,
    loaded: &analysis_result::LoadedAnalysisResult,
    role: &'static str,
) -> Result<String, ModelBatchError> {
    loaded
        .manifest
        .inputs
        .get(role)
        .map(|record| record.fingerprint.clone())
        .ok_or_else(|| ModelBatchError::MissingComparisonInput {
            model_id: model_id.to_string(),
            role,
        })
}

#[derive(Clone, Debug)]
pub(crate) struct ComparisonRow {
    pub(crate) model_id: String,
    pub(crate) analysis_result: String,
    pub(crate) converged: bool,
    pub(crate) converged_starts: usize,
    pub(crate) parameters: usize,
    pub(crate) tips: usize,
    pub(crate) log_likelihood: f64,
    pub(crate) eligible: bool,
    pub(crate) aic: Option<f64>,
    pub(crate) delta_aic: Option<f64>,
    pub(crate) aic_weight: Option<f64>,
    pub(crate) aic_rank: Option<usize>,
    pub(crate) aicc: Option<f64>,
    pub(crate) delta_aicc: Option<f64>,
    pub(crate) aicc_weight: Option<f64>,
    pub(crate) aicc_rank: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct ModelComparison {
    rows: Vec<ComparisonRow>,
    relationships: Vec<NestedRelationship>,
    likelihood_ratio_tests: Vec<LikelihoodRatioTest>,
}

impl ModelComparison {
    pub(crate) fn rows(&self) -> &[ComparisonRow] {
        &self.rows
    }

    pub fn to_tsv(&self) -> String {
        format_comparison(
            &self.rows,
            &self.relationships,
            &self.likelihood_ratio_tests,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NestedRelationshipKind {
    Equivalent,
    NestedInterior,
    NestedBoundary,
    NotNested,
    Undetermined,
}

impl NestedRelationshipKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Equivalent => "equivalent",
            Self::NestedInterior => "nested_interior",
            Self::NestedBoundary => "nested_boundary",
            Self::NotNested => "not_nested",
            Self::Undetermined => "undetermined",
        }
    }

    fn is_strictly_nested(self) -> bool {
        matches!(self, Self::NestedInterior | Self::NestedBoundary)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct NestedRelationship {
    reduced_model: String,
    full_model: String,
    kind: NestedRelationshipKind,
    reduced_parameters: usize,
    full_parameters: usize,
    degrees_of_freedom: usize,
    constrained_parameters: Vec<String>,
    boundary_parameters: Vec<String>,
    reason: String,
}

#[derive(Clone, Debug, PartialEq)]
struct LikelihoodRatioTest {
    reduced_model: String,
    full_model: String,
    relationship: NestedRelationshipKind,
    degrees_of_freedom: usize,
    reduced_log_likelihood: f64,
    full_log_likelihood: f64,
    statistic: Option<f64>,
    p_value_chi_square: Option<f64>,
    p_value_half_chi_square: Option<f64>,
    status: &'static str,
    note: &'static str,
}

fn compare_analysis_results(
    output_dir: &Path,
    jobs: &[PreparedModelBatchJob],
) -> Result<ModelComparison, ModelBatchError> {
    let mut rows = Vec::with_capacity(jobs.len());
    let mut tables = Vec::with_capacity(jobs.len());
    let mut baseline: Option<(String, DatasetIdentity)> = None;
    for job in jobs {
        let loaded = load_validated_result(job)?;
        let optimization = loaded
            .manifest
            .optimization
            .ok_or_else(|| ModelBatchError::MissingOptimizationSummary(job.model_id.clone()))?;
        let table =
            biogeo_core::parse_parameter_table(&loaded.source_parameters).map_err(|source| {
                ModelBatchError::ParameterTable {
                    model_id: job.model_id.clone(),
                    source,
                }
            })?;
        let identity = DatasetIdentity::from_result(&job.model_id, &loaded)?;
        if let Some((baseline_id, expected)) = &baseline {
            if expected != &identity {
                return Err(ModelBatchError::IncompatibleDataset {
                    baseline_model_id: baseline_id.clone(),
                    model_id: job.model_id.clone(),
                    expected: format!("{expected:?}"),
                    actual: format!("{identity:?}"),
                });
            }
        } else {
            baseline = Some((job.model_id.clone(), identity));
        }
        let parameters = table.free_parameter_names().len();
        let eligible = optimization.converged && optimization.converged_starts > 0;
        rows.push(ComparisonRow {
            model_id: job.model_id.clone(),
            analysis_result: relative_result_path(output_dir, &job.analysis_result_path)?,
            converged: optimization.converged,
            converged_starts: optimization.converged_starts,
            parameters,
            tips: loaded.manifest.tips,
            log_likelihood: loaded.manifest.log_likelihood,
            eligible,
            aic: eligible.then(|| aic(loaded.manifest.log_likelihood, parameters)),
            delta_aic: None,
            aic_weight: None,
            aic_rank: None,
            aicc: eligible
                .then(|| {
                    aicc(
                        loaded.manifest.log_likelihood,
                        parameters,
                        loaded.manifest.tips,
                    )
                })
                .flatten(),
            delta_aicc: None,
            aicc_weight: None,
            aicc_rank: None,
        });
        tables.push((job.model_id.clone(), table));
    }
    populate_model_weights(&mut rows);
    let relationships = compare_nested_relationships(&tables);
    let likelihood_ratio_tests = build_likelihood_ratio_tests(&rows, &relationships);
    Ok(ModelComparison {
        rows,
        relationships,
        likelihood_ratio_tests,
    })
}

fn populate_model_weights(rows: &mut [ComparisonRow]) {
    populate_criterion(
        rows,
        |row| row.aic,
        |row, delta, weight, rank| {
            row.delta_aic = Some(delta);
            row.aic_weight = Some(weight);
            row.aic_rank = Some(rank);
        },
    );
    let eligible_models = rows.iter().filter(|row| row.eligible).count();
    let aicc_defined_models = rows.iter().filter(|row| row.aicc.is_some()).count();
    if eligible_models > 0 && aicc_defined_models == eligible_models {
        populate_criterion(
            rows,
            |row| row.aicc,
            |row, delta, weight, rank| {
                row.delta_aicc = Some(delta);
                row.aicc_weight = Some(weight);
                row.aicc_rank = Some(rank);
            },
        );
    }
}

fn load_validated_result(
    job: &PreparedModelBatchJob,
) -> Result<analysis_result::LoadedAnalysisResult, ModelBatchError> {
    let loaded =
        analysis_result::load_analysis_result(&job.analysis_result_path).map_err(|source| {
            ModelBatchError::AnalysisResult {
                model_id: job.model_id.clone(),
                source: Box::new(source),
            }
        })?;
    loaded
        .verify_replay_inputs()
        .map_err(|source| ModelBatchError::AnalysisResult {
            model_id: job.model_id.clone(),
            source: Box::new(source),
        })?;
    let parameter_bytes = fs::read(&job.parameters_path).map_err(|source| ModelBatchError::Io {
        path: job.parameters_path.clone(),
        source,
    })?;
    if loaded.source_parameters.as_bytes() != parameter_bytes {
        return Err(ModelBatchError::ExistingResultParameterMismatch {
            model_id: job.model_id.clone(),
            path: job.analysis_result_path.clone(),
        });
    }
    if loaded.manifest.mode != "optimize" {
        return Err(ModelBatchError::NonOptimizedResult {
            model_id: job.model_id.clone(),
            mode: loaded.manifest.mode.clone(),
        });
    }
    Ok(loaded)
}

fn aic(log_likelihood: f64, parameters: usize) -> f64 {
    2.0 * parameters as f64 - 2.0 * log_likelihood
}

fn aicc(log_likelihood: f64, parameters: usize, sample_size: usize) -> Option<f64> {
    if sample_size <= parameters + 1 {
        return None;
    }
    let k = parameters as f64;
    let n = sample_size as f64;
    Some(aic(log_likelihood, parameters) + 2.0 * k * (k + 1.0) / (n - k - 1.0))
}

fn populate_criterion<G, S>(rows: &mut [ComparisonRow], get: G, mut set: S)
where
    G: Fn(&ComparisonRow) -> Option<f64>,
    S: FnMut(&mut ComparisonRow, f64, f64, usize),
{
    let values = rows.iter().filter_map(&get).collect::<Vec<_>>();
    let Some(best) = values.iter().copied().reduce(f64::min) else {
        return;
    };
    let denominator = values
        .iter()
        .map(|value| (-0.5 * (value - best)).exp())
        .sum::<f64>();
    for row in rows {
        if let Some(value) = get(row) {
            let delta = value - best;
            let weight = (-0.5 * delta).exp() / denominator;
            let rank = 1 + values
                .iter()
                .filter(|candidate| **candidate < value)
                .count();
            set(row, delta, weight, rank);
        }
    }
}

fn compare_nested_relationships(
    tables: &[(String, biogeo_core::ParameterTable)],
) -> Vec<NestedRelationship> {
    let mut relationships = Vec::with_capacity(tables.len().saturating_mul(tables.len() - 1));
    for (reduced_index, (reduced_id, reduced)) in tables.iter().enumerate() {
        for (full_index, (full_id, full)) in tables.iter().enumerate() {
            if reduced_index == full_index {
                continue;
            }
            relationships.push(assess_nested_relationship(
                reduced_id, reduced, full_id, full,
            ));
        }
    }
    relationships
}

fn assess_nested_relationship(
    reduced_id: &str,
    reduced: &biogeo_core::ParameterTable,
    full_id: &str,
    full: &biogeo_core::ParameterTable,
) -> NestedRelationship {
    let reduced_parameters = reduced.free_parameter_names().len();
    let full_parameters = full.free_parameter_names().len();
    let degrees_of_freedom = full_parameters.saturating_sub(reduced_parameters);
    let result = assess_nested_parameter_tables(reduced, full);
    let (kind, constrained_parameters, boundary_parameters, reason) = match result {
        Ok(result) => result,
        Err(reason) => (
            NestedRelationshipKind::Undetermined,
            Vec::new(),
            Vec::new(),
            reason,
        ),
    };
    NestedRelationship {
        reduced_model: reduced_id.to_string(),
        full_model: full_id.to_string(),
        kind,
        reduced_parameters,
        full_parameters,
        degrees_of_freedom,
        constrained_parameters,
        boundary_parameters,
        reason,
    }
}

type RelationshipAssessment = (NestedRelationshipKind, Vec<String>, Vec<String>, String);

fn assess_nested_parameter_tables(
    reduced: &biogeo_core::ParameterTable,
    full: &biogeo_core::ParameterTable,
) -> Result<RelationshipAssessment, String> {
    let reduced_parameters = reduced.free_parameter_names().len();
    let full_parameters = full.free_parameter_names().len();
    if full_parameters < reduced_parameters {
        return Ok((
            NestedRelationshipKind::NotNested,
            Vec::new(),
            Vec::new(),
            "full model has fewer free parameters than reduced model".to_string(),
        ));
    }

    let reduced_names = reduced
        .specs()
        .iter()
        .map(|spec| spec.name())
        .collect::<HashSet<_>>();
    let full_names = full
        .specs()
        .iter()
        .map(|spec| spec.name())
        .collect::<HashSet<_>>();
    if reduced_names != full_names {
        return Ok((
            NestedRelationshipKind::NotNested,
            Vec::new(),
            Vec::new(),
            "parameter target sets differ".to_string(),
        ));
    }

    let dimensions = reduced_parameters;
    let reduced_bindings = reduced
        .free_parameter_names()
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            (
                name.to_string(),
                RationalPolynomial::variable(index, dimensions),
            )
        })
        .collect::<HashMap<_, _>>();
    let reduced_expressions = expand_parameter_table(reduced, &reduced_bindings, dimensions)?;
    let full_bindings = full
        .free_parameter_names()
        .into_iter()
        .map(|name| {
            reduced_expressions
                .get(name)
                .cloned()
                .map(|expression| (name.to_string(), expression))
                .ok_or_else(|| format!("full free parameter {name:?} has no reduced target"))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let full_expressions = expand_parameter_table(full, &full_bindings, dimensions)?;
    for spec in reduced.specs() {
        let name = spec.name();
        if !reduced_expressions[name].equivalent(&full_expressions[name]) {
            return Ok((
                NestedRelationshipKind::NotNested,
                Vec::new(),
                Vec::new(),
                format!(
                    "parameter target {name:?} differs after embedding full free parameters into reduced constraints"
                ),
            ));
        }
    }

    let reduced_intervals = expand_parameter_intervals(reduced)?;
    let mut constrained_parameters = Vec::new();
    let mut boundary_parameters = Vec::new();
    for full_spec in full.free_parameter_specs() {
        let name = full_spec.name();
        let reduced_spec = reduced
            .spec(name)
            .ok_or_else(|| format!("reduced model has no parameter {name:?}"))?;
        let constrained = !matches!(reduced_spec.mode(), biogeo_core::ParameterMode::Free { .. });
        if constrained {
            constrained_parameters.push(name.to_string());
        }
        let interval = reduced_intervals[name];
        let bounds = full_spec.bounds();
        let tolerance = 1e-12 * bounds.min.abs().max(bounds.max.abs()).max(1.0);
        let contained =
            interval.min >= bounds.min - tolerance && interval.max <= bounds.max + tolerance;
        let limiting_zero = constrained
            && interval.is_singleton_zero()
            && bounds.min > 0.0
            && bounds.max > bounds.min;
        if !contained && !limiting_zero {
            return Ok((
                NestedRelationshipKind::NotNested,
                constrained_parameters,
                boundary_parameters,
                format!(
                    "embedded range [{}, {}] for full free parameter {name:?} lies outside optimization bounds [{}, {}]",
                    interval.min, interval.max, bounds.min, bounds.max
                ),
            ));
        }
        if limiting_zero
            || constrained
                && interval.is_singleton()
                && ((interval.min - bounds.min).abs() <= tolerance
                    || (interval.max - bounds.max).abs() <= tolerance)
        {
            boundary_parameters.push(name.to_string());
        }
    }

    constrained_parameters.sort();
    boundary_parameters.sort();
    let kind = if full_parameters == reduced_parameters {
        NestedRelationshipKind::Equivalent
    } else if boundary_parameters.is_empty() {
        NestedRelationshipKind::NestedInterior
    } else {
        NestedRelationshipKind::NestedBoundary
    };
    let reason = match kind {
        NestedRelationshipKind::Equivalent => {
            "exact bidimensional parameter-expression embedding; equal free dimension".to_string()
        }
        NestedRelationshipKind::NestedInterior => {
            "exact parameter-expression embedding with interior constraints".to_string()
        }
        NestedRelationshipKind::NestedBoundary => format!(
            "exact limiting parameter-expression embedding at boundary parameter(s): {}",
            boundary_parameters.join(",")
        ),
        NestedRelationshipKind::NotNested | NestedRelationshipKind::Undetermined => unreachable!(),
    };
    Ok((kind, constrained_parameters, boundary_parameters, reason))
}

#[derive(Clone, Debug)]
struct Polynomial {
    terms: BTreeMap<Vec<u16>, f64>,
    dimensions: usize,
}

impl Polynomial {
    fn constant(value: f64, dimensions: usize) -> Self {
        let mut terms = BTreeMap::new();
        if value != 0.0 {
            terms.insert(vec![0; dimensions], value);
        }
        Self { terms, dimensions }
    }

    fn variable(index: usize, dimensions: usize) -> Self {
        let mut exponent = vec![0; dimensions];
        exponent[index] = 1;
        let mut terms = BTreeMap::new();
        terms.insert(exponent, 1.0);
        Self { terms, dimensions }
    }

    fn add_scaled(&self, other: &Self, scale: f64) -> Self {
        debug_assert_eq!(self.dimensions, other.dimensions);
        let mut result = self.clone();
        for (term, coefficient) in &other.terms {
            let next = result.terms.get(term).copied().unwrap_or(0.0) + scale * coefficient;
            if next == 0.0 {
                result.terms.remove(term);
            } else {
                result.terms.insert(term.clone(), next);
            }
        }
        result
    }

    fn multiply(&self, other: &Self) -> Self {
        debug_assert_eq!(self.dimensions, other.dimensions);
        let mut terms = BTreeMap::new();
        for (left_term, left_coefficient) in &self.terms {
            for (right_term, right_coefficient) in &other.terms {
                let exponent = left_term
                    .iter()
                    .zip(right_term)
                    .map(|(left, right)| left.saturating_add(*right))
                    .collect::<Vec<_>>();
                *terms.entry(exponent).or_insert(0.0) += left_coefficient * right_coefficient;
            }
        }
        terms.retain(|_, coefficient| *coefficient != 0.0);
        Self {
            terms,
            dimensions: self.dimensions,
        }
    }

    fn approximately_equal(&self, other: &Self) -> bool {
        let scale = self
            .terms
            .values()
            .chain(other.terms.values())
            .map(|value| value.abs())
            .fold(1.0, f64::max);
        let tolerance = 1e-13 * scale;
        self.terms.keys().chain(other.terms.keys()).all(|term| {
            (self.terms.get(term).copied().unwrap_or(0.0)
                - other.terms.get(term).copied().unwrap_or(0.0))
            .abs()
                <= tolerance
        })
    }
}

#[derive(Clone, Debug)]
struct RationalPolynomial {
    numerator: Polynomial,
    denominator: Polynomial,
}

impl RationalPolynomial {
    fn constant(value: f64, dimensions: usize) -> Self {
        Self {
            numerator: Polynomial::constant(value, dimensions),
            denominator: Polynomial::constant(1.0, dimensions),
        }
    }

    fn variable(index: usize, dimensions: usize) -> Self {
        Self {
            numerator: Polynomial::variable(index, dimensions),
            denominator: Polynomial::constant(1.0, dimensions),
        }
    }

    fn negate(&self) -> Self {
        Self {
            numerator: Polynomial::constant(0.0, self.numerator.dimensions)
                .add_scaled(&self.numerator, -1.0),
            denominator: self.denominator.clone(),
        }
    }

    fn add_scaled(&self, other: &Self, scale: f64) -> Self {
        Self {
            numerator: self
                .numerator
                .multiply(&other.denominator)
                .add_scaled(&other.numerator.multiply(&self.denominator), scale),
            denominator: self.denominator.multiply(&other.denominator),
        }
    }

    fn multiply(&self, other: &Self) -> Self {
        Self {
            numerator: self.numerator.multiply(&other.numerator),
            denominator: self.denominator.multiply(&other.denominator),
        }
    }

    fn divide(&self, other: &Self) -> Result<Self, String> {
        if other.numerator.terms.is_empty() {
            return Err(
                "parameter expression divides by an identically zero expression".to_string(),
            );
        }
        Ok(Self {
            numerator: self.numerator.multiply(&other.denominator),
            denominator: self.denominator.multiply(&other.numerator),
        })
    }

    fn equivalent(&self, other: &Self) -> bool {
        self.numerator
            .multiply(&other.denominator)
            .approximately_equal(&other.numerator.multiply(&self.denominator))
    }
}

fn expand_parameter_table(
    table: &biogeo_core::ParameterTable,
    free_bindings: &HashMap<String, RationalPolynomial>,
    dimensions: usize,
) -> Result<HashMap<String, RationalPolynomial>, String> {
    let mut memo = HashMap::new();
    let mut visiting = HashSet::new();
    for spec in table.specs() {
        expand_parameter(
            table,
            spec.name(),
            free_bindings,
            dimensions,
            &mut memo,
            &mut visiting,
        )?;
    }
    Ok(memo)
}

fn expand_parameter(
    table: &biogeo_core::ParameterTable,
    name: &str,
    free_bindings: &HashMap<String, RationalPolynomial>,
    dimensions: usize,
    memo: &mut HashMap<String, RationalPolynomial>,
    visiting: &mut HashSet<String>,
) -> Result<RationalPolynomial, String> {
    if let Some(value) = memo.get(name) {
        return Ok(value.clone());
    }
    if !visiting.insert(name.to_string()) {
        return Err(format!("parameter expression cycle at {name:?}"));
    }
    let spec = table
        .spec(name)
        .ok_or_else(|| format!("unknown parameter reference {name:?}"))?;
    let value = match spec.mode() {
        biogeo_core::ParameterMode::Fixed { value } => {
            RationalPolynomial::constant(*value, dimensions)
        }
        biogeo_core::ParameterMode::Free { .. } => free_bindings
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing symbolic binding for free parameter {name:?}"))?,
        biogeo_core::ParameterMode::Derived { expression } => expand_parameter_expression(
            table,
            expression,
            free_bindings,
            dimensions,
            memo,
            visiting,
        )?,
    };
    visiting.remove(name);
    memo.insert(name.to_string(), value.clone());
    Ok(value)
}

fn expand_parameter_expression(
    table: &biogeo_core::ParameterTable,
    expression: &biogeo_core::ParameterExpression,
    free_bindings: &HashMap<String, RationalPolynomial>,
    dimensions: usize,
    memo: &mut HashMap<String, RationalPolynomial>,
    visiting: &mut HashSet<String>,
) -> Result<RationalPolynomial, String> {
    use biogeo_core::ParameterExpression as Expression;
    match expression {
        Expression::Constant(value) => Ok(RationalPolynomial::constant(*value, dimensions)),
        Expression::Reference(name) => {
            expand_parameter(table, name, free_bindings, dimensions, memo, visiting)
        }
        Expression::Negate(value) => Ok(expand_parameter_expression(
            table,
            value,
            free_bindings,
            dimensions,
            memo,
            visiting,
        )?
        .negate()),
        Expression::Add(left, right) => Ok(expand_parameter_expression(
            table,
            left,
            free_bindings,
            dimensions,
            memo,
            visiting,
        )?
        .add_scaled(
            &expand_parameter_expression(table, right, free_bindings, dimensions, memo, visiting)?,
            1.0,
        )),
        Expression::Subtract(left, right) => Ok(expand_parameter_expression(
            table,
            left,
            free_bindings,
            dimensions,
            memo,
            visiting,
        )?
        .add_scaled(
            &expand_parameter_expression(table, right, free_bindings, dimensions, memo, visiting)?,
            -1.0,
        )),
        Expression::Multiply(left, right) => Ok(expand_parameter_expression(
            table,
            left,
            free_bindings,
            dimensions,
            memo,
            visiting,
        )?
        .multiply(&expand_parameter_expression(
            table,
            right,
            free_bindings,
            dimensions,
            memo,
            visiting,
        )?)),
        Expression::Divide(left, right) => {
            expand_parameter_expression(table, left, free_bindings, dimensions, memo, visiting)?
                .divide(&expand_parameter_expression(
                    table,
                    right,
                    free_bindings,
                    dimensions,
                    memo,
                    visiting,
                )?)
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Interval {
    min: f64,
    max: f64,
}

impl Interval {
    fn singleton(value: f64) -> Self {
        Self {
            min: value,
            max: value,
        }
    }

    fn is_singleton(self) -> bool {
        (self.max - self.min).abs() <= 1e-14 * self.min.abs().max(self.max.abs()).max(1.0)
    }

    fn is_singleton_zero(self) -> bool {
        self.is_singleton() && self.min.abs() <= 1e-14
    }

    fn negate(self) -> Self {
        Self {
            min: -self.max,
            max: -self.min,
        }
    }

    fn add(self, other: Self) -> Self {
        Self {
            min: self.min + other.min,
            max: self.max + other.max,
        }
    }

    fn subtract(self, other: Self) -> Self {
        self.add(other.negate())
    }

    fn multiply(self, other: Self) -> Self {
        let products = [
            self.min * other.min,
            self.min * other.max,
            self.max * other.min,
            self.max * other.max,
        ];
        Self {
            min: products.into_iter().fold(f64::INFINITY, f64::min),
            max: products.into_iter().fold(f64::NEG_INFINITY, f64::max),
        }
    }

    fn divide(self, other: Self) -> Result<Self, String> {
        if other.min <= 0.0 && other.max >= 0.0 {
            return Err("parameter interval denominator crosses zero".to_string());
        }
        self.multiply(Self {
            min: 1.0 / other.max,
            max: 1.0 / other.min,
        })
        .validate()
    }

    fn validate(self) -> Result<Self, String> {
        if self.min.is_finite() && self.max.is_finite() && self.min <= self.max {
            Ok(self)
        } else {
            Err(format!(
                "parameter interval is invalid: [{}, {}]",
                self.min, self.max
            ))
        }
    }
}

fn expand_parameter_intervals(
    table: &biogeo_core::ParameterTable,
) -> Result<HashMap<String, Interval>, String> {
    let mut memo = HashMap::new();
    let mut visiting = HashSet::new();
    for spec in table.specs() {
        expand_parameter_interval(table, spec.name(), &mut memo, &mut visiting)?;
    }
    Ok(memo)
}

fn expand_parameter_interval(
    table: &biogeo_core::ParameterTable,
    name: &str,
    memo: &mut HashMap<String, Interval>,
    visiting: &mut HashSet<String>,
) -> Result<Interval, String> {
    if let Some(value) = memo.get(name) {
        return Ok(*value);
    }
    if !visiting.insert(name.to_string()) {
        return Err(format!("parameter interval cycle at {name:?}"));
    }
    let spec = table
        .spec(name)
        .ok_or_else(|| format!("unknown parameter reference {name:?}"))?;
    let value = match spec.mode() {
        biogeo_core::ParameterMode::Fixed { value } => Interval::singleton(*value),
        biogeo_core::ParameterMode::Free { .. } => Interval {
            min: spec.bounds().min,
            max: spec.bounds().max,
        },
        biogeo_core::ParameterMode::Derived { expression } => {
            expand_expression_interval(table, expression, memo, visiting)?
        }
    }
    .validate()?;
    visiting.remove(name);
    memo.insert(name.to_string(), value);
    Ok(value)
}

fn expand_expression_interval(
    table: &biogeo_core::ParameterTable,
    expression: &biogeo_core::ParameterExpression,
    memo: &mut HashMap<String, Interval>,
    visiting: &mut HashSet<String>,
) -> Result<Interval, String> {
    use biogeo_core::ParameterExpression as Expression;
    match expression {
        Expression::Constant(value) => Ok(Interval::singleton(*value)),
        Expression::Reference(name) => expand_parameter_interval(table, name, memo, visiting),
        Expression::Negate(value) => {
            Ok(expand_expression_interval(table, value, memo, visiting)?.negate())
        }
        Expression::Add(left, right) => {
            Ok(expand_expression_interval(table, left, memo, visiting)?
                .add(expand_expression_interval(table, right, memo, visiting)?))
        }
        Expression::Subtract(left, right) => {
            Ok(expand_expression_interval(table, left, memo, visiting)?
                .subtract(expand_expression_interval(table, right, memo, visiting)?))
        }
        Expression::Multiply(left, right) => {
            Ok(expand_expression_interval(table, left, memo, visiting)?
                .multiply(expand_expression_interval(table, right, memo, visiting)?))
        }
        Expression::Divide(left, right) => expand_expression_interval(table, left, memo, visiting)?
            .divide(expand_expression_interval(table, right, memo, visiting)?),
    }
}

fn build_likelihood_ratio_tests(
    rows: &[ComparisonRow],
    relationships: &[NestedRelationship],
) -> Vec<LikelihoodRatioTest> {
    let by_model = rows
        .iter()
        .map(|row| (row.model_id.as_str(), row))
        .collect::<HashMap<_, _>>();
    relationships
        .iter()
        .filter(|relationship| relationship.kind.is_strictly_nested())
        .map(|relationship| {
            let reduced = by_model[relationship.reduced_model.as_str()];
            let full = by_model[relationship.full_model.as_str()];
            let boundary = relationship.kind == NestedRelationshipKind::NestedBoundary;
            let note = if boundary {
                "null is on a parameter-space boundary; the ordinary chi-square p-value is not the exact null law"
            } else {
                "ordinary chi-square reference assumes identifiable regular models and adequate asymptotics"
            };
            if !reduced.eligible || !full.eligible {
                return LikelihoodRatioTest {
                    reduced_model: reduced.model_id.clone(),
                    full_model: full.model_id.clone(),
                    relationship: relationship.kind,
                    degrees_of_freedom: relationship.degrees_of_freedom,
                    reduced_log_likelihood: reduced.log_likelihood,
                    full_log_likelihood: full.log_likelihood,
                    statistic: None,
                    p_value_chi_square: None,
                    p_value_half_chi_square: None,
                    status: "not_eligible",
                    note,
                };
            }
            if full.log_likelihood + 1e-8 < reduced.log_likelihood {
                return LikelihoodRatioTest {
                    reduced_model: reduced.model_id.clone(),
                    full_model: full.model_id.clone(),
                    relationship: relationship.kind,
                    degrees_of_freedom: relationship.degrees_of_freedom,
                    reduced_log_likelihood: reduced.log_likelihood,
                    full_log_likelihood: full.log_likelihood,
                    statistic: None,
                    p_value_chi_square: None,
                    p_value_half_chi_square: None,
                    status: "likelihood_order_violation",
                    note: "full-model lnL is below reduced-model lnL beyond tolerance; refit before LRT",
                };
            }
            let statistic = (2.0 * (full.log_likelihood - reduced.log_likelihood)).max(0.0);
            let chi_square = chi_square_survival(
                statistic,
                relationship.degrees_of_freedom,
            );
            let half_chi_square = (boundary
                && relationship.degrees_of_freedom == 1
                && relationship.boundary_parameters.len() == 1)
                .then_some({
                    if statistic == 0.0 {
                        1.0
                    } else {
                        0.5 * chi_square
                    }
                });
            LikelihoodRatioTest {
                reduced_model: reduced.model_id.clone(),
                full_model: full.model_id.clone(),
                relationship: relationship.kind,
                degrees_of_freedom: relationship.degrees_of_freedom,
                reduced_log_likelihood: reduced.log_likelihood,
                full_log_likelihood: full.log_likelihood,
                statistic: Some(statistic),
                p_value_chi_square: Some(chi_square),
                p_value_half_chi_square: half_chi_square,
                status: "available",
                note,
            }
        })
        .collect()
}

fn chi_square_survival(statistic: f64, degrees_of_freedom: usize) -> f64 {
    debug_assert!(statistic >= 0.0 && degrees_of_freedom > 0);
    regularized_gamma_q(degrees_of_freedom as f64 / 2.0, statistic / 2.0)
}

fn regularized_gamma_q(shape: f64, x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let log_scale = -x + shape * x.ln() - log_gamma(shape);
    if x < shape + 1.0 {
        let mut term = 1.0 / shape;
        let mut sum = term;
        let mut denominator = shape;
        for _ in 0..10_000 {
            denominator += 1.0;
            term *= x / denominator;
            sum += term;
            if term.abs() <= sum.abs() * 1e-15 {
                break;
            }
        }
        (1.0 - sum * log_scale.exp()).clamp(0.0, 1.0)
    } else {
        const TINY: f64 = 1e-300;
        let mut b = x + 1.0 - shape;
        let mut c = 1.0 / TINY;
        let mut d = 1.0 / b.max(TINY);
        let mut product = d;
        for iteration in 1..10_000 {
            let index = iteration as f64;
            let coefficient = -index * (index - shape);
            b += 2.0;
            d = coefficient * d + b;
            if d.abs() < TINY {
                d = TINY;
            }
            c = b + coefficient / c;
            if c.abs() < TINY {
                c = TINY;
            }
            d = 1.0 / d;
            let delta = d * c;
            product *= delta;
            if (delta - 1.0).abs() <= 1e-15 {
                break;
            }
        }
        (product * log_scale.exp()).clamp(0.0, 1.0)
    }
}

fn log_gamma(value: f64) -> f64 {
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if value < 0.5 {
        return std::f64::consts::PI.ln()
            - (std::f64::consts::PI * value).sin().ln()
            - log_gamma(1.0 - value);
    }
    let shifted = value - 1.0;
    let mut series = COEFFICIENTS[0];
    for (index, coefficient) in COEFFICIENTS.iter().enumerate().skip(1) {
        series += coefficient / (shifted + index as f64);
    }
    let base = shifted + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (shifted + 0.5) * base.ln() - base + series.ln()
}

fn format_comparison(
    rows: &[ComparisonRow],
    relationships: &[NestedRelationship],
    likelihood_ratio_tests: &[LikelihoodRatioTest],
) -> String {
    let eligible = rows.iter().filter(|row| row.eligible).count();
    let aicc_defined = rows.iter().filter(|row| row.aicc.is_some()).count();
    let aicc_eligible = rows.iter().filter(|row| row.aicc_weight.is_some()).count();
    let sample_size = rows.first().map_or(0, |row| row.tips);
    let mut output = format!(
        "format\t{}\nmodels\t{}\neligible_models\t{}\naicc_defined_models\t{}\naicc_eligible_models\t{}\nsample_size\t{}\nsample_size_definition\ttips\nnested_relationships\t{}\nlikelihood_ratio_tests\t{}\nnesting_method\texact_parameter-expression_embedding_in_canonical_BioGeoBEARS_coordinates\nboundary_lrt_policy\tstandard_chi_square_reported_with_explicit_warning; half_chi_square_only_for_one_boundary_constraint_and_df_1\n\nmodel_comparison\nmodel_id\tanalysis_result\tconverged\tconverged_starts\tnumparams\ttips\tlnL\teligible\tAIC\tdelta_AIC\tAIC_weight\tAIC_rank\tAICc\tdelta_AICc\tAICc_weight\tAICc_rank\n",
        MODEL_COMPARISON_FORMAT,
        rows.len(),
        eligible,
        aicc_defined,
        aicc_eligible,
        sample_size,
        relationships.len(),
        likelihood_ratio_tests.len(),
    );
    for row in rows {
        output.push_str(&row.model_id);
        output.push('\t');
        output.push_str(&row.analysis_result);
        output.push('\t');
        output.push_str(if row.converged { "true" } else { "false" });
        output.push('\t');
        output.push_str(&row.converged_starts.to_string());
        output.push('\t');
        output.push_str(&row.parameters.to_string());
        output.push('\t');
        output.push_str(&row.tips.to_string());
        output.push('\t');
        output.push_str(&format!("{:.17}", row.log_likelihood));
        output.push('\t');
        output.push_str(if row.eligible { "true" } else { "false" });
        push_optional_float(&mut output, row.aic);
        push_optional_float(&mut output, row.delta_aic);
        push_optional_float(&mut output, row.aic_weight);
        push_optional_usize(&mut output, row.aic_rank);
        push_optional_float(&mut output, row.aicc);
        push_optional_float(&mut output, row.delta_aicc);
        push_optional_float(&mut output, row.aicc_weight);
        push_optional_usize(&mut output, row.aicc_rank);
        output.push('\n');
    }
    output.push_str("\nnested_model_relationships\nreduced_model\tfull_model\trelation\treduced_numparams\tfull_numparams\tdf\tconstrained_parameters\tboundary_parameters\treason\n");
    for relationship in relationships {
        output.push_str(&relationship.reduced_model);
        output.push('\t');
        output.push_str(&relationship.full_model);
        output.push('\t');
        output.push_str(relationship.kind.as_str());
        output.push('\t');
        output.push_str(&relationship.reduced_parameters.to_string());
        output.push('\t');
        output.push_str(&relationship.full_parameters.to_string());
        output.push('\t');
        output.push_str(&relationship.degrees_of_freedom.to_string());
        output.push('\t');
        output.push_str(&encode_field(
            &relationship.constrained_parameters.join(","),
        ));
        output.push('\t');
        output.push_str(&encode_field(&relationship.boundary_parameters.join(",")));
        output.push('\t');
        output.push_str(&encode_field(&relationship.reason));
        output.push('\n');
    }
    output.push_str("\nlikelihood_ratio_tests\nreduced_model\tfull_model\trelation\tdf\tlnL_reduced\tlnL_full\tLR_statistic\tp_value_chi_square\tp_value_half_chi_square\tstatus\tnote\n");
    for test in likelihood_ratio_tests {
        output.push_str(&test.reduced_model);
        output.push('\t');
        output.push_str(&test.full_model);
        output.push('\t');
        output.push_str(test.relationship.as_str());
        output.push('\t');
        output.push_str(&test.degrees_of_freedom.to_string());
        output.push('\t');
        output.push_str(&format!("{:.17}", test.reduced_log_likelihood));
        output.push('\t');
        output.push_str(&format!("{:.17}", test.full_log_likelihood));
        push_optional_float(&mut output, test.statistic);
        push_optional_float(&mut output, test.p_value_chi_square);
        push_optional_float(&mut output, test.p_value_half_chi_square);
        output.push('\t');
        output.push_str(test.status);
        output.push('\t');
        output.push_str(test.note);
        output.push('\n');
    }
    output
}

fn push_optional_float(output: &mut String, value: Option<f64>) {
    output.push('\t');
    match value {
        Some(value) => output.push_str(&format!("{value:.17}")),
        None => output.push_str("NA"),
    }
}

fn push_optional_usize(output: &mut String, value: Option<usize>) {
    output.push('\t');
    match value {
        Some(value) => output.push_str(&value.to_string()),
        None => output.push_str("NA"),
    }
}

fn validate_model_id(
    manifest_path: &Path,
    line_number: usize,
    model_id: &str,
) -> Result<(), ModelBatchError> {
    if !is_portable_id(model_id) {
        return Err(invalid_manifest(
            manifest_path,
            line_number,
            format!(
                "invalid model_id {model_id:?}; use an ASCII letter/digit followed by letters, digits, '.', '_', '+', or '-', without a trailing period or Windows reserved name"
            ),
        ));
    }
    Ok(())
}

pub(crate) fn is_portable_id(value: &str) -> bool {
    let mut characters = value.chars();
    let valid_first = characters
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric());
    let valid_rest = characters.all(|character| {
        character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '+' | '-')
    });
    let windows_stem = value.split('.').next().unwrap_or_default();
    let windows_reserved = matches!(
        windows_stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "CLOCK$"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    );
    valid_first && valid_rest && !value.ends_with('.') && !windows_reserved
}

fn relative_result_path(root: &Path, path: &Path) -> Result<String, ModelBatchError> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| ModelBatchError::ResultOutsideWorkspace(path.to_path_buf()))?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn utf8_path(path: &Path) -> Result<&str, ModelBatchError> {
    path.to_str()
        .ok_or_else(|| ModelBatchError::NonUtf8Path(path.to_path_buf()))
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), ModelBatchError> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| ModelBatchError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.write_all(bytes)
        .map_err(|source| ModelBatchError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    file.sync_all().map_err(|source| ModelBatchError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_once_or_verify(path: &Path, bytes: &[u8]) -> Result<(), ModelBatchError> {
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
            Err(ModelBatchError::Io {
                path: path.to_path_buf(),
                source,
            })
        }
    }
}

fn verify_file(path: &Path, expected: &[u8]) -> Result<(), ModelBatchError> {
    let actual = fs::read(path).map_err(|source| ModelBatchError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    if actual != expected {
        return Err(ModelBatchError::ResumeIdentityMismatch {
            path: path.to_path_buf(),
            expected: analysis_result::stable_fingerprint(expected),
            actual: analysis_result::stable_fingerprint(&actual),
        });
    }
    Ok(())
}

fn invalid_manifest(path: &Path, line: usize, message: impl Into<String>) -> ModelBatchError {
    ModelBatchError::InvalidManifest {
        path: path.to_path_buf(),
        line,
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum ModelBatchError {
    InvalidManifest {
        path: PathBuf,
        line: usize,
        message: String,
    },
    OutputDirectoryExists(PathBuf),
    MissingOutputDirectory(PathBuf),
    MissingModelsDirectory(PathBuf),
    AttemptJobMismatch,
    AttemptIndexOverflow,
    ResumeIdentityMismatch {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    ExistingResultParameterMismatch {
        model_id: String,
        path: PathBuf,
    },
    NonOptimizedResult {
        model_id: String,
        mode: String,
    },
    MissingOptimizationSummary(String),
    MissingComparisonInput {
        model_id: String,
        role: &'static str,
    },
    UnsupportedObservationModel {
        model_id: String,
        value: String,
    },
    IncompatibleDataset {
        baseline_model_id: String,
        model_id: String,
        expected: String,
        actual: String,
    },
    AnalysisResult {
        model_id: String,
        source: Box<analysis_result::AnalysisResultError>,
    },
    ParameterTable {
        model_id: String,
        source: biogeo_core::ParameterTableParseError,
    },
    ResultOutsideWorkspace(PathBuf),
    NonUtf8Path(PathBuf),
    UnsafePath(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for ModelBatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest {
                path,
                line,
                message,
            } => write!(
                f,
                "invalid model-batch manifest {} at line {line}: {message}",
                path.display()
            ),
            Self::OutputDirectoryExists(path) => write!(
                f,
                "model-batch output directory already exists; use --resume only for the same task: {}",
                path.display()
            ),
            Self::MissingOutputDirectory(path) => write!(
                f,
                "cannot resume because model-batch output directory does not exist: {}",
                path.display()
            ),
            Self::MissingModelsDirectory(path) => {
                write!(
                    f,
                    "model-batch models directory is missing: {}",
                    path.display()
                )
            }
            Self::AttemptJobMismatch => {
                write!(f, "model-batch attempt rows do not match the prepared jobs")
            }
            Self::AttemptIndexOverflow => write!(f, "model-batch attempt index overflowed usize"),
            Self::ResumeIdentityMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "model-batch resume identity differs at {} (expected {expected}, found {actual})",
                path.display()
            ),
            Self::ExistingResultParameterMismatch { model_id, path } => write!(
                f,
                "existing result for model {model_id:?} at {} was produced from different parameter-table bytes",
                path.display()
            ),
            Self::NonOptimizedResult { model_id, mode } => write!(
                f,
                "model {model_id:?} has analysis mode {mode:?}; model-batch comparison requires optimize results"
            ),
            Self::MissingOptimizationSummary(model_id) => {
                write!(f, "model {model_id:?} is missing its optimization summary")
            }
            Self::MissingComparisonInput { model_id, role } => write!(
                f,
                "model {model_id:?} is missing comparison input role {role:?}"
            ),
            Self::UnsupportedObservationModel { model_id, value } => write!(
                f,
                "model {model_id:?} uses unsupported tip observation model {value:?}"
            ),
            Self::IncompatibleDataset {
                baseline_model_id,
                model_id,
                expected,
                actual,
            } => write!(
                f,
                "model {model_id:?} cannot be compared with baseline {baseline_model_id:?}: dataset/state-space identity differs (expected {expected}, found {actual})"
            ),
            Self::AnalysisResult { model_id, source } => {
                write!(
                    f,
                    "failed to load model {model_id:?} analysis result: {source}"
                )
            }
            Self::ParameterTable { model_id, source } => write!(
                f,
                "failed to parse source parameter table for model {model_id:?}: {source}"
            ),
            Self::ResultOutsideWorkspace(path) => write!(
                f,
                "analysis result is outside its model-batch workspace: {}",
                path.display()
            ),
            Self::NonUtf8Path(path) => {
                write!(f, "model-batch path is not valid UTF-8: {}", path.display())
            }
            Self::UnsafePath(path) => write!(
                f,
                "model-batch path contains a tab or newline and cannot enter TSV output: {}",
                path.display()
            ),
            Self::Io { path, source } => {
                write!(f, "I/O failed for {}: {source}", path.display())
            }
        }
    }
}

impl Error for ModelBatchError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AnalysisResult { source, .. } => Some(source.as_ref()),
            Self::ParameterTable { source, .. } => Some(source),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_relative_manifest_paths_and_rejects_unsafe_ids() {
        let manifest_path = Path::new("batch/models.tsv");
        let entries = parse_model_batch_manifest(
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\t../dec.tsv\nDEC+J\t../decj.tsv\n",
            manifest_path,
        )
        .unwrap();
        assert_eq!(entries[0].parameters_path, Path::new("batch/../dec.tsv"));
        assert_eq!(entries[1].model_id, "DEC+J");

        let error = parse_model_batch_manifest(
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\n../DEC\tdec.tsv\n",
            manifest_path,
        )
        .unwrap_err();
        assert!(matches!(error, ModelBatchError::InvalidManifest { .. }));

        for unsafe_id in ["CON.txt", "model."] {
            let input = format!(
                "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\n{unsafe_id}\tdec.tsv\n"
            );
            assert!(matches!(
                parse_model_batch_manifest(&input, manifest_path),
                Err(ModelBatchError::InvalidManifest { .. })
            ));
        }

        assert!(matches!(
            parse_model_batch_manifest(
                "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\ndec\tdec2.tsv\n",
                manifest_path,
            ),
            Err(ModelBatchError::InvalidManifest { .. })
        ));
    }

    #[test]
    fn cancelled_attempt_distinguishes_cancelled_and_not_started_models() {
        let jobs = [test_job("DEC"), test_job("DEC+J"), test_job("DIVALIKE")];
        let reports = [
            ModelBatchJobReport::complete(&jobs[0]),
            ModelBatchJobReport::cancelled(&jobs[1], "task_cancelled", "stopped".to_string()),
            ModelBatchJobReport::not_started(&jobs[2]),
        ];

        let output = format_attempt(&reports);
        assert!(output.starts_with("format\tbiogeo-model-batch-attempt-v2\n"));
        assert!(output.contains("status\tcancelled\n"));
        assert!(output.contains("complete_models\t1\n"));
        assert!(output.contains("cancelled_models\t1\n"));
        assert!(output.contains("not_started_models\t1\n"));
        assert!(output.contains("DEC+J\tcancelled\tmodels/DEC+J\ttask_cancelled\tstopped\n"));
        assert!(output.contains("DIVALIKE\tnot_started\tmodels/DIVALIKE\tNA\tNA\n"));
    }

    fn test_job(model_id: &str) -> PreparedModelBatchJob {
        PreparedModelBatchJob {
            model_id: model_id.to_string(),
            parameters_path: PathBuf::from(format!("{model_id}.tsv")),
            parameters_fingerprint: "parameters".to_string(),
            analysis_result_path: PathBuf::from("models").join(model_id),
        }
    }

    #[test]
    fn aic_aicc_and_weights_match_biogeobears_formulas() {
        let mut rows = vec![
            test_row("DEC", -30.0, 2, 19),
            test_row("DEC+J", -28.0, 3, 19),
            test_row("custom", -27.5, 4, 19),
        ];
        for row in &mut rows {
            row.aic = Some(aic(row.log_likelihood, row.parameters));
            row.aicc = aicc(row.log_likelihood, row.parameters, row.tips);
        }
        populate_model_weights(&mut rows);

        let golden = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../validation/golden/biogeobears-model-comparison.tsv"
        ));
        for (row, line) in rows.iter().zip(golden.lines().skip(1)) {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(row.model_id, fields[0]);
            assert_close(row.aic.unwrap(), fields[4].parse().unwrap(), 1e-14);
            assert_close(row.aicc.unwrap(), fields[5].parse().unwrap(), 1e-13);
            assert_close(row.aic_weight.unwrap(), fields[6].parse().unwrap(), 1e-14);
            assert_close(row.aicc_weight.unwrap(), fields[7].parse().unwrap(), 1e-14);
        }
        let aic_weight_sum = rows.iter().map(|row| row.aic_weight.unwrap()).sum::<f64>();
        let aicc_weight_sum = rows.iter().map(|row| row.aicc_weight.unwrap()).sum::<f64>();
        assert!((aic_weight_sum - 1.0).abs() < 1e-15);
        assert!((aicc_weight_sum - 1.0).abs() < 1e-15);
    }

    #[test]
    fn aicc_is_unavailable_when_the_tip_sample_is_too_small() {
        assert_eq!(aicc(-10.0, 3, 4), None);
        assert!(aicc(-10.0, 3, 5).is_some());
    }

    #[test]
    fn aicc_weights_require_the_full_aic_candidate_set() {
        let mut rows = vec![test_row("DEC", -10.0, 2, 4), test_row("DEC+J", -9.0, 3, 4)];
        for row in &mut rows {
            row.aic = Some(aic(row.log_likelihood, row.parameters));
            row.aicc = aicc(row.log_likelihood, row.parameters, row.tips);
        }
        populate_model_weights(&mut rows);

        assert!(rows.iter().all(|row| row.aic_weight.is_some()));
        assert!(rows[0].aicc.is_some());
        assert!(rows[1].aicc.is_none());
        assert!(rows.iter().all(|row| row.aicc_weight.is_none()));
    }

    #[test]
    fn proves_plus_j_presets_are_boundary_nested_and_rejects_cross_family_pairs() {
        for (reduced_preset, full_preset) in [
            (
                biogeo_core::BioGeoBearsPreset::Dec,
                biogeo_core::BioGeoBearsPreset::DecJ,
            ),
            (
                biogeo_core::BioGeoBearsPreset::DivaLike,
                biogeo_core::BioGeoBearsPreset::DivaLikeJ,
            ),
            (
                biogeo_core::BioGeoBearsPreset::BayAreaLike,
                biogeo_core::BioGeoBearsPreset::BayAreaLikeJ,
            ),
        ] {
            let reduced = reduced_preset.parameter_table().unwrap();
            let full = full_preset.parameter_table().unwrap();
            let (kind, constrained, boundary, _) =
                assess_nested_parameter_tables(&reduced, &full).unwrap();
            assert_eq!(kind, NestedRelationshipKind::NestedBoundary);
            assert_eq!(constrained, ["j"]);
            assert_eq!(boundary, ["j"]);

            let (reverse, _, _, _) = assess_nested_parameter_tables(&full, &reduced).unwrap();
            assert_eq!(reverse, NestedRelationshipKind::NotNested);
        }

        let dec = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap();
        let divalikej = biogeo_core::BioGeoBearsPreset::DivaLikeJ
            .parameter_table()
            .unwrap();
        let (cross_family, _, _, _) = assess_nested_parameter_tables(&dec, &divalikej).unwrap();
        assert_eq!(cross_family, NestedRelationshipKind::NotNested);
    }

    #[test]
    fn chi_square_survival_matches_reference_values() {
        assert_close(chi_square_survival(4.0, 1), 0.045_500_263_896_358_4, 2e-15);
        assert_close(
            chi_square_survival(6.0, 2),
            0.049_787_068_367_863_944,
            2e-15,
        );
        assert_eq!(chi_square_survival(0.0, 3), 1.0);
    }

    #[test]
    fn boundary_lrt_reports_both_reference_probabilities() {
        let rows = vec![
            test_row("DEC", -10.0, 2, 20),
            test_row("DEC+J", -8.0, 3, 20),
        ];
        let relationships = vec![NestedRelationship {
            reduced_model: "DEC".to_string(),
            full_model: "DEC+J".to_string(),
            kind: NestedRelationshipKind::NestedBoundary,
            reduced_parameters: 2,
            full_parameters: 3,
            degrees_of_freedom: 1,
            constrained_parameters: vec!["j".to_string()],
            boundary_parameters: vec!["j".to_string()],
            reason: "test".to_string(),
        }];
        let tests = build_likelihood_ratio_tests(&rows, &relationships);
        assert_eq!(tests.len(), 1);
        assert_eq!(tests[0].status, "available");
        assert_eq!(tests[0].statistic, Some(4.0));
        assert_close(
            tests[0].p_value_chi_square.unwrap(),
            0.045_500_263_896_358_4,
            2e-15,
        );
        assert_close(
            tests[0].p_value_half_chi_square.unwrap(),
            0.022_750_131_948_179_2,
            2e-15,
        );
    }

    fn test_row(
        model_id: &str,
        log_likelihood: f64,
        parameters: usize,
        tips: usize,
    ) -> ComparisonRow {
        ComparisonRow {
            model_id: model_id.to_string(),
            analysis_result: format!("models/{model_id}"),
            converged: true,
            converged_starts: 1,
            parameters,
            tips,
            log_likelihood,
            eligible: true,
            aic: None,
            delta_aic: None,
            aic_weight: None,
            aic_rank: None,
            aicc: None,
            delta_aicc: None,
            aicc_weight: None,
            aicc_rank: None,
        }
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!(
            (actual - expected).abs() <= tolerance,
            "expected {expected:.17}, got {actual:.17}"
        );
    }
}
