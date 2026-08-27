mod analysis_request;
mod analysis_result;
mod bsm_inspect;
mod cli_help;
mod dataset_batch;
mod engine_info;
mod fossil_placement;
mod fs_retry;
mod input_bundle;
mod legacy_import;
mod model_average;
mod model_batch;
mod model_workflow;
mod process_telemetry;
mod progress;

use std::env;
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io::{self, BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::mem::size_of;
use std::num::{ParseFloatError, ParseIntError};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::thread;
use std::time::{Duration, Instant};

use progress::{ProgressEvent, ProgressOutputFormat, ProgressReporter};

const USAGE: &str = "\
Usage:
  biogeo-cli --version
  biogeo-cli engine-info
  biogeo-cli convert-tree --tree <tree.newick|tree.nex> [--tree-name <name>]
  biogeo-cli convert-ranges --ranges <ranges.data|ranges.csv> [--input-format <auto|lagrange|csv>] [--taxon-column <name>] [--taxon-map <map.tsv>] [--area-map <map.tsv>]
  biogeo-cli convert-biogeobears-strata --time-boundaries <file> [--dispersal-matrices <file>] [--adjacency-matrices <file>] [--adjacency-range-rule <all-pairs|edge-covered> --max-range-size <n>] --output-dir <dir>
  biogeo-cli fossil-place --tree <tree.newick|tree.nex> --manifest <fossils.tsv> --output-dir <dir> [options]
  biogeo-cli validate-inputs --tree <tree.newick|tree.nex> --ranges <ranges.tsv> [options]
  biogeo-cli dec --tree <tree.newick> --ranges <ranges.tsv> --d <rate> --e <rate> [options]
  biogeo-cli divalike --tree <tree.newick> --ranges <ranges.tsv> --d <rate> --e <rate> [options]
  biogeo-cli bayarealike --tree <tree.newick> --ranges <ranges.tsv> --d <rate> --e <rate> [options]
  biogeo-cli dec-optimize --tree <tree.newick> --ranges <ranges.tsv> [options]
  biogeo-cli divalike-optimize --tree <tree.newick> --ranges <ranges.tsv> [options]
  biogeo-cli bayarealike-optimize --tree <tree.newick> --ranges <ranges.tsv> [options]
  biogeo-cli dec-x-optimize --tree <tree.newick> --ranges <ranges.tsv> (--distance-matrix <matrix.tsv> | --dispersal-strata <strata.tsv>) [options]
  biogeo-cli dec-n-optimize --tree <tree.newick> --ranges <ranges.tsv> (--environment-distance-matrix <matrix.tsv> | --dispersal-strata <strata.tsv>) [options]
  biogeo-cli dec-u-optimize --tree <tree.newick> --ranges <ranges.tsv> (--area-sizes <sizes.tsv> | --dispersal-strata <strata.tsv>) [options]
  biogeo-cli dec-xnu-optimize --tree <tree.newick> --ranges <ranges.tsv> (--distance-matrix <matrix.tsv> --environment-distance-matrix <matrix.tsv> --area-sizes <sizes.tsv> | --dispersal-strata <strata.tsv>) [options]
  biogeo-cli dec-xn-profile --tree <tree.newick> --ranges <ranges.tsv> (--distance-matrix <matrix.tsv> --environment-distance-matrix <matrix.tsv> --area-sizes <sizes.tsv> | --dispersal-strata <strata.tsv>) --area-exponent <u> [options]
  biogeo-cli dec-xu-profile --tree <tree.newick> --ranges <ranges.tsv> (--distance-matrix <matrix.tsv> --environment-distance-matrix <matrix.tsv> --area-sizes <sizes.tsv> | --dispersal-strata <strata.tsv>) --environment-distance-exponent <n> [options]
  biogeo-cli dec-nu-profile --tree <tree.newick> --ranges <ranges.tsv> (--distance-matrix <matrix.tsv> --environment-distance-matrix <matrix.tsv> --area-sizes <sizes.tsv> | --dispersal-strata <strata.tsv>) --distance-exponent <x> [options]
  biogeo-cli decj-optimize --tree <tree.newick> --ranges <ranges.tsv> [options]
  biogeo-cli divalikej-optimize --tree <tree.newick> --ranges <ranges.tsv> [options]
  biogeo-cli bayarealikej-optimize --tree <tree.newick> --ranges <ranges.tsv> [options]
  biogeo-cli parameter-template --preset <preset>
  biogeo-cli analysis-template --preset <preset> --mode <evaluate|optimize> --output-dir <dir>
  biogeo-cli analysis-plan --request <analysis.tsv>
  biogeo-cli analysis-run --request <analysis.tsv> --output-dir <dir>
  biogeo-cli analysis-workflow --request <analysis.tsv> --output-dir <dir> --bsm-samples <n> [options]
  biogeo-cli model-workflow-plan --request <workflow.tsv>
  biogeo-cli model-workflow --request <workflow.tsv> --output-dir <dir> [--resume]
  biogeo-cli model-evaluate --tree <tree.newick> (--ranges <ranges.tsv> | --use-detection-model --detections <counts.tsv> --controls <counts.tsv>) --parameters <parameters.tsv> [options]
  biogeo-cli model-optimize --tree <tree.newick> (--ranges <ranges.tsv> | --use-detection-model --detections <counts.tsv> --controls <counts.tsv>) --parameters <parameters.tsv> [options]
  biogeo-cli model-batch --manifest <models.tsv> --output-dir <dir> --tree <tree.newick> (--ranges <ranges.tsv> | --use-detection-model --detections <counts.tsv> --controls <counts.tsv>) [options]
  biogeo-cli dataset-batch --manifest <datasets.tsv> --output-dir <dir> [--resume]
  biogeo-cli model-bsm --analysis-result <dir> --bsm-samples <n> [options]
  biogeo-cli bsm-inspect --bsm-result <dir> [--deep]
  biogeo-cli analysis-result-inspect --analysis-result <dir> [--replay]
  biogeo-cli analysis-result-migrate --analysis-result <v1-dir> --output-dir <v2-dir>
  biogeo-cli input-bundle-inspect --input-bundle <dir>

Options:
  --error-format <kind>   Global error output: human or tsv. Must precede the command.
  --progress-format <kind>
                           Live stderr progress for model-optimize and batch commands: none or tsv.
                           Must precede the command.
  --tree <tree>           Rooted Newick or NEXUS input. NEXUS TRANSLATE is supported.
  --tree-name <name>      Explicit TREE name for multi-tree NEXUS; never defaults to the first tree.
  --ranges <file>         Canonical TSV, BioGeoBEARS/LAGRANGE .data, or CSV range matrix.
  --input-format <kind>   Range conversion input: auto, lagrange, or csv. Defaults to auto.
  --taxon-column <name>   CSV taxon-name column; auto-detects Name, tip, taxon, or species.
  --taxon-map <map.tsv>   Explicit source_taxon/target_taxon renaming during range conversion.
  --area-map <map.tsv>    Explicit source_area/target_area renaming during range conversion.
  --adjacency-range-rule <kind>
                           Block adjacency import: all-pairs (default) or edge-covered.
  --use-ambiguities       Treat ? in a range table as BioGeoBEARS presence/absence ambiguity.
  --parameters <table.tsv>
                           Versioned biogeo-parameter-table-v1 configuration.
  --request <analysis.tsv> Versioned biogeo-analysis-request-v1 task configuration.
  --mode <kind>           Analysis template mode: evaluate or optimize.
  --analysis-result-dir <dir>
                           Write a portable, non-overwriting biogeo-analysis-result-v2 directory.
  --analysis-result <dir>  Load a completed fitted-model directory for model-bsm.
  --bsm-result <dir>       Inspect an existing streamed BSM result directory.
  --deep                   Scan all BSM rows and validate counts, occupancy, paths, and constraints.
  --input-bundle <dir>     Validate a standalone biogeo-input-bundle-v1 directory.
  --replay                 Recompute model identity and lnL while inspecting an analysis result.
  --manifest <table.tsv>   Versioned fossil-placement, model-batch, or dataset-batch manifest.
  --output-dir <dir>       New non-overwriting result directory.
  --replicates <n>         Fossil-placement tree replicates. Defaults to 1.
  --direct-ancestor-hook-length <x>
                           Short fossil hook branch for direct-ancestor placement. Defaults to 1e-7.
  --resume                 Resume the same workflow or batch task and reuse validated completed results.
  --use-detection-model    Build tip likelihoods from detection and control counts using mf/dp/fdp.
  --detections <counts.tsv>
                           Target-OTU detection counts by tip and area.
  --controls <counts.tsv>  Inclusive taphonomic-control counts by tip and area.
  --preset <name>          Parameter template: dec, dec+j, divalike, divalike+j,
                           bayarealike, or bayarealike+j.
  --max-range-size <n>    Maximum allowed range size. Defaults to the number of areas.
  --max-states <n>        Reject before allocation if the combinatorial state count exceeds n.
  --include-null-range    Include the null range as an absorbing state.
  --root-prior <kind>     Root prior: flat or equal. Defaults to flat.
  --min-branch-length <x> Treat child branches shorter than x as BioGeoBEARS direct-ancestor hooks.
  --fill-missing-branch-length <x>
                          Explicitly fill omitted non-root branch lengths; default is to reject.
                           Defaults to 0 (disabled); BioGeoBEARS defaults to 1e-6.
  --tip-age-tolerance <x> Present-day tip age tolerance for validate-inputs. Defaults to
                           root_age * 1e-9 + 1e-12; use 0 for strict comparison.
  --dispersal-multipliers <matrix.tsv>
                           Directional area-to-area multipliers for anagenetic d and founder-event j.
  --dispersal-strata <strata.tsv>
                           Piecewise anagenetic inputs by oldest age boundary; accepts legacy
                           matrix strata or raw modifier columns with optional range constraints.
  --distance-matrix <matrix.tsv>
                           Pairwise distances, transformed element-wise as distance^x.
  --distance-exponent <x> Distance exponent x; requires a static or stratified distance matrix.
  --environment-distance-matrix <matrix.tsv>
                           Pairwise environmental distances, transformed as envdistance^n.
  --environment-distance-exponent <n>
                           Environmental exponent n; requires a static or stratified matrix.
  --extirpation-multipliers <multipliers.tsv>
                           Area-specific multipliers applied to e.
  --area-sizes <sizes.tsv> Positive raw area sizes, transformed as area_size^u.
  --area-exponent <u>      Area-size exponent u; requires static or stratified area sizes.
  --additional-start <v1,v2,...>
                           Additional model-space start in parameter-table free-parameter order;
                           repeatable and available only to model-optimize.
  --j <weight>            Fixed founder-event weight for a +J preset. Defaults to 0.
  --mx01 <value>          Linked daughter-size constraint. Defaults to 0.0001.
  --mx01y <value>         Override range-copying daughter-size constraint.
  --mx01s <value>         Override subset-sympatry daughter-size constraint.
  --mx01v <value>         Override vicariance daughter-size constraint.
  --mx01j <value>         Override founder-event daughter-size constraint.
  --ancestral-probs       Append internal-node range probabilities to the output.
  --split-probs           Append cladogenetic split scenario probabilities to the output.
  --traceback-samples <n> Append n conditional history skeleton samples (fixed models only).
  --bsm-samples <n>       Sample n full biogeographic stochastic histories (fixed models or model-bsm).
  --bsm-output-dir <dir>  Stream BSM tables to a new directory instead of retaining all histories.
  --bsm-output-level <level>
                           Stream layout: legacy (default), full, compact, or summary.
  --bsm-threads <auto|n>  BSM worker threads. Defaults to available parallelism, capped by samples.
  --bsm-max-in-flight <n> Maximum BSM samples retained before ordered writing. Defaults to 2x workers.
  --bsm-max-events-per-sample <n>
                           Maximum along-branch d/e events in one BSM sample. Defaults to unlimited.
  --bsm-max-events-total <n>
                           Maximum along-branch d/e events in the ordered BSM task prefix. Defaults to unlimited.
  --bsm-memory-budget-mb <n>
                           Completed-history window budget in MiB; requires streamed output and a per-sample event limit.
  --bsm-shard-samples <n>  Store each fixed-size sample interval in its own eight-table directory.
  --bsm-checkpoint-samples <n>
                           Commit streamed BSM tables every n samples. Defaults to max(1024, max-in-flight), capped by samples.
  --bsm-resume             Resume a compatible streamed BSM output directory from its last checkpoint.
  --bsm-time-limit-seconds <seconds>
                           Stop BSM cooperatively after this many seconds. Defaults to unlimited.
  --bsm-interactive        Read pause, resume, status, and cancel commands from standard input.
  --seed <u64>            Random seed for conditional history or BSM sampling. Defaults to 1.
  --init-d <rate>         Initial d for optimization. Defaults to 0.01.
  --init-e <rate>         Initial e for optimization. Defaults to 0.01.
  --init-j <weight>       Initial j for +J optimization. Defaults to 0.01.
  --min-rate <rate>       Lower positive optimization bound. Defaults to 1e-12.
  --max-rate <rate>       Upper optimization bound. Defaults to 10.
  --min-j <weight>        Lower +J optimization bound. Defaults to 1e-5.
  --max-j <weight>        Upper +J bound. Defaults to 2.99999/1.99999/0.99999 by preset.
  --initial-log-step <x>  Initial Nelder-Mead step in log-rate space. Defaults to 0.5.
  --initial-step <x>      Generic parameter-table optimizer step in transformed coordinates.
  --init-exponent <x>     Initial x, n, or u for exponent optimization. Defaults to 0.
  --min-exponent <x>      Lower exponent bound. Defaults to BioGeoBEARS x/n/u bounds.
  --max-exponent <x>      Upper exponent bound. Defaults to BioGeoBEARS x/n/u bounds.
  --initial-exponent-step <x>
                           Initial Nelder-Mead step on the exponent axis. Defaults to 0.5.
  --init-x/--init-n/--init-u <x>
                           Initial exponents for joint x/n/u optimization.
  --min-x/--max-x <x>      Joint x bounds. Defaults to -2.5 and 2.5.
  --min-n/--max-n <x>      Joint n bounds. Defaults to -10 and 10.
  --min-u/--max-u <x>      Joint u bounds. Defaults to -10 and 10.
  --initial-x-step <x>     Initial joint-optimization simplex step for x.
  --initial-n-step <x>     Initial joint-optimization simplex step for n.
  --initial-u-step <x>     Initial joint-optimization simplex step for u.
  --x-min/--x-max <x>      Geographic x profile-grid bounds.
  --x-points <n>           Number of x grid values, including both bounds.
  --n-min/--n-max <x>      Environmental n profile-grid bounds.
  --n-points <n>           Number of n grid values, including both bounds.
  --u-min/--u-max <x>      Area-size u profile-grid bounds.
  --u-points <n>           Number of u grid values, including both bounds.
  --support-delta <x>      Delta-lnL support cutoff. Defaults to 2.995732 (approximate 95% LR region for two parameters).
  --tolerance <x>         Optimization tolerance. Defaults to 1e-8.
  --max-iterations <n>    Maximum optimization iterations. Defaults to 200.
  --multi-start-points <n> Log-spaced starts per axis. Defaults to 1.
  -h, --help              Show this help text.

Range table format:
  tip AreaA AreaB
  A   1     0
  B   0     1
";

const CLI_ERROR_FORMAT: &str = "biogeo-cli-error-v1";
const ANALYSIS_RESULT_INSPECTION_FORMAT: &str = "biogeo-analysis-result-inspection-v1";
const ANALYSIS_RESULT_MIGRATION_FORMAT: &str = "biogeo-analysis-result-migration-v1";
const INPUT_BUNDLE_INSPECTION_FORMAT: &str = "biogeo-input-bundle-inspection-v1";
static NEXT_RESULT_MIGRATION: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ErrorOutputFormat {
    Human,
    Tsv,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct GlobalOutputOptions {
    error: ErrorOutputFormat,
    progress: ProgressOutputFormat,
}

impl Default for GlobalOutputOptions {
    fn default() -> Self {
        Self {
            error: ErrorOutputFormat::Human,
            progress: ProgressOutputFormat::None,
        }
    }
}

fn extract_global_output_options(
    mut args: Vec<String>,
) -> (GlobalOutputOptions, Result<Vec<String>, CliError>) {
    let mut options = GlobalOutputOptions::default();
    let mut error_seen = false;
    let mut progress_seen = false;
    loop {
        match args.first().map(String::as_str) {
            Some("--error-format") => {
                if error_seen {
                    return (options, Err(CliError::DuplicateOption("--error-format")));
                }
                if args.len() < 2 {
                    return (options, Err(CliError::MissingValue("--error-format")));
                }
                options.error = match args[1].as_str() {
                    "human" => ErrorOutputFormat::Human,
                    "tsv" => ErrorOutputFormat::Tsv,
                    value => {
                        return (
                            options,
                            Err(CliError::InvalidErrorFormat(value.to_string())),
                        );
                    }
                };
                error_seen = true;
                args.drain(..2);
            }
            Some("--progress-format") => {
                if progress_seen {
                    return (options, Err(CliError::DuplicateOption("--progress-format")));
                }
                if args.len() < 2 {
                    return (options, Err(CliError::MissingValue("--progress-format")));
                }
                options.progress = match args[1].as_str() {
                    "none" => ProgressOutputFormat::None,
                    "tsv" => ProgressOutputFormat::Tsv,
                    value => {
                        return (
                            options,
                            Err(CliError::InvalidProgressFormat(value.to_string())),
                        );
                    }
                };
                progress_seen = true;
                args.drain(..2);
            }
            _ => return (options, Ok(args)),
        }
    }
}

fn encode_error_field(value: &str) -> String {
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
    String::from_utf8(encoded).expect("encoding a UTF-8 error field preserves UTF-8")
}

fn format_cli_error(error: &CliError, format: ErrorOutputFormat) -> String {
    match format {
        ErrorOutputFormat::Human if error.prints_usage() => {
            format!("error: {error}\n\n{USAGE}\n")
        }
        ErrorOutputFormat::Human => format!("error: {error}\n"),
        ErrorOutputFormat::Tsv => format!(
            "format\t{CLI_ERROR_FORMAT}\ncode\t{}\nmessage\t{}\nexit_code\t{}\n",
            error.stable_code(),
            encode_error_field(&error.to_string()),
            error.exit_code()
        ),
    }
}

fn exit_with_cli_error(error: &CliError, format: ErrorOutputFormat) -> ! {
    eprint!("{}", format_cli_error(error, format));
    process::exit(error.exit_code());
}

#[derive(Debug)]
struct BsmInteractiveProgressInner {
    completed_samples: AtomicUsize,
    total_samples: usize,
}

#[derive(Clone, Debug)]
struct BsmInteractiveProgress {
    inner: Arc<BsmInteractiveProgressInner>,
}

impl BsmInteractiveProgress {
    fn new(total_samples: usize) -> Self {
        Self {
            inner: Arc::new(BsmInteractiveProgressInner {
                completed_samples: AtomicUsize::new(0),
                total_samples,
            }),
        }
    }

    fn set_completed_samples(&self, completed_samples: usize) {
        self.inner
            .completed_samples
            .store(completed_samples, AtomicOrdering::Release);
    }

    fn completed_samples(&self) -> usize {
        self.inner.completed_samples.load(AtomicOrdering::Acquire)
    }

    fn total_samples(&self) -> usize {
        self.inner.total_samples
    }
}

#[derive(Debug)]
struct BsmInteractiveSession {
    pause: biogeo_core::StochasticMapPauseToken,
    progress: BsmInteractiveProgress,
}

impl BsmInteractiveSession {
    fn start(
        total_samples: usize,
        cancellation: biogeo_core::StochasticMapCancellationToken,
    ) -> Result<Self, CliError> {
        let pause = biogeo_core::StochasticMapPauseToken::new();
        let progress = BsmInteractiveProgress::new(total_samples);
        let command_pause = pause.clone();
        let recovery_pause = pause.clone();
        let command_progress = progress.clone();
        let handle = thread::Builder::new()
            .name("biogeo-bsm-control".to_string())
            .spawn(move || {
                let stdin = io::stdin();
                let stderr = io::stderr();
                if let Err(error) = run_bsm_interactive_commands(
                    stdin.lock(),
                    stderr.lock(),
                    command_pause,
                    cancellation,
                    command_progress,
                ) {
                    recovery_pause.resume();
                    eprintln!("error: BSM interactive control stopped: {error}");
                }
            })
            .map_err(|error| CliError::BsmInteractiveControlThread(error.to_string()))?;
        drop(handle);
        Ok(Self { pause, progress })
    }
}

fn run_bsm_interactive_commands<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    pause: biogeo_core::StochasticMapPauseToken,
    cancellation: biogeo_core::StochasticMapCancellationToken,
    progress: BsmInteractiveProgress,
) -> io::Result<()> {
    writeln!(
        writer,
        "BSM interactive control enabled: enter pause, resume, status, or cancel."
    )?;
    writer.flush()?;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            if pause.resume() {
                writeln!(
                    writer,
                    "BSM status: standard input closed; resumed; completed_samples={}/{}",
                    progress.completed_samples(),
                    progress.total_samples()
                )?;
                writer.flush()?;
            }
            return Ok(());
        }
        let command = line.trim().to_ascii_lowercase();
        let state = match command.as_str() {
            "pause" | "p" => {
                if pause.pause() {
                    "pause requested"
                } else {
                    "pause already requested"
                }
            }
            "resume" | "r" => {
                if pause.resume() {
                    "resumed"
                } else {
                    "already running"
                }
            }
            "status" | "s" => {
                if pause.is_paused() {
                    "pause requested"
                } else {
                    "running"
                }
            }
            "cancel" | "quit" | "q" => {
                cancellation.cancel();
                pause.resume();
                "cancellation requested"
            }
            "help" | "h" | "?" => "commands: pause, resume, status, cancel",
            "" => continue,
            _ => "unknown command; enter help",
        };
        writeln!(
            writer,
            "BSM status: {state}; completed_samples={}/{}",
            progress.completed_samples(),
            progress.total_samples()
        )?;
        writer.flush()?;
        if matches!(command.as_str(), "cancel" | "quit" | "q") {
            return Ok(());
        }
    }
}

fn main() {
    let (output_options, args) = extract_global_output_options(env::args().skip(1).collect());
    let args = match args {
        Ok(args) => args,
        Err(error) => exit_with_cli_error(&error, output_options.error),
    };
    let cancellation = biogeo_core::ExecutionCancellationToken::new();
    let signal_cancellation = cancellation.clone();
    if let Err(error) = ctrlc::set_handler(move || signal_cancellation.cancel()) {
        exit_with_cli_error(
            &CliError::SignalHandler(error.to_string()),
            output_options.error,
        );
    }
    let mut progress = ProgressReporter::new(output_options.progress);
    match run_with_progress(args, Some(cancellation), &mut progress) {
        Ok(output) => print!("{output}"),
        Err(error) => exit_with_cli_error(&error, output_options.error),
    }
}

#[cfg(test)]
fn run(args: Vec<String>) -> Result<String, CliError> {
    run_with_cancellation(args, None)
}

#[cfg(test)]
fn run_with_cancellation(
    args: Vec<String>,
    cancellation: Option<biogeo_core::ExecutionCancellationToken>,
) -> Result<String, CliError> {
    let mut progress = ProgressReporter::disabled();
    run_with_progress(args, cancellation, &mut progress)
}

fn run_with_progress(
    args: Vec<String>,
    cancellation: Option<biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
) -> Result<String, CliError> {
    match parse_command(args)? {
        Command::Help => Ok(format!("{USAGE}\n")),
        Command::TopicHelp(command) => {
            cli_help::render_command_help(&command).ok_or(CliError::UnknownCommand(command))
        }
        Command::Version => Ok(engine_info::version_output()),
        Command::EngineInfo => Ok(engine_info::capabilities_output()),
        Command::ConvertTree(config) => run_convert_tree(config),
        Command::ConvertRanges(config) => run_convert_ranges(config),
        Command::ConvertBioGeoBearsStrata(config) => run_convert_biogeobears_strata(config),
        Command::FossilPlace(config) => run_fossil_place(config),
        Command::ValidateInputs(config) => run_validate_inputs(config),
        Command::Fixed(config) => run_fixed(config, cancellation),
        Command::DeOptimize(config) => run_de_optimize(config),
        Command::ExponentOptimize(config) => run_exponent_optimize(config),
        Command::XnuOptimize(config) => run_xnu_optimize(config),
        Command::PairProfile(config) => run_pair_profile(config),
        Command::DecJOptimize(config) => run_decj_optimize(config),
        Command::ParameterTemplate(config) => run_parameter_template(config),
        Command::AnalysisTemplate(config) => run_analysis_template(config),
        Command::AnalysisPlan(config) => run_analysis_plan(config),
        Command::AnalysisRun(config) => {
            run_analysis_request(config, cancellation.as_ref(), progress)
        }
        Command::AnalysisWorkflow(config) => run_analysis_workflow(config, cancellation, progress),
        Command::ModelWorkflowPlan(config) => run_model_workflow_plan(config),
        Command::ModelWorkflow(config) => run_model_workflow(config, cancellation, progress),
        Command::ParameterModel(config) => {
            run_parameter_model(config, cancellation.as_ref(), progress, None, None)
        }
        Command::ParameterBatch(config) => {
            run_parameter_batch(config, cancellation.as_ref(), progress, None)
        }
        Command::DatasetBatch(config) => run_dataset_batch(config, cancellation.as_ref(), progress),
        Command::ParameterBsm(config) => run_parameter_bsm(config, cancellation),
        Command::BsmInspect(config) => run_bsm_inspect(config),
        Command::AnalysisResultInspect(config) => run_analysis_result_inspect(config),
        Command::AnalysisResultMigrate(config) => run_analysis_result_migrate(config),
        Command::InputBundleInspect(config) => run_input_bundle_inspect(config),
    }
}

fn run_parameter_template(config: ParameterTemplateConfig) -> Result<String, CliError> {
    Ok(config.preset.parameter_table()?.to_versioned_tsv())
}

fn run_analysis_template(config: AnalysisTemplateConfig) -> Result<String, CliError> {
    let mut table = config.preset.parameter_table()?;
    if config.mode == analysis_request::AnalysisRequestMode::Evaluate {
        let initial = table.resolve_initial()?;
        let free_names = table
            .free_parameter_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        for name in free_names {
            let value = initial.require(&name)?;
            table = table.with_fixed(&name, value)?;
        }
    }
    let request = analysis_request::format_template_request(config.mode);
    analysis_request::write_template_directory(
        &config.output_dir_path,
        &request,
        &table.to_versioned_tsv(),
    )?;

    let mut output = String::new();
    writeln!(
        output,
        "format\t{}",
        analysis_request::ANALYSIS_TEMPLATE_FORMAT
    )
    .unwrap();
    output.push_str("status\tcomplete\n");
    writeln!(
        output,
        "request_format\t{}",
        analysis_request::ANALYSIS_REQUEST_FORMAT
    )
    .unwrap();
    writeln!(output, "mode\t{}", config.mode.as_str()).unwrap();
    writeln!(
        output,
        "output_dir\t{}",
        analysis_result::encode_field(&config.output_dir_path.display().to_string())
    )
    .unwrap();
    writeln!(output, "request_file\t{}", analysis_request::REQUEST_FILE).unwrap();
    writeln!(
        output,
        "parameters_file\t{}",
        analysis_request::PARAMETERS_FILE
    )
    .unwrap();
    output.push_str("ready_to_plan\tfalse\n");
    Ok(output)
}

#[derive(Debug)]
struct LoadedAnalysisRequest {
    source: String,
    parsed: analysis_request::ParsedAnalysisRequest,
    model: ParameterModelConfig,
}

fn load_analysis_request(path: &Path) -> Result<LoadedAnalysisRequest, CliError> {
    let source = read_file(path)?;
    let parsed = analysis_request::parse_analysis_request(&source, path)?;
    let mut model = match parse_command(parsed.command_arguments.clone())? {
        Command::ParameterModel(config) => config,
        _ => unreachable!("analysis request parser emits only parameter-model commands"),
    };
    model.source_request_path = Some(path.to_path_buf());
    Ok(LoadedAnalysisRequest {
        source,
        parsed,
        model,
    })
}

fn run_analysis_plan(config: AnalysisPlanConfig) -> Result<String, CliError> {
    let loaded = load_analysis_request(&config.request_path)?;
    let model_config = &loaded.model;
    let parameter_input = read_file(&model_config.parameters_path)?;
    let table = biogeo_core::parse_parameter_table(&parameter_input)?;

    let tree_input = read_file(&model_config.tree_path)?;
    let selected_tree = parse_selected_tree_input_with_fill(
        &tree_input,
        model_config.tree_name.as_deref(),
        model_config.missing_branch_length_fill,
    )?;
    let tree_input_format = match selected_tree.format {
        biogeo_core::TreeInputFormat::Newick => "newick",
        biogeo_core::TreeInputFormat::Nexus => "nexus",
    };
    let selected_tree_name = selected_tree.tree_name;
    let parsed_tree = selected_tree
        .parsed_tree
        .with_direct_ancestor_hooks_below(model_config.min_branch_length)
        .map_err(|error| CliError::Newick(error.into()))?;
    let non_binary_nodes = parsed_tree
        .tree
        .postorder_internal_nodes()
        .iter()
        .filter_map(|node| {
            let child_count = parsed_tree
                .tree
                .children(*node)
                .expect("postorder node is inside the tree")
                .len();
            (child_count != 2).then_some((*node, child_count))
        })
        .collect::<Vec<_>>();
    if !non_binary_nodes.is_empty() {
        return Err(CliError::NonBinaryInputTree {
            nodes: non_binary_nodes,
        });
    }

    let tip_input = LoadedParameterTipInput::load(model_config, &parsed_tree)?;
    let parsed_ranges = tip_input.parsed_ranges;
    let context = LoadedParameterModelContext::load(
        model_config,
        &parsed_ranges.area_names,
        tip_input.detection,
    )?;
    validate_parameter_model_table(&table, &context, model_config.mode)?;
    let num_areas = u8::try_from(parsed_ranges.area_names.len())
        .map_err(|_| CliError::AnalysisPlanSizeOverflow("area count"))?;
    let max_range_size = model_config.max_range_size.unwrap_or(num_areas);
    let state_count_estimate = preflight_state_space(
        num_areas,
        max_range_size,
        model_config.include_null_range,
        model_config.max_states,
    )?;
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, model_config.include_null_range)?;
    debug_assert_eq!(states.len(), state_count_estimate);
    let resolved = table.resolve_initial()?;
    let model = context.build_model(&resolved)?;
    if !context.has_detection() || model_config.mode == ParameterRunMode::Evaluate {
        let tip_likelihoods = if context.has_detection() {
            context.detection_tip_likelihoods(&resolved, &states)?
        } else {
            parsed_ranges.tip_likelihoods(&states)?
        };
        validate_tip_state_constraints_for_cli(&parsed_tree, &states, &model, &tip_likelihoods)?;
    }

    let tree = &parsed_tree.tree;
    let node_ages = tree.node_ages_from_present();
    let root_age = node_ages[tree.root()];
    let anagenetic_periods = model
        .anagenesis
        .time_stratified_anagenesis()
        .map_or(1, |schedule| schedule.strata().len());
    let stratum_allowed_state_counts = model
        .anagenesis
        .time_stratified_anagenesis()
        .map(|schedule| {
            schedule
                .strata()
                .iter()
                .map(|stratum| {
                    stratum
                        .state_constraint
                        .as_ref()
                        .map(|constraint| {
                            constraint
                                .state_mask(&states)
                                .map(|mask| mask.allowed_count())
                        })
                        .transpose()
                        .map(|count| count.unwrap_or(states.len()))
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_else(|| vec![states.len()]);
    if let Some(schedule) = model.anagenesis.time_stratified_anagenesis()
        && schedule.oldest_age() + 1e-12 < root_age
    {
        return Err(CliError::AnalysisPlanStrataCoverage {
            oldest_age: schedule.oldest_age(),
            root_age,
        });
    }

    let q_off_diagonal_transitions = if model.anagenesis.time_stratified_anagenesis().is_none() {
        Some(
            model
                .build_q(&states)
                .map_err(biogeo_core::DecAnalysisError::from)?
                .off_diagonal_count(),
        )
    } else {
        None
    };
    let cladogenesis = model
        .cladogenesis
        .build_table(&states)
        .map_err(biogeo_core::DecAnalysisError::from)?;

    let tips = tree.tip_nodes().len();
    let internal_nodes = tree.postorder_internal_nodes().len();
    let nodes = tree.node_count();
    let edges = tree.edges().len();
    let direct_ancestor_nodes = tree
        .postorder_internal_nodes()
        .iter()
        .filter(|node| tree.is_direct_ancestor_node(**node))
        .count();
    let split_nodes = internal_nodes - direct_ancestor_nodes;
    let state_count = states.len();
    let free_parameters = table.free_parameter_names();
    let branch_segments = count_analysis_branch_segments(tree, &node_ages, &model);

    let state_vector_bytes = checked_plan_mul(state_count, size_of::<f64>(), "state vector")?;
    let pruning_likelihood_bytes =
        checked_plan_mul(nodes, state_vector_bytes, "pruning likelihood payload")?;
    let ancestral_payload_bytes = if model_config.ancestral_probs {
        checked_plan_mul(
            internal_nodes,
            state_vector_bytes,
            "ancestral posterior payload",
        )?
    } else {
        0
    };
    let split_rows_upper_bound = if model_config.split_probs {
        checked_plan_mul(
            split_nodes,
            cladogenesis.scenario_count(),
            "split posterior row upper bound",
        )?
    } else {
        0
    };
    let split_payload_bytes_upper_bound = checked_plan_mul(
        split_rows_upper_bound,
        size_of::<biogeo_core::SplitScenarioPosterior>(),
        "split posterior payload upper bound",
    )?;
    let dense_q_reference_bytes = checked_plan_mul(
        checked_plan_mul(state_count, state_count, "dense Q cells")?,
        size_of::<f64>(),
        "dense Q reference",
    )?;
    let combined_numeric_payload_bytes_reference = pruning_likelihood_bytes
        .checked_add(ancestral_payload_bytes)
        .and_then(|value| value.checked_add(split_payload_bytes_upper_bound))
        .ok_or(CliError::AnalysisPlanSizeOverflow(
            "combined numeric payload",
        ))?;

    let mut warnings = Vec::new();
    let mut risk_level = "low";
    if nodes >= 1_000_000
        || state_count >= 1024
        || split_rows_upper_bound >= 10_000_000
        || combined_numeric_payload_bytes_reference >= 1_073_741_824
    {
        risk_level = "high";
    } else if nodes >= 100_000
        || state_count >= 256
        || split_rows_upper_bound >= 1_000_000
        || combined_numeric_payload_bytes_reference >= 134_217_728
        || free_parameters.len() >= 8
    {
        risk_level = "moderate";
    }
    if nodes >= 100_000 {
        warnings.push(format!(
            "tree contains {nodes} nodes; pruning memory and runtime scale with nodes times states"
        ));
    }
    if state_count >= 256 {
        warnings.push(format!(
            "state space contains {state_count} ranges; runtime grows strongly with state count"
        ));
    }
    if split_rows_upper_bound >= 1_000_000 {
        warnings.push(format!(
            "split posterior may emit up to {split_rows_upper_bound} rows"
        ));
    }
    if combined_numeric_payload_bytes_reference >= 134_217_728 {
        warnings.push(format!(
            "core numeric payload reference is {combined_numeric_payload_bytes_reference} bytes before allocator and temporary-workspace overhead"
        ));
    }
    if free_parameters.len() >= 8 {
        warnings.push(format!(
            "optimization has {} free parameters; use multiple starts and inspect convergence",
            free_parameters.len()
        ));
    }
    if !loaded.parsed.portable_paths {
        warnings.push(
            "request contains absolute or parent-relative paths and is not relocatable".to_string(),
        );
    }

    let total_branch_length = tree.edges().iter().map(|edge| edge.length).sum::<f64>();
    let request_fingerprint = analysis_result::stable_fingerprint(loaded.source.as_bytes());
    let available_parallelism = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let mut output = String::new();
    writeln!(output, "format\t{}", analysis_request::ANALYSIS_PLAN_FORMAT).unwrap();
    output.push_str("status\tvalid\n");
    writeln!(
        output,
        "request_format\t{}",
        analysis_request::ANALYSIS_REQUEST_FORMAT
    )
    .unwrap();
    writeln!(output, "request_fingerprint\t{request_fingerprint}").unwrap();
    writeln!(
        output,
        "request_path\t{}",
        analysis_result::encode_field(&config.request_path.display().to_string())
    )
    .unwrap();
    writeln!(output, "portable\t{}", loaded.parsed.portable_paths).unwrap();
    writeln!(output, "mode\t{}", loaded.parsed.mode.as_str()).unwrap();
    writeln!(
        output,
        "tip_observation_model\t{}",
        parameter_tip_observation_model(model_config)
    )
    .unwrap();
    writeln!(output, "tree_input_format\t{tree_input_format}").unwrap();
    writeln!(
        output,
        "tree_name\t{}",
        selected_tree_name
            .as_deref()
            .map(analysis_result::encode_field)
            .unwrap_or_else(|| "none".to_string())
    )
    .unwrap();
    writeln!(
        output,
        "missing_branch_length_fill\t{}",
        format_missing_branch_length_fill(model_config.missing_branch_length_fill)
    )
    .unwrap();
    writeln!(output, "tips\t{tips}").unwrap();
    writeln!(output, "internal_nodes\t{internal_nodes}").unwrap();
    writeln!(output, "nodes\t{nodes}").unwrap();
    writeln!(output, "edges\t{edges}").unwrap();
    writeln!(output, "direct_ancestor_nodes\t{direct_ancestor_nodes}").unwrap();
    writeln!(output, "split_nodes\t{split_nodes}").unwrap();
    writeln!(output, "root_age\t{root_age:.17}").unwrap();
    writeln!(output, "total_branch_length\t{total_branch_length:.17}").unwrap();
    writeln!(output, "areas\t{}", parsed_ranges.area_names.len()).unwrap();
    writeln!(output, "max_range_size\t{max_range_size}").unwrap();
    writeln!(
        output,
        "include_null_range\t{}",
        model_config.include_null_range
    )
    .unwrap();
    writeln!(
        output,
        "state_space_limit\t{}",
        model_config
            .max_states
            .map_or_else(|| "unlimited".to_string(), |limit| limit.to_string())
    )
    .unwrap();
    writeln!(output, "state_count_estimate\t{state_count_estimate}").unwrap();
    writeln!(output, "states\t{state_count}").unwrap();
    writeln!(output, "root_prior\t{}", model_config.root_prior.as_str()).unwrap();
    writeln!(output, "free_parameter_count\t{}", free_parameters.len()).unwrap();
    writeln!(
        output,
        "free_parameters\t{}",
        if free_parameters.is_empty() {
            "none".to_string()
        } else {
            analysis_result::encode_field(&free_parameters.join(","))
        }
    )
    .unwrap();
    writeln!(output, "anagenetic_periods\t{anagenetic_periods}").unwrap();
    writeln!(
        output,
        "stratum_allowed_state_counts\t{}",
        stratum_allowed_state_counts
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(",")
    )
    .unwrap();
    writeln!(output, "branch_segments\t{branch_segments}").unwrap();
    writeln!(
        output,
        "q_off_diagonal_transitions\t{}",
        q_off_diagonal_transitions.map_or_else(|| "NA".to_string(), |value| value.to_string())
    )
    .unwrap();
    writeln!(
        output,
        "cladogenetic_scenarios\t{}",
        cladogenesis.scenario_count()
    )
    .unwrap();
    writeln!(
        output,
        "ancestral_probabilities\t{}",
        model_config.ancestral_probs
    )
    .unwrap();
    writeln!(output, "split_probabilities\t{}", model_config.split_probs).unwrap();
    writeln!(output, "state_vector_bytes\t{state_vector_bytes}").unwrap();
    writeln!(
        output,
        "pruning_likelihood_payload_bytes\t{pruning_likelihood_bytes}"
    )
    .unwrap();
    writeln!(output, "ancestral_payload_bytes\t{ancestral_payload_bytes}").unwrap();
    writeln!(output, "split_rows_upper_bound\t{split_rows_upper_bound}").unwrap();
    writeln!(
        output,
        "split_payload_bytes_upper_bound\t{split_payload_bytes_upper_bound}"
    )
    .unwrap();
    writeln!(output, "dense_q_reference_bytes\t{dense_q_reference_bytes}").unwrap();
    writeln!(
        output,
        "combined_numeric_payload_bytes_reference\t{combined_numeric_payload_bytes_reference}"
    )
    .unwrap();
    output.push_str("process_rss_estimate_available\tfalse\n");
    writeln!(output, "available_parallelism\t{available_parallelism}").unwrap();
    writeln!(output, "risk_level\t{risk_level}").unwrap();
    writeln!(output, "warning_count\t{}", warnings.len()).unwrap();
    writeln!(
        output,
        "warnings\t{}",
        if warnings.is_empty() {
            "none".to_string()
        } else {
            analysis_result::encode_field(&warnings.join(" | "))
        }
    )
    .unwrap();
    Ok(output)
}

fn count_analysis_branch_segments(
    tree: &biogeo_core::Tree,
    node_ages: &[f64],
    model: &biogeo_core::ModelConfig,
) -> usize {
    let Some(schedule) = model.anagenesis.time_stratified_anagenesis() else {
        return tree.edges().len();
    };
    tree.edges()
        .iter()
        .map(|edge| {
            let child_age = node_ages[edge.child];
            let parent_age = node_ages[edge.parent];
            let mut younger_boundary: f64 = 0.0;
            let mut count = 0;
            for stratum in schedule.strata() {
                let segment_young = child_age.max(younger_boundary);
                let segment_old = parent_age.min(stratum.oldest_age);
                if segment_old - segment_young > 1e-12 {
                    count += 1;
                }
                younger_boundary = stratum.oldest_age;
                if younger_boundary >= parent_age {
                    break;
                }
            }
            count
        })
        .sum()
}

fn checked_plan_mul(left: usize, right: usize, quantity: &'static str) -> Result<usize, CliError> {
    left.checked_mul(right)
        .ok_or(CliError::AnalysisPlanSizeOverflow(quantity))
}

fn preflight_state_space(
    num_areas: u8,
    max_range_size: u8,
    include_null_range: bool,
    max_states: Option<usize>,
) -> Result<usize, CliError> {
    let estimated_states = biogeo_core::StateSpace::estimated_state_count(
        num_areas,
        max_range_size,
        include_null_range,
    )?;
    if let Some(max_states) = max_states
        && estimated_states > max_states
    {
        return Err(CliError::StateSpaceLimitExceeded {
            estimated_states,
            max_states,
            num_areas,
            max_range_size,
            include_null_range,
        });
    }
    Ok(estimated_states)
}

const ANALYSIS_WORKFLOW_ANALYSIS_DIR: &str = "analysis-result";
const ANALYSIS_WORKFLOW_BSM_DIR: &str = "bsm-result";

fn validate_analysis_workflow_root(root: &Path) -> Result<(), CliError> {
    let mut entries = fs::read_dir(root)
        .map_err(|source| CliError::Io {
            path: root.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .map_err(|source| CliError::Io {
                    path: root.to_path_buf(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();
    if entries.iter().any(|entry| {
        !matches!(
            entry.as_str(),
            ANALYSIS_WORKFLOW_ANALYSIS_DIR | ANALYSIS_WORKFLOW_BSM_DIR
        )
    }) {
        return Err(CliError::InvalidAnalysisWorkflow {
            path: root.to_path_buf(),
            message: format!("unexpected root entries: {entries:?}"),
        });
    }
    let analysis_result = root.join(ANALYSIS_WORKFLOW_ANALYSIS_DIR);
    let bsm_result = root.join(ANALYSIS_WORKFLOW_BSM_DIR);
    for path in [&analysis_result, &bsm_result] {
        if path.exists() && !path.is_dir() {
            return Err(CliError::InvalidAnalysisWorkflow {
                path: path.to_path_buf(),
                message: "expected a directory".to_string(),
            });
        }
    }
    if bsm_result.is_dir() && !analysis_result.is_dir() {
        return Err(CliError::InvalidAnalysisWorkflow {
            path: root.to_path_buf(),
            message: "bsm-result exists without analysis-result".to_string(),
        });
    }
    Ok(())
}

fn create_analysis_workflow_root(root: &Path) -> Result<(), CliError> {
    let parent = root
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|source| CliError::OutputIo {
        path: parent.to_path_buf(),
        source,
    })?;
    fs::create_dir(root).map_err(|source| CliError::OutputIo {
        path: root.to_path_buf(),
        source,
    })
}

fn run_analysis_workflow(
    mut config: AnalysisWorkflowConfig,
    cancellation: Option<biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
) -> Result<String, CliError> {
    let started = Instant::now();
    let request_source = read_file(&config.request_path)?;
    analysis_request::parse_analysis_request(&request_source, &config.request_path)?;

    if config.resume {
        if !config.output_dir_path.is_dir() {
            return Err(CliError::MissingAnalysisWorkflowOutput(
                config.output_dir_path,
            ));
        }
        validate_analysis_workflow_root(&config.output_dir_path)?;
    } else if config.output_dir_path.exists() {
        return Err(CliError::AnalysisWorkflowOutputExists(
            config.output_dir_path,
        ));
    }

    let analysis_result_dir = config.output_dir_path.join(ANALYSIS_WORKFLOW_ANALYSIS_DIR);
    let bsm_result_dir = config.output_dir_path.join(ANALYSIS_WORKFLOW_BSM_DIR);
    let analysis_reused = analysis_result_dir.is_dir();
    if !analysis_reused {
        run_analysis_plan(AnalysisPlanConfig {
            request_path: config.request_path.clone(),
        })?;
    }
    if !config.resume {
        create_analysis_workflow_root(&config.output_dir_path)?;
    }

    let analysis_started = Instant::now();
    if !analysis_reused {
        run_analysis_request(
            AnalysisRunConfig {
                request_path: config.request_path.clone(),
                output_dir_path: analysis_result_dir.clone(),
            },
            cancellation.as_ref(),
            progress,
        )?;
    }
    let analysis_elapsed = analysis_started.elapsed();
    let analysis = analysis_result::load_analysis_result(&analysis_result_dir)?;
    let stored_request_path = analysis.require_input_path("analysis_request")?;
    let stored_request = read_file(stored_request_path)?;
    let expected_request_fingerprint =
        analysis_result::stable_fingerprint(stored_request.as_bytes());
    let actual_request_fingerprint = analysis_result::stable_fingerprint(request_source.as_bytes());
    if stored_request != request_source {
        return Err(CliError::AnalysisWorkflowRequestMismatch {
            expected: expected_request_fingerprint,
            actual: actual_request_fingerprint,
        });
    }

    let bsm_resumed = bsm_result_dir.is_dir();
    config.bsm.bsm_resume = bsm_resumed;
    let requested_samples = config.bsm.bsm_samples;
    let bsm_started = Instant::now();
    run_parameter_bsm(config.bsm, cancellation)?;
    let bsm_elapsed = bsm_started.elapsed();

    let inspection_started = Instant::now();
    let inspection = bsm_inspect::inspect(&bsm_result_dir, config.deep_inspection)
        .map_err(CliError::BsmInspection)?;
    let inspection_elapsed = inspection_started.elapsed();
    if inspection.run_status != "complete"
        || inspection.completed_samples != requested_samples
        || inspection.requested_samples != requested_samples
    {
        return Err(CliError::InvalidAnalysisWorkflow {
            path: bsm_result_dir,
            message: "BSM did not publish the requested complete sample set".to_string(),
        });
    }

    let mut output = String::new();
    writeln!(
        output,
        "format\t{}",
        analysis_request::ANALYSIS_WORKFLOW_FORMAT
    )
    .unwrap();
    output.push_str("status\tcomplete\n");
    writeln!(
        output,
        "request_format\t{}",
        analysis_request::ANALYSIS_REQUEST_FORMAT
    )
    .unwrap();
    writeln!(output, "request_fingerprint\t{actual_request_fingerprint}").unwrap();
    writeln!(
        output,
        "request_path\t{}",
        analysis_result::encode_field(&config.request_path.display().to_string())
    )
    .unwrap();
    writeln!(
        output,
        "output_dir\t{}",
        analysis_result::encode_field(&config.output_dir_path.display().to_string())
    )
    .unwrap();
    writeln!(
        output,
        "analysis_result_format\t{}",
        analysis.format_version
    )
    .unwrap();
    writeln!(
        output,
        "analysis_result_dir\t{}",
        analysis_result::encode_field(&analysis_result_dir.display().to_string())
    )
    .unwrap();
    writeln!(output, "analysis_reused\t{analysis_reused}").unwrap();
    writeln!(output, "mode\t{}", analysis.manifest.mode).unwrap();
    writeln!(output, "lnL\t{:.17}", analysis.manifest.log_likelihood).unwrap();
    writeln!(output, "states\t{}", analysis.manifest.states).unwrap();
    writeln!(output, "areas\t{}", analysis.manifest.areas).unwrap();
    writeln!(output, "tips\t{}", analysis.manifest.tips).unwrap();
    writeln!(output, "bsm_format\t{}", inspection.bsm_format).unwrap();
    writeln!(
        output,
        "bsm_result_dir\t{}",
        analysis_result::encode_field(&bsm_result_dir.display().to_string())
    )
    .unwrap();
    writeln!(output, "bsm_output_level\t{}", inspection.output_level).unwrap();
    writeln!(output, "bsm_layout\t{}", inspection.layout).unwrap();
    writeln!(output, "bsm_requested_samples\t{requested_samples}").unwrap();
    writeln!(
        output,
        "bsm_completed_samples\t{}",
        inspection.completed_samples
    )
    .unwrap();
    writeln!(
        output,
        "bsm_completed_anagenetic_events\t{}",
        inspection.completed_anagenetic_events
    )
    .unwrap();
    writeln!(output, "bsm_resumed\t{bsm_resumed}").unwrap();
    writeln!(
        output,
        "bsm_validation\t{}",
        if config.deep_inspection {
            "deep"
        } else {
            "quick"
        }
    )
    .unwrap();
    output.push_str("bsm_validation_status\tvalid\n");
    writeln!(output, "bsm_files_checked\t{}", inspection.files_checked).unwrap();
    writeln!(
        output,
        "bsm_data_rows_checked\t{}",
        inspection
            .data_rows_checked
            .map_or_else(|| "NA".to_string(), |value| value.to_string())
    )
    .unwrap();
    writeln!(
        output,
        "analysis_elapsed_seconds\t{:.9}",
        analysis_elapsed.as_secs_f64()
    )
    .unwrap();
    writeln!(
        output,
        "bsm_elapsed_seconds\t{:.9}",
        bsm_elapsed.as_secs_f64()
    )
    .unwrap();
    writeln!(
        output,
        "inspection_elapsed_seconds\t{:.9}",
        inspection_elapsed.as_secs_f64()
    )
    .unwrap();
    writeln!(
        output,
        "elapsed_seconds\t{:.9}",
        started.elapsed().as_secs_f64()
    )
    .unwrap();
    Ok(output)
}

#[derive(Debug)]
struct ModelWorkflowCandidatePlan {
    model_id: String,
    free_parameters: Vec<String>,
    anagenetic_periods: usize,
    q_off_diagonal_transitions: Option<usize>,
    cladogenetic_scenarios: usize,
    branch_segments: usize,
}

#[derive(Debug)]
struct ModelWorkflowPreflight {
    candidates: Vec<ModelWorkflowCandidatePlan>,
    tips: usize,
    internal_nodes: usize,
    nodes: usize,
    edges: usize,
    direct_ancestor_nodes: usize,
    root_age: f64,
    total_branch_length: f64,
    areas: usize,
    max_range_size: u8,
    states: usize,
    state_vector_bytes: usize,
    pruning_likelihood_payload_bytes: usize,
    risk_level: &'static str,
    warnings: Vec<String>,
}

fn load_model_workflow_batch_config(
    loaded: &model_workflow::LoadedModelWorkflowRequest,
    output_dir: &Path,
    resume: bool,
) -> Result<ParameterBatchConfig, CliError> {
    let mut arguments = vec![
        "model-batch".to_string(),
        "--manifest".to_string(),
        loaded
            .parsed
            .models_manifest_path
            .to_string_lossy()
            .into_owned(),
        "--output-dir".to_string(),
        output_dir.to_string_lossy().into_owned(),
    ];
    arguments.extend(dataset_batch::parse_model_batch_config(
        &loaded.source_config,
        &loaded.parsed.model_config_path,
    )?);
    if resume {
        arguments.push("--resume".to_string());
    }
    match parse_command(arguments)? {
        Command::ParameterBatch(config) => Ok(config),
        Command::Help => unreachable!("model-workflow emits a complete model-batch command"),
        _ => unreachable!("model-workflow config must parse as model-batch"),
    }
}

fn preflight_model_workflow(
    loaded: &model_workflow::LoadedModelWorkflowRequest,
    batch: &ParameterBatchConfig,
) -> Result<ModelWorkflowPreflight, CliError> {
    let entries = model_batch::parse_model_batch_manifest(
        &loaded.source_models,
        &loaded.parsed.models_manifest_path,
    )?;
    if let Some(requested_model_id) = loaded.parsed.bsm_selection.requested_model_id()
        && !entries
            .iter()
            .any(|entry| entry.model_id == requested_model_id)
    {
        return Err(model_workflow::ModelWorkflowError::UnknownBsmModelId(
            requested_model_id.to_string(),
        )
        .into());
    }

    let tree_input = read_file(&batch.template.tree_path)?;
    let parsed_tree = parse_analysis_tree_with_fill(
        &tree_input,
        batch.template.tree_name.as_deref(),
        batch.template.min_branch_length,
        batch.template.missing_branch_length_fill,
    )?;
    let non_binary_nodes = parsed_tree
        .tree
        .postorder_internal_nodes()
        .iter()
        .filter_map(|node| {
            let child_count = parsed_tree
                .tree
                .children(*node)
                .expect("postorder node is inside the tree")
                .len();
            (child_count != 2).then_some((*node, child_count))
        })
        .collect::<Vec<_>>();
    if !non_binary_nodes.is_empty() {
        return Err(CliError::NonBinaryInputTree {
            nodes: non_binary_nodes,
        });
    }

    let tip_input = LoadedParameterTipInput::load(&batch.template, &parsed_tree)?;
    let parsed_ranges = tip_input.parsed_ranges;
    let context = LoadedParameterModelContext::load(
        &batch.template,
        &parsed_ranges.area_names,
        tip_input.detection,
    )?;
    let num_areas = u8::try_from(parsed_ranges.area_names.len())
        .map_err(|_| CliError::AnalysisPlanSizeOverflow("area count"))?;
    let max_range_size = batch.template.max_range_size.unwrap_or(num_areas);
    preflight_state_space(
        num_areas,
        max_range_size,
        batch.template.include_null_range,
        batch.template.max_states,
    )?;
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, batch.template.include_null_range)?;
    let exact_tip_likelihoods = if context.has_detection() {
        None
    } else {
        Some(parsed_ranges.tip_likelihoods(&states)?)
    };
    let tree = &parsed_tree.tree;
    let node_ages = tree.node_ages_from_present();
    let root_age = node_ages[tree.root()];
    let mut candidates = Vec::with_capacity(entries.len());
    for entry in entries {
        let parameter_input = read_file(&entry.parameters_path)?;
        let table = biogeo_core::parse_parameter_table(&parameter_input)?;
        validate_parameter_model_table(&table, &context, ParameterRunMode::Optimize)?;
        let resolved = table.resolve_initial()?;
        let model = context.build_model(&resolved)?;
        if let Some(tip_likelihoods) = exact_tip_likelihoods.as_deref() {
            validate_tip_state_constraints_for_cli(&parsed_tree, &states, &model, tip_likelihoods)?;
        }
        if let Some(schedule) = model.anagenesis.time_stratified_anagenesis()
            && schedule.oldest_age() + 1e-12 < root_age
        {
            return Err(CliError::AnalysisPlanStrataCoverage {
                oldest_age: schedule.oldest_age(),
                root_age,
            });
        }
        let q_off_diagonal_transitions = if model.anagenesis.time_stratified_anagenesis().is_none()
        {
            Some(
                model
                    .build_q(&states)
                    .map_err(biogeo_core::DecAnalysisError::from)?
                    .off_diagonal_count(),
            )
        } else {
            None
        };
        let cladogenetic_scenarios = model
            .cladogenesis
            .build_table(&states)
            .map_err(biogeo_core::DecAnalysisError::from)?
            .scenario_count();
        candidates.push(ModelWorkflowCandidatePlan {
            model_id: entry.model_id,
            free_parameters: table
                .free_parameter_names()
                .into_iter()
                .map(str::to_owned)
                .collect(),
            anagenetic_periods: model
                .anagenesis
                .time_stratified_anagenesis()
                .map_or(1, |schedule| schedule.strata().len()),
            q_off_diagonal_transitions,
            cladogenetic_scenarios,
            branch_segments: count_analysis_branch_segments(tree, &node_ages, &model),
        });
    }

    if loaded.parsed.comparison_criterion == model_workflow::ComparisonCriterion::Aicc
        && loaded.parsed.bsm_selection == model_workflow::BsmSelection::BestByCriterion
        && candidates
            .iter()
            .all(|candidate| tree.tip_nodes().len() <= candidate.free_parameters.len() + 1)
    {
        return Err(model_workflow::ModelWorkflowError::MissingBestModel(
            model_workflow::ComparisonCriterion::Aicc,
        )
        .into());
    }

    let tips = tree.tip_nodes().len();
    let internal_nodes = tree.postorder_internal_nodes().len();
    let nodes = tree.node_count();
    let state_count = states.len();
    let state_vector_bytes = checked_plan_mul(state_count, size_of::<f64>(), "state vector")?;
    let pruning_likelihood_payload_bytes =
        checked_plan_mul(nodes, state_vector_bytes, "pruning likelihood payload")?;
    let max_free_parameters = candidates
        .iter()
        .map(|candidate| candidate.free_parameters.len())
        .max()
        .unwrap_or(0);
    let mut warnings = Vec::new();
    let risk_level = if nodes >= 1_000_000
        || state_count >= 1024
        || pruning_likelihood_payload_bytes >= 1_073_741_824
    {
        "high"
    } else if nodes >= 100_000
        || state_count >= 256
        || pruning_likelihood_payload_bytes >= 134_217_728
        || max_free_parameters >= 8
    {
        "moderate"
    } else {
        "low"
    };
    if nodes >= 100_000 {
        warnings.push(format!(
            "tree contains {nodes} nodes; each candidate performs pruning over nodes times states"
        ));
    }
    if state_count >= 256 {
        warnings.push(format!(
            "state space contains {state_count} ranges; runtime grows strongly with state count"
        ));
    }
    if pruning_likelihood_payload_bytes >= 134_217_728 {
        warnings.push(format!(
            "one candidate's pruning likelihood payload is {pruning_likelihood_payload_bytes} bytes before temporary workspaces"
        ));
    }
    if max_free_parameters >= 8 {
        warnings.push(format!(
            "at least one candidate has {max_free_parameters} free parameters; inspect multi-start convergence"
        ));
    }
    if loaded.parsed.bsm_selection == model_workflow::BsmSelection::BestByCriterion {
        warnings.push(
            "best_by_criterion is an explicit execution policy, not a claim that the selected model is scientifically true"
                .to_string(),
        );
    }
    if !loaded.parsed.request_paths_portable {
        warnings.push(
            "top-level request contains absolute or parent-relative paths and is not relocatable as a standalone file"
                .to_string(),
        );
    }

    Ok(ModelWorkflowPreflight {
        candidates,
        tips,
        internal_nodes,
        nodes,
        edges: tree.edges().len(),
        direct_ancestor_nodes: tree
            .postorder_internal_nodes()
            .iter()
            .filter(|node| tree.is_direct_ancestor_node(**node))
            .count(),
        root_age,
        total_branch_length: tree.edges().iter().map(|edge| edge.length).sum(),
        areas: parsed_ranges.area_names.len(),
        max_range_size,
        states: state_count,
        state_vector_bytes,
        pruning_likelihood_payload_bytes,
        risk_level,
        warnings,
    })
}

fn workflow_bsm_execution_request(
    bsm: &model_workflow::WorkflowBsmConfig,
) -> Result<BsmExecutionRequest, CliError> {
    let time_limit = bsm
        .time_limit_seconds
        .map(|seconds| {
            Duration::try_from_secs_f64(seconds).map_err(|_| CliError::InvalidBsmTimeLimit {
                option: "bsm_time_limit_seconds",
                seconds,
            })
        })
        .transpose()?;
    Ok(BsmExecutionRequest {
        thread_selection: match bsm.threads {
            model_workflow::WorkflowBsmThreads::Auto => BsmThreadSelection::Auto,
            model_workflow::WorkflowBsmThreads::Fixed(threads) => {
                BsmThreadSelection::Fixed(threads)
            }
        },
        max_in_flight: bsm.max_in_flight,
        max_events_per_sample: bsm.max_events_per_sample,
        max_events_total: bsm.max_events_total,
        memory_budget_mb: bsm.memory_budget_mb,
        shard_samples: bsm.shard_samples,
        checkpoint_samples: bsm.checkpoint_samples,
        time_limit,
    })
}

fn workflow_bsm_output_level(level: model_workflow::WorkflowBsmOutputLevel) -> BsmOutputLevel {
    match level {
        model_workflow::WorkflowBsmOutputLevel::Legacy => BsmOutputLevel::Legacy,
        model_workflow::WorkflowBsmOutputLevel::Full => BsmOutputLevel::Full,
        model_workflow::WorkflowBsmOutputLevel::Compact => BsmOutputLevel::Compact,
        model_workflow::WorkflowBsmOutputLevel::Summary => BsmOutputLevel::Summary,
    }
}

fn run_model_workflow_plan(config: ModelWorkflowPlanConfig) -> Result<String, CliError> {
    let loaded = model_workflow::load_model_workflow_request(&config.request_path)?;
    let batch = load_model_workflow_batch_config(
        &loaded,
        Path::new("__model_workflow_plan_output__"),
        false,
    )?;
    let plan = preflight_model_workflow(&loaded, &batch)?;
    let available_parallelism = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let bsm_execution = loaded
        .parsed
        .bsm
        .as_ref()
        .map(|bsm| {
            resolve_bsm_execution_with_available(
                workflow_bsm_execution_request(bsm)?,
                bsm.samples,
                available_parallelism,
            )
            .map(|resolved| {
                resolved.expect("model-workflow BSM requests have positive sample counts")
            })
        })
        .transpose()?;
    let bsm_format = bsm_execution
        .zip(loaded.parsed.bsm.as_ref())
        .map(|(execution, bsm)| {
            bsm_stream_format(execution, workflow_bsm_output_level(bsm.output_level))
        });
    let candidate_model_ids = plan
        .candidates
        .iter()
        .map(|candidate| candidate.model_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let candidate_free_parameter_counts = plan
        .candidates
        .iter()
        .map(|candidate| format!("{}={}", candidate.model_id, candidate.free_parameters.len()))
        .collect::<Vec<_>>()
        .join(";");
    let candidate_free_parameters = plan
        .candidates
        .iter()
        .map(|candidate| {
            format!(
                "{}={}",
                candidate.model_id,
                if candidate.free_parameters.is_empty() {
                    "none".to_string()
                } else {
                    candidate.free_parameters.join(",")
                }
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let max_anagenetic_periods = plan
        .candidates
        .iter()
        .map(|candidate| candidate.anagenetic_periods)
        .max()
        .unwrap_or(0);
    let max_q_off_diagonal = plan
        .candidates
        .iter()
        .filter_map(|candidate| candidate.q_off_diagonal_transitions)
        .max();
    let max_cladogenetic_scenarios = plan
        .candidates
        .iter()
        .map(|candidate| candidate.cladogenetic_scenarios)
        .max()
        .unwrap_or(0);
    let max_branch_segments = plan
        .candidates
        .iter()
        .map(|candidate| candidate.branch_segments)
        .max()
        .unwrap_or(0);
    let bsm = loaded.parsed.bsm.as_ref();
    let mut output = String::new();
    writeln!(
        output,
        "format\t{}",
        model_workflow::MODEL_WORKFLOW_PLAN_FORMAT
    )
    .unwrap();
    output.push_str("status\tvalid\n");
    writeln!(
        output,
        "request_format\t{}",
        model_workflow::MODEL_WORKFLOW_REQUEST_FORMAT
    )
    .unwrap();
    writeln!(
        output,
        "request_fingerprint\t{}",
        loaded.request_fingerprint()
    )
    .unwrap();
    writeln!(
        output,
        "request_path\t{}",
        analysis_result::encode_field(&config.request_path.display().to_string())
    )
    .unwrap();
    writeln!(
        output,
        "request_paths_portable\t{}",
        loaded.parsed.request_paths_portable
    )
    .unwrap();
    writeln!(
        output,
        "models_manifest_format\t{}",
        model_batch::MODEL_BATCH_MANIFEST_FORMAT
    )
    .unwrap();
    writeln!(
        output,
        "models_fingerprint\t{}",
        loaded.models_fingerprint()
    )
    .unwrap();
    output.push_str("model_config_format\tbiogeo-model-batch-config-v1\n");
    writeln!(
        output,
        "config_fingerprint\t{}",
        loaded.config_fingerprint()
    )
    .unwrap();
    writeln!(output, "candidate_models\t{}", plan.candidates.len()).unwrap();
    writeln!(output, "candidate_model_ids\t{candidate_model_ids}").unwrap();
    writeln!(
        output,
        "candidate_free_parameter_counts\t{}",
        analysis_result::encode_field(&candidate_free_parameter_counts)
    )
    .unwrap();
    writeln!(
        output,
        "candidate_free_parameters\t{}",
        analysis_result::encode_field(&candidate_free_parameters)
    )
    .unwrap();
    writeln!(output, "tips\t{}", plan.tips).unwrap();
    writeln!(output, "internal_nodes\t{}", plan.internal_nodes).unwrap();
    writeln!(output, "nodes\t{}", plan.nodes).unwrap();
    writeln!(output, "edges\t{}", plan.edges).unwrap();
    writeln!(
        output,
        "direct_ancestor_nodes\t{}",
        plan.direct_ancestor_nodes
    )
    .unwrap();
    writeln!(output, "root_age\t{:.17}", plan.root_age).unwrap();
    writeln!(
        output,
        "total_branch_length\t{:.17}",
        plan.total_branch_length
    )
    .unwrap();
    writeln!(output, "areas\t{}", plan.areas).unwrap();
    writeln!(output, "max_range_size\t{}", plan.max_range_size).unwrap();
    writeln!(output, "states\t{}", plan.states).unwrap();
    writeln!(output, "max_anagenetic_periods\t{max_anagenetic_periods}").unwrap();
    writeln!(
        output,
        "max_q_off_diagonal_transitions\t{}",
        max_q_off_diagonal.map_or_else(|| "NA".to_string(), |value| value.to_string())
    )
    .unwrap();
    writeln!(
        output,
        "max_cladogenetic_scenarios\t{max_cladogenetic_scenarios}"
    )
    .unwrap();
    writeln!(output, "max_branch_segments\t{max_branch_segments}").unwrap();
    writeln!(output, "state_vector_bytes\t{}", plan.state_vector_bytes).unwrap();
    writeln!(
        output,
        "pruning_likelihood_payload_bytes\t{}",
        plan.pruning_likelihood_payload_bytes
    )
    .unwrap();
    writeln!(
        output,
        "comparison_criterion\t{}",
        loaded.parsed.comparison_criterion.as_str()
    )
    .unwrap();
    writeln!(
        output,
        "bsm_selection\t{}",
        loaded.parsed.bsm_selection.as_str()
    )
    .unwrap();
    writeln!(
        output,
        "bsm_requested_model_id\t{}",
        loaded
            .parsed
            .bsm_selection
            .requested_model_id()
            .unwrap_or("none")
    )
    .unwrap();
    writeln!(output, "bsm_enabled\t{}", bsm.is_some()).unwrap();
    writeln!(
        output,
        "bsm_samples\t{}",
        bsm.map_or(0, |config| config.samples)
    )
    .unwrap();
    writeln!(
        output,
        "bsm_output_level\t{}",
        bsm.map_or("none", |config| config.output_level.as_str())
    )
    .unwrap();
    writeln!(
        output,
        "bsm_output_format\t{}",
        bsm_format.unwrap_or("none")
    )
    .unwrap();
    writeln!(output, "bsm_available_parallelism\t{available_parallelism}").unwrap();
    writeln!(
        output,
        "bsm_resolved_threads\t{}",
        bsm_execution.map_or(0, |execution| execution.threads)
    )
    .unwrap();
    writeln!(
        output,
        "bsm_resolved_max_in_flight\t{}",
        bsm_execution.map_or(0, |execution| execution.max_in_flight)
    )
    .unwrap();
    writeln!(
        output,
        "bsm_resolved_checkpoint_samples\t{}",
        bsm_execution.map_or(0, |execution| execution.checkpoint_samples)
    )
    .unwrap();
    writeln!(
        output,
        "bsm_max_events_per_sample\t{}",
        format_optional_limit(bsm_execution.and_then(|execution| execution.max_events_per_sample))
    )
    .unwrap();
    writeln!(
        output,
        "bsm_max_events_total\t{}",
        format_optional_limit(bsm_execution.and_then(|execution| execution.max_events_total))
    )
    .unwrap();
    writeln!(
        output,
        "bsm_memory_budget_mb\t{}",
        format_optional_limit(bsm_execution.and_then(|execution| execution.memory_budget_mb))
    )
    .unwrap();
    writeln!(
        output,
        "bsm_shard_samples\t{}",
        bsm_execution
            .and_then(|execution| execution.shard_samples)
            .map_or_else(|| "none".to_string(), |value| value.to_string())
    )
    .unwrap();
    writeln!(
        output,
        "bsm_time_limit_seconds\t{}",
        format_optional_duration(bsm_execution.and_then(|execution| execution.time_limit))
    )
    .unwrap();
    writeln!(output, "risk_level\t{}", plan.risk_level).unwrap();
    writeln!(output, "warning_count\t{}", plan.warnings.len()).unwrap();
    writeln!(
        output,
        "warnings\t{}",
        if plan.warnings.is_empty() {
            "none".to_string()
        } else {
            analysis_result::encode_field(&plan.warnings.join(" | "))
        }
    )
    .unwrap();
    Ok(output)
}

#[derive(Clone, Debug)]
struct SelectedWorkflowModel {
    model_id: String,
    analysis_result: String,
    log_likelihood: f64,
    criterion_value: Option<f64>,
    criterion_weight: Option<f64>,
    criterion_rank: Option<usize>,
}

#[derive(Clone, Debug)]
struct WorkflowModelSelection {
    policy: &'static str,
    reason: &'static str,
    requested_model_id: Option<String>,
    selected: Option<SelectedWorkflowModel>,
}

fn select_workflow_model(
    comparison: &model_batch::ModelComparison,
    criterion: model_workflow::ComparisonCriterion,
    policy: &model_workflow::BsmSelection,
) -> Result<WorkflowModelSelection, CliError> {
    let criterion_values = |row: &model_batch::ComparisonRow| match criterion {
        model_workflow::ComparisonCriterion::Aic => (row.aic, row.aic_weight, row.aic_rank),
        model_workflow::ComparisonCriterion::Aicc => (row.aicc, row.aicc_weight, row.aicc_rank),
    };
    match policy {
        model_workflow::BsmSelection::None => Ok(WorkflowModelSelection {
            policy: "none",
            reason: "bsm_disabled",
            requested_model_id: None,
            selected: None,
        }),
        model_workflow::BsmSelection::ModelId(requested_model_id) => {
            let row = comparison
                .rows()
                .iter()
                .find(|row| row.model_id == *requested_model_id)
                .ok_or_else(|| {
                    model_workflow::ModelWorkflowError::UnknownBsmModelId(
                        requested_model_id.clone(),
                    )
                })?;
            if !row.eligible {
                return Err(model_workflow::ModelWorkflowError::IneligibleBsmModel(
                    row.model_id.clone(),
                )
                .into());
            }
            let (criterion_value, criterion_weight, criterion_rank) = criterion_values(row);
            Ok(WorkflowModelSelection {
                policy: "model_id",
                reason: "explicit_model_id",
                requested_model_id: Some(requested_model_id.clone()),
                selected: Some(SelectedWorkflowModel {
                    model_id: row.model_id.clone(),
                    analysis_result: checked_workflow_analysis_result_path(&row.analysis_result)?,
                    log_likelihood: row.log_likelihood,
                    criterion_value,
                    criterion_weight,
                    criterion_rank,
                }),
            })
        }
        model_workflow::BsmSelection::BestByCriterion => {
            let best = comparison
                .rows()
                .iter()
                .filter(|row| row.eligible && criterion_values(row).2 == Some(1))
                .collect::<Vec<_>>();
            if best.is_empty() {
                return Err(model_workflow::ModelWorkflowError::MissingBestModel(criterion).into());
            }
            if best.len() > 1 {
                return Err(model_workflow::ModelWorkflowError::TiedBestModels {
                    criterion,
                    models: best.iter().map(|row| row.model_id.clone()).collect(),
                }
                .into());
            }
            let row = best[0];
            let (criterion_value, criterion_weight, criterion_rank) = criterion_values(row);
            Ok(WorkflowModelSelection {
                policy: "best_by_criterion",
                reason: match criterion {
                    model_workflow::ComparisonCriterion::Aic => "unique_aic_rank_1",
                    model_workflow::ComparisonCriterion::Aicc => "unique_aicc_rank_1",
                },
                requested_model_id: None,
                selected: Some(SelectedWorkflowModel {
                    model_id: row.model_id.clone(),
                    analysis_result: checked_workflow_analysis_result_path(&row.analysis_result)?,
                    log_likelihood: row.log_likelihood,
                    criterion_value,
                    criterion_weight,
                    criterion_rank,
                }),
            })
        }
    }
}

fn checked_workflow_analysis_result_path(path: &str) -> Result<String, CliError> {
    let path_buf = Path::new(path);
    let components = path_buf.components().collect::<Vec<_>>();
    if path_buf.is_absolute()
        || components.len() != 2
        || !components
            .iter()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
        || components[0].as_os_str() != "models"
    {
        return Err(
            model_workflow::ModelWorkflowError::UnsafeAnalysisResultPath(path.to_string()).into(),
        );
    }
    Ok(components
        .iter()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn format_workflow_selection(
    loaded: &model_workflow::LoadedModelWorkflowRequest,
    selection: &WorkflowModelSelection,
) -> String {
    let selected = selection.selected.as_ref();
    let mut output = String::new();
    output.push_str("key\tvalue\n");
    writeln!(
        output,
        "format\t{}",
        model_workflow::MODEL_WORKFLOW_SELECTION_FORMAT
    )
    .unwrap();
    writeln!(
        output,
        "request_fingerprint\t{}",
        loaded.request_fingerprint()
    )
    .unwrap();
    writeln!(output, "selection_policy\t{}", selection.policy).unwrap();
    writeln!(
        output,
        "comparison_criterion\t{}",
        loaded.parsed.comparison_criterion.as_str()
    )
    .unwrap();
    writeln!(
        output,
        "requested_model_id\t{}",
        selection.requested_model_id.as_deref().unwrap_or("none")
    )
    .unwrap();
    writeln!(
        output,
        "selected_model_id\t{}",
        selected.map_or("none", |model| model.model_id.as_str())
    )
    .unwrap();
    writeln!(
        output,
        "selected_analysis_result\t{}",
        selected.map_or_else(
            || "none".to_string(),
            |model| format!(
                "{}/{}",
                model_workflow::MODEL_BATCH_DIRECTORY,
                model.analysis_result
            )
        )
    )
    .unwrap();
    writeln!(output, "selection_reason\t{}", selection.reason).unwrap();
    writeln!(
        output,
        "selected_eligible\t{}",
        if selected.is_some() { "true" } else { "NA" }
    )
    .unwrap();
    writeln!(
        output,
        "selected_lnL\t{}",
        selected.map_or_else(
            || "NA".to_string(),
            |model| format!("{:.17}", model.log_likelihood)
        )
    )
    .unwrap();
    writeln!(
        output,
        "selected_criterion_value\t{}",
        selected
            .and_then(|model| model.criterion_value)
            .map_or_else(|| "NA".to_string(), |value| format!("{value:.17}"))
    )
    .unwrap();
    writeln!(
        output,
        "selected_criterion_weight\t{}",
        selected
            .and_then(|model| model.criterion_weight)
            .map_or_else(|| "NA".to_string(), |value| format!("{value:.17}"))
    )
    .unwrap();
    writeln!(
        output,
        "selected_criterion_rank\t{}",
        selected
            .and_then(|model| model.criterion_rank)
            .map_or_else(|| "NA".to_string(), |value| value.to_string())
    )
    .unwrap();
    output
}

#[derive(Clone, Debug)]
struct WorkflowBsmOutcome {
    status: &'static str,
    format: String,
    output_level: String,
    layout: String,
    requested_samples: usize,
    completed_samples: usize,
    completed_anagenetic_events: usize,
    resumed: bool,
    validation: &'static str,
    files_checked: usize,
    data_rows_checked: Option<u64>,
}

impl WorkflowBsmOutcome {
    fn skipped() -> Self {
        Self {
            status: "skipped",
            format: "none".to_string(),
            output_level: "none".to_string(),
            layout: "none".to_string(),
            requested_samples: 0,
            completed_samples: 0,
            completed_anagenetic_events: 0,
            resumed: false,
            validation: "not_run",
            files_checked: 0,
            data_rows_checked: None,
        }
    }
}

fn run_model_workflow(
    config: ModelWorkflowConfig,
    cancellation: Option<biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
) -> Result<String, CliError> {
    let started = Instant::now();
    let loaded = model_workflow::load_model_workflow_request(&config.request_path)?;
    if !config.resume && config.output_dir_path.exists() {
        return Err(model_workflow::ModelWorkflowError::OutputDirectoryExists(
            config.output_dir_path,
        )
        .into());
    }
    let resumed_workspace = config
        .resume
        .then(|| {
            model_workflow::ModelWorkflowWorkspace::prepare(&loaded, &config.output_dir_path, true)
        })
        .transpose()?;
    let plan_batch = load_model_workflow_batch_config(
        &loaded,
        Path::new("__model_workflow_preflight_output__"),
        false,
    )?;
    let preflight = preflight_model_workflow(&loaded, &plan_batch)?;
    let workspace = match resumed_workspace {
        Some(workspace) => workspace,
        None => model_workflow::ModelWorkflowWorkspace::prepare(
            &loaded,
            &config.output_dir_path,
            false,
        )?,
    };

    let model_batch_dir = workspace.model_batch_dir();
    let batch_resumed = model_batch_dir.is_dir();
    let batch = load_model_workflow_batch_config(&loaded, &model_batch_dir, batch_resumed)?;
    let batch_started = Instant::now();
    let comparison = execute_parameter_batch(batch, cancellation.as_ref(), progress, None)?;
    let batch_elapsed = batch_started.elapsed();

    let selection = select_workflow_model(
        &comparison,
        loaded.parsed.comparison_criterion,
        &loaded.parsed.bsm_selection,
    )?;
    let selection_text = format_workflow_selection(&loaded, &selection);
    workspace.publish_selection(&selection_text)?;

    let mut bsm_elapsed = Duration::ZERO;
    let mut inspection_elapsed = Duration::ZERO;
    let bsm_outcome = if let (Some(bsm), Some(selected)) =
        (loaded.parsed.bsm.as_ref(), selection.selected.as_ref())
    {
        let bsm_result_dir = workspace.bsm_result_dir();
        let bsm_resumed = bsm_result_dir.is_dir();
        let analysis_result_dir = model_batch_dir.join(Path::new(&selected.analysis_result));
        let bsm_config = ParameterBsmConfig {
            analysis_result_dir_path: analysis_result_dir,
            bsm_samples: bsm.samples,
            bsm_output_dir_path: Some(bsm_result_dir.clone()),
            bsm_output_level: workflow_bsm_output_level(bsm.output_level),
            execution_request: workflow_bsm_execution_request(bsm)?,
            bsm_resume: bsm_resumed,
            bsm_interactive: bsm.interactive,
            seed: bsm.seed,
        };
        let bsm_started = Instant::now();
        run_parameter_bsm(bsm_config, cancellation)?;
        bsm_elapsed = bsm_started.elapsed();

        let inspection_started = Instant::now();
        let inspection = bsm_inspect::inspect(&bsm_result_dir, bsm.deep_inspection)
            .map_err(CliError::BsmInspection)?;
        inspection_elapsed = inspection_started.elapsed();
        if inspection.run_status != "complete"
            || inspection.completed_samples != bsm.samples
            || inspection.requested_samples != bsm.samples
        {
            return Err(model_workflow::ModelWorkflowError::InvalidWorkspace {
                path: bsm_result_dir,
                message: "BSM did not publish the requested complete sample set".to_string(),
            }
            .into());
        }
        WorkflowBsmOutcome {
            status: "complete",
            format: inspection.bsm_format,
            output_level: inspection.output_level,
            layout: inspection.layout,
            requested_samples: inspection.requested_samples,
            completed_samples: inspection.completed_samples,
            completed_anagenetic_events: inspection.completed_anagenetic_events,
            resumed: bsm_resumed,
            validation: if bsm.deep_inspection { "deep" } else { "quick" },
            files_checked: inspection.files_checked,
            data_rows_checked: inspection.data_rows_checked,
        }
    } else {
        debug_assert!(loaded.parsed.bsm.is_none() && selection.selected.is_none());
        WorkflowBsmOutcome::skipped()
    };

    let comparison_path = model_batch_dir.join("comparison.tsv");
    let model_average_path = model_batch_dir.join("model-averaged-ancestral-ranges.tsv");
    let comparison_source = read_file(&comparison_path)?;
    let model_average_source = read_file(&model_average_path)?;
    let completion = format_model_workflow_completion(
        &loaded,
        &preflight,
        &selection,
        &selection_text,
        &comparison_source,
        &model_average_source,
        &bsm_outcome,
    );
    workspace.publish_completion(&completion)?;

    Ok(format_model_workflow_run_output(
        &config,
        &loaded,
        &preflight,
        &selection,
        &bsm_outcome,
        batch_resumed,
        batch_elapsed,
        bsm_elapsed,
        inspection_elapsed,
        started.elapsed(),
    ))
}

fn format_model_workflow_completion(
    loaded: &model_workflow::LoadedModelWorkflowRequest,
    preflight: &ModelWorkflowPreflight,
    selection: &WorkflowModelSelection,
    selection_text: &str,
    comparison_source: &str,
    model_average_source: &str,
    bsm: &WorkflowBsmOutcome,
) -> String {
    let selected_model_id = selection
        .selected
        .as_ref()
        .map_or("none", |model| model.model_id.as_str());
    let mut output = String::new();
    output.push_str("key\tvalue\n");
    writeln!(
        output,
        "format\t{}",
        model_workflow::MODEL_WORKFLOW_COMPLETION_FORMAT
    )
    .unwrap();
    output.push_str("status\tcomplete\n");
    writeln!(
        output,
        "request_fingerprint\t{}",
        loaded.request_fingerprint()
    )
    .unwrap();
    writeln!(output, "candidate_models\t{}", preflight.candidates.len()).unwrap();
    writeln!(
        output,
        "model_batch_format\t{}",
        model_batch::MODEL_BATCH_RESULT_FORMAT
    )
    .unwrap();
    output.push_str("comparison_format\tbiogeo-model-comparison-v3\n");
    output.push_str("comparison_file\tmodel-batch/comparison.tsv\n");
    writeln!(
        output,
        "comparison_fingerprint\t{}",
        analysis_result::stable_fingerprint(comparison_source.as_bytes())
    )
    .unwrap();
    writeln!(
        output,
        "model_average_format\t{}",
        model_average::MODEL_AVERAGED_ANCESTRAL_RANGES_FORMAT
    )
    .unwrap();
    output.push_str("model_average_file\tmodel-batch/model-averaged-ancestral-ranges.tsv\n");
    writeln!(
        output,
        "model_average_fingerprint\t{}",
        analysis_result::stable_fingerprint(model_average_source.as_bytes())
    )
    .unwrap();
    output.push_str("selection_file\tselection.tsv\n");
    writeln!(
        output,
        "selection_fingerprint\t{}",
        analysis_result::stable_fingerprint(selection_text.as_bytes())
    )
    .unwrap();
    writeln!(output, "selected_model_id\t{selected_model_id}").unwrap();
    writeln!(output, "bsm_status\t{}", bsm.status).unwrap();
    writeln!(
        output,
        "bsm_result_dir\t{}",
        if bsm.status == "complete" {
            model_workflow::BSM_RESULT_DIRECTORY
        } else {
            "none"
        }
    )
    .unwrap();
    writeln!(output, "bsm_format\t{}", bsm.format).unwrap();
    writeln!(output, "bsm_requested_samples\t{}", bsm.requested_samples).unwrap();
    writeln!(output, "bsm_completed_samples\t{}", bsm.completed_samples).unwrap();
    writeln!(
        output,
        "bsm_completed_anagenetic_events\t{}",
        bsm.completed_anagenetic_events
    )
    .unwrap();
    output
}

#[allow(clippy::too_many_arguments)]
fn format_model_workflow_run_output(
    config: &ModelWorkflowConfig,
    loaded: &model_workflow::LoadedModelWorkflowRequest,
    preflight: &ModelWorkflowPreflight,
    selection: &WorkflowModelSelection,
    bsm: &WorkflowBsmOutcome,
    batch_resumed: bool,
    batch_elapsed: Duration,
    bsm_elapsed: Duration,
    inspection_elapsed: Duration,
    elapsed: Duration,
) -> String {
    let selected = selection.selected.as_ref();
    let mut output = String::new();
    writeln!(
        output,
        "format\t{}",
        model_workflow::MODEL_WORKFLOW_RUN_FORMAT
    )
    .unwrap();
    output.push_str("status\tcomplete\n");
    writeln!(
        output,
        "request_format\t{}",
        model_workflow::MODEL_WORKFLOW_REQUEST_FORMAT
    )
    .unwrap();
    writeln!(
        output,
        "request_fingerprint\t{}",
        loaded.request_fingerprint()
    )
    .unwrap();
    writeln!(
        output,
        "request_path\t{}",
        analysis_result::encode_field(&config.request_path.display().to_string())
    )
    .unwrap();
    writeln!(
        output,
        "output_dir\t{}",
        analysis_result::encode_field(&config.output_dir_path.display().to_string())
    )
    .unwrap();
    writeln!(
        output,
        "request_paths_portable\t{}",
        loaded.parsed.request_paths_portable
    )
    .unwrap();
    writeln!(output, "candidate_models\t{}", preflight.candidates.len()).unwrap();
    writeln!(
        output,
        "model_batch_format\t{}",
        model_batch::MODEL_BATCH_RESULT_FORMAT
    )
    .unwrap();
    writeln!(
        output,
        "model_batch_dir\t{}",
        analysis_result::encode_field(
            &config
                .output_dir_path
                .join(model_workflow::MODEL_BATCH_DIRECTORY)
                .display()
                .to_string()
        )
    )
    .unwrap();
    writeln!(output, "model_batch_resumed\t{batch_resumed}").unwrap();
    output.push_str("comparison_format\tbiogeo-model-comparison-v3\n");
    output.push_str("comparison_file\tmodel-batch/comparison.tsv\n");
    writeln!(
        output,
        "model_average_format\t{}",
        model_average::MODEL_AVERAGED_ANCESTRAL_RANGES_FORMAT
    )
    .unwrap();
    output.push_str("model_average_file\tmodel-batch/model-averaged-ancestral-ranges.tsv\n");
    writeln!(
        output,
        "comparison_criterion\t{}",
        loaded.parsed.comparison_criterion.as_str()
    )
    .unwrap();
    writeln!(output, "bsm_selection\t{}", selection.policy).unwrap();
    writeln!(
        output,
        "bsm_requested_model_id\t{}",
        selection.requested_model_id.as_deref().unwrap_or("none")
    )
    .unwrap();
    writeln!(
        output,
        "selected_model_id\t{}",
        selected.map_or("none", |model| model.model_id.as_str())
    )
    .unwrap();
    writeln!(
        output,
        "selected_analysis_result\t{}",
        selected.map_or_else(
            || "none".to_string(),
            |model| format!("model-batch/{}", model.analysis_result)
        )
    )
    .unwrap();
    writeln!(output, "selection_reason\t{}", selection.reason).unwrap();
    writeln!(
        output,
        "selected_lnL\t{}",
        selected.map_or_else(
            || "NA".to_string(),
            |model| format!("{:.17}", model.log_likelihood)
        )
    )
    .unwrap();
    writeln!(
        output,
        "selected_criterion_value\t{}",
        selected
            .and_then(|model| model.criterion_value)
            .map_or_else(|| "NA".to_string(), |value| format!("{value:.17}"))
    )
    .unwrap();
    writeln!(
        output,
        "selected_criterion_weight\t{}",
        selected
            .and_then(|model| model.criterion_weight)
            .map_or_else(|| "NA".to_string(), |value| format!("{value:.17}"))
    )
    .unwrap();
    writeln!(
        output,
        "selected_criterion_rank\t{}",
        selected
            .and_then(|model| model.criterion_rank)
            .map_or_else(|| "NA".to_string(), |value| value.to_string())
    )
    .unwrap();
    writeln!(output, "bsm_status\t{}", bsm.status).unwrap();
    writeln!(output, "bsm_format\t{}", bsm.format).unwrap();
    writeln!(
        output,
        "bsm_result_dir\t{}",
        if bsm.status == "complete" {
            analysis_result::encode_field(
                &config
                    .output_dir_path
                    .join(model_workflow::BSM_RESULT_DIRECTORY)
                    .display()
                    .to_string(),
            )
        } else {
            "none".to_string()
        }
    )
    .unwrap();
    writeln!(output, "bsm_output_level\t{}", bsm.output_level).unwrap();
    writeln!(output, "bsm_layout\t{}", bsm.layout).unwrap();
    writeln!(output, "bsm_requested_samples\t{}", bsm.requested_samples).unwrap();
    writeln!(output, "bsm_completed_samples\t{}", bsm.completed_samples).unwrap();
    writeln!(
        output,
        "bsm_completed_anagenetic_events\t{}",
        bsm.completed_anagenetic_events
    )
    .unwrap();
    writeln!(output, "bsm_resumed\t{}", bsm.resumed).unwrap();
    writeln!(output, "bsm_validation\t{}", bsm.validation).unwrap();
    writeln!(output, "bsm_files_checked\t{}", bsm.files_checked).unwrap();
    writeln!(
        output,
        "bsm_data_rows_checked\t{}",
        bsm.data_rows_checked
            .map_or_else(|| "NA".to_string(), |value| value.to_string())
    )
    .unwrap();
    writeln!(
        output,
        "model_batch_elapsed_seconds\t{:.9}",
        batch_elapsed.as_secs_f64()
    )
    .unwrap();
    writeln!(
        output,
        "bsm_elapsed_seconds\t{:.9}",
        bsm_elapsed.as_secs_f64()
    )
    .unwrap();
    writeln!(
        output,
        "inspection_elapsed_seconds\t{:.9}",
        inspection_elapsed.as_secs_f64()
    )
    .unwrap();
    writeln!(output, "elapsed_seconds\t{:.9}", elapsed.as_secs_f64()).unwrap();
    output
}

fn run_analysis_request(
    config: AnalysisRunConfig,
    cancellation: Option<&biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
) -> Result<String, CliError> {
    let telemetry_start = process_telemetry::ProcessTelemetryStart::capture();
    let started = Instant::now();
    let mut loaded_request = load_analysis_request(&config.request_path)?;
    loaded_request.model.analysis_result_dir_path = Some(config.output_dir_path.clone());
    run_parameter_model(loaded_request.model, cancellation, progress, None, None)?;
    let result = analysis_result::load_analysis_result(&config.output_dir_path)?;
    let result_bytes = directory_size(&result.root)?;
    let elapsed = started.elapsed();
    let elapsed_seconds = elapsed.as_secs_f64();
    let telemetry = telemetry_start.finish(elapsed);
    let request_fingerprint = analysis_result::stable_fingerprint(loaded_request.source.as_bytes());
    let available_parallelism = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);

    let mut output = String::new();
    writeln!(output, "format\t{}", analysis_request::ANALYSIS_RUN_FORMAT).unwrap();
    output.push_str("status\tcomplete\n");
    writeln!(
        output,
        "request_format\t{}",
        analysis_request::ANALYSIS_REQUEST_FORMAT
    )
    .unwrap();
    writeln!(output, "request_fingerprint\t{request_fingerprint}").unwrap();
    writeln!(
        output,
        "request_path\t{}",
        analysis_result::encode_field(&config.request_path.display().to_string())
    )
    .unwrap();
    writeln!(
        output,
        "portable_request\t{}",
        loaded_request.parsed.portable_paths
    )
    .unwrap();
    writeln!(output, "analysis_result_format\t{}", result.format_version).unwrap();
    writeln!(
        output,
        "analysis_result_dir\t{}",
        analysis_result::encode_field(&result.root.display().to_string())
    )
    .unwrap();
    writeln!(output, "mode\t{}", result.manifest.mode).unwrap();
    writeln!(output, "lnL\t{:.17}", result.manifest.log_likelihood).unwrap();
    writeln!(output, "states\t{}", result.manifest.states).unwrap();
    writeln!(output, "areas\t{}", result.manifest.areas).unwrap();
    writeln!(output, "tips\t{}", result.manifest.tips).unwrap();
    writeln!(output, "elapsed_seconds\t{elapsed_seconds:.9}").unwrap();
    writeln!(output, "result_bytes\t{result_bytes}").unwrap();
    writeln!(output, "telemetry_provider\t{}", telemetry.provider).unwrap();
    writeln!(
        output,
        "process_telemetry_available\t{}",
        telemetry.available
    )
    .unwrap();
    output.push_str("telemetry_scope\tprocess_lifetime_peak_and_analysis_run_cpu_delta\n");
    writeln!(
        output,
        "process_peak_working_set_bytes\t{}",
        telemetry_optional_u64_label(telemetry.peak_working_set_bytes)
    )
    .unwrap();
    writeln!(
        output,
        "process_cpu_user_seconds\t{}",
        telemetry_optional_float_label(telemetry.cpu_user_seconds)
    )
    .unwrap();
    writeln!(
        output,
        "process_cpu_kernel_seconds\t{}",
        telemetry_optional_float_label(telemetry.cpu_kernel_seconds)
    )
    .unwrap();
    writeln!(
        output,
        "process_cpu_total_seconds\t{}",
        telemetry_optional_float_label(telemetry.cpu_total_seconds)
    )
    .unwrap();
    writeln!(
        output,
        "average_logical_cores_used\t{}",
        telemetry_optional_float_label(telemetry.average_logical_cores_used)
    )
    .unwrap();
    output.push_str("analysis_worker_threads\t1\n");
    writeln!(output, "available_parallelism\t{available_parallelism}").unwrap();
    writeln!(
        output,
        "optimization_evaluations\t{}",
        result.manifest.optimization.map_or_else(
            || "NA".to_string(),
            |summary| summary.evaluations.to_string()
        )
    )
    .unwrap();
    writeln!(
        output,
        "optimization_converged\t{}",
        result
            .manifest
            .optimization
            .map_or_else(|| "NA".to_string(), |summary| summary.converged.to_string())
    )
    .unwrap();
    Ok(output)
}

fn telemetry_optional_u64_label(value: Option<u64>) -> String {
    value.map_or_else(|| "NA".to_string(), |value| value.to_string())
}

fn telemetry_optional_float_label(value: Option<f64>) -> String {
    value.map_or_else(|| "NA".to_string(), |value| format!("{value:.9}"))
}

fn directory_size(root: &Path) -> Result<u64, CliError> {
    let mut total = 0_u64;
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        for entry in fs::read_dir(&path).map_err(|source| CliError::Io {
            path: path.clone(),
            source,
        })? {
            let entry = entry.map_err(|source| CliError::Io {
                path: path.clone(),
                source,
            })?;
            let entry_path = entry.path();
            let metadata = entry.metadata().map_err(|source| CliError::Io {
                path: entry_path.clone(),
                source,
            })?;
            if metadata.is_dir() {
                pending.push(entry_path);
            } else if metadata.is_file() {
                total =
                    total
                        .checked_add(metadata.len())
                        .ok_or(CliError::AnalysisPlanSizeOverflow(
                            "analysis result byte count",
                        ))?;
            }
        }
    }
    Ok(total)
}

fn run_parameter_batch(
    config: ParameterBatchConfig,
    cancellation: Option<&biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
    dataset_id: Option<&str>,
) -> Result<String, CliError> {
    Ok(execute_parameter_batch(config, cancellation, progress, dataset_id)?.to_tsv())
}

fn execute_parameter_batch(
    config: ParameterBatchConfig,
    cancellation: Option<&biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
    dataset_id: Option<&str>,
) -> Result<model_batch::ModelComparison, CliError> {
    let local_cancellation;
    let cancellation = match cancellation {
        Some(cancellation) => cancellation,
        None => {
            local_cancellation = biogeo_core::ExecutionCancellationToken::new();
            &local_cancellation
        }
    };
    let manifest_input = read_file(&config.manifest_path)?;
    let invocation_fingerprint = parameter_batch_invocation_fingerprint(&config.invocation_tokens);
    let workspace = model_batch::prepare_model_batch_workspace(
        &config.manifest_path,
        &manifest_input,
        &config.output_dir_path,
        &invocation_fingerprint,
        config.resume,
    )?;

    progress
        .emit(ProgressEvent {
            event: "task_started",
            command: "model-batch",
            dataset_id,
            completed: Some(0),
            total: Some(workspace.jobs().len()),
            ..ProgressEvent::default()
        })
        .map_err(CliError::ProgressOutput)?;
    let mut reports = Vec::with_capacity(workspace.jobs().len());
    for (job_index, job) in workspace.jobs().iter().enumerate() {
        if cancellation.is_cancelled() {
            reports.extend(
                workspace.jobs()[job_index..]
                    .iter()
                    .map(model_batch::ModelBatchJobReport::not_started),
            );
            let attempt_path = workspace.record_attempt(&reports)?;
            progress
                .emit(ProgressEvent {
                    event: "task_cancelled",
                    command: "model-batch",
                    dataset_id,
                    completed: Some(job_index),
                    total: Some(workspace.jobs().len()),
                    ..ProgressEvent::default()
                })
                .map_err(CliError::ProgressOutput)?;
            return Err(CliError::TaskCancelled {
                operation: "model batch",
                attempt_path: Some(attempt_path),
            });
        }
        progress
            .emit(ProgressEvent {
                event: "unit_started",
                command: "model-batch",
                dataset_id,
                model_id: Some(&job.model_id),
                completed: Some(job_index),
                total: Some(workspace.jobs().len()),
                ..ProgressEvent::default()
            })
            .map_err(CliError::ProgressOutput)?;
        let result: Result<(), CliError> = (|| {
            if job.analysis_result_path.exists() {
                workspace
                    .validate_existing_result(job)
                    .map_err(CliError::from)?;
            } else {
                let mut model_config = config.template.clone();
                model_config.parameters_path = job.parameters_path.clone();
                model_config.analysis_result_dir_path = Some(job.analysis_result_path.clone());
                run_parameter_model(
                    model_config,
                    Some(cancellation),
                    progress,
                    dataset_id,
                    Some(&job.model_id),
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                reports.push(model_batch::ModelBatchJobReport::complete(job));
                progress
                    .emit(ProgressEvent {
                        event: "unit_completed",
                        command: "model-batch",
                        dataset_id,
                        model_id: Some(&job.model_id),
                        completed: Some(job_index + 1),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
            }
            Err(error) if error.is_cancelled() => {
                reports.push(model_batch::ModelBatchJobReport::cancelled(
                    job,
                    error.stable_code(),
                    error.to_string(),
                ));
                reports.extend(
                    workspace.jobs()[job_index + 1..]
                        .iter()
                        .map(model_batch::ModelBatchJobReport::not_started),
                );
                let attempt_path = workspace.record_attempt(&reports)?;
                progress
                    .emit(ProgressEvent {
                        event: "task_cancelled",
                        command: "model-batch",
                        dataset_id,
                        model_id: Some(&job.model_id),
                        completed: Some(job_index),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
                return Err(CliError::TaskCancelled {
                    operation: "model batch",
                    attempt_path: Some(attempt_path),
                });
            }
            Err(source) => {
                let error = CliError::ModelBatchJob {
                    model_id: job.model_id.clone(),
                    source: Box::new(source),
                };
                reports.push(model_batch::ModelBatchJobReport::failed(
                    job,
                    error.stable_code(),
                    error.to_string(),
                ));
                progress
                    .emit(ProgressEvent {
                        event: "unit_failed",
                        command: "model-batch",
                        dataset_id,
                        model_id: Some(&job.model_id),
                        completed: Some(job_index + 1),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
            }
        }
    }
    let failed = reports
        .iter()
        .filter(|report| {
            matches!(
                report.outcome,
                model_batch::ModelBatchJobOutcome::Failed { .. }
            )
        })
        .count();
    let attempt_path = workspace.record_attempt(&reports)?;
    if failed > 0 {
        return Err(CliError::ModelBatchFailures {
            failed,
            attempt_path,
        });
    }

    let comparison = workspace.compare_results()?;
    if cancellation.is_cancelled() {
        progress
            .emit(ProgressEvent {
                event: "task_cancelled",
                command: "model-batch",
                dataset_id,
                completed: Some(workspace.jobs().len()),
                total: Some(workspace.jobs().len()),
                ..ProgressEvent::default()
            })
            .map_err(CliError::ProgressOutput)?;
        return Err(CliError::TaskCancelled {
            operation: "model batch",
            attempt_path: Some(attempt_path.clone()),
        });
    }
    let model_average =
        match build_model_averaged_ancestral_ranges(&workspace, &comparison, cancellation) {
            Ok(model_average) => model_average,
            Err(error) if error.is_cancelled() => {
                progress
                    .emit(ProgressEvent {
                        event: "task_cancelled",
                        command: "model-batch",
                        dataset_id,
                        completed: Some(workspace.jobs().len()),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
                return Err(CliError::TaskCancelled {
                    operation: "model batch",
                    attempt_path: Some(attempt_path.clone()),
                });
            }
            Err(error) => return Err(error),
        };
    if cancellation.is_cancelled() {
        progress
            .emit(ProgressEvent {
                event: "task_cancelled",
                command: "model-batch",
                dataset_id,
                completed: Some(workspace.jobs().len()),
                total: Some(workspace.jobs().len()),
                ..ProgressEvent::default()
            })
            .map_err(CliError::ProgressOutput)?;
        return Err(CliError::TaskCancelled {
            operation: "model batch",
            attempt_path: Some(attempt_path),
        });
    }
    let comparison_tsv = comparison.to_tsv();
    workspace.finalize(&comparison_tsv, &model_average)?;
    progress
        .emit(ProgressEvent {
            event: "task_completed",
            command: "model-batch",
            dataset_id,
            completed: Some(workspace.jobs().len()),
            total: Some(workspace.jobs().len()),
            ..ProgressEvent::default()
        })
        .map_err(CliError::ProgressOutput)?;
    Ok(comparison)
}

fn build_model_averaged_ancestral_ranges(
    workspace: &model_batch::ModelBatchWorkspace,
    comparison: &model_batch::ModelComparison,
    cancellation: &biogeo_core::ExecutionCancellationToken,
) -> Result<String, CliError> {
    let mut accumulator = None;
    for (job, row) in workspace.jobs().iter().zip(comparison.rows()) {
        if cancellation.is_cancelled() {
            return Err(CliError::TaskCancelled {
                operation: "model averaging",
                attempt_path: None,
            });
        }
        debug_assert_eq!(job.model_id, row.model_id);
        let has_weight = row.aic_weight.is_some() || row.aicc_weight.is_some();
        if accumulator.is_some() && !has_weight {
            continue;
        }

        let loaded = analysis_result::load_analysis_result(&job.analysis_result_path)?;
        let replayed = replay_parameter_analysis(&loaded)?;
        if accumulator.is_none() {
            accumulator = Some(build_model_average_accumulator(
                workspace.jobs().len(),
                &replayed,
            )?);
        }
        let average = accumulator
            .as_mut()
            .expect("model-average accumulator was initialized");
        if !has_weight {
            continue;
        }

        let posteriors = biogeo_core::model_node_state_posteriors(
            &replayed.parsed_tree.tree,
            &replayed.states,
            &replayed.pruning,
            &replayed.model,
            analysis_result_root_prior(&loaded).to_core(),
        )?;
        let splits = biogeo_core::model_split_scenario_posteriors(
            &replayed.parsed_tree.tree,
            &replayed.states,
            &replayed.pruning,
            &replayed.model,
            analysis_result_root_prior(&loaded).to_core(),
        )?;
        average.add_model(
            model_average::WeightedModel {
                model_id: row.model_id.clone(),
                analysis_result: row.analysis_result.clone(),
                log_likelihood: row.log_likelihood,
                aic: comparison_weight(row.aic, row.delta_aic, row.aic_weight),
                aicc: comparison_weight(row.aicc, row.delta_aicc, row.aicc_weight),
            },
            &posteriors,
            &splits,
        )?;
    }

    accumulator
        .expect("model-batch manifests contain at least one model")
        .finish()
        .map_err(CliError::from)
}

fn build_model_average_accumulator(
    total_models: usize,
    replayed: &ReplayedParameterAnalysis,
) -> Result<model_average::ModelAverageAccumulator, CliError> {
    let tree = &replayed.parsed_tree.tree;
    let internal_nodes = (0..tree.node_count())
        .filter(|node| !tree.is_tip(*node))
        .map(|node| model_average::NodeMetadata {
            node,
            label: node_label(&replayed.parsed_tree, node),
            kind: if tree.is_direct_ancestor_node(node) {
                "direct_ancestor"
            } else if node == tree.root() {
                "root"
            } else {
                "internal"
            },
            clade: clade_label(&replayed.parsed_tree, node),
        })
        .collect::<Vec<_>>();
    let split_nodes = internal_nodes
        .iter()
        .filter(|metadata| metadata.kind != "direct_ancestor")
        .map(|metadata| {
            let children = tree
                .children(metadata.node)
                .expect("model-average internal node is inside the tree");
            if children.len() != 2 {
                return Err(CliError::NonBinaryInputTree {
                    nodes: vec![(metadata.node, children.len())],
                });
            }
            Ok(model_average::SplitNodeMetadata {
                node: metadata.node,
                left_node: children[0].node,
                right_node: children[1].node,
                left_clade: clade_label(&replayed.parsed_tree, children[0].node),
                right_clade: clade_label(&replayed.parsed_tree, children[1].node),
            })
        })
        .collect::<Result<Vec<_>, CliError>>()?;
    let states = replayed
        .states
        .states()
        .iter()
        .enumerate()
        .map(|(state_index, state)| model_average::StateMetadata {
            state_index,
            range_bits: state.bits(),
            range: range_label(*state, &replayed.parsed_ranges.area_names),
        })
        .collect();
    let areas = replayed
        .parsed_ranges
        .area_names
        .iter()
        .enumerate()
        .map(|(area_index, area)| model_average::AreaMetadata {
            area_index,
            area_bit: 1_u64 << area_index,
            area: area.clone(),
        })
        .collect();
    Ok(model_average::ModelAverageAccumulator::new(
        total_models,
        tree.node_count(),
        internal_nodes,
        split_nodes,
        areas,
        states,
    )?)
}

fn comparison_weight(
    value: Option<f64>,
    delta: Option<f64>,
    weight: Option<f64>,
) -> Option<model_average::CriterionWeight> {
    match (value, delta, weight) {
        (Some(value), Some(delta), Some(weight)) => Some(model_average::CriterionWeight {
            value,
            delta,
            weight,
        }),
        (None, None, None) => None,
        _ => unreachable!("model comparison criterion fields must be populated together"),
    }
}

fn run_dataset_batch(
    config: DatasetBatchConfig,
    cancellation: Option<&biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
) -> Result<String, CliError> {
    let local_cancellation;
    let cancellation = match cancellation {
        Some(cancellation) => cancellation,
        None => {
            local_cancellation = biogeo_core::ExecutionCancellationToken::new();
            &local_cancellation
        }
    };
    let manifest_input = read_file(&config.manifest_path)?;
    let workspace = dataset_batch::prepare_dataset_batch_workspace(
        &config.manifest_path,
        &manifest_input,
        &config.output_dir_path,
        config.resume,
    )?;

    progress
        .emit(ProgressEvent {
            event: "task_started",
            command: "dataset-batch",
            completed: Some(0),
            total: Some(workspace.jobs().len()),
            ..ProgressEvent::default()
        })
        .map_err(CliError::ProgressOutput)?;
    let mut reports = Vec::with_capacity(workspace.jobs().len());
    for (job_index, job) in workspace.jobs().iter().enumerate() {
        if cancellation.is_cancelled() {
            reports.extend(
                workspace.jobs()[job_index..]
                    .iter()
                    .map(dataset_batch::DatasetBatchJobReport::not_started),
            );
            let attempt_path = workspace.record_attempt(&reports)?;
            progress
                .emit(ProgressEvent {
                    event: "task_cancelled",
                    command: "dataset-batch",
                    completed: Some(job_index),
                    total: Some(workspace.jobs().len()),
                    ..ProgressEvent::default()
                })
                .map_err(CliError::ProgressOutput)?;
            return Err(CliError::TaskCancelled {
                operation: "dataset batch",
                attempt_path: Some(attempt_path),
            });
        }
        progress
            .emit(ProgressEvent {
                event: "unit_started",
                command: "dataset-batch",
                dataset_id: Some(&job.dataset_id),
                completed: Some(job_index),
                total: Some(workspace.jobs().len()),
                ..ProgressEvent::default()
            })
            .map_err(CliError::ProgressOutput)?;
        let result = (|| {
            let config_input = read_file(&job.config_path)?;
            let model_arguments =
                dataset_batch::parse_model_batch_config(&config_input, &job.config_path)?;
            let mut arguments = vec![
                "model-batch".to_string(),
                "--manifest".to_string(),
                job.models_manifest_path.to_string_lossy().into_owned(),
                "--output-dir".to_string(),
                job.result_path.to_string_lossy().into_owned(),
            ];
            arguments.extend(model_arguments);
            if config.resume && job.result_path.exists() {
                arguments.push("--resume".to_string());
            }
            let nested = match parse_command(arguments)? {
                Command::ParameterBatch(config) => config,
                Command::Help => {
                    return Err(CliError::DatasetBatchUnexpectedHelp {
                        dataset_id: job.dataset_id.clone(),
                    });
                }
                _ => unreachable!("dataset-batch built an unrelated command"),
            };
            run_parameter_batch(nested, Some(cancellation), progress, Some(&job.dataset_id))
                .map(|_| ())
        })();
        match result {
            Ok(()) => {
                reports.push(dataset_batch::DatasetBatchJobReport::complete(job));
                progress
                    .emit(ProgressEvent {
                        event: "unit_completed",
                        command: "dataset-batch",
                        dataset_id: Some(&job.dataset_id),
                        completed: Some(job_index + 1),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
            }
            Err(error) if error.is_cancelled() => {
                reports.push(dataset_batch::DatasetBatchJobReport::cancelled(
                    job,
                    error.stable_code(),
                    error.to_string(),
                ));
                reports.extend(
                    workspace.jobs()[job_index + 1..]
                        .iter()
                        .map(dataset_batch::DatasetBatchJobReport::not_started),
                );
                let attempt_path = workspace.record_attempt(&reports)?;
                progress
                    .emit(ProgressEvent {
                        event: "task_cancelled",
                        command: "dataset-batch",
                        dataset_id: Some(&job.dataset_id),
                        completed: Some(job_index),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
                return Err(CliError::TaskCancelled {
                    operation: "dataset batch",
                    attempt_path: Some(attempt_path),
                });
            }
            Err(source) => {
                let error = CliError::DatasetBatchJob {
                    dataset_id: job.dataset_id.clone(),
                    source: Box::new(source),
                };
                reports.push(dataset_batch::DatasetBatchJobReport::failed(
                    job,
                    error.stable_code(),
                    error.to_string(),
                ));
                progress
                    .emit(ProgressEvent {
                        event: "unit_failed",
                        command: "dataset-batch",
                        dataset_id: Some(&job.dataset_id),
                        completed: Some(job_index + 1),
                        total: Some(workspace.jobs().len()),
                        ..ProgressEvent::default()
                    })
                    .map_err(CliError::ProgressOutput)?;
            }
        }
    }
    let failed = reports
        .iter()
        .filter(|report| {
            matches!(
                report.outcome,
                dataset_batch::DatasetBatchJobOutcome::Failed { .. }
            )
        })
        .count();
    let attempt_path = workspace.record_attempt(&reports)?;
    if failed > 0 {
        return Err(CliError::DatasetBatchFailures {
            failed,
            attempt_path,
        });
    }
    let completion = workspace.finalize()?;
    progress
        .emit(ProgressEvent {
            event: "task_completed",
            command: "dataset-batch",
            completed: Some(workspace.jobs().len()),
            total: Some(workspace.jobs().len()),
            ..ProgressEvent::default()
        })
        .map_err(CliError::ProgressOutput)?;
    Ok(completion)
}

fn parameter_batch_invocation_fingerprint(tokens: &[String]) -> String {
    let mut identity = b"biogeo-model-batch-invocation-v1".to_vec();
    for token in tokens {
        identity.extend_from_slice(&(token.len() as u64).to_le_bytes());
        identity.extend_from_slice(token.as_bytes());
    }
    analysis_result::stable_fingerprint(&identity)
}

fn run_convert_tree(config: ConvertTreeConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed = parse_selected_tree_input_with_fill(
        &tree_input,
        config.tree_name.as_deref(),
        config.missing_branch_length_fill,
    )?;
    Ok(format!(
        "{}\n",
        biogeo_core::format_newick(&parsed.parsed_tree)
    ))
}

fn run_convert_ranges(config: ConvertRangesConfig) -> Result<String, CliError> {
    let input = read_file(&config.ranges_path)?;
    let mut table = legacy_import::import_range_table(
        &input,
        config.input_format,
        config.taxon_column.as_deref(),
    )?;
    if let Some(path) = config.taxon_map_path.as_ref() {
        legacy_import::apply_taxon_map(&mut table, &read_file(path)?)?;
    }
    if let Some(path) = config.area_map_path.as_ref() {
        legacy_import::apply_area_map(&mut table, &read_file(path)?)?;
    }
    legacy_import::validate_canonical_range_table(&table)?;
    Ok(table.to_tsv())
}

fn run_convert_biogeobears_strata(
    config: ConvertBioGeoBearsStrataConfig,
) -> Result<String, CliError> {
    Ok(legacy_import::import_biogeobears_strata(
        &config.time_boundaries_path,
        config.dispersal_matrices_path.as_deref(),
        config.adjacency_matrices_path.as_deref(),
        config.adjacency_range_rule,
        config.max_range_size,
        &config.output_dir,
    )?
    .to_tsv())
}

fn run_fossil_place(
    config: fossil_placement::FossilPlacementRunConfig,
) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree =
        parse_selected_tree_input(&tree_input, config.tree_name.as_deref())?.parsed_tree;
    let manifest_input = read_file(&config.manifest_path)?;
    Ok(fossil_placement::run(
        &config,
        &parsed_tree,
        &manifest_input,
    )?)
}

fn run_validate_inputs(config: ValidateInputsConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_input = parse_selected_tree_input_with_fill(
        &tree_input,
        config.tree_name.as_deref(),
        config.missing_branch_length_fill,
    )?;
    let tree_input_format = match parsed_input.format {
        biogeo_core::TreeInputFormat::Newick => "newick",
        biogeo_core::TreeInputFormat::Nexus => "nexus",
    };
    let tree_name = parsed_input.tree_name.unwrap_or_else(|| "none".to_string());
    let parsed_tree = parsed_input
        .parsed_tree
        .with_direct_ancestor_hooks_below(config.min_branch_length)
        .map_err(|error| CliError::Newick(error.into()))?;
    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;

    let non_binary_nodes = parsed_tree
        .tree
        .postorder_internal_nodes()
        .iter()
        .filter_map(|node| {
            let child_count = parsed_tree
                .tree
                .children(*node)
                .expect("postorder node is inside the tree")
                .len();
            (child_count != 2).then_some((*node, child_count))
        })
        .collect::<Vec<_>>();
    if !non_binary_nodes.is_empty() {
        return Err(CliError::NonBinaryInputTree {
            nodes: non_binary_nodes,
        });
    }

    let tree = &parsed_tree.tree;
    let node_ages = tree.node_ages_from_present();
    let automatic_tip_age_tolerance = config.tip_age_tolerance.is_none();
    let tip_age_tolerance = config
        .tip_age_tolerance
        .unwrap_or_else(|| biogeo_core::default_tip_age_tolerance(node_ages[tree.root()]));
    let mut ancient_tips = parsed_tree
        .tip_labels
        .iter()
        .filter_map(|tip| {
            let age = node_ages[tip.node];
            (age > tip_age_tolerance).then_some((tip.node, tip.label.as_str(), age))
        })
        .collect::<Vec<_>>();
    ancient_tips.sort_by_key(|(node, _, _)| *node);

    let total_branch_length = tree.edges().iter().map(|edge| edge.length).sum::<f64>();
    let minimum_branch_length = tree
        .edges()
        .iter()
        .map(|edge| edge.length)
        .reduce(f64::min)
        .unwrap_or(0.0);
    let maximum_branch_length = tree
        .edges()
        .iter()
        .map(|edge| edge.length)
        .reduce(f64::max)
        .unwrap_or(0.0);
    let zero_length_edges = tree
        .edges()
        .iter()
        .filter(|edge| edge.length == 0.0)
        .count();
    let maximum_observed_range_size = parsed_ranges
        .tip_ranges
        .iter()
        .map(|tip| tip.range.size())
        .max()
        .unwrap_or(0);
    let null_tip_ranges = parsed_ranges.exact_null_tip_count();

    let mut output = String::new();
    output.push_str("format\tbiogeo-input-validation-v1\n");
    output.push_str("status\tvalid\n");
    writeln!(output, "tree_input_format\t{tree_input_format}").unwrap();
    writeln!(output, "tree_name\t{tree_name}").unwrap();
    writeln!(
        output,
        "missing_branch_length_fill\t{}",
        format_missing_branch_length_fill(config.missing_branch_length_fill)
    )
    .unwrap();
    writeln!(output, "tips\t{}", tree.tip_nodes().len()).unwrap();
    writeln!(
        output,
        "internal_nodes\t{}",
        tree.postorder_internal_nodes().len()
    )
    .unwrap();
    writeln!(output, "nodes\t{}", tree.node_count()).unwrap();
    writeln!(output, "edges\t{}", tree.edges().len()).unwrap();
    output.push_str("binary\ttrue\n");
    writeln!(output, "root_age\t{:.17}", node_ages[tree.root()]).unwrap();
    writeln!(output, "tip_age_tolerance\t{tip_age_tolerance:.17}").unwrap();
    writeln!(
        output,
        "tip_age_tolerance_mode\t{}",
        if automatic_tip_age_tolerance {
            "auto"
        } else {
            "explicit"
        }
    )
    .unwrap();
    writeln!(output, "total_branch_length\t{total_branch_length:.17}").unwrap();
    writeln!(output, "minimum_branch_length\t{minimum_branch_length:.17}").unwrap();
    writeln!(output, "maximum_branch_length\t{maximum_branch_length:.17}").unwrap();
    writeln!(output, "zero_length_edges\t{zero_length_edges}").unwrap();
    writeln!(output, "ultrametric\t{}", ancient_tips.is_empty()).unwrap();
    writeln!(output, "ancient_tips\t{}", ancient_tips.len()).unwrap();
    writeln!(output, "areas\t{}", parsed_ranges.area_names.len()).unwrap();
    writeln!(output, "area_names\t{}", parsed_ranges.area_names.join(",")).unwrap();
    writeln!(output, "range_rows\t{}", parsed_ranges.tip_ranges.len()).unwrap();
    writeln!(
        output,
        "maximum_observed_range_size\t{maximum_observed_range_size}"
    )
    .unwrap();
    writeln!(output, "null_tip_ranges\t{null_tip_ranges}").unwrap();
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, &parsed_ranges);
    append_direct_ancestor_hooks(&mut output, &parsed_tree);
    if !ancient_tips.is_empty() {
        output.push_str("ancient_tip_ages\n");
        output.push_str("node\tlabel\tage\n");
        for (node, label, age) in ancient_tips {
            writeln!(output, "{node}\t{label}\t{age:.17}").unwrap();
        }
    }
    Ok(output)
}

#[derive(Clone, Debug)]
struct LoadedParameterTipInput {
    parsed_ranges: biogeo_core::ParsedTipRanges,
    detection: Option<biogeo_core::DetectionData>,
}

impl LoadedParameterTipInput {
    fn load(
        config: &ParameterModelConfig,
        parsed_tree: &biogeo_core::ParsedNewickTree,
    ) -> Result<Self, CliError> {
        if config.use_detection_model {
            let detections_path = config
                .detections_path
                .as_ref()
                .expect("detection CLI configuration was validated");
            let controls_path = config
                .controls_path
                .as_ref()
                .expect("detection CLI configuration was validated");
            let detection = biogeo_core::parse_detection_data(
                &read_file(detections_path)?,
                &read_file(controls_path)?,
                parsed_tree,
            )?;
            let parsed_ranges = biogeo_core::ParsedTipRanges::from_exact(
                detection.area_names.clone(),
                detection.observed_tip_ranges(),
            );
            Ok(Self {
                parsed_ranges,
                detection: Some(detection),
            })
        } else {
            let ranges_path = config
                .ranges_path
                .as_ref()
                .expect("range CLI configuration was validated");
            let parsed_ranges = parse_tip_ranges_input(
                &read_file(ranges_path)?,
                parsed_tree,
                config.use_ambiguities,
            )?;
            Ok(Self {
                parsed_ranges,
                detection: None,
            })
        }
    }
}

fn run_parameter_model(
    config: ParameterModelConfig,
    cancellation: Option<&biogeo_core::ExecutionCancellationToken>,
    progress: &mut ProgressReporter,
    dataset_id: Option<&str>,
    model_id: Option<&str>,
) -> Result<String, CliError> {
    let parameter_input = read_file(&config.parameters_path)?;
    let table = biogeo_core::parse_parameter_table(&parameter_input)?;

    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree_with_fill(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
        config.missing_branch_length_fill,
    )?;
    let tip_input = LoadedParameterTipInput::load(&config, &parsed_tree)?;
    let parsed_ranges = tip_input.parsed_ranges;
    let context =
        LoadedParameterModelContext::load(&config, &parsed_ranges.area_names, tip_input.detection)?;
    validate_parameter_model_table(&table, &context, config.mode)?;

    let num_areas = u8::try_from(parsed_ranges.area_names.len())
        .map_err(|_| CliError::AnalysisPlanSizeOverflow("area count"))?;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    preflight_state_space(
        num_areas,
        max_range_size,
        config.include_null_range,
        config.max_states,
    )?;
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let initial_resolved = table.resolve_initial()?;
    let initial_model = context.build_model(&initial_resolved)?;
    if !context.has_detection() {
        let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;
        validate_tip_state_constraints_for_cli(
            &parsed_tree,
            &states,
            &initial_model,
            &tip_likelihoods,
        )?;
    } else if config.mode == ParameterRunMode::Evaluate {
        let tip_likelihoods = context.detection_tip_likelihoods(&initial_resolved, &states)?;
        validate_tip_state_constraints_for_cli(
            &parsed_tree,
            &states,
            &initial_model,
            &tip_likelihoods,
        )?;
    }

    match config.mode {
        ParameterRunMode::Evaluate => {
            let resolved = initial_resolved;
            let model = initial_model;
            let pruning = if context.has_detection() {
                let tip_likelihoods = context.detection_tip_likelihoods(&resolved, &states)?;
                biogeo_core::LikelihoodEngine::new(
                    &parsed_tree.tree,
                    &states,
                    config.root_prior.to_core(),
                )
                .evaluate(&model, &tip_likelihoods)
                .map_err(biogeo_core::DecAnalysisError::from)?
            } else {
                let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;
                biogeo_core::run_fixed_model_likelihoods(
                    &parsed_tree.tree,
                    &states,
                    &tip_likelihoods,
                    &model,
                    config.root_prior.to_core(),
                )?
            };
            let posteriors =
                parameter_model_posteriors(&config, &parsed_tree, &states, &model, &pruning)?;
            write_parameter_analysis_result(
                &config,
                &parameter_input,
                &parsed_tree,
                &parsed_ranges,
                &states,
                &table,
                &resolved,
                &model,
                &pruning,
                None,
            )?;
            Ok(format_parameter_model_output(
                &config,
                &parsed_tree,
                &parsed_ranges,
                &states,
                &table,
                &resolved,
                &pruning,
                None,
                posteriors.ancestral.as_deref(),
                posteriors.splits.as_deref(),
            ))
        }
        ParameterRunMode::Optimize => {
            let local_cancellation;
            let cancellation = match cancellation {
                Some(cancellation) => cancellation,
                None => {
                    local_cancellation = biogeo_core::ExecutionCancellationToken::new();
                    &local_cancellation
                }
            };
            progress
                .emit(ProgressEvent {
                    event: "task_started",
                    command: "model-optimize",
                    dataset_id,
                    model_id,
                    ..ProgressEvent::default()
                })
                .map_err(CliError::ProgressOutput)?;
            let optimizer_context = context.clone();
            let mut progress_error = None;
            let optimized_result = {
                let mut report_optimization =
                    |event: biogeo_core::ParameterOptimizationProgress| {
                        if progress_error.is_some() {
                            return;
                        }
                        let event_name = match event.phase {
                            biogeo_core::ParameterOptimizationProgressPhase::StartInitialized => {
                                "optimization_start"
                            }
                            biogeo_core::ParameterOptimizationProgressPhase::IterationCompleted => {
                                "optimization_iteration"
                            }
                            biogeo_core::ParameterOptimizationProgressPhase::StartCompleted => {
                                "optimization_start_complete"
                            }
                        };
                        if let Err(error) = progress.emit(ProgressEvent {
                            event: event_name,
                            command: "model-optimize",
                            dataset_id,
                            model_id,
                            start: Some(event.start),
                            starts: Some(event.starts),
                            iteration: Some(event.iteration),
                            max_iterations: Some(event.max_iterations),
                            evaluations: Some(event.evaluations),
                            best_log_likelihood: event.best_log_likelihood,
                            ..ProgressEvent::default()
                        }) {
                            progress_error = Some(error);
                            cancellation.cancel();
                        }
                    };
                if context.has_detection() {
                    let optimizer_states = &states;
                    biogeo_core::optimize_parameter_table_dynamic_likelihoods_with_control(
                        &parsed_tree.tree,
                        &states,
                        config.root_prior.to_core(),
                        &table,
                        biogeo_core::ParameterOptimizationExecution::new(
                            &config.optimization,
                            cancellation,
                            &mut report_optimization,
                        ),
                        move |parameters| {
                            optimizer_context
                                .build_detection_evaluation(parameters, optimizer_states)
                        },
                    )
                } else {
                    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;
                    biogeo_core::optimize_parameter_table_likelihoods_with_control(
                        &parsed_tree.tree,
                        &states,
                        &tip_likelihoods,
                        config.root_prior.to_core(),
                        &table,
                        biogeo_core::ParameterOptimizationExecution::new(
                            &config.optimization,
                            cancellation,
                            &mut report_optimization,
                        ),
                        move |parameters| optimizer_context.build_model(parameters),
                    )
                }
            };
            if let Some(error) = progress_error {
                return Err(CliError::ProgressOutput(error));
            }
            let optimized = match optimized_result {
                Ok(optimized) => optimized,
                Err(error) => {
                    if error == biogeo_core::ParameterOptimizationError::Cancelled {
                        progress
                            .emit(ProgressEvent {
                                event: "task_cancelled",
                                command: "model-optimize",
                                dataset_id,
                                model_id,
                                ..ProgressEvent::default()
                            })
                            .map_err(CliError::ProgressOutput)?;
                    }
                    return Err(error.into());
                }
            };
            check_parameter_optimization_cancelled(cancellation, progress, dataset_id, model_id)?;
            let posteriors = parameter_model_posteriors(
                &config,
                &parsed_tree,
                &states,
                &optimized.model,
                &optimized.pruning,
            )?;
            check_parameter_optimization_cancelled(cancellation, progress, dataset_id, model_id)?;
            write_parameter_analysis_result(
                &config,
                &parameter_input,
                &parsed_tree,
                &parsed_ranges,
                &states,
                &table,
                &optimized.resolved_parameters,
                &optimized.model,
                &optimized.pruning,
                Some(&optimized),
            )?;
            progress
                .emit(ProgressEvent {
                    event: "task_completed",
                    command: "model-optimize",
                    dataset_id,
                    model_id,
                    evaluations: Some(optimized.evaluations),
                    best_log_likelihood: Some(optimized.log_likelihood),
                    ..ProgressEvent::default()
                })
                .map_err(CliError::ProgressOutput)?;
            Ok(format_parameter_model_output(
                &config,
                &parsed_tree,
                &parsed_ranges,
                &states,
                &table,
                &optimized.resolved_parameters,
                &optimized.pruning,
                Some(&optimized),
                posteriors.ancestral.as_deref(),
                posteriors.splits.as_deref(),
            ))
        }
    }
}

fn check_parameter_optimization_cancelled(
    cancellation: &biogeo_core::ExecutionCancellationToken,
    progress: &mut ProgressReporter,
    dataset_id: Option<&str>,
    model_id: Option<&str>,
) -> Result<(), CliError> {
    if !cancellation.is_cancelled() {
        return Ok(());
    }
    progress
        .emit(ProgressEvent {
            event: "task_cancelled",
            command: "model-optimize",
            dataset_id,
            model_id,
            ..ProgressEvent::default()
        })
        .map_err(CliError::ProgressOutput)?;
    Err(CliError::TaskCancelled {
        operation: "model optimization",
        attempt_path: None,
    })
}

#[derive(Debug)]
struct ReplayedParameterAnalysis {
    tree_input: String,
    parsed_tree: biogeo_core::ParsedNewickTree,
    parsed_ranges: biogeo_core::ParsedTipRanges,
    states: biogeo_core::StateSpace,
    resolved: biogeo_core::ResolvedParameters,
    model: biogeo_core::ModelConfig,
    pruning: biogeo_core::PruningResult,
}

fn run_bsm_inspect(config: BsmInspectConfig) -> Result<String, CliError> {
    let report = bsm_inspect::inspect(&config.bsm_result_dir_path, config.deep)
        .map_err(CliError::BsmInspection)?;
    Ok(format!(
        "format\t{}\n\
status\tvalid\n\
bsm_format\t{}\n\
bsm_result_dir\t{}\n\
output_level\t{}\n\
layout\t{}\n\
run_status\t{}\n\
completed_samples\t{}\n\
requested_samples\t{}\n\
completed_anagenetic_events\t{}\n\
completed_shards\t{}\n\
states\t{}\n\
areas\t{}\n\
nodes\t{}\n\
edges\t{}\n\
periods\t{}\n\
path_details\t{}\n\
sparse_occupancy\t{}\n\
validation\t{}\n\
files_checked\t{}\n\
data_rows_checked\t{}\n\
event_count_validation\t{}\n\
occupancy_validation\t{}\n\
path_validation\t{}\n\
state_constraint_validation\t{}\n\
diagnostic_violations\t{}\n",
        bsm_inspect::BSM_INSPECTION_FORMAT,
        report.bsm_format,
        analysis_result::encode_field(&config.bsm_result_dir_path.display().to_string()),
        report.output_level,
        report.layout,
        report.run_status,
        report.completed_samples,
        report.requested_samples,
        report.completed_anagenetic_events,
        report.shards,
        report.states,
        report.areas,
        report
            .nodes
            .map_or_else(|| "NA".to_string(), |value| value.to_string()),
        report
            .edges
            .map_or_else(|| "NA".to_string(), |value| value.to_string()),
        report
            .periods
            .map_or_else(|| "NA".to_string(), |value| value.to_string()),
        report.path_details,
        report.sparse_occupancy,
        if report.deep { "deep" } else { "quick" },
        report.files_checked,
        report
            .data_rows_checked
            .map_or_else(|| "NA".to_string(), |value| value.to_string()),
        report.event_count_validation,
        report.occupancy_validation,
        report.path_validation,
        report.state_constraint_validation,
        report
            .diagnostic_violations
            .map_or_else(|| "NA".to_string(), |value| value.to_string()),
    ))
}

fn run_analysis_result_inspect(config: AnalysisResultInspectConfig) -> Result<String, CliError> {
    let loaded = analysis_result::load_analysis_result(&config.analysis_result_dir_path)?;
    loaded.verify_replay_inputs()?;
    let replayed = config
        .replay
        .then(|| replay_parameter_analysis(&loaded))
        .transpose()?;
    let (bundle_format, bundle_fingerprint, bundle_files, dependencies, provenance) = loaded
        .input_bundle
        .as_ref()
        .map(|bundle| {
            (
                input_bundle::INPUT_BUNDLE_FORMAT_VERSION,
                bundle.fingerprint.as_str(),
                bundle.files.len(),
                bundle.dependency_count(),
                bundle.provenance_count(),
            )
        })
        .unwrap_or(("none", "none", 0, 0, 0));
    let mut output = format!(
        "format\t{}\n\
status\tvalid\n\
analysis_result_format\t{}\n\
analysis_result_dir\t{}\n\
analysis_result_fingerprint\t{}\n\
portable\t{}\n\
input_bundle_format\t{}\n\
input_bundle_fingerprint\t{}\n\
input_count\t{}\n\
bundle_file_count\t{}\n\
dependency_count\t{}\n\
provenance_count\t{}\n\
mode\t{}\n\
lnL\t{:.17}\n\
lnL_bits\t{:016x}\n\
model_fingerprint\t{}\n\
tip_observation_model\t{}\n\
states\t{}\n\
areas\t{}\n\
tips\t{}\n\
replay_validation\t{}\n",
        ANALYSIS_RESULT_INSPECTION_FORMAT,
        loaded.format_version,
        analysis_result::encode_field(&loaded.root.display().to_string()),
        loaded.fingerprint,
        loaded.is_portable(),
        bundle_format,
        bundle_fingerprint,
        loaded.manifest.inputs.len(),
        bundle_files,
        dependencies,
        provenance,
        loaded.manifest.mode,
        loaded.manifest.log_likelihood,
        loaded.manifest.log_likelihood.to_bits(),
        loaded.manifest.model_fingerprint,
        loaded.manifest.tip_observation_model,
        loaded.manifest.states,
        loaded.manifest.areas,
        loaded.manifest.tips,
        if replayed.is_some() {
            "passed"
        } else {
            "not_requested"
        },
    );
    if let Some(replayed) = replayed {
        writeln!(
            output,
            "replayed_lnL\t{:.17}",
            replayed.pruning.log_likelihood
        )
        .unwrap();
        writeln!(
            output,
            "replayed_lnL_bits\t{:016x}",
            replayed.pruning.log_likelihood.to_bits()
        )
        .unwrap();
    }
    Ok(output)
}

fn run_input_bundle_inspect(config: InputBundleInspectConfig) -> Result<String, CliError> {
    let bundle = input_bundle::load_input_bundle(&config.input_bundle_dir_path)
        .map_err(analysis_result::AnalysisResultError::from)?;
    Ok(format!(
        "format\t{}\n\
status\tvalid\n\
input_bundle_format\t{}\n\
input_bundle_dir\t{}\n\
input_bundle_fingerprint\t{}\n\
input_count\t{}\n\
dependency_count\t{}\n\
provenance_count\t{}\n\
file_count\t{}\n",
        INPUT_BUNDLE_INSPECTION_FORMAT,
        input_bundle::INPUT_BUNDLE_FORMAT_VERSION,
        analysis_result::encode_field(&bundle.root.display().to_string()),
        bundle.fingerprint,
        bundle.top_level_inputs.len(),
        bundle.dependency_count(),
        bundle.provenance_count(),
        bundle.files.len(),
    ))
}

fn run_analysis_result_migrate(config: AnalysisResultMigrateConfig) -> Result<String, CliError> {
    if config.output_dir_path.exists() {
        return Err(
            analysis_result::AnalysisResultError::OutputExists(config.output_dir_path).into(),
        );
    }
    let source = analysis_result::load_analysis_result(&config.analysis_result_dir_path)?;
    if source.format_version == analysis_result::ANALYSIS_RESULT_FORMAT_VERSION {
        return Err(analysis_result::AnalysisResultError::AlreadyCurrentFormat(
            source.format_version.clone(),
        )
        .into());
    }
    replay_parameter_analysis(&source)?;
    let staging_path = next_result_migration_path(&config.output_dir_path);
    let migrated = match (|| {
        let migrated = analysis_result::migrate_analysis_result(&source, &staging_path)?;
        replay_parameter_analysis(&migrated)?;
        verify_migrated_analysis_result(&source, &migrated)?;
        if config.output_dir_path.exists() {
            return Err(analysis_result::AnalysisResultError::OutputExists(
                config.output_dir_path.clone(),
            )
            .into());
        }
        fs_retry::rename(&staging_path, &config.output_dir_path).map_err(|source| {
            analysis_result::AnalysisResultError::Io {
                path: config.output_dir_path.clone(),
                source,
            }
        })?;
        Ok::<_, CliError>(migrated)
    })() {
        Ok(migrated) => migrated,
        Err(error) => {
            if staging_path.exists() {
                let _ = fs::remove_dir_all(&staging_path);
            }
            return Err(error);
        }
    };
    let target_root = fs::canonicalize(&config.output_dir_path).map_err(|source| CliError::Io {
        path: config.output_dir_path.clone(),
        source,
    })?;
    Ok(format!(
        "format\t{}\n\
status\tcomplete\n\
source_format\t{}\n\
target_format\t{}\n\
source_dir\t{}\n\
target_dir\t{}\n\
source_fingerprint\t{}\n\
target_fingerprint\t{}\n\
scientific_replay\tpassed\n\
portable\ttrue\n",
        ANALYSIS_RESULT_MIGRATION_FORMAT,
        source.format_version,
        migrated.format_version,
        analysis_result::encode_field(&source.root.display().to_string()),
        analysis_result::encode_field(&target_root.display().to_string()),
        source.fingerprint,
        migrated.fingerprint,
    ))
}

fn next_result_migration_path(output_dir: &Path) -> PathBuf {
    let parent = output_dir
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let output_name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("analysis-result");
    loop {
        let sequence = NEXT_RESULT_MIGRATION.fetch_add(1, AtomicOrdering::Relaxed);
        let candidate = parent.join(format!(
            ".{output_name}.migrate-{}-{sequence}",
            std::process::id()
        ));
        if !candidate.exists() {
            return candidate;
        }
    }
}

fn verify_migrated_analysis_result(
    source: &analysis_result::LoadedAnalysisResult,
    migrated: &analysis_result::LoadedAnalysisResult,
) -> Result<(), CliError> {
    let checks = [
        (
            "migration_mode",
            source.manifest.mode.as_str(),
            migrated.manifest.mode.as_str(),
        ),
        (
            "migration_model_fingerprint",
            source.manifest.model_fingerprint.as_str(),
            migrated.manifest.model_fingerprint.as_str(),
        ),
        (
            "migration_tip_observation_model",
            source.manifest.tip_observation_model.as_str(),
            migrated.manifest.tip_observation_model.as_str(),
        ),
        (
            "migration_root_prior",
            source.manifest.root_prior.as_str(),
            migrated.manifest.root_prior.as_str(),
        ),
    ];
    for (field, expected, actual) in checks {
        verify_analysis_replay_value(field, expected, actual)?;
    }
    verify_analysis_replay_value(
        "migration_lnL_bits",
        &format!("{:016x}", source.manifest.log_likelihood.to_bits()),
        &format!("{:016x}", migrated.manifest.log_likelihood.to_bits()),
    )?;
    verify_analysis_replay_value(
        "migration_min_branch_length_bits",
        &format!("{:016x}", source.manifest.min_branch_length.to_bits()),
        &format!("{:016x}", migrated.manifest.min_branch_length.to_bits()),
    )?;
    verify_analysis_replay_value(
        "migration_missing_branch_length_fill_bits",
        &source.manifest.missing_branch_length_fill.map_or_else(
            || "reject".to_string(),
            |value| format!("{:016x}", value.to_bits()),
        ),
        &migrated.manifest.missing_branch_length_fill.map_or_else(
            || "reject".to_string(),
            |value| format!("{:016x}", value.to_bits()),
        ),
    )?;
    verify_analysis_replay_count(
        "migration_states",
        source.manifest.states,
        migrated.manifest.states,
    )?;
    verify_analysis_replay_count(
        "migration_areas",
        source.manifest.areas,
        migrated.manifest.areas,
    )?;
    verify_analysis_replay_count(
        "migration_tips",
        source.manifest.tips,
        migrated.manifest.tips,
    )?;
    let source_roles: Vec<&str> = source.manifest.inputs.keys().map(String::as_str).collect();
    let target_roles: Vec<&str> = migrated
        .manifest
        .inputs
        .keys()
        .map(String::as_str)
        .collect();
    verify_analysis_replay_value(
        "migration_input_roles",
        &source_roles.join(","),
        &target_roles.join(","),
    )?;
    Ok(())
}

fn run_parameter_bsm(
    config: ParameterBsmConfig,
    cancellation: Option<biogeo_core::StochasticMapCancellationToken>,
) -> Result<String, CliError> {
    let loaded = analysis_result::load_analysis_result(&config.analysis_result_dir_path)?;
    let replayed = replay_parameter_analysis(&loaded)?;
    let runtime = BsmRuntimeConfig::from_analysis(&config, &loaded, &replayed.resolved);
    let available_parallelism = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let mut execution = resolve_bsm_execution_with_available(
        config.execution_request,
        config.bsm_samples,
        available_parallelism,
    )?
    .expect("model-bsm requires a positive sample count");
    let cancellation = if config.bsm_interactive && cancellation.is_none() {
        Some(biogeo_core::StochasticMapCancellationToken::new())
    } else {
        cancellation
    };
    let interactive_session = if config.bsm_interactive {
        Some(BsmInteractiveSession::start(
            config.bsm_samples,
            cancellation
                .as_ref()
                .expect("interactive BSM must have a cancellation token")
                .clone(),
        )?)
    } else {
        None
    };
    let execution_control = resolve_bsm_execution_control_values(
        config.bsm_samples,
        config.execution_request.time_limit,
        cancellation,
        interactive_session
            .as_ref()
            .map(|session| session.pause.clone()),
    )?;
    let stream = config.bsm_output_dir_path.is_some();
    let stochastic_maps = if stream {
        None
    } else {
        let engine = biogeo_core::LikelihoodEngine::new(
            &replayed.parsed_tree.tree,
            &replayed.states,
            runtime.root_prior.to_core(),
        );
        let mut maps = Vec::with_capacity(config.bsm_samples);
        engine
            .try_for_each_stochastic_map_parallel_seeded(
                &replayed.model,
                &replayed.pruning,
                config.bsm_samples,
                config.seed,
                bsm_parallel_options(execution, execution_control.as_ref(), 0),
                |sample_index, map| {
                    debug_assert_eq!(sample_index, maps.len());
                    maps.push(map.clone());
                    if let Some(session) = interactive_session.as_ref() {
                        session.progress.set_completed_samples(maps.len());
                    }
                    Ok::<(), CliError>(())
                },
            )
            .map_err(map_parallel_bsm_error)?;
        Some(maps)
    };

    if let Some(output_dir) = config.bsm_output_dir_path.as_deref() {
        execution = write_stochastic_histories_to_directory(
            output_dir,
            &BsmRunContext {
                runtime: &runtime,
                tree_input: &replayed.tree_input,
                tree_name: loaded.manifest.tree_name.as_deref(),
                fixed_ranges_input: None,
                parsed_tree: &replayed.parsed_tree,
                parsed_ranges: &replayed.parsed_ranges,
                states: &replayed.states,
                model: &replayed.model,
                result: &replayed.pruning,
                execution,
                execution_control: execution_control.clone(),
                interactive_progress: interactive_session
                    .as_ref()
                    .map(|session| &session.progress),
            },
        )?;
    }

    let mut output = format_parameter_bsm_output(
        &loaded,
        &replayed,
        stochastic_maps.as_deref(),
        &runtime,
        execution,
    )?;
    if let Some(output_dir) = config.bsm_output_dir_path.as_deref() {
        append_streamed_bsm_summary(&mut output, output_dir, &runtime, execution);
    }
    Ok(output)
}

fn replay_parameter_analysis(
    loaded: &analysis_result::LoadedAnalysisResult,
) -> Result<ReplayedParameterAnalysis, CliError> {
    loaded.verify_replay_inputs()?;
    let use_detection_model = loaded.manifest.tip_observation_model == "mf_dp_fdp_detection";
    let root_prior = analysis_result_root_prior(loaded);
    let config = ParameterModelConfig {
        mode: ParameterRunMode::Evaluate,
        tree_path: loaded.require_input_path("tree")?.to_path_buf(),
        tree_name: loaded.manifest.tree_name.clone(),
        ranges_path: if use_detection_model {
            None
        } else {
            Some(loaded.require_input_path("ranges")?.to_path_buf())
        },
        detections_path: use_detection_model
            .then(|| {
                loaded
                    .require_input_path("detections")
                    .map(Path::to_path_buf)
            })
            .transpose()?,
        controls_path: use_detection_model
            .then(|| loaded.require_input_path("controls").map(Path::to_path_buf))
            .transpose()?,
        use_detection_model,
        use_ambiguities: loaded.manifest.tip_observation_model == "ambiguous_ranges",
        parameters_path: loaded.root.join(analysis_result::RESOLVED_PARAMETERS_FILE),
        source_request_path: None,
        analysis_result_dir_path: None,
        min_branch_length: loaded.manifest.min_branch_length,
        missing_branch_length_fill: loaded.manifest.missing_branch_length_fill,
        max_range_size: Some(loaded.manifest.max_range_size),
        max_states: None,
        dispersal_multipliers_path: analysis_input_path(loaded, "dispersal_multipliers"),
        dispersal_strata_path: analysis_input_path(loaded, "dispersal_strata"),
        distance_matrix_path: analysis_input_path(loaded, "distance_matrix"),
        environment_distance_matrix_path: analysis_input_path(
            loaded,
            "environment_distance_matrix",
        ),
        extirpation_multipliers_path: analysis_input_path(loaded, "extirpation_multipliers"),
        area_sizes_path: analysis_input_path(loaded, "area_sizes"),
        include_null_range: loaded.manifest.include_null_range,
        root_prior,
        ancestral_probs: false,
        split_probs: false,
        optimization: biogeo_core::ParameterOptimizationConfig::default(),
    };
    let table = biogeo_core::parse_parameter_table(&loaded.resolved_parameters)?;
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree_with_fill(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
        config.missing_branch_length_fill,
    )?;
    let tip_input = LoadedParameterTipInput::load(&config, &parsed_tree)?;
    let parsed_ranges = tip_input.parsed_ranges;
    let context =
        LoadedParameterModelContext::load(&config, &parsed_ranges.area_names, tip_input.detection)?;
    validate_parameter_model_table(&table, &context, ParameterRunMode::Evaluate)?;
    let states = biogeo_core::StateSpace::new(
        parsed_ranges.area_names.len() as u8,
        loaded.manifest.max_range_size,
        loaded.manifest.include_null_range,
    )?;
    let resolved = table.resolve_initial()?;
    let model = context.build_model(&resolved)?;
    verify_analysis_replay_value(
        "model_fingerprint",
        &loaded.manifest.model_fingerprint,
        &analysis_result::model_fingerprint(&model),
    )?;
    let pruning = if context.has_detection() {
        let tip_likelihoods = context.detection_tip_likelihoods(&resolved, &states)?;
        biogeo_core::LikelihoodEngine::new(&parsed_tree.tree, &states, root_prior.to_core())
            .evaluate(&model, &tip_likelihoods)
            .map_err(biogeo_core::DecAnalysisError::from)?
    } else {
        let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;
        biogeo_core::run_fixed_model_likelihoods(
            &parsed_tree.tree,
            &states,
            &tip_likelihoods,
            &model,
            root_prior.to_core(),
        )?
    };
    verify_analysis_replay_count("states", loaded.manifest.states, states.len())?;
    verify_analysis_replay_count(
        "areas",
        loaded.manifest.areas,
        parsed_ranges.area_names.len(),
    )?;
    verify_analysis_replay_count("tips", loaded.manifest.tips, parsed_tree.tip_labels.len())?;
    let likelihood_tolerance = 1e-10 * loaded.manifest.log_likelihood.abs().max(1.0);
    if (pruning.log_likelihood - loaded.manifest.log_likelihood).abs() > likelihood_tolerance {
        return Err(CliError::AnalysisReplayMismatch {
            field: "lnL",
            expected: format!("{:.17}", loaded.manifest.log_likelihood),
            actual: format!("{:.17}", pruning.log_likelihood),
        });
    }
    Ok(ReplayedParameterAnalysis {
        tree_input,
        parsed_tree,
        parsed_ranges,
        states,
        resolved,
        model,
        pruning,
    })
}

fn analysis_result_root_prior(loaded: &analysis_result::LoadedAnalysisResult) -> RootPriorKind {
    match loaded.manifest.root_prior.as_str() {
        "flat" => RootPriorKind::Flat,
        "equal" => RootPriorKind::Equal,
        _ => unreachable!("analysis result loader validates root_prior"),
    }
}

fn analysis_input_path(
    loaded: &analysis_result::LoadedAnalysisResult,
    role: &str,
) -> Option<PathBuf> {
    loaded.input_path(role).map(Path::to_path_buf)
}

fn verify_analysis_replay_count(
    field: &'static str,
    expected: usize,
    actual: usize,
) -> Result<(), CliError> {
    verify_analysis_replay_value(field, &expected.to_string(), &actual.to_string())
}

fn verify_analysis_replay_value(
    field: &'static str,
    expected: &str,
    actual: &str,
) -> Result<(), CliError> {
    if expected == actual {
        Ok(())
    } else {
        Err(CliError::AnalysisReplayMismatch {
            field,
            expected: expected.to_string(),
            actual: actual.to_string(),
        })
    }
}

fn format_parameter_bsm_output(
    loaded: &analysis_result::LoadedAnalysisResult,
    replayed: &ReplayedParameterAnalysis,
    stochastic_maps: Option<&[biogeo_core::BiogeographicStochasticMap]>,
    runtime: &BsmRuntimeConfig,
    execution: ResolvedBsmExecution,
) -> Result<String, CliError> {
    let mut output = format!(
        "model\t{}\nmode\tbsm\nanalysis_result_format\t{}\nanalysis_result_dir\t{}\nanalysis_result_fingerprint\t{}\nsource_mode\t{}\nsource_lnL\t{:.15}\nlnL\t{:.15}\nstates\t{}\nareas\t{}\ntips\t{}\nmax_range_size\t{}\ninclude_null_range\t{}\nroot_prior\t{}\ntip_observation_model\t{}\n",
        runtime.model_name,
        loaded.format_version,
        loaded.root.display(),
        loaded.fingerprint,
        loaded.manifest.mode,
        loaded.manifest.log_likelihood,
        replayed.pruning.log_likelihood,
        replayed.states.len(),
        replayed.parsed_ranges.area_names.len(),
        replayed.parsed_tree.tip_labels.len(),
        replayed.states.max_range_size(),
        replayed.states.include_null_range(),
        runtime.root_prior.as_str(),
        loaded.manifest.tip_observation_model,
    );
    if let Some(optimization) = loaded.manifest.optimization {
        writeln!(
            output,
            "source_optimization_converged\t{}",
            optimization.converged
        )
        .unwrap();
        writeln!(
            output,
            "source_optimization_iterations\t{}",
            optimization.iterations
        )
        .unwrap();
        writeln!(
            output,
            "source_optimization_evaluations\t{}",
            optimization.evaluations
        )
        .unwrap();
        writeln!(
            output,
            "source_optimization_starts\t{}",
            optimization.starts
        )
        .unwrap();
        writeln!(
            output,
            "source_optimization_converged_starts\t{}",
            optimization.converged_starts
        )
        .unwrap();
    }
    append_selected_tree_name(&mut output, loaded.manifest.tree_name.as_deref());
    append_direct_ancestor_hooks(&mut output, &replayed.parsed_tree);
    output.push_str("\nparameters\nname\tvalue\n");
    for (name, value) in replayed.resolved.iter() {
        writeln!(output, "{name}\t{value}").unwrap();
    }
    append_stochastic_maps(
        &mut output,
        &replayed.parsed_tree,
        &replayed.parsed_ranges,
        &replayed.states,
        RetainedBsmOutput {
            model: &replayed.model,
            stochastic_maps,
            runtime,
            execution: Some(execution),
        },
    )?;
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn write_parameter_analysis_result(
    config: &ParameterModelConfig,
    source_parameters: &str,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    table: &biogeo_core::ParameterTable,
    resolved: &biogeo_core::ResolvedParameters,
    model: &biogeo_core::ModelConfig,
    pruning: &biogeo_core::PruningResult,
    optimization: Option<&biogeo_core::ParameterOptimizationResult>,
) -> Result<(), CliError> {
    let Some(output_dir) = config.analysis_result_dir_path.as_deref() else {
        return Ok(());
    };
    let mut fixed_table = table.clone();
    for (name, value) in resolved.iter() {
        fixed_table = fixed_table.with_fixed(name, value)?;
    }
    let resolved_parameters = fixed_table.to_versioned_tsv();
    let mut inputs = vec![analysis_result::AnalysisInputSpec {
        role: "tree",
        path: &config.tree_path,
        required_for_replay: true,
    }];
    push_analysis_input(&mut inputs, "ranges", config.ranges_path.as_deref(), true);
    push_analysis_input(
        &mut inputs,
        "detections",
        config.detections_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "controls",
        config.controls_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "dispersal_multipliers",
        config.dispersal_multipliers_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "dispersal_strata",
        config.dispersal_strata_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "distance_matrix",
        config.distance_matrix_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "environment_distance_matrix",
        config.environment_distance_matrix_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "extirpation_multipliers",
        config.extirpation_multipliers_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "area_sizes",
        config.area_sizes_path.as_deref(),
        true,
    );
    push_analysis_input(
        &mut inputs,
        "source_parameters",
        Some(&config.parameters_path),
        false,
    );
    push_analysis_input(
        &mut inputs,
        "analysis_request",
        config.source_request_path.as_deref(),
        false,
    );
    analysis_result::write_analysis_result(
        output_dir,
        &analysis_result::AnalysisResultWriteRequest {
            mode: config.mode.as_str(),
            log_likelihood: pruning.log_likelihood,
            model_fingerprint: &analysis_result::model_fingerprint(model),
            tip_observation_model: parameter_tip_observation_model(config),
            tree_name: config.tree_name.as_deref(),
            max_range_size: states.max_range_size(),
            include_null_range: states.include_null_range(),
            root_prior: config.root_prior.as_str(),
            min_branch_length: config.min_branch_length,
            missing_branch_length_fill: config.missing_branch_length_fill,
            states: states.len(),
            areas: parsed_ranges.area_names.len(),
            tips: parsed_tree.tip_labels.len(),
            optimization: optimization.map(|result| analysis_result::AnalysisOptimizationSummary {
                converged: result.converged,
                iterations: result.iterations,
                evaluations: result.evaluations,
                starts: result.starts,
                converged_starts: result.converged_starts,
            }),
            source_parameters,
            resolved_parameters: &resolved_parameters,
            inputs,
        },
    )?;
    Ok(())
}

fn parameter_tip_observation_model(config: &ParameterModelConfig) -> &'static str {
    if config.use_detection_model {
        "mf_dp_fdp_detection"
    } else if config.use_ambiguities {
        "ambiguous_ranges"
    } else {
        "exact_ranges"
    }
}

fn push_analysis_input<'a>(
    inputs: &mut Vec<analysis_result::AnalysisInputSpec<'a>>,
    role: &'static str,
    path: Option<&'a Path>,
    required_for_replay: bool,
) {
    if let Some(path) = path {
        inputs.push(analysis_result::AnalysisInputSpec {
            role,
            path,
            required_for_replay,
        });
    }
}

struct ParameterModelPosteriors {
    ancestral: Option<Vec<biogeo_core::NodeStatePosterior>>,
    splits: Option<Vec<biogeo_core::SplitScenarioPosterior>>,
}

fn parameter_model_posteriors(
    config: &ParameterModelConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    states: &biogeo_core::StateSpace,
    model: &biogeo_core::ModelConfig,
    pruning: &biogeo_core::PruningResult,
) -> Result<ParameterModelPosteriors, CliError> {
    let ancestral = config
        .ancestral_probs
        .then(|| {
            biogeo_core::model_node_state_posteriors(
                &parsed_tree.tree,
                states,
                pruning,
                model,
                config.root_prior.to_core(),
            )
        })
        .transpose()?;
    let splits = config
        .split_probs
        .then(|| {
            biogeo_core::model_split_scenario_posteriors(
                &parsed_tree.tree,
                states,
                pruning,
                model,
                config.root_prior.to_core(),
            )
        })
        .transpose()?;
    Ok(ParameterModelPosteriors { ancestral, splits })
}

#[allow(clippy::too_many_arguments)]
fn format_parameter_model_output(
    config: &ParameterModelConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    table: &biogeo_core::ParameterTable,
    resolved: &biogeo_core::ResolvedParameters,
    pruning: &biogeo_core::PruningResult,
    optimization: Option<&biogeo_core::ParameterOptimizationResult>,
    ancestral: Option<&[biogeo_core::NodeStatePosterior]>,
    splits: Option<&[biogeo_core::SplitScenarioPosterior]>,
) -> String {
    let mut output = format!(
        "model\tBIOGEOBEARS_LIKE_CONFIGURABLE\n\
mode\t{}\n\
parameter_table_format\t{}\n\
parameter_table\t{}\n\
lnL\t{:.15}\n\
states\t{}\n\
areas\t{}\n\
tips\t{}\n\
max_range_size\t{}\n\
include_null_range\t{}\n\
root_prior\t{}\n\
tip_observation_model\t{}\n\
ranges\t{}\n\
detections\t{}\n\
controls\t{}\n\
dispersal_multipliers\t{}\n\
dispersal_strata\t{}\n\
distance_matrix\t{}\n\
environment_distance_matrix\t{}\n\
extirpation_multipliers\t{}\n\
area_sizes\t{}\n",
        config.mode.as_str(),
        biogeo_core::PARAMETER_TABLE_FORMAT_VERSION,
        config.parameters_path.display(),
        pruning.log_likelihood,
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        parameter_tip_observation_model(config),
        optional_path_label(config.ranges_path.as_ref(), "none"),
        optional_path_label(config.detections_path.as_ref(), "none"),
        optional_path_label(config.controls_path.as_ref(), "none"),
        optional_path_label(config.dispersal_multipliers_path.as_ref(), "none"),
        optional_path_label(config.dispersal_strata_path.as_ref(), "none"),
        optional_path_label(config.distance_matrix_path.as_ref(), "none"),
        optional_path_label(config.environment_distance_matrix_path.as_ref(), "none",),
        optional_path_label(config.extirpation_multipliers_path.as_ref(), "uniform"),
        optional_path_label(config.area_sizes_path.as_ref(), "none"),
    );
    if let Some(optimization) = optimization {
        writeln!(output, "initial_step\t{}", config.optimization.initial_step).unwrap();
        writeln!(output, "tolerance\t{}", config.optimization.tolerance).unwrap();
        writeln!(
            output,
            "max_iterations\t{}",
            config.optimization.max_iterations
        )
        .unwrap();
        writeln!(output, "iterations\t{}", optimization.iterations).unwrap();
        writeln!(output, "evaluations\t{}", optimization.evaluations).unwrap();
        writeln!(output, "converged\t{}", optimization.converged).unwrap();
        writeln!(output, "starts\t{}", optimization.starts).unwrap();
        writeln!(
            output,
            "converged_starts\t{}",
            optimization.converged_starts
        )
        .unwrap();
    }
    if let Some(result_dir) = config.analysis_result_dir_path.as_deref() {
        writeln!(
            output,
            "analysis_result_format\t{}",
            analysis_result::ANALYSIS_RESULT_FORMAT_VERSION
        )
        .unwrap();
        writeln!(output, "analysis_result_dir\t{}", result_dir.display()).unwrap();
    }

    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    if config.use_ambiguities {
        append_ambiguous_range_counts(&mut output, parsed_ranges);
    }
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    output.push_str("\nparameters\nname\tmode\tvalue\tmin\tmax\ttransform\texpression\tbound\n");
    for spec in table.specs() {
        let (mode, expression) = match spec.mode() {
            biogeo_core::ParameterMode::Fixed { .. } => ("fixed", String::new()),
            biogeo_core::ParameterMode::Free { .. } => ("free", String::new()),
            biogeo_core::ParameterMode::Derived { expression } => {
                ("derived", expression.to_string())
            }
        };
        let transform = match spec.transform() {
            biogeo_core::ParameterTransform::Linear => "linear",
            biogeo_core::ParameterTransform::Log => "log",
            biogeo_core::ParameterTransform::Logit => "logit",
        };
        let bound = optimization
            .and_then(|result| {
                result
                    .free_parameters
                    .iter()
                    .find(|parameter| parameter.name == spec.name())
            })
            .map_or("not_free", |parameter| match parameter.bound {
                Some(biogeo_core::OptimizationBound::Lower) => "lower",
                Some(biogeo_core::OptimizationBound::Upper) => "upper",
                None => "interior",
            });
        writeln!(
            output,
            "{}\t{}\t{:.15}\t{}\t{}\t{}\t{}\t{}",
            spec.name(),
            mode,
            resolved
                .get(spec.name())
                .expect("validated parameter table must resolve every row"),
            spec.bounds().min,
            spec.bounds().max,
            transform,
            expression,
            bound,
        )
        .unwrap();
    }
    append_ancestral_probabilities(&mut output, parsed_tree, parsed_ranges, states, ancestral);
    append_split_probabilities(&mut output, parsed_tree, parsed_ranges, states, splits);
    output
}

fn run_fixed(
    config: FixedModelConfig,
    cancellation: Option<biogeo_core::StochasticMapCancellationToken>,
) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
    )?;

    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;
    let modifiers = load_anagenetic_modifiers(
        AnageneticModifierFiles {
            dispersal_multipliers: config.dispersal_multipliers_path.as_ref(),
            dispersal_strata: config.dispersal_strata_path.as_ref(),
            distance_matrix: config.distance_matrix_path.as_ref(),
            distance_exponent: config.distance_exponent,
            environment_distance_matrix: config.environment_distance_matrix_path.as_ref(),
            environment_distance_exponent: config.environment_distance_exponent,
            extirpation_multipliers: config.extirpation_multipliers_path.as_ref(),
            area_sizes: config.area_sizes_path.as_ref(),
            area_exponent: config.area_exponent,
        },
        &parsed_ranges.area_names,
    )?;

    let num_areas = parsed_ranges.area_names.len() as u8;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;

    let mut model = config
        .preset
        .build_model(config.d, config.e, config.j)
        .map_err(biogeo_core::DecAnalysisError::from)?
        .with_range_size_config(config.range_size);
    model = modifiers.apply(model);
    validate_tip_state_constraints_for_cli(&parsed_tree, &states, &model, &tip_likelihoods)?;
    let result = biogeo_core::run_fixed_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        &model,
        config.root_prior.to_core(),
    )?;
    let ancestral_probabilities = if config.ancestral_probs {
        Some(biogeo_core::model_node_state_posteriors(
            &parsed_tree.tree,
            &states,
            &result,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };
    let split_probabilities = if config.split_probs {
        Some(biogeo_core::model_split_scenario_posteriors(
            &parsed_tree.tree,
            &states,
            &result,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };
    let history_skeletons = if config.traceback_samples > 0 {
        let engine = biogeo_core::LikelihoodEngine::new(
            &parsed_tree.tree,
            &states,
            config.root_prior.to_core(),
        );
        Some(engine.sample_history_skeletons_seeded(
            &model,
            &result,
            config.traceback_samples,
            config.seed,
        )?)
    } else {
        None
    };
    let stream_stochastic_histories =
        config.bsm_samples > 0 && config.bsm_output_dir_path.is_some();
    let mut bsm_execution = resolve_bsm_execution(&config)?;
    let bsm_runtime = BsmRuntimeConfig::from_fixed(&config);
    let cancellation = if config.bsm_interactive && cancellation.is_none() {
        Some(biogeo_core::StochasticMapCancellationToken::new())
    } else {
        cancellation
    };
    let interactive_session = if config.bsm_interactive {
        Some(BsmInteractiveSession::start(
            config.bsm_samples,
            cancellation
                .as_ref()
                .expect("interactive BSM must have a cancellation token")
                .clone(),
        )?)
    } else {
        None
    };
    let bsm_execution_control = resolve_bsm_execution_control(
        &config,
        cancellation,
        interactive_session
            .as_ref()
            .map(|session| session.pause.clone()),
    )?;
    let stochastic_maps = if let Some(execution) = bsm_execution
        && !stream_stochastic_histories
    {
        let engine = biogeo_core::LikelihoodEngine::new(
            &parsed_tree.tree,
            &states,
            config.root_prior.to_core(),
        );
        let mut maps = Vec::with_capacity(config.bsm_samples);
        engine
            .try_for_each_stochastic_map_parallel_seeded(
                &model,
                &result,
                config.bsm_samples,
                config.seed,
                bsm_parallel_options(execution, bsm_execution_control.as_ref(), 0),
                |sample_index, map| {
                    debug_assert_eq!(sample_index, maps.len());
                    maps.push(map.clone());
                    if let Some(session) = interactive_session.as_ref() {
                        session.progress.set_completed_samples(maps.len());
                    }
                    Ok::<(), CliError>(())
                },
            )
            .map_err(map_parallel_bsm_error)?;
        Some(maps)
    } else {
        None
    };

    if let Some(output_dir) = config.bsm_output_dir_path.as_deref() {
        bsm_execution = Some(write_stochastic_histories_to_directory(
            output_dir,
            &BsmRunContext {
                runtime: &bsm_runtime,
                tree_input: &tree_input,
                tree_name: config.tree_name.as_deref(),
                fixed_ranges_input: Some(&ranges_input),
                parsed_tree: &parsed_tree,
                parsed_ranges: &parsed_ranges,
                states: &states,
                model: &model,
                result: &result,
                execution: bsm_execution.expect("BSM output requires a resolved execution plan"),
                execution_control: bsm_execution_control.clone(),
                interactive_progress: interactive_session
                    .as_ref()
                    .map(|session| &session.progress),
            },
        )?);
    }

    let mut output = format_fixed_output(
        &config,
        &parsed_tree,
        &parsed_ranges,
        &states,
        &result,
        FixedOutputExtras {
            model: &model,
            ancestral_probabilities: ancestral_probabilities.as_deref(),
            split_probabilities: split_probabilities.as_deref(),
            history_skeletons: history_skeletons.as_deref(),
            stochastic_maps: stochastic_maps.as_deref(),
            bsm_execution,
        },
        &bsm_runtime,
    )?;
    if let Some(output_dir) = config.bsm_output_dir_path.as_deref() {
        let execution = bsm_execution.expect("streamed BSM has an execution plan");
        append_streamed_bsm_summary(&mut output, output_dir, &bsm_runtime, execution);
    }
    Ok(output)
}

fn append_streamed_bsm_summary(
    output: &mut String,
    output_dir: &Path,
    runtime: &BsmRuntimeConfig,
    execution: ResolvedBsmExecution,
) {
    output.push_str("biogeographic_stochastic_histories\n");
    output.push_str(&format!(
        "bsm_format\t{}\n",
        bsm_stream_format(execution, runtime.output_level)
    ));
    output.push_str(&format!(
        "bsm_output_level\t{}\n",
        runtime.output_level.as_str()
    ));
    output.push_str(&format!("bsm_seed\t{}\n", runtime.seed));
    output.push_str(&format!("bsm_samples\t{}\n", runtime.sample_count));
    output.push_str(&format!(
        "bsm_rng_protocol\t{}\n",
        biogeo_core::INDEXED_BSM_RNG_PROTOCOL
    ));
    output.push_str(&format!("bsm_threads\t{}\n", execution.threads));
    output.push_str(&format!("bsm_max_in_flight\t{}\n", execution.max_in_flight));
    output.push_str(&format!(
        "bsm_checkpoint_samples\t{}\n",
        execution.checkpoint_samples
    ));
    output.push_str(&format!(
        "bsm_shard_samples\t{}\n",
        format_optional_limit(execution.shard_samples)
    ));
    output.push_str(&format!("bsm_resume\t{}\n", runtime.resume));
    output.push_str(&format!("bsm_interactive\t{}\n", runtime.interactive));
    output.push_str(&format!(
        "bsm_time_limit_seconds\t{}\n",
        format_optional_duration(execution.time_limit)
    ));
    output.push_str(&format!(
        "bsm_max_events_per_sample\t{}\n",
        format_optional_limit(execution.max_events_per_sample)
    ));
    output.push_str(&format!(
        "bsm_max_events_total\t{}\n",
        format_optional_limit(execution.max_events_total)
    ));
    output.push_str(&format!(
        "bsm_memory_budget_mb\t{}\n",
        format_optional_limit(execution.memory_budget_mb)
    ));
    output.push_str(&format!(
        "bsm_retained_bytes_per_sample_upper_bound\t{}\n",
        format_optional_estimate(execution.retained_bytes_per_sample_upper_bound)
    ));
    output.push_str(&format!(
        "bsm_buffered_history_bytes_upper_bound\t{}\n",
        format_optional_estimate(execution.buffered_history_bytes_upper_bound)
    ));
    output.push_str(&format!("bsm_output_dir\t{}\n", output_dir.display()));
}

fn run_de_optimize(config: DeOptimizeConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
    )?;

    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;
    let modifiers = load_anagenetic_modifiers(
        AnageneticModifierFiles {
            dispersal_multipliers: config.dispersal_multipliers_path.as_ref(),
            dispersal_strata: config.dispersal_strata_path.as_ref(),
            distance_matrix: config.distance_matrix_path.as_ref(),
            distance_exponent: config.distance_exponent,
            environment_distance_matrix: config.environment_distance_matrix_path.as_ref(),
            environment_distance_exponent: config.environment_distance_exponent,
            extirpation_multipliers: config.extirpation_multipliers_path.as_ref(),
            area_sizes: config.area_sizes_path.as_ref(),
            area_exponent: config.area_exponent,
        },
        &parsed_ranges.area_names,
    )?;

    let num_areas = parsed_ranges.area_names.len() as u8;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;
    let preset = config.preset;
    let range_size = config.optimization.range_size;
    let optimizer_modifiers = modifiers.clone();
    let result = biogeo_core::optimize_de_with_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        config.root_prior.to_core(),
        config.optimization,
        move |d, e| {
            Ok(optimizer_modifiers.apply(
                preset
                    .build_model(d, e, 0.0)?
                    .with_range_size_config(range_size),
            ))
        },
    )?;
    let mut model = config
        .preset
        .build_model(result.d, result.e, 0.0)
        .map_err(biogeo_core::DecAnalysisError::from)?
        .with_range_size_config(config.optimization.range_size);
    model = modifiers.apply(model);
    let ancestral_probabilities = if config.ancestral_probs {
        Some(biogeo_core::model_node_state_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };
    let split_probabilities = if config.split_probs {
        Some(biogeo_core::model_split_scenario_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };

    Ok(format_de_optimize_output(
        &config,
        &parsed_tree,
        &parsed_ranges,
        &states,
        &result,
        ancestral_probabilities.as_deref(),
        split_probabilities.as_deref(),
    ))
}

fn run_exponent_optimize(config: ExponentOptimizeConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
    )?;

    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;
    let modifiers = load_exponent_modifiers(&config, &parsed_ranges.area_names)?;

    let num_areas = parsed_ranges.area_names.len() as u8;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;

    let range_size = config.optimization.de.range_size;
    let optimizer_modifiers = modifiers.clone();
    let result = biogeo_core::optimize_de_exponent_with_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        config.root_prior.to_core(),
        config.optimization,
        move |d, e, exponent| {
            let model =
                biogeo_core::ModelConfig::preset_dec(d, e)?.with_range_size_config(range_size);
            optimizer_modifiers.apply(model, exponent)
        },
    )?;
    let model = modifiers.apply(
        biogeo_core::ModelConfig::preset_dec(result.d, result.e)
            .map_err(biogeo_core::DecAnalysisError::from)?
            .with_range_size_config(range_size),
        result.exponent,
    )?;
    let ancestral_probabilities = if config.ancestral_probs {
        Some(biogeo_core::model_node_state_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };
    let split_probabilities = if config.split_probs {
        Some(biogeo_core::model_split_scenario_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };

    Ok(format_exponent_optimize_output(
        &config,
        &parsed_tree,
        &parsed_ranges,
        &states,
        &result,
        ancestral_probabilities.as_deref(),
        split_probabilities.as_deref(),
    ))
}

fn run_xnu_optimize(config: XnuOptimizeConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
    )?;

    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;
    let modifiers = if let Some(strata_path) = &config.dispersal_strata_path {
        let schedule = LoadedRawAnageneticSchedule::load(strata_path, &parsed_ranges.area_names)?;
        if !schedule.has_distance() {
            return Err(CliError::MissingStratifiedModifier("distance_matrix"));
        }
        if !schedule.has_environment() {
            return Err(CliError::MissingStratifiedModifier(
                "environment_distance_matrix",
            ));
        }
        if !schedule.has_area_sizes() {
            return Err(CliError::MissingStratifiedModifier("area_sizes"));
        }
        if schedule.area_sizes_are_constant() {
            return Err(CliError::UnidentifiableAreaExponent);
        }
        LoadedXnuModifiers::TimeStratified(schedule)
    } else {
        load_xnu_modifiers(
            config
                .distance_matrix_path
                .as_ref()
                .expect("validated static xnu config must have a distance matrix"),
            config
                .environment_distance_matrix_path
                .as_ref()
                .expect("validated static xnu config must have an environment matrix"),
            config
                .area_sizes_path
                .as_ref()
                .expect("validated static xnu config must have area sizes"),
            config.dispersal_multipliers_path.as_deref(),
            &parsed_ranges.area_names,
            true,
        )?
    };

    let num_areas = parsed_ranges.area_names.len() as u8;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;

    let range_size = config.optimization.de.range_size;
    let optimizer_modifiers = modifiers.clone();
    let result = biogeo_core::optimize_de_xnu_with_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        config.root_prior.to_core(),
        config.optimization,
        move |d, e, x, n, u| {
            let model =
                biogeo_core::ModelConfig::preset_dec(d, e)?.with_range_size_config(range_size);
            optimizer_modifiers.apply(model, x, n, u)
        },
    )?;
    let model = modifiers.apply(
        biogeo_core::ModelConfig::preset_dec(result.d, result.e)
            .map_err(biogeo_core::DecAnalysisError::from)?
            .with_range_size_config(range_size),
        result.x,
        result.n,
        result.u,
    )?;
    let ancestral_probabilities = if config.ancestral_probs {
        Some(biogeo_core::model_node_state_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };
    let split_probabilities = if config.split_probs {
        Some(biogeo_core::model_split_scenario_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };

    Ok(format_xnu_optimize_output(
        &config,
        &parsed_tree,
        &parsed_ranges,
        &states,
        &result,
        ancestral_probabilities.as_deref(),
        split_probabilities.as_deref(),
    ))
}

fn run_pair_profile(config: PairProfileConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
    )?;

    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;
    let modifiers = load_pair_profile_modifiers(&config, &parsed_ranges.area_names)?;

    let num_areas = parsed_ranges.area_names.len() as u8;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;

    let pair = config.pair;
    let fixed_exponent = config.fixed_exponent;
    let range_size = config.profile.de.range_size;
    let result = biogeo_core::profile_de_pair_with_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        config.root_prior.to_core(),
        config.profile.clone(),
        move |d, e, first, second| {
            let (x, n, u) = pair.exponents(first, second, fixed_exponent);
            let model =
                biogeo_core::ModelConfig::preset_dec(d, e)?.with_range_size_config(range_size);
            modifiers.apply(model, x, n, u)
        },
    )?;

    Ok(format_pair_profile_output(
        &config,
        &parsed_tree,
        &parsed_ranges,
        &states,
        &result,
    ))
}

fn run_decj_optimize(config: DecJOptimizeConfig) -> Result<String, CliError> {
    let tree_input = read_file(&config.tree_path)?;
    let parsed_tree = parse_analysis_tree(
        &tree_input,
        config.tree_name.as_deref(),
        config.min_branch_length,
    )?;

    let ranges_input = read_file(&config.ranges_path)?;
    let parsed_ranges =
        parse_tip_ranges_input(&ranges_input, &parsed_tree, config.use_ambiguities)?;
    let modifiers = load_anagenetic_modifiers(
        AnageneticModifierFiles {
            dispersal_multipliers: config.dispersal_multipliers_path.as_ref(),
            dispersal_strata: config.dispersal_strata_path.as_ref(),
            distance_matrix: config.distance_matrix_path.as_ref(),
            distance_exponent: config.distance_exponent,
            environment_distance_matrix: config.environment_distance_matrix_path.as_ref(),
            environment_distance_exponent: config.environment_distance_exponent,
            extirpation_multipliers: config.extirpation_multipliers_path.as_ref(),
            area_sizes: config.area_sizes_path.as_ref(),
            area_exponent: config.area_exponent,
        },
        &parsed_ranges.area_names,
    )?;

    let num_areas = parsed_ranges.area_names.len() as u8;
    let max_range_size = config.max_range_size.unwrap_or(num_areas);
    let states =
        biogeo_core::StateSpace::new(num_areas, max_range_size, config.include_null_range)?;
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states)?;

    let range_size = config.optimization.range_size;
    let preset = config.preset;
    let optimizer_modifiers = modifiers.clone();
    let result = biogeo_core::optimize_decj_dej_with_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        config.root_prior.to_core(),
        config.optimization,
        move |d, e, j| {
            Ok(optimizer_modifiers.apply(
                preset
                    .build_model(d, e, j)?
                    .with_range_size_config(range_size),
            ))
        },
    )?;
    let model = modifiers.apply(
        config
            .preset
            .build_model(result.d, result.e, result.j)
            .map_err(biogeo_core::DecAnalysisError::from)?
            .with_range_size_config(range_size),
    );
    let ancestral_probabilities = if config.ancestral_probs {
        Some(biogeo_core::model_node_state_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };
    let split_probabilities = if config.split_probs {
        Some(biogeo_core::model_split_scenario_posteriors(
            &parsed_tree.tree,
            &states,
            &result.pruning,
            &model,
            config.root_prior.to_core(),
        )?)
    } else {
        None
    };

    Ok(format_decj_optimize_output(
        &config,
        &parsed_tree,
        &parsed_ranges,
        &states,
        &result,
        ancestral_probabilities.as_deref(),
        split_probabilities.as_deref(),
    ))
}

fn read_file(path: &Path) -> Result<String, CliError> {
    fs::read_to_string(path).map_err(|source| CliError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Clone, Debug)]
enum LoadedAnageneticDispersal {
    Static(biogeo_core::DispersalMultiplierMatrix),
    TimeStratified(biogeo_core::TimeStratifiedAnagenesis),
}

impl LoadedAnageneticDispersal {
    fn apply(&self, model: biogeo_core::ModelConfig) -> biogeo_core::ModelConfig {
        match self {
            Self::Static(matrix) => model.with_dispersal_multipliers(matrix.clone()),
            Self::TimeStratified(schedule) => {
                model.with_time_stratified_anagenesis(schedule.clone())
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
struct LoadedAnageneticModifiers {
    dispersal: Option<LoadedAnageneticDispersal>,
    extirpation: Option<biogeo_core::ExtirpationMultiplierVector>,
}

impl LoadedAnageneticModifiers {
    fn apply(&self, mut model: biogeo_core::ModelConfig) -> biogeo_core::ModelConfig {
        if let Some(dispersal) = &self.dispersal {
            model = dispersal.apply(model);
        }
        if let Some(extirpation) = &self.extirpation {
            model = model.with_extirpation_multipliers(extirpation.clone());
        }
        model
    }
}

#[derive(Clone, Debug)]
enum LoadedExponentModifiers {
    Pairwise {
        optimized_matrix: biogeo_core::DispersalMultiplierMatrix,
        fixed_pairwise_multipliers: Option<biogeo_core::DispersalMultiplierMatrix>,
        extirpation: Option<biogeo_core::ExtirpationMultiplierVector>,
    },
    AreaSize {
        area_sizes: biogeo_core::AreaSizeVector,
        fixed: LoadedAnageneticModifiers,
    },
    TimeStratified {
        kind: ExponentKind,
        schedule: LoadedRawAnageneticSchedule,
        fixed_x: Option<f64>,
        fixed_n: Option<f64>,
        fixed_u: Option<f64>,
        optimized_matrix: Option<biogeo_core::DispersalMultiplierMatrix>,
        fixed_pairwise: Option<biogeo_core::DispersalMultiplierMatrix>,
        optimized_area_sizes: Option<biogeo_core::AreaSizeVector>,
        fixed_extirpation: Option<biogeo_core::ExtirpationMultiplierVector>,
    },
}

impl LoadedExponentModifiers {
    fn apply(
        &self,
        mut model: biogeo_core::ModelConfig,
        exponent: f64,
    ) -> Result<biogeo_core::ModelConfig, biogeo_core::DecAnalysisError> {
        match self {
            Self::Pairwise {
                optimized_matrix,
                fixed_pairwise_multipliers,
                extirpation,
            } => {
                let dynamic = optimized_matrix
                    .distance_power_checked(exponent)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                let effective = match fixed_pairwise_multipliers {
                    Some(fixed) => dynamic
                        .elementwise_product(fixed)
                        .map_err(biogeo_core::AnagenesisError::from)?,
                    None => dynamic,
                };
                model = model.with_dispersal_multipliers(effective);
                if let Some(extirpation) = extirpation {
                    model = model.with_extirpation_multipliers(extirpation.clone());
                }
                Ok(model)
            }
            Self::AreaSize { area_sizes, fixed } => {
                let extirpation = area_sizes
                    .extirpation_multipliers(exponent)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                Ok(fixed.apply(model).with_extirpation_multipliers(extirpation))
            }
            Self::TimeStratified {
                kind,
                schedule,
                fixed_x,
                fixed_n,
                fixed_u,
                optimized_matrix,
                fixed_pairwise,
                optimized_area_sizes,
                fixed_extirpation,
            } => {
                let (x, n, u) = match kind {
                    ExponentKind::GeographicX => (Some(exponent), *fixed_n, *fixed_u),
                    ExponentKind::EnvironmentN => (*fixed_x, Some(exponent), *fixed_u),
                    ExponentKind::AreaSizeU => (*fixed_x, *fixed_n, Some(exponent)),
                };
                let mut static_pairwise = fixed_pairwise.clone();
                if let Some(matrix) = optimized_matrix {
                    let dynamic = matrix
                        .distance_power_checked(exponent)
                        .map_err(biogeo_core::AnagenesisError::from)?;
                    static_pairwise = Some(combine_optional_matrix(static_pairwise, dynamic)?);
                }
                if let Some(extirpation) = fixed_extirpation {
                    model = model.with_extirpation_multipliers(extirpation.clone());
                }
                if let Some(area_sizes) = optimized_area_sizes {
                    model = model.with_extirpation_multipliers(
                        area_sizes
                            .extirpation_multipliers(exponent)
                            .map_err(biogeo_core::AnagenesisError::from)?,
                    );
                }
                schedule.apply(model, x, n, u, 1.0, static_pairwise.as_ref())
            }
        }
    }
}

#[derive(Clone, Debug)]
enum LoadedXnuModifiers {
    Static {
        distance: biogeo_core::DispersalMultiplierMatrix,
        environment: biogeo_core::DispersalMultiplierMatrix,
        area_sizes: biogeo_core::AreaSizeVector,
        manual: Option<biogeo_core::DispersalMultiplierMatrix>,
    },
    TimeStratified(LoadedRawAnageneticSchedule),
}

#[derive(Clone, Debug)]
struct LoadedRawAnageneticStratum {
    oldest_age: f64,
    manual: Option<biogeo_core::DispersalMultiplierMatrix>,
    distance: Option<biogeo_core::DispersalMultiplierMatrix>,
    environment: Option<biogeo_core::DispersalMultiplierMatrix>,
    area_sizes: Option<biogeo_core::AreaSizeVector>,
    state_constraint: Option<biogeo_core::RangeStateConstraint>,
}

#[derive(Clone, Debug)]
struct LoadedRawAnageneticSchedule {
    strata: Vec<LoadedRawAnageneticStratum>,
}

impl LoadedRawAnageneticSchedule {
    fn load_any(path: &Path, area_names: &[String]) -> Result<Self, CliError> {
        let input = read_file(path)?;
        if is_anagenetic_strata_input(&input) {
            return Self::load(path, area_names);
        }

        let specs = biogeo_core::parse_dispersal_strata_table(&input)?;
        let base_dir = path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut strata = Vec::with_capacity(specs.len());
        for spec in specs {
            strata.push(LoadedRawAnageneticStratum {
                oldest_age: spec.oldest_age,
                manual: load_optional_stratum_matrix(
                    Some(&spec.matrix_path),
                    &base_dir,
                    area_names,
                )?,
                distance: None,
                environment: None,
                area_sizes: None,
                state_constraint: None,
            });
        }
        Ok(Self { strata })
    }

    fn load(path: &Path, area_names: &[String]) -> Result<Self, CliError> {
        let input = read_file(path)?;
        let specs = biogeo_core::parse_anagenetic_strata_table(&input)?;
        let base_dir = path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut strata = Vec::with_capacity(specs.len());
        for spec in specs {
            let manual = load_optional_stratum_matrix(
                spec.dispersal_matrix_path.as_deref(),
                &base_dir,
                area_names,
            )?;
            let distance = load_optional_stratum_matrix(
                spec.distance_matrix_path.as_deref(),
                &base_dir,
                area_names,
            )?;
            let environment = load_optional_stratum_matrix(
                spec.environment_distance_matrix_path.as_deref(),
                &base_dir,
                area_names,
            )?;
            let area_sizes = if let Some(raw_path) = spec.area_sizes_path.as_deref() {
                let resolved = resolve_stratum_path(&base_dir, raw_path);
                Some(biogeo_core::parse_area_sizes_table(
                    &read_file(&resolved)?,
                    area_names,
                )?)
            } else {
                None
            };
            let areas_allowed = load_optional_state_constraint_matrix(
                spec.areas_allowed_path.as_deref(),
                &base_dir,
                area_names,
            )?;
            let areas_adjacency = load_optional_state_constraint_matrix(
                spec.areas_adjacency_path.as_deref(),
                &base_dir,
                area_names,
            )?;
            let allowed_ranges = if let Some(raw_path) = spec.allowed_ranges_path.as_deref() {
                let resolved = resolve_stratum_path(&base_dir, raw_path);
                Some(biogeo_core::parse_allowed_range_states(
                    &read_file(&resolved)?,
                    area_names,
                )?)
            } else {
                None
            };
            let state_constraint =
                if areas_allowed.is_some() || areas_adjacency.is_some() || allowed_ranges.is_some()
                {
                    let constraint =
                        biogeo_core::RangeStateConstraint::new(areas_allowed, areas_adjacency)?;
                    Some(if let Some(allowed_ranges) = allowed_ranges {
                        constraint.with_allowed_ranges(allowed_ranges)?
                    } else {
                        constraint
                    })
                } else {
                    None
                };
            strata.push(LoadedRawAnageneticStratum {
                oldest_age: spec.oldest_age,
                manual,
                distance,
                environment,
                area_sizes,
                state_constraint,
            });
        }
        Ok(Self { strata })
    }

    fn has_distance(&self) -> bool {
        self.strata.iter().any(|stratum| stratum.distance.is_some())
    }

    fn has_environment(&self) -> bool {
        self.strata
            .iter()
            .any(|stratum| stratum.environment.is_some())
    }

    fn has_area_sizes(&self) -> bool {
        self.strata
            .iter()
            .any(|stratum| stratum.area_sizes.is_some())
    }

    fn has_manual(&self) -> bool {
        self.strata.iter().any(|stratum| stratum.manual.is_some())
    }

    fn area_sizes_are_constant(&self) -> bool {
        let mut values = self
            .strata
            .iter()
            .filter_map(|stratum| stratum.area_sizes.as_ref())
            .flat_map(|sizes| sizes.values().iter().copied());
        let Some(first) = values.next() else {
            return true;
        };
        values.all(|value| value == first)
    }

    fn apply(
        &self,
        model: biogeo_core::ModelConfig,
        x: Option<f64>,
        n: Option<f64>,
        u: Option<f64>,
        w: f64,
        static_pairwise: Option<&biogeo_core::DispersalMultiplierMatrix>,
    ) -> Result<biogeo_core::ModelConfig, biogeo_core::DecAnalysisError> {
        Ok(model.with_time_stratified_anagenesis(self.transformed(x, n, u, w, static_pairwise)?))
    }

    fn transformed(
        &self,
        x: Option<f64>,
        n: Option<f64>,
        u: Option<f64>,
        w: f64,
        static_pairwise: Option<&biogeo_core::DispersalMultiplierMatrix>,
    ) -> Result<biogeo_core::TimeStratifiedAnagenesis, biogeo_core::DecAnalysisError> {
        let mut transformed = Vec::with_capacity(self.strata.len());
        for stratum in &self.strata {
            let mut dispersal = static_pairwise.cloned();
            if let Some(manual) = &stratum.manual {
                let manual = manual
                    .distance_power_checked(w)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                dispersal = Some(combine_optional_matrix(dispersal, manual)?);
            }
            if let Some(distance) = &stratum.distance {
                let exponent = x.expect("validated raw distance stratum must have x");
                let distance = distance
                    .distance_power_checked(exponent)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                dispersal = Some(combine_optional_matrix(dispersal, distance)?);
            }
            if let Some(environment) = &stratum.environment {
                let exponent = n.expect("validated raw environment stratum must have n");
                let environment = environment
                    .distance_power_checked(exponent)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                dispersal = Some(combine_optional_matrix(dispersal, environment)?);
            }
            let extirpation = if let Some(area_sizes) = &stratum.area_sizes {
                Some(
                    area_sizes
                        .extirpation_multipliers(
                            u.expect("validated raw area-size stratum must have u"),
                        )
                        .map_err(biogeo_core::AnagenesisError::from)?,
                )
            } else {
                None
            };
            let mut transformed_stratum =
                biogeo_core::AnageneticTimeStratum::new(stratum.oldest_age, dispersal, extirpation)
                    .map_err(biogeo_core::AnagenesisError::from)?;
            if let Some(constraint) = &stratum.state_constraint {
                transformed_stratum = transformed_stratum
                    .with_state_constraint(constraint.clone())
                    .map_err(biogeo_core::AnagenesisError::from)?;
            }
            transformed.push(transformed_stratum);
        }
        biogeo_core::TimeStratifiedAnagenesis::new(transformed)
            .map_err(biogeo_core::AnagenesisError::from)
            .map_err(biogeo_core::DecAnalysisError::from)
    }
}

#[derive(Clone, Debug, Default)]
struct LoadedParameterModelContext {
    manual: Option<biogeo_core::DispersalMultiplierMatrix>,
    schedule: Option<LoadedRawAnageneticSchedule>,
    distance: Option<biogeo_core::DispersalMultiplierMatrix>,
    environment: Option<biogeo_core::DispersalMultiplierMatrix>,
    extirpation: Option<biogeo_core::ExtirpationMultiplierVector>,
    area_sizes: Option<biogeo_core::AreaSizeVector>,
    detection: Option<biogeo_core::DetectionData>,
}

impl LoadedParameterModelContext {
    fn load(
        config: &ParameterModelConfig,
        area_names: &[String],
        detection: Option<biogeo_core::DetectionData>,
    ) -> Result<Self, CliError> {
        if config.dispersal_multipliers_path.is_some() && config.dispersal_strata_path.is_some() {
            return Err(CliError::ConflictingDispersalModifiers);
        }
        let schedule = config
            .dispersal_strata_path
            .as_ref()
            .map(|path| LoadedRawAnageneticSchedule::load_any(path, area_names))
            .transpose()?;
        let manual = config
            .dispersal_multipliers_path
            .as_ref()
            .map(|path| {
                biogeo_core::parse_dispersal_multipliers_table(&read_file(path)?, area_names)
                    .map_err(CliError::from)
            })
            .transpose()?;
        let distance = config
            .distance_matrix_path
            .as_ref()
            .map(|path| {
                biogeo_core::parse_dispersal_multipliers_table(&read_file(path)?, area_names)
                    .map_err(CliError::from)
            })
            .transpose()?;
        let environment = config
            .environment_distance_matrix_path
            .as_ref()
            .map(|path| {
                biogeo_core::parse_dispersal_multipliers_table(&read_file(path)?, area_names)
                    .map_err(CliError::from)
            })
            .transpose()?;
        let extirpation = config
            .extirpation_multipliers_path
            .as_ref()
            .map(|path| {
                biogeo_core::parse_extirpation_multipliers_table(&read_file(path)?, area_names)
                    .map_err(CliError::from)
            })
            .transpose()?;
        let area_sizes = config
            .area_sizes_path
            .as_ref()
            .map(|path| {
                biogeo_core::parse_area_sizes_table(&read_file(path)?, area_names)
                    .map_err(CliError::from)
            })
            .transpose()?;

        if schedule
            .as_ref()
            .is_some_and(LoadedRawAnageneticSchedule::has_distance)
            && distance.is_some()
        {
            return Err(CliError::ConflictingParameterModifierSources("x"));
        }
        if schedule
            .as_ref()
            .is_some_and(LoadedRawAnageneticSchedule::has_environment)
            && environment.is_some()
        {
            return Err(CliError::ConflictingParameterModifierSources("n"));
        }
        if schedule
            .as_ref()
            .is_some_and(LoadedRawAnageneticSchedule::has_area_sizes)
            && area_sizes.is_some()
        {
            return Err(CliError::ConflictingParameterModifierSources("u"));
        }
        let has_any_area_sizes = area_sizes.is_some()
            || schedule
                .as_ref()
                .is_some_and(LoadedRawAnageneticSchedule::has_area_sizes);
        if extirpation.is_some() && has_any_area_sizes {
            return Err(CliError::ConflictingExtirpationModifiers);
        }

        Ok(Self {
            manual,
            schedule,
            distance,
            environment,
            extirpation,
            area_sizes,
            detection,
        })
    }

    fn has_detection(&self) -> bool {
        self.detection.is_some()
    }

    fn has_distance(&self) -> bool {
        self.distance.is_some()
            || self
                .schedule
                .as_ref()
                .is_some_and(LoadedRawAnageneticSchedule::has_distance)
    }

    fn has_environment(&self) -> bool {
        self.environment.is_some()
            || self
                .schedule
                .as_ref()
                .is_some_and(LoadedRawAnageneticSchedule::has_environment)
    }

    fn has_area_sizes(&self) -> bool {
        self.area_sizes.is_some()
            || self
                .schedule
                .as_ref()
                .is_some_and(LoadedRawAnageneticSchedule::has_area_sizes)
    }

    fn has_manual(&self) -> bool {
        self.manual.is_some()
            || self
                .schedule
                .as_ref()
                .is_some_and(LoadedRawAnageneticSchedule::has_manual)
    }

    fn area_sizes_are_uniform(&self) -> bool {
        if let Some(area_sizes) = &self.area_sizes {
            area_sizes.is_uniform()
        } else {
            self.schedule.as_ref().is_some_and(|schedule| {
                schedule.has_area_sizes() && schedule.area_sizes_are_constant()
            })
        }
    }

    fn active_parameter_targets(&self) -> Vec<&'static str> {
        let mut targets = vec![
            "d", "e", "a", "b", "j", "y", "s", "v", "mx01y", "mx01s", "mx01v", "mx01j",
        ];
        if self.has_distance() {
            targets.push("x");
        }
        if self.has_environment() {
            targets.push("n");
        }
        if self.has_area_sizes() {
            targets.push("u");
        }
        if self.has_manual() {
            targets.push("w");
        }
        if self.has_detection() {
            targets.extend(["mf", "dp", "fdp"]);
        }
        targets
    }

    fn build_model(
        &self,
        parameters: &biogeo_core::ResolvedParameters,
    ) -> Result<biogeo_core::ModelConfig, CliError> {
        let mut model = biogeo_core::ModelConfig::from_biogeobears_core_parameters(parameters)?;
        let x = parameters.require("x")?;
        let n = parameters.require("n")?;
        let u = parameters.require("u")?;
        let w = parameters.require("w")?;

        let mut pairwise = self
            .manual
            .as_ref()
            .map(|manual| manual.distance_power_checked(w))
            .transpose()?;
        if let Some(distance) = &self.distance {
            multiply_parameter_matrix(&mut pairwise, distance.distance_power_checked(x)?)?;
        }
        if let Some(environment) = &self.environment {
            multiply_parameter_matrix(&mut pairwise, environment.distance_power_checked(n)?)?;
        }
        if let Some(schedule) = &self.schedule {
            model = schedule.apply(model, Some(x), Some(n), Some(u), w, pairwise.as_ref())?;
        } else if let Some(pairwise) = pairwise {
            model = model.with_dispersal_multipliers(pairwise);
        }

        if let Some(extirpation) = &self.extirpation {
            model = model.with_extirpation_multipliers(extirpation.clone());
        } else if let Some(area_sizes) = &self.area_sizes {
            model = model.with_extirpation_multipliers(area_sizes.extirpation_multipliers(u)?);
        }
        Ok(model)
    }

    fn detection_tip_likelihoods(
        &self,
        parameters: &biogeo_core::ResolvedParameters,
        states: &biogeo_core::StateSpace,
    ) -> Result<Vec<biogeo_core::TipLikelihood>, CliError> {
        let data = self
            .detection
            .as_ref()
            .expect("detection likelihoods require detection input");
        let model = biogeo_core::DetectionModel::new(
            parameters.require("mf")?,
            parameters.require("dp")?,
            parameters.require("fdp")?,
        )?;
        Ok(model.tip_likelihoods(data, states)?)
    }

    fn build_detection_evaluation(
        &self,
        parameters: &biogeo_core::ResolvedParameters,
        states: &biogeo_core::StateSpace,
    ) -> Result<(biogeo_core::ModelConfig, Vec<biogeo_core::TipLikelihood>), CliError> {
        Ok((
            self.build_model(parameters)?,
            self.detection_tip_likelihoods(parameters, states)?,
        ))
    }
}

fn multiply_parameter_matrix(
    current: &mut Option<biogeo_core::DispersalMultiplierMatrix>,
    next: biogeo_core::DispersalMultiplierMatrix,
) -> Result<(), CliError> {
    *current = Some(match current.take() {
        Some(current) => current.elementwise_product(&next)?,
        None => next,
    });
    Ok(())
}

fn validate_parameter_model_table(
    table: &biogeo_core::ParameterTable,
    context: &LoadedParameterModelContext,
    mode: ParameterRunMode,
) -> Result<(), CliError> {
    for name in biogeo_core::BIOGEOBEARS_PARAMETER_NAMES {
        if table.spec(name).is_none() {
            return Err(CliError::MissingParameterTableParameter(name));
        }
    }
    for spec in table.specs() {
        if !biogeo_core::BIOGEOBEARS_PARAMETER_NAMES.contains(&spec.name()) {
            return Err(CliError::UnknownParameterTableParameter(
                spec.name().to_owned(),
            ));
        }
    }

    require_fixed_parameter_value(table, "mx01r", 0.5)?;
    if !context.has_detection() {
        for (name, expected) in [("mf", 0.1), ("dp", 1.0), ("fdp", 0.0)] {
            require_fixed_parameter_value(table, name, expected)?;
        }
    }
    if context.schedule.is_some() {
        let b = table
            .spec("b")
            .expect("complete BioGeoBEARS table was checked above");
        if !matches!(b.mode(), biogeo_core::ParameterMode::Fixed { value } if (*value - 1.0).abs() <= 1e-12)
        {
            return Err(CliError::StratifiedBranchLengthExponent);
        }
    }
    for (name, has_input, option) in [
        ("x", context.has_distance(), "--distance-matrix"),
        (
            "n",
            context.has_environment(),
            "--environment-distance-matrix",
        ),
        ("u", context.has_area_sizes(), "--area-sizes"),
    ] {
        if !has_input {
            let spec = table
                .spec(name)
                .expect("complete BioGeoBEARS table was checked above");
            if !matches!(spec.mode(), biogeo_core::ParameterMode::Fixed { value } if *value == 0.0)
            {
                return Err(CliError::ParameterInputRequired {
                    parameter: name,
                    option,
                });
            }
        }
    }
    if !context.has_manual() {
        let spec = table
            .spec("w")
            .expect("complete BioGeoBEARS table was checked above");
        if !matches!(spec.mode(), biogeo_core::ParameterMode::Fixed { value } if *value == 1.0) {
            return Err(CliError::ParameterInputRequired {
                parameter: "w",
                option: "--dispersal-multipliers",
            });
        }
    }

    let free = table.free_parameter_names();
    match mode {
        ParameterRunMode::Evaluate if !free.is_empty() => {
            return Err(CliError::ParameterEvaluateHasFree(
                free.into_iter().map(str::to_owned).collect(),
            ));
        }
        ParameterRunMode::Optimize if free.is_empty() => {
            return Err(CliError::ParameterOptimizeHasNoFree);
        }
        _ => {}
    }
    let active_targets = context.active_parameter_targets();
    let affecting = table.free_parameters_affecting(&active_targets)?;
    if let Some(unused) = table
        .free_parameter_names()
        .into_iter()
        .find(|name| !affecting.contains(name))
    {
        return Err(CliError::UnusedFreeParameter(unused.to_owned()));
    }
    if context.area_sizes_are_uniform() && !table.free_parameters_affecting(&["u"])?.is_empty() {
        return Err(CliError::UnidentifiableAreaExponent);
    }
    Ok(())
}

fn require_fixed_parameter_value(
    table: &biogeo_core::ParameterTable,
    name: &'static str,
    expected: f64,
) -> Result<(), CliError> {
    let spec = table
        .spec(name)
        .expect("complete BioGeoBEARS table was checked before compatibility checks");
    if matches!(spec.mode(), biogeo_core::ParameterMode::Fixed { value } if (*value - expected).abs() <= 1e-12)
    {
        Ok(())
    } else {
        Err(CliError::UnsupportedParameterSemantics {
            parameter: name,
            required_value: expected,
        })
    }
}

fn resolve_stratum_path(base_dir: &std::path::Path, raw_path: &str) -> PathBuf {
    let candidate = PathBuf::from(raw_path);
    if candidate.is_absolute() {
        candidate
    } else {
        base_dir.join(candidate)
    }
}

fn load_optional_stratum_matrix(
    raw_path: Option<&str>,
    base_dir: &std::path::Path,
    area_names: &[String],
) -> Result<Option<biogeo_core::DispersalMultiplierMatrix>, CliError> {
    let Some(raw_path) = raw_path else {
        return Ok(None);
    };
    let resolved = resolve_stratum_path(base_dir, raw_path);
    Ok(Some(biogeo_core::parse_dispersal_multipliers_table(
        &read_file(&resolved)?,
        area_names,
    )?))
}

fn load_optional_state_constraint_matrix(
    raw_path: Option<&str>,
    base_dir: &std::path::Path,
    area_names: &[String],
) -> Result<Option<biogeo_core::BinaryAreaMatrix>, CliError> {
    let Some(raw_path) = raw_path else {
        return Ok(None);
    };
    let resolved = resolve_stratum_path(base_dir, raw_path);
    Ok(Some(biogeo_core::parse_binary_area_matrix(
        &read_file(&resolved)?,
        area_names,
    )?))
}

fn combine_optional_matrix(
    current: Option<biogeo_core::DispersalMultiplierMatrix>,
    next: biogeo_core::DispersalMultiplierMatrix,
) -> Result<biogeo_core::DispersalMultiplierMatrix, biogeo_core::AnagenesisError> {
    match current {
        Some(current) => current
            .elementwise_product(&next)
            .map_err(biogeo_core::AnagenesisError::from),
        None => Ok(next),
    }
}

impl LoadedXnuModifiers {
    fn apply(
        &self,
        model: biogeo_core::ModelConfig,
        x: f64,
        n: f64,
        u: f64,
    ) -> Result<biogeo_core::ModelConfig, biogeo_core::DecAnalysisError> {
        match self {
            Self::Static {
                distance,
                environment,
                area_sizes,
                manual,
            } => {
                let geographic = distance
                    .distance_power_checked(x)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                let environmental = environment
                    .distance_power_checked(n)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                let mut dispersal = geographic
                    .elementwise_product(&environmental)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                if let Some(manual) = manual {
                    dispersal = dispersal
                        .elementwise_product(manual)
                        .map_err(biogeo_core::AnagenesisError::from)?;
                }
                let extirpation = area_sizes
                    .extirpation_multipliers(u)
                    .map_err(biogeo_core::AnagenesisError::from)?;
                Ok(model
                    .with_dispersal_multipliers(dispersal)
                    .with_extirpation_multipliers(extirpation))
            }
            Self::TimeStratified(schedule) => {
                schedule.apply(model, Some(x), Some(n), Some(u), 1.0, None)
            }
        }
    }
}

fn load_pair_profile_modifiers(
    config: &PairProfileConfig,
    area_names: &[String],
) -> Result<LoadedXnuModifiers, CliError> {
    if let Some(path) = &config.dispersal_strata_path {
        let schedule = LoadedRawAnageneticSchedule::load(path, area_names)?;
        for (present, column) in [
            (schedule.has_distance(), "distance_matrix"),
            (schedule.has_environment(), "environment_distance_matrix"),
            (schedule.has_area_sizes(), "area_sizes"),
        ] {
            if !present {
                return Err(CliError::MissingStratifiedModifier(column));
            }
        }
        if (config.pair.first() == ExponentKind::AreaSizeU
            || config.pair.second() == ExponentKind::AreaSizeU)
            && schedule.area_sizes_are_constant()
        {
            return Err(CliError::UnidentifiableAreaExponent);
        }
        return Ok(LoadedXnuModifiers::TimeStratified(schedule));
    }

    load_xnu_modifiers(
        config
            .distance_matrix_path
            .as_ref()
            .expect("validated static profile must have a distance matrix"),
        config
            .environment_distance_matrix_path
            .as_ref()
            .expect("validated static profile must have an environment matrix"),
        config
            .area_sizes_path
            .as_ref()
            .expect("validated static profile must have area sizes"),
        config.dispersal_multipliers_path.as_deref(),
        area_names,
        config.pair.first() == ExponentKind::AreaSizeU
            || config.pair.second() == ExponentKind::AreaSizeU,
    )
}

fn load_xnu_modifiers(
    distance_path: &Path,
    environment_path: &Path,
    area_sizes_path: &Path,
    manual_path: Option<&Path>,
    area_names: &[String],
    require_varying_area_sizes: bool,
) -> Result<LoadedXnuModifiers, CliError> {
    let distance =
        biogeo_core::parse_dispersal_multipliers_table(&read_file(distance_path)?, area_names)?;
    let environment =
        biogeo_core::parse_dispersal_multipliers_table(&read_file(environment_path)?, area_names)?;
    let area_sizes = biogeo_core::parse_area_sizes_table(&read_file(area_sizes_path)?, area_names)?;
    if require_varying_area_sizes && area_sizes.is_uniform() {
        return Err(CliError::UnidentifiableAreaExponent);
    }
    let manual = if let Some(path) = manual_path {
        Some(biogeo_core::parse_dispersal_multipliers_table(
            &read_file(path)?,
            area_names,
        )?)
    } else {
        None
    };

    Ok(LoadedXnuModifiers::Static {
        distance,
        environment,
        area_sizes,
        manual,
    })
}

fn load_exponent_modifiers(
    config: &ExponentOptimizeConfig,
    area_names: &[String],
) -> Result<LoadedExponentModifiers, CliError> {
    if let Some(strata_path) = &config.dispersal_strata_path {
        return load_stratified_exponent_modifiers(config, strata_path, area_names);
    }

    if config.kind == ExponentKind::AreaSizeU {
        let area_sizes_path = config
            .area_sizes_path
            .as_ref()
            .expect("validated free-u config must have area sizes");
        let area_sizes =
            biogeo_core::parse_area_sizes_table(&read_file(area_sizes_path)?, area_names)?;
        if area_sizes.is_uniform() {
            return Err(CliError::UnidentifiableAreaExponent);
        }
        let fixed = load_anagenetic_modifiers(
            AnageneticModifierFiles {
                dispersal_multipliers: config.dispersal_multipliers_path.as_ref(),
                dispersal_strata: config.dispersal_strata_path.as_ref(),
                distance_matrix: config.distance_matrix_path.as_ref(),
                distance_exponent: config.distance_exponent,
                environment_distance_matrix: config.environment_distance_matrix_path.as_ref(),
                environment_distance_exponent: config.environment_distance_exponent,
                extirpation_multipliers: None,
                area_sizes: None,
                area_exponent: None,
            },
            area_names,
        )?;
        return Ok(LoadedExponentModifiers::AreaSize { area_sizes, fixed });
    }

    let optimized_path = match config.kind {
        ExponentKind::GeographicX => config.distance_matrix_path.as_ref(),
        ExponentKind::EnvironmentN => config.environment_distance_matrix_path.as_ref(),
        ExponentKind::AreaSizeU => unreachable!(),
    }
    .expect("validated free pairwise exponent config must have its matrix");
    let optimized_matrix =
        biogeo_core::parse_dispersal_multipliers_table(&read_file(optimized_path)?, area_names)?;
    let fixed = load_anagenetic_modifiers(
        AnageneticModifierFiles {
            dispersal_multipliers: config.dispersal_multipliers_path.as_ref(),
            dispersal_strata: None,
            distance_matrix: if config.kind == ExponentKind::GeographicX {
                None
            } else {
                config.distance_matrix_path.as_ref()
            },
            distance_exponent: config.distance_exponent,
            environment_distance_matrix: if config.kind == ExponentKind::EnvironmentN {
                None
            } else {
                config.environment_distance_matrix_path.as_ref()
            },
            environment_distance_exponent: config.environment_distance_exponent,
            extirpation_multipliers: config.extirpation_multipliers_path.as_ref(),
            area_sizes: config.area_sizes_path.as_ref(),
            area_exponent: config.area_exponent,
        },
        area_names,
    )?;
    let fixed_pairwise_multipliers = match fixed.dispersal {
        Some(LoadedAnageneticDispersal::Static(matrix)) => Some(matrix),
        Some(LoadedAnageneticDispersal::TimeStratified(_)) => {
            unreachable!("free x/n validation rejects time-stratified dispersal")
        }
        None => None,
    };

    Ok(LoadedExponentModifiers::Pairwise {
        optimized_matrix,
        fixed_pairwise_multipliers,
        extirpation: fixed.extirpation,
    })
}

fn load_stratified_exponent_modifiers(
    config: &ExponentOptimizeConfig,
    strata_path: &Path,
    area_names: &[String],
) -> Result<LoadedExponentModifiers, CliError> {
    let schedule = LoadedRawAnageneticSchedule::load_any(strata_path, area_names)?;
    let schedule_has_target = match config.kind {
        ExponentKind::GeographicX => schedule.has_distance(),
        ExponentKind::EnvironmentN => schedule.has_environment(),
        ExponentKind::AreaSizeU => schedule.has_area_sizes(),
    };
    let optimized_path = match config.kind {
        ExponentKind::GeographicX => config.distance_matrix_path.as_ref(),
        ExponentKind::EnvironmentN => config.environment_distance_matrix_path.as_ref(),
        ExponentKind::AreaSizeU => None,
    };
    let optimized_area_path = if config.kind == ExponentKind::AreaSizeU {
        config.area_sizes_path.as_ref()
    } else {
        None
    };
    if !schedule_has_target && optimized_path.is_none() && optimized_area_path.is_none() {
        return Err(CliError::MissingStratifiedModifier(match config.kind {
            ExponentKind::GeographicX => "distance_matrix",
            ExponentKind::EnvironmentN => "environment_distance_matrix",
            ExponentKind::AreaSizeU => "area_sizes",
        }));
    }
    if config.kind == ExponentKind::AreaSizeU
        && schedule.has_area_sizes()
        && optimized_area_path.is_some()
    {
        return Err(CliError::ConflictingExtirpationModifiers);
    }

    let optimized_matrix = if let Some(path) = optimized_path {
        Some(biogeo_core::parse_dispersal_multipliers_table(
            &read_file(path)?,
            area_names,
        )?)
    } else {
        None
    };
    let optimized_area_sizes = if let Some(path) = optimized_area_path {
        let sizes = biogeo_core::parse_area_sizes_table(&read_file(path)?, area_names)?;
        if sizes.is_uniform() {
            return Err(CliError::UnidentifiableAreaExponent);
        }
        Some(sizes)
    } else {
        None
    };
    if config.kind == ExponentKind::AreaSizeU
        && schedule.has_area_sizes()
        && schedule.area_sizes_are_constant()
    {
        return Err(CliError::UnidentifiableAreaExponent);
    }

    let mut fixed_pairwise = None;
    if config.kind != ExponentKind::GeographicX
        && let (Some(path), Some(exponent)) = (
            config.distance_matrix_path.as_ref(),
            config.distance_exponent,
        )
    {
        let matrix = biogeo_core::parse_dispersal_multipliers_table(&read_file(path)?, area_names)?
            .distance_power_checked(exponent)?;
        fixed_pairwise = Some(
            combine_optional_matrix(fixed_pairwise, matrix)
                .map_err(biogeo_core::DecAnalysisError::from)?,
        );
    }
    if config.kind != ExponentKind::EnvironmentN
        && let (Some(path), Some(exponent)) = (
            config.environment_distance_matrix_path.as_ref(),
            config.environment_distance_exponent,
        )
    {
        let matrix = biogeo_core::parse_dispersal_multipliers_table(&read_file(path)?, area_names)?
            .distance_power_checked(exponent)?;
        fixed_pairwise = Some(
            combine_optional_matrix(fixed_pairwise, matrix)
                .map_err(biogeo_core::DecAnalysisError::from)?,
        );
    }

    let fixed_extirpation = if config.kind != ExponentKind::AreaSizeU {
        if let Some(path) = &config.extirpation_multipliers_path {
            Some(biogeo_core::parse_extirpation_multipliers_table(
                &read_file(path)?,
                area_names,
            )?)
        } else if let (Some(path), Some(exponent)) =
            (config.area_sizes_path.as_ref(), config.area_exponent)
        {
            Some(
                biogeo_core::parse_area_sizes_table(&read_file(path)?, area_names)?
                    .extirpation_multipliers(exponent)?,
            )
        } else {
            None
        }
    } else {
        None
    };
    if schedule.has_area_sizes() && fixed_extirpation.is_some() {
        return Err(CliError::ConflictingExtirpationModifiers);
    }

    Ok(LoadedExponentModifiers::TimeStratified {
        kind: config.kind,
        schedule,
        fixed_x: config.distance_exponent,
        fixed_n: config.environment_distance_exponent,
        fixed_u: config.area_exponent,
        optimized_matrix,
        fixed_pairwise,
        optimized_area_sizes,
        fixed_extirpation,
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct AnageneticModifierFiles<'a> {
    dispersal_multipliers: Option<&'a PathBuf>,
    dispersal_strata: Option<&'a PathBuf>,
    distance_matrix: Option<&'a PathBuf>,
    distance_exponent: Option<f64>,
    environment_distance_matrix: Option<&'a PathBuf>,
    environment_distance_exponent: Option<f64>,
    extirpation_multipliers: Option<&'a PathBuf>,
    area_sizes: Option<&'a PathBuf>,
    area_exponent: Option<f64>,
}

fn load_anagenetic_modifiers(
    files: AnageneticModifierFiles<'_>,
    area_names: &[String],
) -> Result<LoadedAnageneticModifiers, CliError> {
    let AnageneticModifierFiles {
        dispersal_multipliers: matrix_path,
        dispersal_strata: strata_path,
        distance_matrix: distance_matrix_path,
        distance_exponent,
        environment_distance_matrix: environment_distance_matrix_path,
        environment_distance_exponent,
        extirpation_multipliers: extirpation_path,
        area_sizes: area_sizes_path,
        area_exponent,
    } = files;
    if matrix_path.is_some() && strata_path.is_some() {
        return Err(CliError::ConflictingDispersalModifiers);
    }

    let raw_schedule = if let Some(path) = strata_path {
        let input = read_file(path)?;
        if is_anagenetic_strata_input(&input) {
            Some(LoadedRawAnageneticSchedule::load(path, area_names)?)
        } else {
            None
        }
    } else {
        None
    };
    let has_distance = distance_matrix_path.is_some()
        || raw_schedule
            .as_ref()
            .is_some_and(LoadedRawAnageneticSchedule::has_distance);
    let has_environment = environment_distance_matrix_path.is_some()
        || raw_schedule
            .as_ref()
            .is_some_and(LoadedRawAnageneticSchedule::has_environment);
    let has_area_sizes = area_sizes_path.is_some()
        || raw_schedule
            .as_ref()
            .is_some_and(LoadedRawAnageneticSchedule::has_area_sizes);
    if has_distance != distance_exponent.is_some() {
        return Err(CliError::IncompleteDistanceModifier);
    }
    if has_environment != environment_distance_exponent.is_some() {
        return Err(CliError::IncompleteEnvironmentDistanceModifier);
    }
    if has_area_sizes != area_exponent.is_some() {
        return Err(CliError::IncompleteAreaSizeModifier);
    }
    if extirpation_path.is_some() && has_area_sizes
        || area_sizes_path.is_some()
            && raw_schedule
                .as_ref()
                .is_some_and(LoadedRawAnageneticSchedule::has_area_sizes)
    {
        return Err(CliError::ConflictingExtirpationModifiers);
    }

    let mut static_pairwise_multipliers =
        if let (Some(path), Some(exponent)) = (distance_matrix_path, distance_exponent) {
            let input = read_file(path)?;
            Some(
                biogeo_core::parse_dispersal_multipliers_table(&input, area_names)?
                    .distance_power_checked(exponent)?,
            )
        } else {
            None
        };
    if let (Some(path), Some(exponent)) = (
        environment_distance_matrix_path,
        environment_distance_exponent,
    ) {
        let input = read_file(path)?;
        let environment = biogeo_core::parse_dispersal_multipliers_table(&input, area_names)?
            .distance_power_checked(exponent)?;
        static_pairwise_multipliers = Some(match static_pairwise_multipliers {
            Some(distance) => distance.elementwise_product(&environment)?,
            None => environment,
        });
    }

    let dispersal = if let Some(path) = matrix_path {
        let input = read_file(path)?;
        let mut matrix = biogeo_core::parse_dispersal_multipliers_table(&input, area_names)?;
        if let Some(static_multipliers) = &static_pairwise_multipliers {
            matrix = matrix.elementwise_product(static_multipliers)?;
        }
        Some(LoadedAnageneticDispersal::Static(matrix))
    } else if let Some(raw_schedule) = &raw_schedule {
        Some(LoadedAnageneticDispersal::TimeStratified(
            raw_schedule.transformed(
                distance_exponent,
                environment_distance_exponent,
                area_exponent,
                1.0,
                static_pairwise_multipliers.as_ref(),
            )?,
        ))
    } else if let Some(path) = strata_path {
        let input = read_file(path)?;
        let specs = biogeo_core::parse_dispersal_strata_table(&input)?;
        let base_dir = path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let mut strata = Vec::with_capacity(specs.len());
        for spec in specs {
            let candidate = PathBuf::from(&spec.matrix_path);
            let matrix_path = if candidate.is_absolute() {
                candidate
            } else {
                base_dir.join(candidate)
            };
            let matrix_input = read_file(&matrix_path)?;
            let mut matrix =
                biogeo_core::parse_dispersal_multipliers_table(&matrix_input, area_names)?;
            if let Some(static_multipliers) = &static_pairwise_multipliers {
                matrix = matrix.elementwise_product(static_multipliers)?;
            }
            strata.push(biogeo_core::DispersalTimeStratum::new(
                spec.oldest_age,
                matrix,
            )?);
        }
        Some(LoadedAnageneticDispersal::TimeStratified(
            biogeo_core::TimeStratifiedDispersal::new(strata)?.into(),
        ))
    } else {
        static_pairwise_multipliers.map(LoadedAnageneticDispersal::Static)
    };

    let extirpation = if let Some(path) = extirpation_path {
        let input = read_file(path)?;
        Some(biogeo_core::parse_extirpation_multipliers_table(
            &input, area_names,
        )?)
    } else if let (Some(path), Some(exponent)) = (area_sizes_path, area_exponent) {
        let input = read_file(path)?;
        Some(
            biogeo_core::parse_area_sizes_table(&input, area_names)?
                .extirpation_multipliers(exponent)?,
        )
    } else {
        None
    };

    Ok(LoadedAnageneticModifiers {
        dispersal,
        extirpation,
    })
}

fn is_anagenetic_strata_input(input: &str) -> bool {
    input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .is_some_and(|header| {
            let fields: Vec<&str> = header.split_whitespace().collect();
            fields
                == [
                    "oldest_age",
                    "matrix",
                    "distance_matrix",
                    "environment_distance_matrix",
                    "area_sizes",
                ]
                || fields
                    == [
                        "oldest_age",
                        "matrix",
                        "distance_matrix",
                        "environment_distance_matrix",
                        "area_sizes",
                        "areas_allowed",
                        "areas_adjacency",
                    ]
                || fields
                    == [
                        "oldest_age",
                        "matrix",
                        "distance_matrix",
                        "environment_distance_matrix",
                        "area_sizes",
                        "areas_allowed",
                        "areas_adjacency",
                        "allowed_ranges",
                    ]
        })
}

#[derive(Clone, Copy, Debug)]
struct FixedOutputExtras<'a> {
    model: &'a biogeo_core::ModelConfig,
    ancestral_probabilities: Option<&'a [biogeo_core::NodeStatePosterior]>,
    split_probabilities: Option<&'a [biogeo_core::SplitScenarioPosterior]>,
    history_skeletons: Option<&'a [biogeo_core::HistorySkeleton]>,
    stochastic_maps: Option<&'a [biogeo_core::BiogeographicStochasticMap]>,
    bsm_execution: Option<ResolvedBsmExecution>,
}

fn format_fixed_output(
    config: &FixedModelConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    result: &biogeo_core::PruningResult,
    extras: FixedOutputExtras<'_>,
    bsm_runtime: &BsmRuntimeConfig,
) -> Result<String, CliError> {
    let mut output = format!(
        "\
model\t{}
lnL\t{:.15}
states\t{}
areas\t{}
tips\t{}
max_range_size\t{}
include_null_range\t{}
root_prior\t{}
dispersal_multipliers\t{}
distance_matrix\t{}
distance_exponent\t{}
environment_distance_matrix\t{}
environment_distance_exponent\t{}
extirpation_multipliers\t{}
area_sizes\t{}
area_exponent\t{}
d\t{}
e\t{}
j\t{}
mx01y\t{}
mx01s\t{}
mx01v\t{}
mx01j\t{}
",
        config.preset.model_name(config.j),
        result.log_likelihood,
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        anagenetic_dispersal_label(
            config.dispersal_multipliers_path.as_ref(),
            config.dispersal_strata_path.as_ref(),
        ),
        optional_path_label(config.distance_matrix_path.as_ref(), "none"),
        optional_float_label(config.distance_exponent),
        optional_path_label(config.environment_distance_matrix_path.as_ref(), "none"),
        optional_float_label(config.environment_distance_exponent),
        optional_path_label(config.extirpation_multipliers_path.as_ref(), "uniform"),
        optional_path_label(config.area_sizes_path.as_ref(), "none"),
        optional_float_label(config.area_exponent),
        config.d,
        config.e,
        config.j,
        config.range_size.mx01y,
        config.range_size.mx01s,
        config.range_size.mx01v,
        config.range_size.mx01j,
    );
    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, parsed_ranges);
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    append_ancestral_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        extras.ancestral_probabilities,
    );
    append_split_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        extras.split_probabilities,
    );
    append_history_skeletons(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        extras.history_skeletons,
        config.seed,
    );
    append_stochastic_maps(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        RetainedBsmOutput {
            model: extras.model,
            stochastic_maps: extras.stochastic_maps,
            runtime: bsm_runtime,
            execution: extras.bsm_execution,
        },
    )?;
    Ok(output)
}

fn format_de_optimize_output(
    config: &DeOptimizeConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    result: &biogeo_core::DecOptimizationResult,
    ancestral_probabilities: Option<&[biogeo_core::NodeStatePosterior]>,
    split_probabilities: Option<&[biogeo_core::SplitScenarioPosterior]>,
) -> String {
    let mut output = format!(
        "\
model\t{}
mode\toptimize
lnL\t{:.15}
d\t{:.15}
e\t{:.15}
states\t{}
areas\t{}
tips\t{}
max_range_size\t{}
include_null_range\t{}
root_prior\t{}
dispersal_multipliers\t{}
distance_matrix\t{}
distance_exponent\t{}
environment_distance_matrix\t{}
environment_distance_exponent\t{}
extirpation_multipliers\t{}
area_sizes\t{}
area_exponent\t{}
mx01y\t{}
mx01s\t{}
mx01v\t{}
mx01j\t{}
init_d\t{}
init_e\t{}
min_rate\t{}
max_rate\t{}
initial_log_step\t{}
tolerance\t{}
max_iterations\t{}
multi_start_points_per_axis\t{}
iterations\t{}
evaluations\t{}
converged\t{}
starts\t{}
",
        config.preset.model_name(0.0),
        result.log_likelihood,
        result.d,
        result.e,
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        anagenetic_dispersal_label(
            config.dispersal_multipliers_path.as_ref(),
            config.dispersal_strata_path.as_ref(),
        ),
        optional_path_label(config.distance_matrix_path.as_ref(), "none"),
        optional_float_label(config.distance_exponent),
        optional_path_label(config.environment_distance_matrix_path.as_ref(), "none"),
        optional_float_label(config.environment_distance_exponent),
        optional_path_label(config.extirpation_multipliers_path.as_ref(), "uniform"),
        optional_path_label(config.area_sizes_path.as_ref(), "none"),
        optional_float_label(config.area_exponent),
        config.optimization.range_size.mx01y,
        config.optimization.range_size.mx01s,
        config.optimization.range_size.mx01v,
        config.optimization.range_size.mx01j,
        config.optimization.initial_d,
        config.optimization.initial_e,
        config.optimization.min_rate,
        config.optimization.max_rate,
        config.optimization.initial_log_step,
        config.optimization.tolerance,
        config.optimization.max_iterations,
        config.optimization.multi_start_points_per_axis,
        result.iterations,
        result.evaluations,
        result.converged,
        result.starts,
    );
    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, parsed_ranges);
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    append_ancestral_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        ancestral_probabilities,
    );
    append_split_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        split_probabilities,
    );
    output
}

fn format_exponent_optimize_output(
    config: &ExponentOptimizeConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    result: &biogeo_core::DecExponentOptimizationResult,
    ancestral_probabilities: Option<&[biogeo_core::NodeStatePosterior]>,
    split_probabilities: Option<&[biogeo_core::SplitScenarioPosterior]>,
) -> String {
    let distance_exponent = if config.kind == ExponentKind::GeographicX {
        Some(result.exponent)
    } else {
        config.distance_exponent
    };
    let environment_exponent = if config.kind == ExponentKind::EnvironmentN {
        Some(result.exponent)
    } else {
        config.environment_distance_exponent
    };
    let area_exponent = if config.kind == ExponentKind::AreaSizeU {
        Some(result.exponent)
    } else {
        config.area_exponent
    };
    let mut output = format!(
        "\
model\t{}
mode\toptimize
lnL\t{:.15}
d\t{:.15}
e\t{:.15}
exponent_parameter\t{}
exponent\t{:.15}
exponent_bound\t{}
states\t{}
areas\t{}
tips\t{}
max_range_size\t{}
include_null_range\t{}
root_prior\t{}
dispersal_multipliers\t{}
distance_matrix\t{}
distance_exponent\t{}
environment_distance_matrix\t{}
environment_distance_exponent\t{}
extirpation_multipliers\t{}
area_sizes\t{}
area_exponent\t{}
mx01y\t{}
mx01s\t{}
mx01v\t{}
mx01j\t{}
init_d\t{}
init_e\t{}
min_rate\t{}
max_rate\t{}
init_exponent\t{}
min_exponent\t{}
max_exponent\t{}
initial_log_step\t{}
initial_exponent_step\t{}
tolerance\t{}
max_iterations\t{}
multi_start_points_per_axis\t{}
iterations\t{}
evaluations\t{}
converged\t{}
converged_starts\t{}
starts\t{}
",
        config.kind.model_name(),
        result.log_likelihood,
        result.d,
        result.e,
        config.kind.parameter_name(),
        result.exponent,
        optimization_bound_label(result.exponent_bound),
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        anagenetic_dispersal_label(
            config.dispersal_multipliers_path.as_ref(),
            config.dispersal_strata_path.as_ref(),
        ),
        optional_path_label(config.distance_matrix_path.as_ref(), "none"),
        optional_float_label(distance_exponent),
        optional_path_label(config.environment_distance_matrix_path.as_ref(), "none"),
        optional_float_label(environment_exponent),
        optional_path_label(config.extirpation_multipliers_path.as_ref(), "uniform"),
        optional_path_label(config.area_sizes_path.as_ref(), "none"),
        optional_float_label(area_exponent),
        config.optimization.de.range_size.mx01y,
        config.optimization.de.range_size.mx01s,
        config.optimization.de.range_size.mx01v,
        config.optimization.de.range_size.mx01j,
        config.optimization.de.initial_d,
        config.optimization.de.initial_e,
        config.optimization.de.min_rate,
        config.optimization.de.max_rate,
        config.optimization.initial_exponent,
        config.optimization.min_exponent,
        config.optimization.max_exponent,
        config.optimization.de.initial_log_step,
        config.optimization.initial_exponent_step,
        config.optimization.de.tolerance,
        config.optimization.de.max_iterations,
        config.optimization.de.multi_start_points_per_axis,
        result.iterations,
        result.evaluations,
        result.converged,
        result.converged_starts,
        result.starts,
    );
    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, parsed_ranges);
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    append_ancestral_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        ancestral_probabilities,
    );
    append_split_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        split_probabilities,
    );
    output
}

fn format_xnu_optimize_output(
    config: &XnuOptimizeConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    result: &biogeo_core::DecXnuOptimizationResult,
    ancestral_probabilities: Option<&[biogeo_core::NodeStatePosterior]>,
    split_probabilities: Option<&[biogeo_core::SplitScenarioPosterior]>,
) -> String {
    let mut output = format!(
        "\
model\tDEC
mode\toptimize
optimization_parameters\td,e,x,n,u
lnL\t{:.15}
d\t{:.15}
e\t{:.15}
x\t{:.15}
n\t{:.15}
u\t{:.15}
x_bound\t{}
n_bound\t{}
u_bound\t{}
states\t{}
areas\t{}
tips\t{}
max_range_size\t{}
include_null_range\t{}
root_prior\t{}
dispersal_multipliers\t{}
distance_matrix\t{}
environment_distance_matrix\t{}
area_sizes\t{}
mx01y\t{}
mx01s\t{}
mx01v\t{}
mx01j\t{}
init_d\t{}
init_e\t{}
min_rate\t{}
max_rate\t{}
init_x\t{}
min_x\t{}
max_x\t{}
initial_x_step\t{}
init_n\t{}
min_n\t{}
max_n\t{}
initial_n_step\t{}
init_u\t{}
min_u\t{}
max_u\t{}
initial_u_step\t{}
initial_log_step\t{}
tolerance\t{}
max_iterations\t{}
multi_start_points_per_axis\t{}
iterations\t{}
evaluations\t{}
converged\t{}
converged_starts\t{}
starts\t{}
",
        result.log_likelihood,
        result.d,
        result.e,
        result.x,
        result.n,
        result.u,
        optimization_bound_label(result.x_bound),
        optimization_bound_label(result.n_bound),
        optimization_bound_label(result.u_bound),
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        anagenetic_dispersal_label(
            config.dispersal_multipliers_path.as_ref(),
            config.dispersal_strata_path.as_ref(),
        ),
        optional_path_label(config.distance_matrix_path.as_ref(), "stratified"),
        optional_path_label(
            config.environment_distance_matrix_path.as_ref(),
            "stratified",
        ),
        optional_path_label(config.area_sizes_path.as_ref(), "stratified"),
        config.optimization.de.range_size.mx01y,
        config.optimization.de.range_size.mx01s,
        config.optimization.de.range_size.mx01v,
        config.optimization.de.range_size.mx01j,
        config.optimization.de.initial_d,
        config.optimization.de.initial_e,
        config.optimization.de.min_rate,
        config.optimization.de.max_rate,
        config.optimization.initial_x,
        config.optimization.min_x,
        config.optimization.max_x,
        config.optimization.initial_x_step,
        config.optimization.initial_n,
        config.optimization.min_n,
        config.optimization.max_n,
        config.optimization.initial_n_step,
        config.optimization.initial_u,
        config.optimization.min_u,
        config.optimization.max_u,
        config.optimization.initial_u_step,
        config.optimization.de.initial_log_step,
        config.optimization.de.tolerance,
        config.optimization.de.max_iterations,
        config.optimization.de.multi_start_points_per_axis,
        result.iterations,
        result.evaluations,
        result.converged,
        result.converged_starts,
        result.starts,
    );
    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, parsed_ranges);
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    append_ancestral_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        ancestral_probabilities,
    );
    append_split_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        split_probabilities,
    );
    output
}

fn format_pair_profile_output(
    config: &PairProfileConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    result: &biogeo_core::DecPairProfileResult,
) -> String {
    let best = result.best_point();
    let (best_x, best_n, best_u) =
        config
            .pair
            .exponents(best.first, best.second, config.fixed_exponent);
    let support_basis = if (result.support_delta - biogeo_core::PROFILE_95_SUPPORT_DELTA).abs()
        <= f64::EPSILON * 8.0
    {
        "approximate_95pct_two_parameter_likelihood_ratio"
    } else {
        "user_defined_delta_lnL"
    };
    let mut output = format!(
        "\
model\tDEC
mode\tpair-profile
lnL\t{:.15}
best_d\t{:.15}
best_e\t{:.15}
best_x\t{:.15}
best_n\t{:.15}
best_u\t{:.15}
first_parameter\t{}
second_parameter\t{}
fixed_parameter\t{}
fixed_exponent\t{:.15}
best_first_grid_bound\t{}
best_second_grid_bound\t{}
support_delta\t{:.15}
support_basis\t{}
support_points\t{}
total_points\t{}
finite_points\t{}
failed_points\t{}
converged_points\t{}
likelihood_weighted_correlation\t{}
total_iterations\t{}
total_evaluations\t{}
states\t{}
areas\t{}
tips\t{}
max_range_size\t{}
include_null_range\t{}
root_prior\t{}
dispersal_multipliers\t{}
distance_matrix\t{}
environment_distance_matrix\t{}
area_sizes\t{}
mx01y\t{}
mx01s\t{}
mx01v\t{}
mx01j\t{}
init_d\t{}
init_e\t{}
min_rate\t{}
max_rate\t{}
initial_log_step\t{}
tolerance\t{}
max_iterations\t{}
multi_start_points_per_axis\t{}
",
        best.log_likelihood,
        best.d,
        best.e,
        best_x,
        best_n,
        best_u,
        result.first_parameter,
        result.second_parameter,
        config.pair.fixed().parameter_name(),
        config.fixed_exponent,
        optimization_bound_label(result.best_first_grid_bound),
        optimization_bound_label(result.best_second_grid_bound),
        result.support_delta,
        support_basis,
        result.support_points,
        result.points.len(),
        result.finite_points,
        result.failed_points,
        result.converged_points,
        optional_float_label(result.likelihood_weighted_correlation),
        result.total_iterations,
        result.total_evaluations,
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        anagenetic_dispersal_label(
            config.dispersal_multipliers_path.as_ref(),
            config.dispersal_strata_path.as_ref(),
        ),
        optional_path_label(config.distance_matrix_path.as_ref(), "stratified"),
        optional_path_label(
            config.environment_distance_matrix_path.as_ref(),
            "stratified",
        ),
        optional_path_label(config.area_sizes_path.as_ref(), "stratified"),
        config.profile.de.range_size.mx01y,
        config.profile.de.range_size.mx01s,
        config.profile.de.range_size.mx01v,
        config.profile.de.range_size.mx01j,
        config.profile.de.initial_d,
        config.profile.de.initial_e,
        config.profile.de.min_rate,
        config.profile.de.max_rate,
        config.profile.de.initial_log_step,
        config.profile.de.tolerance,
        config.profile.de.max_iterations,
        config.profile.de.multi_start_points_per_axis,
    );
    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, parsed_ranges);
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    output.push_str(&format!(
        "{}_grid_min\t{:.15}\n{}_grid_max\t{:.15}\n{}_grid_points\t{}\n",
        result.first_parameter,
        config.profile.first.values[0],
        result.first_parameter,
        config.profile.first.values[config.profile.first.values.len() - 1],
        result.first_parameter,
        config.profile.first.values.len(),
    ));
    output.push_str(&format!(
        "{}_grid_min\t{:.15}\n{}_grid_max\t{:.15}\n{}_grid_points\t{}\n",
        result.second_parameter,
        config.profile.second.values[0],
        result.second_parameter,
        config.profile.second.values[config.profile.second.values.len() - 1],
        result.second_parameter,
        config.profile.second.values.len(),
    ));
    output.push_str(&format!(
        "{}_support_min\t{:.15}\n{}_support_max\t{:.15}\n{}_support_grid_values\t{}\n",
        result.first_parameter,
        result.first_support.min,
        result.first_parameter,
        result.first_support.max,
        result.first_parameter,
        result.first_support.grid_values,
    ));
    output.push_str(&format!(
        "{}_support_min\t{:.15}\n{}_support_max\t{:.15}\n{}_support_grid_values\t{}\n",
        result.second_parameter,
        result.second_support.min,
        result.second_parameter,
        result.second_support.max,
        result.second_parameter,
        result.second_support.grid_values,
    ));
    output.push_str("profile_points\n");
    output.push_str(
        "x\tn\tu\td\te\tlnL\tdelta_lnL\tfinite\twithin_support\tconverged\titerations\tevaluations\tstarts\n",
    );
    for point in &result.points {
        let (x, n, u) = config
            .pair
            .exponents(point.first, point.second, config.fixed_exponent);
        output.push_str(&format!(
            "{x:.15}\t{n:.15}\t{u:.15}\t{:.15}\t{:.15}\t{:.15}\t{:.15}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            point.d,
            point.e,
            point.log_likelihood,
            point.delta_log_likelihood,
            point.finite,
            point.within_support,
            point.converged,
            point.iterations,
            point.evaluations,
            point.starts,
        ));
    }

    output
}

fn optimization_bound_label(bound: Option<biogeo_core::OptimizationBound>) -> &'static str {
    match bound {
        Some(biogeo_core::OptimizationBound::Lower) => "lower",
        Some(biogeo_core::OptimizationBound::Upper) => "upper",
        None => "interior",
    }
}

fn anagenetic_dispersal_label(
    matrix_path: Option<&PathBuf>,
    strata_path: Option<&PathBuf>,
) -> String {
    if let Some(path) = matrix_path {
        return path.display().to_string();
    }
    if let Some(path) = strata_path {
        return format!("stratified:{}", path.display());
    }
    "uniform".to_string()
}

fn optional_path_label(path: Option<&PathBuf>, default: &str) -> String {
    path.map_or_else(|| default.to_string(), |path| path.display().to_string())
}

fn optional_float_label(value: Option<f64>) -> String {
    value.map_or_else(|| "none".to_string(), |value| value.to_string())
}

fn format_decj_optimize_output(
    config: &DecJOptimizeConfig,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    result: &biogeo_core::DecJOptimizationResult,
    ancestral_probabilities: Option<&[biogeo_core::NodeStatePosterior]>,
    split_probabilities: Option<&[biogeo_core::SplitScenarioPosterior]>,
) -> String {
    let mut output = format!(
        "\
model\t{}
mode\toptimize
lnL\t{:.15}
d\t{:.15}
e\t{:.15}
j\t{:.15}
states\t{}
areas\t{}
tips\t{}
max_range_size\t{}
include_null_range\t{}
root_prior\t{}
dispersal_multipliers\t{}
distance_matrix\t{}
distance_exponent\t{}
environment_distance_matrix\t{}
environment_distance_exponent\t{}
extirpation_multipliers\t{}
area_sizes\t{}
area_exponent\t{}
mx01y\t{}
mx01s\t{}
mx01v\t{}
mx01j\t{}
init_d\t{}
init_e\t{}
init_j\t{}
min_rate\t{}
max_rate\t{}
min_j\t{}
max_j\t{}
initial_log_step\t{}
tolerance\t{}
max_iterations\t{}
multi_start_points_per_axis\t{}
iterations\t{}
evaluations\t{}
converged\t{}
starts\t{}
",
        config.preset.model_name(result.j),
        result.log_likelihood,
        result.d,
        result.e,
        result.j,
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        config.root_prior.as_str(),
        anagenetic_dispersal_label(
            config.dispersal_multipliers_path.as_ref(),
            config.dispersal_strata_path.as_ref(),
        ),
        optional_path_label(config.distance_matrix_path.as_ref(), "none"),
        optional_float_label(config.distance_exponent),
        optional_path_label(config.environment_distance_matrix_path.as_ref(), "none"),
        optional_float_label(config.environment_distance_exponent),
        optional_path_label(config.extirpation_multipliers_path.as_ref(), "uniform"),
        optional_path_label(config.area_sizes_path.as_ref(), "none"),
        optional_float_label(config.area_exponent),
        config.optimization.range_size.mx01y,
        config.optimization.range_size.mx01s,
        config.optimization.range_size.mx01v,
        config.optimization.range_size.mx01j,
        config.optimization.initial_d,
        config.optimization.initial_e,
        config.optimization.initial_j,
        config.optimization.min_rate,
        config.optimization.max_rate,
        config.optimization.min_j,
        config.optimization.max_j,
        config.optimization.initial_log_step,
        config.optimization.tolerance,
        config.optimization.max_iterations,
        config.optimization.multi_start_points_per_axis,
        result.iterations,
        result.evaluations,
        result.converged,
        result.starts,
    );
    append_selected_tree_name(&mut output, config.tree_name.as_deref());
    append_ambiguous_range_summary(&mut output, config.use_ambiguities, parsed_ranges);
    append_direct_ancestor_hooks(&mut output, parsed_tree);
    append_ancestral_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        ancestral_probabilities,
    );
    append_split_probabilities(
        &mut output,
        parsed_tree,
        parsed_ranges,
        states,
        split_probabilities,
    );
    output
}

fn append_ambiguous_range_summary(
    output: &mut String,
    enabled: bool,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
) {
    if !enabled {
        return;
    }

    output.push_str("tip_observation_model\tambiguous_ranges\n");
    append_ambiguous_range_counts(output, parsed_ranges);
}

fn append_ambiguous_range_counts(
    output: &mut String,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
) {
    writeln!(
        output,
        "ambiguous_tips\t{}",
        parsed_ranges.ambiguous_tip_count()
    )
    .unwrap();
    writeln!(
        output,
        "unknown_range_cells\t{}",
        parsed_ranges.unknown_cell_count()
    )
    .unwrap();
    writeln!(
        output,
        "all_unknown_tips\t{}",
        parsed_ranges.all_unknown_tip_count()
    )
    .unwrap();
    writeln!(
        output,
        "maximum_possible_range_size\t{}",
        parsed_ranges.maximum_possible_range_size()
    )
    .unwrap();
}

fn append_direct_ancestor_hooks(output: &mut String, parsed_tree: &biogeo_core::ParsedNewickTree) {
    let tree = &parsed_tree.tree;
    let direct_ancestor_nodes = tree
        .postorder_internal_nodes()
        .iter()
        .filter(|node| tree.is_direct_ancestor_node(**node))
        .count();
    writeln!(
        output,
        "min_branch_length\t{:.17}",
        tree.direct_ancestor_threshold()
    )
    .unwrap();
    writeln!(output, "direct_ancestor_nodes\t{direct_ancestor_nodes}").unwrap();
    writeln!(
        output,
        "direct_ancestor_hook_edges\t{}",
        tree.direct_ancestor_hook_edges().len()
    )
    .unwrap();
    if tree.direct_ancestor_hook_edges().is_empty() {
        return;
    }

    output.push_str("direct_ancestor_hooks\n");
    output.push_str(
        "node\tnode_label\tnode_clade\tedge\tchild\tchild_label\tchild_clade\tbranch_length\tthreshold\n",
    );
    for edge_index in tree.direct_ancestor_hook_edges() {
        let edge = tree.edges()[*edge_index];
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.17}\t{:.17}",
            edge.parent,
            node_label(parsed_tree, edge.parent),
            clade_label(parsed_tree, edge.parent),
            edge_index,
            edge.child,
            node_label(parsed_tree, edge.child),
            clade_label(parsed_tree, edge.child),
            edge.length,
            tree.direct_ancestor_threshold(),
        )
        .unwrap();
    }
}

fn append_selected_tree_name(output: &mut String, tree_name: Option<&str>) {
    if let Some(tree_name) = tree_name {
        writeln!(output, "tree_name\t{tree_name}").unwrap();
    }
}

fn format_missing_branch_length_fill(value: Option<f64>) -> String {
    value.map_or_else(|| "reject".to_string(), |value| format!("{value:.17}"))
}

fn append_ancestral_probabilities(
    output: &mut String,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    ancestral_probabilities: Option<&[biogeo_core::NodeStatePosterior]>,
) {
    let Some(ancestral_probabilities) = ancestral_probabilities else {
        return;
    };

    output.push_str("ancestral_state_probabilities\n");
    output.push_str("node\tlabel\tkind\tclade\tstate_index\trange_bits\trange\tprobability\n");

    for posterior in ancestral_probabilities {
        if parsed_tree.tree.is_tip(posterior.node) {
            continue;
        }

        let kind = if posterior.node == parsed_tree.tree.root() {
            "root"
        } else {
            "internal"
        };
        let label = node_label(parsed_tree, posterior.node);
        let clade = clade_label(parsed_tree, posterior.node);
        for (state_index, probability) in posterior.probabilities.iter().enumerate() {
            let state = states
                .get(state_index)
                .expect("posterior state index should exist in state space");
            output.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\n",
                posterior.node,
                label,
                kind,
                clade,
                state_index,
                state.bits(),
                range_label(state, &parsed_ranges.area_names),
                probability,
            ));
        }
    }
}

fn append_split_probabilities(
    output: &mut String,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    split_probabilities: Option<&[biogeo_core::SplitScenarioPosterior]>,
) {
    let Some(split_probabilities) = split_probabilities else {
        return;
    };

    output.push_str("split_scenario_probabilities\n");
    output.push_str(
        "node\tlabel\tkind\tclade\tleft_clade\tright_clade\tancestor_state_index\tancestor_range_bits\tancestor_range\tleft_state_index\tleft_range_bits\tleft_range\tright_state_index\tright_range_bits\tright_range\tscenario_weight\tprobability\n",
    );

    for posterior in split_probabilities {
        let kind = if posterior.node == parsed_tree.tree.root() {
            "root"
        } else {
            "internal"
        };
        let label = node_label(parsed_tree, posterior.node);
        let clade = clade_label(parsed_tree, posterior.node);
        let children = parsed_tree
            .tree
            .children(posterior.node)
            .expect("split posterior node should exist in tree");
        let left_clade = clade_label(parsed_tree, children[0].node);
        let right_clade = clade_label(parsed_tree, children[1].node);
        let ancestor = states
            .get(posterior.ancestor)
            .expect("posterior ancestor state index should exist in state space");
        let left = states
            .get(posterior.left)
            .expect("posterior left state index should exist in state space");
        let right = states
            .get(posterior.right)
            .expect("posterior right state index should exist in state space");

        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\t{:.15}\n",
            posterior.node,
            label,
            kind,
            clade,
            left_clade,
            right_clade,
            posterior.ancestor,
            ancestor.bits(),
            range_label(ancestor, &parsed_ranges.area_names),
            posterior.left,
            left.bits(),
            range_label(left, &parsed_ranges.area_names),
            posterior.right,
            right.bits(),
            range_label(right, &parsed_ranges.area_names),
            posterior.weight,
            posterior.probability,
        ));
    }
}

fn append_history_skeletons(
    output: &mut String,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    history_skeletons: Option<&[biogeo_core::HistorySkeleton]>,
    seed: u64,
) {
    let Some(history_skeletons) = history_skeletons else {
        return;
    };

    output.push_str("conditional_history_skeletons\n");
    output.push_str(&format!("traceback_seed\t{seed}\n"));
    output.push_str(&format!("traceback_samples\t{}\n", history_skeletons.len()));
    output.push_str("traceback_node_states\n");
    output.push_str("sample\tnode\tlabel\tkind\tclade\tstate_index\trange_bits\trange\n");
    for (sample_index, history) in history_skeletons.iter().enumerate() {
        for (node, state_index) in history.node_states.iter().copied().enumerate() {
            let kind = if node == parsed_tree.tree.root() {
                "root"
            } else if parsed_tree.tree.is_tip(node) {
                "tip"
            } else {
                "internal"
            };
            let state = states
                .get(state_index)
                .expect("sampled node state index should exist in state space");
            output.push_str(&format!(
                "{sample_index}\t{node}\t{}\t{kind}\t{}\t{state_index}\t{}\t{}\n",
                node_label(parsed_tree, node),
                clade_label(parsed_tree, node),
                state.bits(),
                range_label(state, &parsed_ranges.area_names),
            ));
        }
    }

    output.push_str("traceback_splits\n");
    output.push_str(
        "sample\tnode\tlabel\tkind\tclade\tleft_clade\tright_clade\tancestor_state_index\tancestor_range_bits\tancestor_range\tleft_state_index\tleft_range_bits\tleft_range\tright_state_index\tright_range_bits\tright_range\tscenario_weight\n",
    );
    for (sample_index, history) in history_skeletons.iter().enumerate() {
        for split in &history.splits {
            let kind = if split.node == parsed_tree.tree.root() {
                "root"
            } else {
                "internal"
            };
            let children = parsed_tree
                .tree
                .children(split.node)
                .expect("sampled split node should exist in tree");
            let ancestor = states
                .get(split.ancestor)
                .expect("sampled ancestor state index should exist in state space");
            let left = states
                .get(split.left)
                .expect("sampled left state index should exist in state space");
            let right = states
                .get(split.right)
                .expect("sampled right state index should exist in state space");
            output.push_str(&format!(
                "{sample_index}\t{}\t{}\t{kind}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\n",
                split.node,
                node_label(parsed_tree, split.node),
                clade_label(parsed_tree, split.node),
                clade_label(parsed_tree, children[0].node),
                clade_label(parsed_tree, children[1].node),
                split.ancestor,
                ancestor.bits(),
                range_label(ancestor, &parsed_ranges.area_names),
                split.left,
                left.bits(),
                range_label(left, &parsed_ranges.area_names),
                split.right,
                right.bits(),
                range_label(right, &parsed_ranges.area_names),
                split.weight,
            ));
        }
    }

    output.push_str("traceback_branch_endpoints\n");
    output.push_str(
        "sample\tedge\tparent\tparent_clade\tchild\tchild_clade\tlength\tstart_state_index\tstart_range_bits\tstart_range\tend_state_index\tend_range_bits\tend_range\n",
    );
    for (sample_index, history) in history_skeletons.iter().enumerate() {
        for endpoint in &history.branch_endpoints {
            let edge = parsed_tree
                .tree
                .edges()
                .get(endpoint.edge_index)
                .expect("sampled edge index should exist in tree");
            let start = states
                .get(endpoint.start_state)
                .expect("sampled branch start state should exist in state space");
            let end = states
                .get(endpoint.end_state)
                .expect("sampled branch end state should exist in state space");
            output.push_str(&format!(
                "{sample_index}\t{}\t{}\t{}\t{}\t{}\t{:.15}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                endpoint.edge_index,
                endpoint.parent,
                clade_label(parsed_tree, endpoint.parent),
                endpoint.child,
                clade_label(parsed_tree, endpoint.child),
                edge.length,
                endpoint.start_state,
                start.bits(),
                range_label(start, &parsed_ranges.area_names),
                endpoint.end_state,
                end.bits(),
                range_label(end, &parsed_ranges.area_names),
            ));
        }
    }
}

struct RetainedBsmOutput<'a> {
    model: &'a biogeo_core::ModelConfig,
    stochastic_maps: Option<&'a [biogeo_core::BiogeographicStochasticMap]>,
    runtime: &'a BsmRuntimeConfig,
    execution: Option<ResolvedBsmExecution>,
}

fn append_stochastic_maps(
    output: &mut String,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    parsed_ranges: &biogeo_core::ParsedTipRanges,
    states: &biogeo_core::StateSpace,
    retained: RetainedBsmOutput<'_>,
) -> Result<(), CliError> {
    let Some(stochastic_maps) = retained.stochastic_maps else {
        return Ok(());
    };
    let runtime = retained.runtime;

    output.push_str("biogeographic_stochastic_maps\n");
    output.push_str(&format!("bsm_seed\t{}\n", runtime.seed));
    output.push_str(&format!("bsm_samples\t{}\n", stochastic_maps.len()));
    output.push_str(&format!(
        "bsm_rng_protocol\t{}\n",
        biogeo_core::INDEXED_BSM_RNG_PROTOCOL
    ));
    let execution = retained
        .execution
        .expect("retained BSM samples require an execution plan");
    output.push_str(&format!("bsm_threads\t{}\n", execution.threads));
    output.push_str(&format!("bsm_max_in_flight\t{}\n", execution.max_in_flight));
    output.push_str(&format!("bsm_interactive\t{}\n", runtime.interactive));
    output.push_str(&format!(
        "bsm_time_limit_seconds\t{}\n",
        format_optional_duration(execution.time_limit)
    ));
    output.push_str(&format!(
        "bsm_max_events_per_sample\t{}\n",
        format_optional_limit(execution.max_events_per_sample)
    ));
    output.push_str(&format!(
        "bsm_max_events_total\t{}\n",
        format_optional_limit(execution.max_events_total)
    ));
    output.push_str(&format!(
        "bsm_memory_budget_mb\t{}\n",
        format_optional_limit(execution.memory_budget_mb)
    ));
    output.push_str(&format!(
        "bsm_retained_bytes_per_sample_upper_bound\t{}\n",
        format_optional_estimate(execution.retained_bytes_per_sample_upper_bound)
    ));
    output.push_str(&format!(
        "bsm_buffered_history_bytes_upper_bound\t{}\n",
        format_optional_estimate(execution.buffered_history_bytes_upper_bound)
    ));

    let formatting = BsmFormattingContext::new(
        parsed_tree,
        parsed_ranges,
        states,
        retained.model,
        runtime.output_level,
    )?;
    let mut combined_rows: [String; BSM_TABLE_SPECS.len()] = std::array::from_fn(|_| String::new());
    for (sample_index, stochastic_map) in stochastic_maps.iter().enumerate() {
        let rows = format_stochastic_history_rows(sample_index, stochastic_map, &formatting)?;
        for (combined, sample_rows) in combined_rows.iter_mut().zip(rows.tables) {
            combined.push_str(&sample_rows);
        }
    }

    for (spec, rows) in BSM_TABLE_SPECS.iter().zip(combined_rows) {
        output.push_str(spec.section);
        output.push('\n');
        output.push_str(spec.header);
        output.push('\n');
        output.push_str(&rows);
    }
    Ok(())
}

const BSM_NODE_STATES: usize = 0;
const BSM_CLADOGENETIC_SPLITS: usize = 1;
const BSM_BRANCH_SEGMENTS: usize = 2;
const BSM_SAMPLE_EVENT_COUNTS: usize = 3;
const BSM_SAMPLE_PERIOD_EVENT_COUNTS: usize = 4;
const BSM_SAMPLE_STATE_OCCUPANCY: usize = 5;
const BSM_SAMPLE_PERIOD_STATE_OCCUPANCY: usize = 6;
const BSM_ANAGENETIC_EVENTS: usize = 7;

#[derive(Clone, Copy)]
struct BsmTableSpec {
    section: &'static str,
    file_name: &'static str,
    header: &'static str,
}

const BSM_TABLE_SPECS: [BsmTableSpec; 8] = [
    BsmTableSpec {
        section: "bsm_node_states",
        file_name: "node_states.tsv",
        header: "sample\tnode\tlabel\tkind\tclade\tstate_index\trange_bits\trange",
    },
    BsmTableSpec {
        section: "bsm_cladogenetic_splits",
        file_name: "cladogenetic_splits.tsv",
        header: "sample\tnode\tlabel\tkind\tclade\tleft_clade\tright_clade\tancestor_state_index\tancestor_range_bits\tancestor_range\tleft_state_index\tleft_range_bits\tleft_range\tright_state_index\tright_range_bits\tright_range\tscenario_weight",
    },
    BsmTableSpec {
        section: "bsm_branch_segments",
        file_name: "branch_segments.tsv",
        header: "sample\tedge\tparent\tparent_clade\tchild\tchild_clade\tsegment\tq_index\tstart_time_from_parent\tend_time_from_parent\tstart_state_index\tstart_range_bits\tstart_range\tend_state_index\tend_range_bits\tend_range\tendpoint_probability\tvirtual_jump_count\tevent_count",
    },
    BsmTableSpec {
        section: "bsm_sample_event_counts",
        file_name: "sample_event_counts.tsv",
        header: "sample\tanagenetic_total\trange_expansion\tlocal_extirpation\tcladogenetic_total\trange_copying\tsubset_sympatry\tvicariance\tfounder_event\ttotal_branch_time",
    },
    BsmTableSpec {
        section: "bsm_sample_period_event_counts",
        file_name: "sample_period_event_counts.tsv",
        header: "sample\tq_index\tanagenetic_event_count\tevent_fraction",
    },
    BsmTableSpec {
        section: "bsm_sample_state_occupancy",
        file_name: "sample_state_occupancy.tsv",
        header: "sample\tstate_index\trange_bits\trange\toccupancy_time\toccupancy_fraction",
    },
    BsmTableSpec {
        section: "bsm_sample_period_state_occupancy",
        file_name: "sample_period_state_occupancy.tsv",
        header: "sample\tq_index\tstate_index\trange_bits\trange\toccupancy_time",
    },
    BsmTableSpec {
        section: "bsm_anagenetic_events",
        file_name: "anagenetic_events.tsv",
        header: "sample\tedge\tparent\tparent_clade\tchild\tchild_clade\tsegment\tq_index\ttime_from_parent\tevent_kind\tparameter\tarea_index\tarea\tfrom_state_index\tfrom_range_bits\tfrom_range\tto_state_index\tto_range_bits\tto_range",
    },
];

const BSM_V2_SAMPLE_EVENT_COUNTS_HEADER: &str = "sample\tanagenetic_total\trange_expansion\tlocal_extirpation\trange_switching\tcladogenetic_total\trange_copying\tsubset_sympatry\tvicariance\tfounder_event\ttotal_branch_time\tsegment_count\tconstrained_segment_count\tminimum_endpoint_probability\tmaximum_virtual_jump_count\tmaximum_anagenetic_events_per_segment\tforbidden_state_transitions\tforbidden_state_endpoints\tforbidden_state_time";

const BSM_V2_COMPACT_HEADERS: [&str; BSM_TABLE_SPECS.len()] = [
    "sample\tnode\tstate_index",
    "sample\tnode\tancestor_state_index\tleft_state_index\tright_state_index\tscenario_weight",
    "sample\tedge\tsegment\tq_index\tstart_time_from_parent\tend_time_from_parent\tstart_state_index\tend_state_index\tendpoint_probability\tvirtual_jump_count\tevent_count",
    BSM_V2_SAMPLE_EVENT_COUNTS_HEADER,
    "sample\tq_index\tanagenetic_event_count\tevent_fraction",
    "sample\tstate_index\toccupancy_time\toccupancy_fraction",
    "sample\tq_index\tstate_index\toccupancy_time",
    "sample\tedge\tsegment\tq_index\ttime_from_parent\tevent_kind\tparameter\tarea_index\tfrom_state_index\tto_state_index",
];

fn bsm_table_header(output_level: BsmOutputLevel, table_index: usize) -> &'static str {
    match output_level {
        BsmOutputLevel::Legacy => BSM_TABLE_SPECS[table_index].header,
        BsmOutputLevel::Full if table_index == BSM_SAMPLE_EVENT_COUNTS => {
            BSM_V2_SAMPLE_EVENT_COUNTS_HEADER
        }
        BsmOutputLevel::Full => BSM_TABLE_SPECS[table_index].header,
        BsmOutputLevel::Compact | BsmOutputLevel::Summary => BSM_V2_COMPACT_HEADERS[table_index],
    }
}

struct BsmSampleTableRows {
    tables: [String; BSM_TABLE_SPECS.len()],
}

struct BsmFormattingContext<'a> {
    parsed_tree: &'a biogeo_core::ParsedNewickTree,
    parsed_ranges: &'a biogeo_core::ParsedTipRanges,
    states: &'a biogeo_core::StateSpace,
    node_labels: Vec<String>,
    clade_labels: Option<Vec<String>>,
    range_labels: Vec<String>,
    state_masks: Option<Vec<biogeo_core::StateMask>>,
    output_level: BsmOutputLevel,
}

impl<'a> BsmFormattingContext<'a> {
    fn new(
        parsed_tree: &'a biogeo_core::ParsedNewickTree,
        parsed_ranges: &'a biogeo_core::ParsedTipRanges,
        states: &'a biogeo_core::StateSpace,
        model: &biogeo_core::ModelConfig,
        output_level: BsmOutputLevel,
    ) -> Result<Self, CliError> {
        let node_labels = (0..parsed_tree.tree.node_count())
            .map(|node| node_label(parsed_tree, node))
            .collect();
        let clade_labels = (!output_level.is_compact()).then(|| {
            (0..parsed_tree.tree.node_count())
                .map(|node| clade_label(parsed_tree, node))
                .collect()
        });
        let range_labels = (0..states.len())
            .map(|state_index| {
                let state = states
                    .get(state_index)
                    .expect("state index should exist while preparing BSM labels");
                range_label(state, &parsed_ranges.area_names)
            })
            .collect();
        let state_masks = model
            .anagenesis
            .stratified_state_masks(states)
            .map_err(|error| CliError::Dec(biogeo_core::DecAnalysisError::Anagenesis(error)))?;
        Ok(Self {
            parsed_tree,
            parsed_ranges,
            states,
            node_labels,
            clade_labels,
            range_labels,
            state_masks,
            output_level,
        })
    }

    fn node_label(&self, node: usize) -> &str {
        &self.node_labels[node]
    }

    fn clade_label(&self, node: usize) -> &str {
        &self
            .clade_labels
            .as_ref()
            .expect("verbose BSM output requires cached clade labels")[node]
    }

    fn range_label(&self, state_index: usize) -> &str {
        &self.range_labels[state_index]
    }
}

fn format_bsm_reference_tables(
    context: &BsmFormattingContext<'_>,
    model: &biogeo_core::ModelConfig,
) -> Vec<(&'static str, String)> {
    let mut areas = String::from("area_index\tarea\n");
    for (area_index, area) in context.parsed_ranges.area_names.iter().enumerate() {
        writeln!(
            areas,
            "{area_index}\t{}",
            analysis_result::encode_field(area)
        )
        .expect("writing BSM area references to a String cannot fail");
    }

    let mut states = String::from("state_index\trange_bits\trange\n");
    for state_index in 0..context.states.len() {
        let state = context
            .states
            .get(state_index)
            .expect("state reference index should exist");
        writeln!(
            states,
            "{state_index}\t{}\t{}",
            state.bits(),
            analysis_result::encode_field(context.range_label(state_index)),
        )
        .expect("writing BSM state references to a String cannot fail");
    }

    let mut nodes = String::from("node\tlabel\tkind\n");
    for node in 0..context.parsed_tree.tree.node_count() {
        let kind = if node == context.parsed_tree.tree.root() {
            "root"
        } else if context.parsed_tree.tree.is_tip(node) {
            "tip"
        } else {
            "internal"
        };
        writeln!(
            nodes,
            "{node}\t{}\t{kind}",
            analysis_result::encode_field(context.node_label(node)),
        )
        .expect("writing BSM node references to a String cannot fail");
    }

    let mut edges = String::from("edge\tparent\tchild\tlength\n");
    for (edge_index, edge) in context.parsed_tree.tree.edges().iter().enumerate() {
        writeln!(
            edges,
            "{edge_index}\t{}\t{}\t{:.17}",
            edge.parent, edge.child, edge.length,
        )
        .expect("writing BSM edge references to a String cannot fail");
    }

    let mut periods =
        String::from("q_index\toldest_age\thas_state_constraint\tallowed_state_count\n");
    if let Some(schedule) = model.anagenesis.time_stratified_anagenesis() {
        for (q_index, stratum) in schedule.strata().iter().enumerate() {
            let allowed_state_count = context
                .state_masks
                .as_ref()
                .map_or(context.states.len(), |masks| masks[q_index].allowed_count());
            writeln!(
                periods,
                "{q_index}\t{:.17}\t{}\t{allowed_state_count}",
                stratum.oldest_age,
                stratum.state_constraint.is_some(),
            )
            .expect("writing BSM period references to a String cannot fail");
        }
    } else {
        writeln!(periods, "0\tunbounded\tfalse\t{}", context.states.len())
            .expect("writing BSM period references to a String cannot fail");
    }

    vec![
        ("areas.tsv", areas),
        ("states.tsv", states),
        ("nodes.tsv", nodes),
        ("edges.tsv", edges),
        ("periods.tsv", periods),
    ]
}

fn ensure_bsm_reference_tables(
    output_dir: &Path,
    context: &BsmFormattingContext<'_>,
    model: &biogeo_core::ModelConfig,
    resume: bool,
) -> Result<(), CliError> {
    if !context.output_level.is_v2() {
        return Ok(());
    }
    for (file_name, expected) in format_bsm_reference_tables(context, model) {
        let path = output_dir.join(file_name);
        if resume {
            let actual = fs::read_to_string(&path).map_err(|source| CliError::OutputIo {
                path: path.clone(),
                source,
            })?;
            if actual != expected {
                return Err(CliError::InvalidBsmReference {
                    path,
                    message: "contents do not match the resumed tree, areas, states, or periods"
                        .to_string(),
                });
            }
            continue;
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|source| CliError::OutputIo {
                path: path.clone(),
                source,
            })?;
        file.write_all(expected.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| CliError::OutputIo { path, source })?;
    }
    Ok(())
}

fn validate_bsm_tree_references(
    sample_index: usize,
    map: &biogeo_core::BiogeographicStochasticMap,
    context: &BsmFormattingContext<'_>,
) -> Result<(), CliError> {
    let tree = &context.parsed_tree.tree;
    if map.skeleton.node_states.len() != tree.node_count() {
        return Err(CliError::BsmTreeReference {
            sample_index,
            message: format!(
                "skeleton has {} node states for a {}-node tree",
                map.skeleton.node_states.len(),
                tree.node_count()
            ),
        });
    }
    for split in &map.skeleton.splits {
        if split.node >= tree.node_count() || tree.children(split.node).is_none() {
            return Err(CliError::BsmTreeReference {
                sample_index,
                message: format!("split references non-internal node {}", split.node),
            });
        }
    }
    for (context_name, edge_index, parent, child) in map
        .skeleton
        .branch_endpoints
        .iter()
        .map(|endpoint| {
            (
                "skeleton endpoint",
                endpoint.edge_index,
                endpoint.parent,
                endpoint.child,
            )
        })
        .chain(map.branches.iter().map(|branch| {
            (
                "branch history",
                branch.edge_index,
                branch.parent,
                branch.child,
            )
        }))
    {
        let Some(edge) = tree.edges().get(edge_index) else {
            return Err(CliError::BsmTreeReference {
                sample_index,
                message: format!("{context_name} references missing edge {edge_index}"),
            });
        };
        if edge.parent != parent || edge.child != child {
            return Err(CliError::BsmTreeReference {
                sample_index,
                message: format!(
                    "{context_name} edge {edge_index} has endpoints {parent}->{child}, expected {}->{}",
                    edge.parent, edge.child
                ),
            });
        }
    }
    for event in map
        .branches
        .iter()
        .flat_map(|branch| &branch.segments)
        .flat_map(|segment| &segment.events)
    {
        let area = match event.kind {
            biogeo_core::AnageneticEventKind::RangeExpansion { area }
            | biogeo_core::AnageneticEventKind::LocalExtirpation { area } => area,
            biogeo_core::AnageneticEventKind::RangeSwitching { to_area, .. } => to_area,
        };
        if usize::from(area) >= context.parsed_ranges.area_names.len() {
            return Err(CliError::BsmTreeReference {
                sample_index,
                message: format!("event references missing area {area}"),
            });
        }
    }
    Ok(())
}

fn format_stochastic_history_rows(
    sample_index: usize,
    stochastic_map: &biogeo_core::BiogeographicStochasticMap,
    context: &BsmFormattingContext<'_>,
) -> Result<BsmSampleTableRows, CliError> {
    let parsed_tree = context.parsed_tree;
    let parsed_ranges = context.parsed_ranges;
    let states = context.states;
    let mut tables: [String; BSM_TABLE_SPECS.len()] = std::array::from_fn(|_| String::new());
    validate_bsm_tree_references(sample_index, stochastic_map, context)?;
    let summary = stochastic_map
        .summarize_with_state_masks(states, context.state_masks.as_deref())
        .map_err(|source| CliError::BsmSummary {
            sample_index,
            source,
        })?;
    if summary.diagnostics.has_state_constraint_violations() {
        return Err(CliError::BsmStateConstraintViolation {
            sample_index,
            forbidden_state_transitions: summary.diagnostics.forbidden_state_transitions,
            forbidden_state_endpoints: summary.diagnostics.forbidden_state_endpoints,
            forbidden_state_time: summary.diagnostics.forbidden_state_time,
        });
    }

    if context.output_level.includes_path_details() {
        for (node, state_index) in stochastic_map
            .skeleton
            .node_states
            .iter()
            .copied()
            .enumerate()
        {
            if context.output_level.is_compact() {
                writeln!(
                    tables[BSM_NODE_STATES],
                    "{sample_index}\t{node}\t{state_index}"
                )
                .expect("writing to a String cannot fail");
                continue;
            }
            let kind = if node == parsed_tree.tree.root() {
                "root"
            } else if parsed_tree.tree.is_tip(node) {
                "tip"
            } else {
                "internal"
            };
            let state = states
                .get(state_index)
                .expect("validated sampled node state index should exist in state space");
            writeln!(
                tables[BSM_NODE_STATES],
                "{sample_index}\t{node}\t{}\t{kind}\t{}\t{state_index}\t{}\t{}",
                context.node_label(node),
                context.clade_label(node),
                state.bits(),
                context.range_label(state_index),
            )
            .expect("writing to a String cannot fail");
        }
    }

    if context.output_level.includes_path_details() {
        for split in &stochastic_map.skeleton.splits {
            if context.output_level.is_compact() {
                writeln!(
                    tables[BSM_CLADOGENETIC_SPLITS],
                    "{sample_index}\t{}\t{}\t{}\t{}\t{:.15}",
                    split.node, split.ancestor, split.left, split.right, split.weight,
                )
                .expect("writing to a String cannot fail");
                continue;
            }
            let kind = if split.node == parsed_tree.tree.root() {
                "root"
            } else {
                "internal"
            };
            let children = parsed_tree
                .tree
                .children(split.node)
                .expect("sampled split node should exist in tree");
            let ancestor = states
                .get(split.ancestor)
                .expect("validated sampled ancestor state index should exist in state space");
            let left = states
                .get(split.left)
                .expect("validated sampled left state index should exist in state space");
            let right = states
                .get(split.right)
                .expect("validated sampled right state index should exist in state space");
            writeln!(
                tables[BSM_CLADOGENETIC_SPLITS],
                "{sample_index}\t{}\t{}\t{kind}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}",
                split.node,
                context.node_label(split.node),
                context.clade_label(split.node),
                context.clade_label(children[0].node),
                context.clade_label(children[1].node),
                split.ancestor,
                ancestor.bits(),
                context.range_label(split.ancestor),
                split.left,
                left.bits(),
                context.range_label(split.left),
                split.right,
                right.bits(),
                context.range_label(split.right),
                split.weight,
            )
            .expect("writing to a String cannot fail");
        }
    }

    if context.output_level.includes_path_details() {
        for branch in &stochastic_map.branches {
            for segment in &branch.segments {
                if context.output_level.is_compact() {
                    writeln!(
                        tables[BSM_BRANCH_SEGMENTS],
                        "{sample_index}\t{}\t{}\t{}\t{:.15}\t{:.15}\t{}\t{}\t{:.15}\t{}\t{}",
                        branch.edge_index,
                        segment.segment_index,
                        segment.q_index,
                        segment.start_time_from_parent,
                        segment.end_time_from_parent,
                        segment.start_state,
                        segment.end_state,
                        segment.endpoint_probability,
                        segment.virtual_jump_count,
                        segment.events.len(),
                    )
                    .expect("writing to a String cannot fail");
                    continue;
                }
                let start = states
                    .get(segment.start_state)
                    .expect("validated sampled segment start state should exist in state space");
                let end = states
                    .get(segment.end_state)
                    .expect("validated sampled segment end state should exist in state space");
                writeln!(
                    tables[BSM_BRANCH_SEGMENTS],
                    "{sample_index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\t{:.15}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\t{}\t{}",
                    branch.edge_index,
                    branch.parent,
                    context.clade_label(branch.parent),
                    branch.child,
                    context.clade_label(branch.child),
                    segment.segment_index,
                    segment.q_index,
                    segment.start_time_from_parent,
                    segment.end_time_from_parent,
                    segment.start_state,
                    start.bits(),
                    context.range_label(segment.start_state),
                    segment.end_state,
                    end.bits(),
                    context.range_label(segment.end_state),
                    segment.endpoint_probability,
                    segment.virtual_jump_count,
                    segment.events.len(),
                )
                .expect("writing to a String cannot fail");
            }
        }
    }

    let clado = summary.cladogenetic_event_counts;
    if context.output_level == BsmOutputLevel::Legacy {
        writeln!(
            tables[BSM_SAMPLE_EVENT_COUNTS],
            "{sample_index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}",
            summary.anagenetic_event_count,
            summary.range_expansion_count,
            summary.local_extirpation_count,
            clado.total(),
            clado.range_copying,
            clado.subset_sympatry,
            clado.vicariance,
            clado.founder_event,
            summary.total_branch_time,
        )
        .expect("writing to a String cannot fail");
    } else {
        let minimum_endpoint_probability = summary
            .diagnostics
            .minimum_endpoint_probability
            .map_or_else(|| "NA".to_string(), |value| format!("{value:.15}"));
        writeln!(
            tables[BSM_SAMPLE_EVENT_COUNTS],
            "{sample_index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}",
            summary.anagenetic_event_count,
            summary.range_expansion_count,
            summary.local_extirpation_count,
            summary.range_switching_count,
            clado.total(),
            clado.range_copying,
            clado.subset_sympatry,
            clado.vicariance,
            clado.founder_event,
            summary.total_branch_time,
            summary.diagnostics.segment_count,
            summary.diagnostics.constrained_segment_count,
            minimum_endpoint_probability,
            summary.diagnostics.maximum_virtual_jump_count,
            summary.diagnostics.maximum_anagenetic_events_per_segment,
            summary.diagnostics.forbidden_state_transitions,
            summary.diagnostics.forbidden_state_endpoints,
            summary.diagnostics.forbidden_state_time,
        )
        .expect("writing to a String cannot fail");
    }

    for (q_index, event_count) in summary.event_counts_by_q.iter().copied().enumerate() {
        let event_fraction = if summary.anagenetic_event_count == 0 {
            0.0
        } else {
            event_count as f64 / summary.anagenetic_event_count as f64
        };
        writeln!(
            tables[BSM_SAMPLE_PERIOD_EVENT_COUNTS],
            "{sample_index}\t{q_index}\t{event_count}\t{event_fraction:.15}"
        )
        .expect("writing to a String cannot fail");
    }

    for (state_index, occupancy_time) in summary.occupancy_time_by_state.iter().copied().enumerate()
    {
        if context.output_level.is_compact() && occupancy_time == 0.0 {
            continue;
        }
        let occupancy_fraction = if summary.total_branch_time == 0.0 {
            0.0
        } else {
            occupancy_time / summary.total_branch_time
        };
        if context.output_level.is_compact() {
            writeln!(
                tables[BSM_SAMPLE_STATE_OCCUPANCY],
                "{sample_index}\t{state_index}\t{occupancy_time:.15}\t{occupancy_fraction:.15}"
            )
            .expect("writing to a String cannot fail");
        } else {
            let state = states
                .get(state_index)
                .expect("summary state index should exist in state space");
            writeln!(
                tables[BSM_SAMPLE_STATE_OCCUPANCY],
                "{sample_index}\t{state_index}\t{}\t{}\t{occupancy_time:.15}\t{occupancy_fraction:.15}",
                state.bits(),
                context.range_label(state_index),
            )
            .expect("writing to a String cannot fail");
        }
    }

    for (q_index, occupancy_by_state) in summary.occupancy_time_by_q_and_state.iter().enumerate() {
        for (state_index, occupancy_time) in occupancy_by_state.iter().copied().enumerate() {
            if context.output_level.is_compact() && occupancy_time == 0.0 {
                continue;
            }
            if context.output_level.is_compact() {
                writeln!(
                    tables[BSM_SAMPLE_PERIOD_STATE_OCCUPANCY],
                    "{sample_index}\t{q_index}\t{state_index}\t{occupancy_time:.15}"
                )
                .expect("writing to a String cannot fail");
            } else {
                let state = states
                    .get(state_index)
                    .expect("summary state index should exist in state space");
                writeln!(
                    tables[BSM_SAMPLE_PERIOD_STATE_OCCUPANCY],
                    "{sample_index}\t{q_index}\t{state_index}\t{}\t{}\t{occupancy_time:.15}",
                    state.bits(),
                    context.range_label(state_index),
                )
                .expect("writing to a String cannot fail");
            }
        }
    }

    if context.output_level.includes_path_details() {
        for branch in &stochastic_map.branches {
            for segment in &branch.segments {
                for event in &segment.events {
                    let (event_kind, parameter, area) = match event.kind {
                        biogeo_core::AnageneticEventKind::RangeExpansion { area } => {
                            ("range_expansion", "d", area)
                        }
                        biogeo_core::AnageneticEventKind::LocalExtirpation { area } => {
                            ("local_extirpation", "e", area)
                        }
                        biogeo_core::AnageneticEventKind::RangeSwitching { to_area, .. } => {
                            ("range_switching", "a", to_area)
                        }
                    };
                    if context.output_level.is_compact() {
                        writeln!(
                            tables[BSM_ANAGENETIC_EVENTS],
                            "{sample_index}\t{}\t{}\t{}\t{:.15}\t{event_kind}\t{parameter}\t{area}\t{}\t{}",
                            branch.edge_index,
                            segment.segment_index,
                            segment.q_index,
                            event.time_from_parent,
                            event.from_state,
                            event.to_state,
                        )
                        .expect("writing to a String cannot fail");
                        continue;
                    }
                    let from = states
                        .get(event.from_state)
                        .expect("validated sampled event source state should exist in state space");
                    let to = states.get(event.to_state).expect(
                        "validated sampled event destination state should exist in state space",
                    );
                    let area_name = parsed_ranges
                        .area_names
                        .get(usize::from(area))
                        .expect("sampled event area should exist in range table");
                    writeln!(
                        tables[BSM_ANAGENETIC_EVENTS],
                        "{sample_index}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.15}\t{event_kind}\t{parameter}\t{area}\t{area_name}\t{}\t{}\t{}\t{}\t{}\t{}",
                        branch.edge_index,
                        branch.parent,
                        context.clade_label(branch.parent),
                        branch.child,
                        context.clade_label(branch.child),
                        segment.segment_index,
                        segment.q_index,
                        event.time_from_parent,
                        event.from_state,
                        from.bits(),
                        context.range_label(event.from_state),
                        event.to_state,
                        to.bits(),
                        context.range_label(event.to_state),
                    )
                    .expect("writing to a String cannot fail");
                }
            }
        }
    }

    Ok(BsmSampleTableRows { tables })
}

const BSM_STREAM_FORMAT: &str = "biogeo-bsm-tsv-v1";
const BSM_SHARDED_STREAM_FORMAT: &str = "biogeo-bsm-sharded-tsv-v1";
const BSM_FULL_STREAM_FORMAT_V2: &str = "biogeo-bsm-full-tsv-v2";
const BSM_FULL_SHARDED_STREAM_FORMAT_V2: &str = "biogeo-bsm-full-sharded-tsv-v2";
const BSM_COMPACT_STREAM_FORMAT_V2: &str = "biogeo-bsm-compact-tsv-v2";
const BSM_COMPACT_SHARDED_STREAM_FORMAT_V2: &str = "biogeo-bsm-compact-sharded-tsv-v2";
const BSM_SUMMARY_STREAM_FORMAT_V2: &str = "biogeo-bsm-summary-tsv-v2";
const BSM_SUMMARY_SHARDED_STREAM_FORMAT_V2: &str = "biogeo-bsm-summary-sharded-tsv-v2";
const BSM_SHARD_MANIFEST_FORMAT: &str = "biogeo-bsm-shard-manifest-v1";
const BSM_SHARD_DIRECTORY: &str = "shards";
const BSM_SHARD_IN_PROGRESS_DIRECTORY: &str = "in-progress";
const BSM_SHARD_MANIFEST_FILE: &str = "manifest.tsv";
const BSM_CHECKPOINT_FORMAT_V1: &str = "biogeo-bsm-checkpoint-v1";
const BSM_CHECKPOINT_FORMAT: &str = "biogeo-bsm-checkpoint-v2";
const BSM_CHECKPOINT_DIRECTORY: &str = "checkpoints";

#[derive(Clone, Debug, Eq, PartialEq)]
struct BsmCheckpoint {
    completed_samples: usize,
    completed_anagenetic_events: Option<usize>,
    run_fingerprint: String,
    table_lengths: [u64; BSM_TABLE_SPECS.len()],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BsmShardRange {
    index: usize,
    start: usize,
    end_exclusive: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CompletedBsmShard {
    range: BsmShardRange,
    checkpoint: BsmCheckpoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct InProgressBsmShard {
    range: BsmShardRange,
    checkpoint: BsmCheckpoint,
    path: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BsmShardLayout {
    completed: Vec<CompletedBsmShard>,
    in_progress: Option<InProgressBsmShard>,
}

impl BsmShardRange {
    fn for_start(start: usize, shard_samples: usize, sample_count: usize) -> Option<Self> {
        if start >= sample_count || shard_samples == 0 || !start.is_multiple_of(shard_samples) {
            return None;
        }
        Some(Self {
            index: start / shard_samples,
            start,
            end_exclusive: start.saturating_add(shard_samples).min(sample_count),
        })
    }

    fn sample_count(self) -> usize {
        self.end_exclusive - self.start
    }

    fn directory_name(self) -> String {
        format!("shard-{:020}-{:020}", self.start, self.end_exclusive)
    }

    fn relative_directory(self) -> String {
        format!("{BSM_SHARD_DIRECTORY}/{}", self.directory_name())
    }
}

fn bsm_total_shards(sample_count: usize, shard_samples: usize) -> usize {
    sample_count.div_ceil(shard_samples)
}

fn bsm_completed_shards(
    completed_samples: usize,
    sample_count: usize,
    shard_samples: usize,
) -> usize {
    if completed_samples == sample_count {
        bsm_total_shards(sample_count, shard_samples)
    } else {
        completed_samples / shard_samples
    }
}

struct BsmTableWriter {
    path: PathBuf,
    writer: BufWriter<fs::File>,
}

struct BsmTableWriters {
    tables: Vec<BsmTableWriter>,
}

#[cfg(test)]
thread_local! {
    static BSM_TEST_STORAGE_FULL_AFTER_WRITES: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
    static BSM_TEST_STORAGE_FULL_AFTER_FLUSHES: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
    static BSM_TEST_STORAGE_FULL_AFTER_CHECKPOINT_WRITES: std::cell::Cell<Option<usize>> = const {
        std::cell::Cell::new(None)
    };
}

#[cfg(test)]
struct BsmStorageFullFaultGuard;

#[cfg(test)]
impl Drop for BsmStorageFullFaultGuard {
    fn drop(&mut self) {
        BSM_TEST_STORAGE_FULL_AFTER_WRITES.with(|remaining| remaining.set(None));
        BSM_TEST_STORAGE_FULL_AFTER_FLUSHES.with(|remaining| remaining.set(None));
        BSM_TEST_STORAGE_FULL_AFTER_CHECKPOINT_WRITES.with(|remaining| remaining.set(None));
    }
}

#[cfg(test)]
fn inject_bsm_storage_full(
    writes_before_failure: Option<usize>,
    flushes_before_failure: Option<usize>,
    checkpoint_writes_before_failure: Option<usize>,
) -> BsmStorageFullFaultGuard {
    BSM_TEST_STORAGE_FULL_AFTER_WRITES.with(|remaining| {
        assert!(remaining.replace(writes_before_failure).is_none());
    });
    BSM_TEST_STORAGE_FULL_AFTER_FLUSHES.with(|remaining| {
        assert!(remaining.replace(flushes_before_failure).is_none());
    });
    BSM_TEST_STORAGE_FULL_AFTER_CHECKPOINT_WRITES.with(|remaining| {
        assert!(
            remaining
                .replace(checkpoint_writes_before_failure)
                .is_none()
        );
    });
    BsmStorageFullFaultGuard
}

#[cfg(test)]
fn maybe_inject_bsm_storage_full(
    counter: &'static std::thread::LocalKey<std::cell::Cell<Option<usize>>>,
) -> io::Result<()> {
    counter.with(|remaining| match remaining.get() {
        Some(0) => {
            remaining.set(None);
            Err(io::Error::new(
                io::ErrorKind::StorageFull,
                "injected BSM storage-full failure",
            ))
        }
        Some(count) => {
            remaining.set(Some(count - 1));
            Ok(())
        }
        None => Ok(()),
    })
}

impl BsmTableWriters {
    fn create(output_dir: &Path, output_level: BsmOutputLevel) -> Result<Self, CliError> {
        let mut tables = Vec::with_capacity(BSM_TABLE_SPECS.len());
        for (table_index, spec) in BSM_TABLE_SPECS.iter().enumerate() {
            let path = output_dir.join(spec.file_name);
            let file = fs::File::create(&path).map_err(|source| CliError::OutputIo {
                path: path.clone(),
                source,
            })?;
            let mut writer = BufWriter::new(file);
            writeln!(writer, "{}", bsm_table_header(output_level, table_index)).map_err(
                |source| CliError::OutputIo {
                    path: path.clone(),
                    source,
                },
            )?;
            tables.push(BsmTableWriter { path, writer });
        }
        Ok(Self { tables })
    }

    fn open_at_checkpoint(output_dir: &Path, checkpoint: &BsmCheckpoint) -> Result<Self, CliError> {
        let mut files = Vec::with_capacity(BSM_TABLE_SPECS.len());
        for (spec, expected_length) in BSM_TABLE_SPECS
            .iter()
            .zip(checkpoint.table_lengths.iter().copied())
        {
            let path = output_dir.join(spec.file_name);
            let file = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .map_err(|source| CliError::OutputIo {
                    path: path.clone(),
                    source,
                })?;
            let actual_length = file
                .metadata()
                .map_err(|source| CliError::OutputIo {
                    path: path.clone(),
                    source,
                })?
                .len();
            if actual_length < expected_length {
                return Err(CliError::BsmTableShorterThanCheckpoint {
                    path,
                    expected: expected_length,
                    actual: actual_length,
                });
            }
            files.push((path, file, expected_length));
        }

        let mut tables = Vec::with_capacity(BSM_TABLE_SPECS.len());
        for (path, mut file, expected_length) in files {
            file.set_len(expected_length)
                .and_then(|()| file.sync_data())
                .and_then(|()| file.seek(SeekFrom::End(0)).map(|_| ()))
                .map_err(|source| CliError::OutputIo {
                    path: path.clone(),
                    source,
                })?;
            tables.push(BsmTableWriter {
                path,
                writer: BufWriter::new(file),
            });
        }
        Ok(Self { tables })
    }

    fn write_sample(&mut self, rows: &BsmSampleTableRows) -> Result<(), CliError> {
        for (table, contents) in self.tables.iter_mut().zip(&rows.tables) {
            #[cfg(test)]
            maybe_inject_bsm_storage_full(&BSM_TEST_STORAGE_FULL_AFTER_WRITES).map_err(
                |source| CliError::OutputIo {
                    path: table.path.clone(),
                    source,
                },
            )?;
            table
                .writer
                .write_all(contents.as_bytes())
                .map_err(|source| CliError::OutputIo {
                    path: table.path.clone(),
                    source,
                })?;
        }
        Ok(())
    }

    fn flush_and_sync(&mut self) -> Result<[u64; BSM_TABLE_SPECS.len()], CliError> {
        let mut lengths = [0; BSM_TABLE_SPECS.len()];
        for (index, table) in self.tables.iter_mut().enumerate() {
            #[cfg(test)]
            maybe_inject_bsm_storage_full(&BSM_TEST_STORAGE_FULL_AFTER_FLUSHES).map_err(
                |source| CliError::OutputIo {
                    path: table.path.clone(),
                    source,
                },
            )?;
            table.writer.flush().map_err(|source| CliError::OutputIo {
                path: table.path.clone(),
                source,
            })?;
            table
                .writer
                .get_ref()
                .sync_data()
                .map_err(|source| CliError::OutputIo {
                    path: table.path.clone(),
                    source,
                })?;
            lengths[index] = table
                .writer
                .get_ref()
                .metadata()
                .map_err(|source| CliError::OutputIo {
                    path: table.path.clone(),
                    source,
                })?
                .len();
        }
        Ok(lengths)
    }

    fn rollback(self, checkpoint: &BsmCheckpoint) -> Result<(), CliError> {
        let mut first_error = None;
        for (table, expected_length) in self
            .tables
            .into_iter()
            .zip(checkpoint.table_lengths.iter().copied())
        {
            let (file, _) = table.writer.into_parts();
            if let Err(source) = file
                .set_len(expected_length)
                .and_then(|()| file.sync_data())
                && first_error.is_none()
            {
                first_error = Some(CliError::OutputIo {
                    path: table.path,
                    source,
                });
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn checkpoint_path(output_dir: &Path, completed_samples: usize) -> PathBuf {
    output_dir
        .join(BSM_CHECKPOINT_DIRECTORY)
        .join(format!("checkpoint-{completed_samples:020}.tsv"))
}

fn invalid_bsm_checkpoint(path: &Path, message: impl Into<String>) -> CliError {
    CliError::InvalidBsmCheckpoint {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn format_bsm_checkpoint(checkpoint: &BsmCheckpoint) -> String {
    let completed_anagenetic_events = checkpoint
        .completed_anagenetic_events
        .expect("new BSM checkpoints must include the cumulative event count");
    let mut contents = format!(
        "key\tvalue\nformat\t{BSM_CHECKPOINT_FORMAT}\ncompleted_samples\t{}\ncompleted_anagenetic_events\t{completed_anagenetic_events}\nrun_fingerprint\t{}\n",
        checkpoint.completed_samples, checkpoint.run_fingerprint
    );
    for (spec, length) in BSM_TABLE_SPECS.iter().zip(checkpoint.table_lengths.iter()) {
        writeln!(contents, "{}\t{length}", spec.file_name)
            .expect("writing checkpoint metadata to a String cannot fail");
    }
    contents
}

fn parse_bsm_checkpoint(path: &Path) -> Result<BsmCheckpoint, CliError> {
    let contents = fs::read_to_string(path).map_err(|source| CliError::OutputIo {
        path: path.to_path_buf(),
        source,
    })?;
    let mut lines = contents.lines();
    if lines.next() != Some("key\tvalue") {
        return Err(invalid_bsm_checkpoint(path, "missing key/value header"));
    }

    let mut format = None;
    let mut completed_samples = None;
    let mut completed_anagenetic_events = None;
    let mut run_fingerprint = None;
    let mut table_lengths: [Option<u64>; BSM_TABLE_SPECS.len()] = [None; BSM_TABLE_SPECS.len()];
    for (line_offset, line) in lines.enumerate() {
        let line_number = line_offset + 2;
        let Some((key, value)) = line.split_once('\t') else {
            return Err(invalid_bsm_checkpoint(
                path,
                format!("line {line_number} is not a key/value row"),
            ));
        };
        match key {
            "format" if format.replace(value.to_string()).is_none() => {}
            "completed_samples" if completed_samples.is_none() => {
                completed_samples = Some(value.parse::<usize>().map_err(|error| {
                    invalid_bsm_checkpoint(
                        path,
                        format!("invalid completed_samples on line {line_number}: {error}"),
                    )
                })?);
            }
            "completed_anagenetic_events" if completed_anagenetic_events.is_none() => {
                completed_anagenetic_events = Some(value.parse::<usize>().map_err(|error| {
                    invalid_bsm_checkpoint(
                        path,
                        format!(
                            "invalid completed_anagenetic_events on line {line_number}: {error}"
                        ),
                    )
                })?);
            }
            "run_fingerprint" if run_fingerprint.replace(value.to_string()).is_none() => {}
            _ => {
                let Some(table_index) = BSM_TABLE_SPECS
                    .iter()
                    .position(|spec| spec.file_name == key)
                else {
                    return Err(invalid_bsm_checkpoint(
                        path,
                        format!("unknown or duplicate key {key:?} on line {line_number}"),
                    ));
                };
                if table_lengths[table_index].is_some() {
                    return Err(invalid_bsm_checkpoint(
                        path,
                        format!("duplicate table length for {key:?}"),
                    ));
                }
                table_lengths[table_index] = Some(value.parse::<u64>().map_err(|error| {
                    invalid_bsm_checkpoint(
                        path,
                        format!("invalid length for {key:?} on line {line_number}: {error}"),
                    )
                })?);
            }
        }
    }

    if !matches!(
        format.as_deref(),
        Some(BSM_CHECKPOINT_FORMAT) | Some(BSM_CHECKPOINT_FORMAT_V1)
    ) {
        return Err(invalid_bsm_checkpoint(
            path,
            format!("expected format {BSM_CHECKPOINT_FORMAT:?} or {BSM_CHECKPOINT_FORMAT_V1:?}"),
        ));
    }
    if format.as_deref() == Some(BSM_CHECKPOINT_FORMAT) && completed_anagenetic_events.is_none() {
        return Err(invalid_bsm_checkpoint(
            path,
            "missing completed_anagenetic_events",
        ));
    }
    let completed_samples = completed_samples
        .ok_or_else(|| invalid_bsm_checkpoint(path, "missing completed_samples"))?;
    let run_fingerprint = run_fingerprint
        .filter(|value| !value.is_empty())
        .ok_or_else(|| invalid_bsm_checkpoint(path, "missing run_fingerprint"))?;
    let mut resolved_lengths = [0; BSM_TABLE_SPECS.len()];
    for (index, length) in table_lengths.into_iter().enumerate() {
        resolved_lengths[index] = length.ok_or_else(|| {
            invalid_bsm_checkpoint(
                path,
                format!("missing length for {:?}", BSM_TABLE_SPECS[index].file_name),
            )
        })?;
    }
    Ok(BsmCheckpoint {
        completed_samples,
        completed_anagenetic_events,
        run_fingerprint,
        table_lengths: resolved_lengths,
    })
}

fn write_bsm_checkpoint(output_dir: &Path, checkpoint: &BsmCheckpoint) -> Result<(), CliError> {
    let checkpoint_dir = output_dir.join(BSM_CHECKPOINT_DIRECTORY);
    fs::create_dir_all(&checkpoint_dir).map_err(|source| CliError::OutputIo {
        path: checkpoint_dir.clone(),
        source,
    })?;
    let final_path = checkpoint_path(output_dir, checkpoint.completed_samples);
    if final_path.exists() {
        let existing = parse_bsm_checkpoint(&final_path)?;
        return if existing == *checkpoint {
            Ok(())
        } else {
            Err(invalid_bsm_checkpoint(
                &final_path,
                "an incompatible checkpoint already exists at this sample index",
            ))
        };
    }

    let temporary_path = checkpoint_dir.join(format!(
        ".checkpoint-{:020}-{}.tmp",
        checkpoint.completed_samples,
        process::id()
    ));
    if temporary_path.exists() {
        fs::remove_file(&temporary_path).map_err(|source| CliError::OutputIo {
            path: temporary_path.clone(),
            source,
        })?;
    }
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|source| CliError::OutputIo {
                path: temporary_path.clone(),
                source,
            })?;
        #[cfg(test)]
        maybe_inject_bsm_storage_full(&BSM_TEST_STORAGE_FULL_AFTER_CHECKPOINT_WRITES).map_err(
            |source| CliError::OutputIo {
                path: temporary_path.clone(),
                source,
            },
        )?;
        file.write_all(format_bsm_checkpoint(checkpoint).as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| CliError::OutputIo {
                path: temporary_path.clone(),
                source,
            })?;
        drop(file);
        fs_retry::rename(&temporary_path, &final_path).map_err(|source| CliError::OutputIo {
            path: final_path.clone(),
            source,
        })
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn load_latest_bsm_checkpoint(
    output_dir: &Path,
    sample_count: usize,
) -> Result<BsmCheckpoint, CliError> {
    let checkpoint_dir = output_dir.join(BSM_CHECKPOINT_DIRECTORY);
    let entries = match fs::read_dir(&checkpoint_dir) {
        Ok(entries) => entries,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Err(CliError::MissingBsmCheckpoint(checkpoint_dir));
        }
        Err(source) => {
            return Err(CliError::OutputIo {
                path: checkpoint_dir,
                source,
            });
        }
    };
    let mut latest = None;
    for entry in entries {
        let entry = entry.map_err(|source| CliError::OutputIo {
            path: checkpoint_dir.clone(),
            source,
        })?;
        let file_name = entry.file_name();
        let Some(file_name) = file_name.to_str() else {
            continue;
        };
        let Some(number) = file_name
            .strip_prefix("checkpoint-")
            .and_then(|value| value.strip_suffix(".tsv"))
        else {
            continue;
        };
        let name_samples = number.parse::<usize>().map_err(|error| {
            invalid_bsm_checkpoint(
                &entry.path(),
                format!("invalid checkpoint file name: {error}"),
            )
        })?;
        let checkpoint = parse_bsm_checkpoint(&entry.path())?;
        if checkpoint.completed_samples != name_samples {
            return Err(invalid_bsm_checkpoint(
                &entry.path(),
                "file name and completed_samples disagree",
            ));
        }
        if checkpoint.completed_samples > sample_count {
            return Err(invalid_bsm_checkpoint(
                &entry.path(),
                format!(
                    "checkpoint contains {} samples but this run requests {sample_count}",
                    checkpoint.completed_samples
                ),
            ));
        }
        if latest.as_ref().is_none_or(|current: &BsmCheckpoint| {
            checkpoint.completed_samples > current.completed_samples
        }) {
            latest = Some(checkpoint);
        }
    }
    let checkpoint = latest.ok_or(CliError::MissingBsmCheckpoint(checkpoint_dir))?;
    hydrate_checkpoint_event_count(output_dir, checkpoint)
}

fn hydrate_checkpoint_event_count(
    output_dir: &Path,
    mut checkpoint: BsmCheckpoint,
) -> Result<BsmCheckpoint, CliError> {
    if checkpoint.completed_anagenetic_events.is_some() {
        return Ok(checkpoint);
    }

    let table_index = BSM_SAMPLE_EVENT_COUNTS;
    let path = output_dir.join(BSM_TABLE_SPECS[table_index].file_name);
    let file = fs::File::open(&path).map_err(|source| CliError::OutputIo {
        path: path.clone(),
        source,
    })?;
    let expected_length = checkpoint.table_lengths[table_index];
    let actual_length = file
        .metadata()
        .map_err(|source| CliError::OutputIo {
            path: path.clone(),
            source,
        })?
        .len();
    if actual_length < expected_length {
        return Err(CliError::BsmTableShorterThanCheckpoint {
            path,
            expected: expected_length,
            actual: actual_length,
        });
    }

    let mut reader = BufReader::new(file.take(expected_length));
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|source| CliError::OutputIo {
            path: path.clone(),
            source,
        })?;
    if line.trim_end_matches(['\r', '\n']) != BSM_TABLE_SPECS[table_index].header {
        return Err(invalid_bsm_checkpoint(
            &path,
            "sample event-count table header does not match the checkpoint format",
        ));
    }

    let mut completed_anagenetic_events = 0_usize;
    let mut row_count = 0_usize;
    loop {
        line.clear();
        let bytes_read = reader
            .read_line(&mut line)
            .map_err(|source| CliError::OutputIo {
                path: path.clone(),
                source,
            })?;
        if bytes_read == 0 {
            break;
        }
        let row = line.trim_end_matches(['\r', '\n']);
        let mut fields = row.split('\t');
        let sample_index = fields
            .next()
            .ok_or_else(|| invalid_bsm_checkpoint(&path, "missing sample index"))?
            .parse::<usize>()
            .map_err(|error| {
                invalid_bsm_checkpoint(&path, format!("invalid sample index: {error}"))
            })?;
        if sample_index != row_count {
            return Err(invalid_bsm_checkpoint(
                &path,
                format!("expected sample index {row_count}, found {sample_index}"),
            ));
        }
        let event_count = fields
            .next()
            .ok_or_else(|| invalid_bsm_checkpoint(&path, "missing anagenetic event count"))?
            .parse::<usize>()
            .map_err(|error| {
                invalid_bsm_checkpoint(&path, format!("invalid anagenetic event count: {error}"))
            })?;
        completed_anagenetic_events = completed_anagenetic_events
            .checked_add(event_count)
            .ok_or_else(|| {
                invalid_bsm_checkpoint(&path, "cumulative anagenetic event count overflowed usize")
            })?;
        row_count += 1;
    }
    if row_count != checkpoint.completed_samples {
        return Err(invalid_bsm_checkpoint(
            &path,
            format!(
                "checkpoint records {} samples but the committed event-count table contains {row_count}",
                checkpoint.completed_samples
            ),
        ));
    }
    checkpoint.completed_anagenetic_events = Some(completed_anagenetic_events);
    Ok(checkpoint)
}

fn commit_bsm_checkpoint(
    writers: &mut BsmTableWriters,
    output_dir: &Path,
    completed_samples: usize,
    completed_anagenetic_events: usize,
    run_fingerprint: &str,
) -> Result<BsmCheckpoint, CliError> {
    let checkpoint = BsmCheckpoint {
        completed_samples,
        completed_anagenetic_events: Some(completed_anagenetic_events),
        run_fingerprint: run_fingerprint.to_string(),
        table_lengths: writers.flush_and_sync()?,
    };
    write_bsm_checkpoint(output_dir, &checkpoint)?;
    Ok(checkpoint)
}

fn prepare_bsm_output_directory(output_dir: &Path) -> Result<(), CliError> {
    if let Some(parent) = output_dir.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| CliError::OutputIo {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    match fs::create_dir(output_dir) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(CliError::BsmOutputDirectoryExists(output_dir.to_path_buf()))
        }
        Err(source) => Err(CliError::OutputIo {
            path: output_dir.to_path_buf(),
            source,
        }),
    }
}

fn invalid_bsm_shard(path: &Path, message: impl Into<String>) -> CliError {
    CliError::InvalidBsmShard {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn parse_bsm_shard_directory_name(
    path: &Path,
    shard_samples: usize,
    sample_count: usize,
) -> Result<BsmShardRange, CliError> {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| invalid_bsm_shard(path, "shard directory name is not valid UTF-8"))?;
    let suffix = name
        .strip_prefix("shard-")
        .ok_or_else(|| invalid_bsm_shard(path, "expected a shard-<start>-<end> directory"))?;
    let (start, end_exclusive) = suffix
        .split_once('-')
        .ok_or_else(|| invalid_bsm_shard(path, "missing shard range separator"))?;
    let start = start
        .parse::<usize>()
        .map_err(|error| invalid_bsm_shard(path, format!("invalid shard start: {error}")))?;
    let end_exclusive = end_exclusive
        .parse::<usize>()
        .map_err(|error| invalid_bsm_shard(path, format!("invalid shard end: {error}")))?;
    let range = BsmShardRange::for_start(start, shard_samples, sample_count).ok_or_else(|| {
        invalid_bsm_shard(
            path,
            format!("sample start {start} is not a valid fixed shard boundary"),
        )
    })?;
    if range.end_exclusive != end_exclusive || range.directory_name() != name {
        return Err(invalid_bsm_shard(
            path,
            format!("expected canonical directory {:?}", range.directory_name()),
        ));
    }
    Ok(range)
}

fn list_bsm_shard_directories(
    parent: &Path,
    shard_samples: usize,
    sample_count: usize,
) -> Result<Vec<(BsmShardRange, PathBuf)>, CliError> {
    let entries = fs::read_dir(parent).map_err(|source| CliError::OutputIo {
        path: parent.to_path_buf(),
        source,
    })?;
    let mut shards = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| CliError::OutputIo {
            path: parent.to_path_buf(),
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| CliError::OutputIo {
            path: entry.path(),
            source,
        })?;
        if !file_type.is_dir() {
            return Err(invalid_bsm_shard(
                &entry.path(),
                "shard container may only contain shard directories",
            ));
        }
        let path = entry.path();
        let range = parse_bsm_shard_directory_name(&path, shard_samples, sample_count)?;
        shards.push((range, path));
    }
    shards.sort_unstable_by_key(|(range, _)| range.start);
    Ok(shards)
}

fn validate_complete_bsm_shard(
    path: &Path,
    range: BsmShardRange,
    sample_count: usize,
    run_fingerprint: &str,
) -> Result<CompletedBsmShard, CliError> {
    let checkpoint = load_latest_bsm_checkpoint(path, sample_count)?;
    if checkpoint.completed_samples != range.end_exclusive {
        return Err(invalid_bsm_shard(
            path,
            format!(
                "complete shard ends at {}, but its latest checkpoint ends at {}",
                range.end_exclusive, checkpoint.completed_samples
            ),
        ));
    }
    if checkpoint.run_fingerprint != run_fingerprint {
        return Err(CliError::BsmResumeFingerprintMismatch {
            expected: checkpoint.run_fingerprint,
            actual: run_fingerprint.to_string(),
        });
    }
    for (spec, expected) in BSM_TABLE_SPECS
        .iter()
        .zip(checkpoint.table_lengths.iter().copied())
    {
        let table_path = path.join(spec.file_name);
        let actual = fs::metadata(&table_path)
            .map_err(|source| CliError::OutputIo {
                path: table_path.clone(),
                source,
            })?
            .len();
        if actual != expected {
            return Err(invalid_bsm_shard(
                &table_path,
                format!("published shard table length is {actual}, expected exactly {expected}"),
            ));
        }
    }
    Ok(CompletedBsmShard { range, checkpoint })
}

fn load_bsm_shard_layout(
    output_dir: &Path,
    sample_count: usize,
    shard_samples: usize,
    run_fingerprint: &str,
) -> Result<BsmShardLayout, CliError> {
    let complete_root = output_dir.join(BSM_SHARD_DIRECTORY);
    let in_progress_root = output_dir.join(BSM_SHARD_IN_PROGRESS_DIRECTORY);
    let complete_directories =
        list_bsm_shard_directories(&complete_root, shard_samples, sample_count)?;
    let in_progress_directories =
        list_bsm_shard_directories(&in_progress_root, shard_samples, sample_count)?;
    if in_progress_directories.len() > 1 {
        return Err(invalid_bsm_shard(
            &in_progress_root,
            "found more than one in-progress shard",
        ));
    }

    let mut completed = Vec::with_capacity(complete_directories.len());
    let mut expected_start = 0_usize;
    let mut completed_events = 0_usize;
    for (range, path) in complete_directories {
        let expected = BsmShardRange::for_start(expected_start, shard_samples, sample_count)
            .ok_or_else(|| {
                invalid_bsm_shard(&path, "found a shard after the requested sample end")
            })?;
        if range != expected {
            return Err(invalid_bsm_shard(
                &path,
                format!(
                    "non-contiguous shard range; expected {:?}",
                    expected.directory_name()
                ),
            ));
        }
        let shard = validate_complete_bsm_shard(&path, range, sample_count, run_fingerprint)?;
        let shard_events = shard
            .checkpoint
            .completed_anagenetic_events
            .expect("sharded output only writes v2 checkpoints");
        if shard_events < completed_events {
            return Err(invalid_bsm_shard(
                &path,
                "cumulative anagenetic-event count decreased between shards",
            ));
        }
        completed_events = shard_events;
        expected_start = range.end_exclusive;
        completed.push(shard);
    }

    let in_progress = match in_progress_directories.into_iter().next() {
        None => None,
        Some((range, path)) => {
            let expected = BsmShardRange::for_start(expected_start, shard_samples, sample_count)
                .ok_or_else(|| {
                    invalid_bsm_shard(&path, "found an in-progress shard after all samples")
                })?;
            if range != expected {
                return Err(invalid_bsm_shard(
                    &path,
                    format!(
                        "in-progress shard is not the next range {:?}",
                        expected.directory_name()
                    ),
                ));
            }
            let checkpoint = match load_latest_bsm_checkpoint(&path, sample_count) {
                Ok(checkpoint) => checkpoint,
                Err(CliError::MissingBsmCheckpoint(_)) => {
                    fs::remove_dir_all(&path).map_err(|source| CliError::OutputIo {
                        path: path.clone(),
                        source,
                    })?;
                    return Ok(BsmShardLayout {
                        completed,
                        in_progress: None,
                    });
                }
                Err(error) => return Err(error),
            };
            if checkpoint.run_fingerprint != run_fingerprint {
                return Err(CliError::BsmResumeFingerprintMismatch {
                    expected: checkpoint.run_fingerprint,
                    actual: run_fingerprint.to_string(),
                });
            }
            if !(range.start..=range.end_exclusive).contains(&checkpoint.completed_samples) {
                return Err(invalid_bsm_shard(
                    &path,
                    format!(
                        "checkpoint sample {} is outside shard range {}..{}",
                        checkpoint.completed_samples, range.start, range.end_exclusive
                    ),
                ));
            }
            let checkpoint_events = checkpoint
                .completed_anagenetic_events
                .expect("sharded output only writes v2 checkpoints");
            if checkpoint_events < completed_events {
                return Err(invalid_bsm_shard(
                    &path,
                    "in-progress cumulative event count is below the completed-shard prefix",
                ));
            }
            Some(InProgressBsmShard {
                range,
                checkpoint,
                path,
            })
        }
    };
    Ok(BsmShardLayout {
        completed,
        in_progress,
    })
}

fn read_bsm_sharded_root_identity(
    path: &Path,
    expected_format: &str,
    section_end: Option<&str>,
) -> Result<String, CliError> {
    let contents = fs::read_to_string(path).map_err(|source| CliError::OutputIo {
        path: path.to_path_buf(),
        source,
    })?;
    let mut lines = contents.lines();
    if lines.next() != Some("key\tvalue") {
        return Err(invalid_bsm_shard(path, "missing metadata key/value header"));
    }
    let mut format = None;
    let mut found_fingerprint = None;
    for line in lines {
        if section_end == Some(line) {
            break;
        }
        let Some((key, value)) = line.split_once('\t') else {
            return Err(invalid_bsm_shard(
                path,
                "metadata contains a non-key/value row",
            ));
        };
        match key {
            "format" if format.replace(value).is_none() => {}
            "run_fingerprint" if found_fingerprint.replace(value).is_none() => {}
            "format" | "run_fingerprint" => {
                return Err(invalid_bsm_shard(
                    path,
                    format!("duplicate metadata key {key:?}"),
                ));
            }
            _ => {}
        }
    }
    if format != Some(expected_format) {
        return Err(invalid_bsm_shard(
            path,
            format!("expected format {expected_format:?}"),
        ));
    }
    found_fingerprint
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| invalid_bsm_shard(path, "missing run_fingerprint"))
}

fn validate_bsm_sharded_root_identity(
    output_dir: &Path,
    run_fingerprint: &str,
    expected_format: &str,
) -> Result<(), CliError> {
    let metadata_path = output_dir.join("metadata.tsv");
    let found_fingerprint =
        match read_bsm_sharded_root_identity(&metadata_path, expected_format, None) {
            Ok(fingerprint) => fingerprint,
            Err(_) => read_bsm_sharded_root_identity(
                &output_dir.join(BSM_SHARD_MANIFEST_FILE),
                BSM_SHARD_MANIFEST_FORMAT,
                Some("shards"),
            )?,
        };
    if found_fingerprint != run_fingerprint {
        return Err(CliError::BsmResumeFingerprintMismatch {
            expected: found_fingerprint,
            actual: run_fingerprint.to_string(),
        });
    }
    Ok(())
}

fn format_bsm_shard_manifest(
    sample_count: usize,
    shard_samples: usize,
    run_fingerprint: &str,
    completed: &[CompletedBsmShard],
) -> Result<String, CliError> {
    let completed_samples = completed
        .last()
        .map_or(0, |shard| shard.range.end_exclusive);
    let completed_events = completed.last().map_or(0, |shard| {
        shard
            .checkpoint
            .completed_anagenetic_events
            .expect("sharded output only writes v2 checkpoints")
    });
    let mut contents = format!(
        "key\tvalue\nformat\t{BSM_SHARD_MANIFEST_FORMAT}\nrun_fingerprint\t{run_fingerprint}\nsamples\t{sample_count}\nshard_samples\t{shard_samples}\ncompleted_shards\t{}\ncompleted_samples\t{completed_samples}\ncompleted_anagenetic_events\t{completed_events}\nshards\nshard_index\tsample_start\tsample_end_exclusive\tsample_count\tanagenetic_events\tcumulative_anagenetic_events\tdirectory",
        completed.len()
    );
    for spec in BSM_TABLE_SPECS {
        write!(contents, "\t{}_bytes", spec.file_name)
            .expect("writing shard manifest header to a String cannot fail");
    }
    contents.push('\n');

    let mut previous_events = 0_usize;
    for shard in completed {
        let cumulative_events = shard
            .checkpoint
            .completed_anagenetic_events
            .expect("sharded output only writes v2 checkpoints");
        let shard_events = cumulative_events
            .checked_sub(previous_events)
            .ok_or_else(|| {
                invalid_bsm_shard(
                    Path::new(BSM_SHARD_MANIFEST_FILE),
                    "cumulative event count decreased while formatting manifest",
                )
            })?;
        write!(
            contents,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            shard.range.index,
            shard.range.start,
            shard.range.end_exclusive,
            shard.range.sample_count(),
            shard_events,
            cumulative_events,
            shard.range.relative_directory(),
        )
        .expect("writing shard manifest row to a String cannot fail");
        for length in shard.checkpoint.table_lengths {
            write!(contents, "\t{length}")
                .expect("writing shard manifest row to a String cannot fail");
        }
        contents.push('\n');
        previous_events = cumulative_events;
    }
    Ok(contents)
}

fn write_bsm_shard_manifest(
    output_dir: &Path,
    sample_count: usize,
    shard_samples: usize,
    run_fingerprint: &str,
    completed: &[CompletedBsmShard],
) -> Result<(), CliError> {
    let final_path = output_dir.join(BSM_SHARD_MANIFEST_FILE);
    let temporary_path =
        output_dir.join(format!(".{BSM_SHARD_MANIFEST_FILE}-{}.tmp", process::id()));
    if temporary_path.exists() {
        fs::remove_file(&temporary_path).map_err(|source| CliError::OutputIo {
            path: temporary_path.clone(),
            source,
        })?;
    }
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_path)
            .map_err(|source| CliError::OutputIo {
                path: temporary_path.clone(),
                source,
            })?;
        file.write_all(
            format_bsm_shard_manifest(sample_count, shard_samples, run_fingerprint, completed)?
                .as_bytes(),
        )
        .and_then(|()| file.sync_all())
        .map_err(|source| CliError::OutputIo {
            path: temporary_path.clone(),
            source,
        })?;
        drop(file);
        if final_path.exists() {
            fs::remove_file(&final_path).map_err(|source| CliError::OutputIo {
                path: final_path.clone(),
                source,
            })?;
        }
        fs_retry::rename(&temporary_path, &final_path).map_err(|source| CliError::OutputIo {
            path: final_path.clone(),
            source,
        })
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary_path);
    }
    write_result
}

fn prepare_bsm_sharded_output_directory(
    output_dir: &Path,
    sample_count: usize,
    shard_samples: usize,
    run_fingerprint: &str,
) -> Result<(), CliError> {
    prepare_bsm_output_directory(output_dir)?;
    for child in [BSM_SHARD_DIRECTORY, BSM_SHARD_IN_PROGRESS_DIRECTORY] {
        let path = output_dir.join(child);
        fs::create_dir(&path).map_err(|source| CliError::OutputIo { path, source })?;
    }
    write_bsm_shard_manifest(
        output_dir,
        sample_count,
        shard_samples,
        run_fingerprint,
        &[],
    )
}

fn create_in_progress_bsm_shard(
    output_dir: &Path,
    range: BsmShardRange,
    completed_anagenetic_events: usize,
    run_fingerprint: &str,
    output_level: BsmOutputLevel,
) -> Result<(BsmTableWriters, BsmCheckpoint, PathBuf), CliError> {
    let path = output_dir
        .join(BSM_SHARD_IN_PROGRESS_DIRECTORY)
        .join(range.directory_name());
    fs::create_dir(&path).map_err(|source| CliError::OutputIo {
        path: path.clone(),
        source,
    })?;
    let mut writers = BsmTableWriters::create(&path, output_level)?;
    let checkpoint = commit_bsm_checkpoint(
        &mut writers,
        &path,
        range.start,
        completed_anagenetic_events,
        run_fingerprint,
    )?;
    Ok((writers, checkpoint, path))
}

fn publish_bsm_shard(
    output_dir: &Path,
    staging_path: &Path,
    range: BsmShardRange,
    checkpoint: &BsmCheckpoint,
    sample_count: usize,
    run_fingerprint: &str,
) -> Result<CompletedBsmShard, CliError> {
    if checkpoint.completed_samples != range.end_exclusive {
        return Err(invalid_bsm_shard(
            staging_path,
            "cannot publish a shard before its complete-range checkpoint",
        ));
    }
    if staging_path.file_name().and_then(|value| value.to_str())
        != Some(range.directory_name().as_str())
    {
        return Err(invalid_bsm_shard(
            staging_path,
            "in-progress shard directory name changed before publication",
        ));
    }
    let final_path = output_dir
        .join(BSM_SHARD_DIRECTORY)
        .join(range.directory_name());
    if final_path.exists() {
        return Err(invalid_bsm_shard(
            &final_path,
            "refusing to replace an already published shard",
        ));
    }
    const MAX_RENAME_ATTEMPTS: usize = 20;
    for attempt in 0..MAX_RENAME_ATTEMPTS {
        match fs_retry::rename(staging_path, &final_path) {
            Ok(()) => break,
            Err(source)
                if source.kind() == std::io::ErrorKind::PermissionDenied
                    && !final_path.exists()
                    && attempt + 1 < MAX_RENAME_ATTEMPTS =>
            {
                let delay_ms = (5 * (attempt as u64 + 1)).min(50);
                thread::sleep(Duration::from_millis(delay_ms));
            }
            Err(source) => {
                return Err(CliError::OutputIo {
                    path: final_path.clone(),
                    source,
                });
            }
        }
    }
    validate_complete_bsm_shard(&final_path, range, sample_count, run_fingerprint)
}

fn write_sharded_stochastic_histories_to_directory(
    output_dir: &Path,
    context: &BsmRunContext<'_>,
    sampler: &biogeo_core::StochasticMapSampler<'_>,
) -> Result<ResolvedBsmExecution, CliError> {
    let runtime = context.runtime;
    let execution = context.execution;
    let shard_samples = execution
        .shard_samples
        .expect("sharded writer requires a resolved shard size");
    let run_fingerprint = bsm_run_fingerprint(context);
    let formatting = BsmFormattingContext::new(
        context.parsed_tree,
        context.parsed_ranges,
        context.states,
        context.model,
        context.runtime.output_level,
    )?;
    let mut layout = if runtime.resume {
        if !output_dir.is_dir() {
            return Err(CliError::MissingBsmOutputDirectory(
                output_dir.to_path_buf(),
            ));
        }
        validate_bsm_sharded_root_identity(
            output_dir,
            &run_fingerprint,
            bsm_stream_format(execution, runtime.output_level),
        )?;
        load_bsm_shard_layout(
            output_dir,
            runtime.sample_count,
            shard_samples,
            &run_fingerprint,
        )?
    } else {
        prepare_bsm_sharded_output_directory(
            output_dir,
            runtime.sample_count,
            shard_samples,
            &run_fingerprint,
        )?;
        BsmShardLayout {
            completed: Vec::new(),
            in_progress: None,
        }
    };
    ensure_bsm_reference_tables(output_dir, &formatting, context.model, runtime.resume)?;
    if layout
        .in_progress
        .as_ref()
        .is_some_and(|shard| shard.checkpoint.completed_samples == shard.range.end_exclusive)
    {
        let shard = layout
            .in_progress
            .take()
            .expect("checked in-progress shard must exist");
        let completed_shard = publish_bsm_shard(
            output_dir,
            &shard.path,
            shard.range,
            &shard.checkpoint,
            runtime.sample_count,
            &run_fingerprint,
        )?;
        layout.completed.push(completed_shard);
    }
    write_bsm_shard_manifest(
        output_dir,
        runtime.sample_count,
        shard_samples,
        &run_fingerprint,
        &layout.completed,
    )?;

    let mut completed_samples = layout.in_progress.as_ref().map_or_else(
        || {
            layout
                .completed
                .last()
                .map_or(0, |shard| shard.range.end_exclusive)
        },
        |shard| shard.checkpoint.completed_samples,
    );
    let mut completed_anagenetic_events = layout.in_progress.as_ref().map_or_else(
        || {
            layout.completed.last().map_or(0, |shard| {
                shard
                    .checkpoint
                    .completed_anagenetic_events
                    .expect("sharded output only writes v2 checkpoints")
            })
        },
        |shard| {
            shard
                .checkpoint
                .completed_anagenetic_events
                .expect("sharded output only writes v2 checkpoints")
        },
    );
    if let Some(progress) = context.interactive_progress {
        progress.set_completed_samples(completed_samples);
    }
    write_bsm_stream_metadata(
        output_dir,
        if completed_samples == runtime.sample_count {
            "complete"
        } else {
            "incomplete"
        },
        completed_samples,
        completed_anagenetic_events,
        context,
    )?;
    if completed_samples == runtime.sample_count {
        return Ok(execution);
    }

    loop {
        let range = BsmShardRange::for_start(
            (completed_samples / shard_samples) * shard_samples,
            shard_samples,
            runtime.sample_count,
        )
        .expect("incomplete sample prefix must belong to one fixed shard");
        let (mut writers, mut last_checkpoint, staging_path) = match layout.in_progress.take() {
            Some(in_progress) => {
                debug_assert_eq!(in_progress.range, range);
                let writers = BsmTableWriters::open_at_checkpoint(
                    &in_progress.path,
                    &in_progress.checkpoint,
                )?;
                (writers, in_progress.checkpoint, in_progress.path)
            }
            None => create_in_progress_bsm_shard(
                output_dir,
                range,
                completed_anagenetic_events,
                &run_fingerprint,
                runtime.output_level,
            )?,
        };

        let stream_result = sampler.try_for_each_map_indexed_parallel_range_with_options(
            completed_samples..range.end_exclusive,
            runtime.seed,
            bsm_parallel_options(
                execution,
                context.execution_control.as_ref(),
                completed_anagenetic_events,
            ),
            |sample_index, stochastic_history| {
                debug_assert_eq!(sample_index, completed_samples);
                let event_count = stochastic_history.anagenetic_event_count()?;
                let rows =
                    format_stochastic_history_rows(sample_index, stochastic_history, &formatting)?;
                writers.write_sample(&rows)?;
                completed_samples += 1;
                completed_anagenetic_events = completed_anagenetic_events
                    .checked_add(event_count)
                    .ok_or(biogeo_core::BsmError::AnageneticEventCountOverflow)?;
                if let Some(progress) = context.interactive_progress {
                    progress.set_completed_samples(completed_samples);
                }
                if completed_samples == range.end_exclusive
                    || completed_samples - last_checkpoint.completed_samples
                        >= execution.checkpoint_samples
                {
                    last_checkpoint = commit_bsm_checkpoint(
                        &mut writers,
                        &staging_path,
                        completed_samples,
                        completed_anagenetic_events,
                        &run_fingerprint,
                    )?;
                }
                Ok(())
            },
        );

        if let Err(error) = stream_result {
            let error = map_parallel_bsm_error(error);
            if let Some(status) = bsm_stop_status(&error) {
                if completed_samples > last_checkpoint.completed_samples {
                    match commit_bsm_checkpoint(
                        &mut writers,
                        &staging_path,
                        completed_samples,
                        completed_anagenetic_events,
                        &run_fingerprint,
                    ) {
                        Ok(checkpoint) => last_checkpoint = checkpoint,
                        Err(commit_error) => {
                            let committed_samples = last_checkpoint.completed_samples;
                            let committed_events = last_checkpoint
                                .completed_anagenetic_events
                                .expect("committed checkpoints must have an event count");
                            let recovery = match writers.rollback(&last_checkpoint) {
                                Ok(()) => commit_error,
                                Err(rollback_error) => CliError::BsmRecoveryFailed {
                                    original: Box::new(commit_error),
                                    recovery: Box::new(rollback_error),
                                },
                            };
                            let _ = write_bsm_stream_metadata(
                                output_dir,
                                "incomplete",
                                committed_samples,
                                committed_events,
                                context,
                            );
                            return Err(CliError::BsmRecoveryFailed {
                                original: Box::new(error),
                                recovery: Box::new(recovery),
                            });
                        }
                    }
                }
                let committed_samples = last_checkpoint.completed_samples;
                let committed_events = last_checkpoint
                    .completed_anagenetic_events
                    .expect("committed checkpoints must have an event count");
                write_bsm_stream_metadata(
                    output_dir,
                    status,
                    committed_samples,
                    committed_events,
                    context,
                )?;
                return Err(error);
            }

            let committed_samples = last_checkpoint.completed_samples;
            let committed_events = last_checkpoint
                .completed_anagenetic_events
                .expect("committed checkpoints must have an event count");
            if let Err(recovery) = writers.rollback(&last_checkpoint) {
                return Err(CliError::BsmRecoveryFailed {
                    original: Box::new(error),
                    recovery: Box::new(recovery),
                });
            }
            let _ = write_bsm_stream_metadata(
                output_dir,
                "incomplete",
                committed_samples,
                committed_events,
                context,
            );
            return Err(error);
        }

        debug_assert_eq!(completed_samples, range.end_exclusive);
        debug_assert_eq!(last_checkpoint.completed_samples, range.end_exclusive);
        drop(writers);
        let completed_shard = publish_bsm_shard(
            output_dir,
            &staging_path,
            range,
            &last_checkpoint,
            runtime.sample_count,
            &run_fingerprint,
        )?;
        layout.completed.push(completed_shard);
        write_bsm_shard_manifest(
            output_dir,
            runtime.sample_count,
            shard_samples,
            &run_fingerprint,
            &layout.completed,
        )?;
        let status = if completed_samples == runtime.sample_count {
            "complete"
        } else {
            "incomplete"
        };
        write_bsm_stream_metadata(
            output_dir,
            status,
            completed_samples,
            completed_anagenetic_events,
            context,
        )?;
        if completed_samples == runtime.sample_count {
            return Ok(execution);
        }
    }
}

#[derive(Clone, Debug)]
enum BsmRunIdentity {
    FixedRanges {
        use_ambiguities: bool,
    },
    AnalysisResult {
        format_version: String,
        fingerprint: String,
        tip_observation_model: String,
    },
}

#[derive(Clone, Debug)]
struct BsmRuntimeConfig {
    model_name: String,
    root_prior: RootPriorKind,
    sample_count: usize,
    seed: u64,
    output_level: BsmOutputLevel,
    resume: bool,
    interactive: bool,
    identity: BsmRunIdentity,
    parameter_metadata: String,
}

impl BsmRuntimeConfig {
    fn from_fixed(config: &FixedModelConfig) -> Self {
        let mut parameter_metadata = format!(
            "d\t{}\ne\t{}\nj\t{}\nmx01y\t{}\nmx01s\t{}\nmx01v\t{}\nmx01j\t{}\n",
            config.d,
            config.e,
            config.j,
            config.range_size.mx01y,
            config.range_size.mx01s,
            config.range_size.mx01v,
            config.range_size.mx01j,
        );
        if config.use_ambiguities {
            parameter_metadata.push_str("tip_observation_model\tambiguous_ranges\n");
        }
        Self {
            model_name: config.preset.model_name(config.j).to_string(),
            root_prior: config.root_prior,
            sample_count: config.bsm_samples,
            seed: config.seed,
            output_level: config.bsm_output_level,
            resume: config.bsm_resume,
            interactive: config.bsm_interactive,
            identity: BsmRunIdentity::FixedRanges {
                use_ambiguities: config.use_ambiguities,
            },
            parameter_metadata,
        }
    }

    fn from_analysis(
        config: &ParameterBsmConfig,
        loaded: &analysis_result::LoadedAnalysisResult,
        resolved: &biogeo_core::ResolvedParameters,
    ) -> Self {
        let mut parameter_metadata = format!(
            "analysis_result_format\t{}\nanalysis_result_fingerprint\t{}\nanalysis_result_source_mode\t{}\ntip_observation_model\t{}\nsource_parameters_fingerprint\t{}\nresolved_parameters_fingerprint\t{}\n",
            loaded.format_version,
            loaded.fingerprint,
            loaded.manifest.mode,
            loaded.manifest.tip_observation_model,
            analysis_result::stable_fingerprint(loaded.source_parameters.as_bytes()),
            analysis_result::stable_fingerprint(loaded.resolved_parameters.as_bytes()),
        );
        if let Some(optimization) = loaded.manifest.optimization {
            writeln!(
                parameter_metadata,
                "source_optimization_converged\t{}",
                optimization.converged
            )
            .unwrap();
            writeln!(
                parameter_metadata,
                "source_optimization_iterations\t{}",
                optimization.iterations
            )
            .unwrap();
            writeln!(
                parameter_metadata,
                "source_optimization_evaluations\t{}",
                optimization.evaluations
            )
            .unwrap();
            writeln!(
                parameter_metadata,
                "source_optimization_starts\t{}",
                optimization.starts
            )
            .unwrap();
            writeln!(
                parameter_metadata,
                "source_optimization_converged_starts\t{}",
                optimization.converged_starts
            )
            .unwrap();
        }
        for (name, value) in resolved.iter() {
            writeln!(parameter_metadata, "parameter_{name}\t{value}").unwrap();
        }
        Self {
            model_name: "BIOGEOBEARS_LIKE_CONFIGURABLE".to_string(),
            root_prior: match loaded.manifest.root_prior.as_str() {
                "flat" => RootPriorKind::Flat,
                "equal" => RootPriorKind::Equal,
                _ => unreachable!("analysis result loader validates root_prior"),
            },
            sample_count: config.bsm_samples,
            seed: config.seed,
            output_level: config.bsm_output_level,
            resume: config.bsm_resume,
            interactive: config.bsm_interactive,
            identity: BsmRunIdentity::AnalysisResult {
                format_version: loaded.format_version.clone(),
                fingerprint: loaded.fingerprint.clone(),
                tip_observation_model: loaded.manifest.tip_observation_model.clone(),
            },
            parameter_metadata,
        }
    }
}

#[derive(Clone)]
struct BsmRunContext<'a> {
    runtime: &'a BsmRuntimeConfig,
    tree_input: &'a str,
    tree_name: Option<&'a str>,
    fixed_ranges_input: Option<&'a str>,
    parsed_tree: &'a biogeo_core::ParsedNewickTree,
    parsed_ranges: &'a biogeo_core::ParsedTipRanges,
    states: &'a biogeo_core::StateSpace,
    model: &'a biogeo_core::ModelConfig,
    result: &'a biogeo_core::PruningResult,
    execution: ResolvedBsmExecution,
    execution_control: Option<biogeo_core::StochasticMapExecutionControl>,
    interactive_progress: Option<&'a BsmInteractiveProgress>,
}

fn stable_bsm_fingerprint(bytes: &[u8]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn bsm_run_fingerprint(context: &BsmRunContext<'_>) -> String {
    let mut identity = String::new();
    writeln!(
        identity,
        "stream_format={}",
        bsm_stream_format(context.execution, context.runtime.output_level)
    )
    .expect("writing BSM identity to a String cannot fail");
    if let Some(shard_samples) = context.execution.shard_samples {
        writeln!(identity, "shard_samples={shard_samples}")
            .expect("writing BSM identity to a String cannot fail");
    }
    writeln!(
        identity,
        "rng_protocol={}",
        biogeo_core::INDEXED_BSM_RNG_PROTOCOL
    )
    .expect("writing BSM identity to a String cannot fail");
    writeln!(identity, "tree_bytes={}", context.tree_input.len())
        .expect("writing BSM identity to a String cannot fail");
    identity.push_str(context.tree_input);
    if let Some(tree_name) = context.tree_name {
        writeln!(identity, "\ntree_name_bytes={}", tree_name.len())
            .expect("writing BSM identity to a String cannot fail");
        identity.push_str(tree_name);
    }
    writeln!(
        identity,
        "\nmin_branch_length_bits={:016x}",
        context
            .parsed_tree
            .tree
            .direct_ancestor_threshold()
            .to_bits()
    )
    .expect("writing BSM identity to a String cannot fail");
    writeln!(
        identity,
        "direct_ancestor_hook_edges={:?}",
        context.parsed_tree.tree.direct_ancestor_hook_edges()
    )
    .expect("writing BSM identity to a String cannot fail");
    match &context.runtime.identity {
        BsmRunIdentity::FixedRanges { use_ambiguities } => {
            let ranges_input = context
                .fixed_ranges_input
                .expect("fixed BSM identity requires range-table bytes");
            writeln!(identity, "\nranges_bytes={}", ranges_input.len())
                .expect("writing BSM identity to a String cannot fail");
            identity.push_str(ranges_input);
            if *use_ambiguities {
                identity.push_str("\ntip_observation_model=ambiguous_ranges");
            }
        }
        BsmRunIdentity::AnalysisResult {
            format_version,
            fingerprint,
            tip_observation_model,
        } => {
            writeln!(identity, "\nanalysis_result_format={}", format_version)
                .expect("writing BSM identity to a String cannot fail");
            writeln!(identity, "analysis_result_fingerprint={fingerprint}")
                .expect("writing BSM identity to a String cannot fail");
            writeln!(identity, "tip_observation_model={tip_observation_model}")
                .expect("writing BSM identity to a String cannot fail");
        }
    }
    writeln!(identity, "\nmodel={:?}", context.model)
        .expect("writing BSM identity to a String cannot fail");
    writeln!(
        identity,
        "state_space={},{},{}",
        context.states.num_areas(),
        context.states.max_range_size(),
        context.states.include_null_range()
    )
    .expect("writing BSM identity to a String cannot fail");
    writeln!(
        identity,
        "root_prior={}",
        context.runtime.root_prior.as_str()
    )
    .expect("writing BSM identity to a String cannot fail");
    writeln!(identity, "seed={}", context.runtime.seed)
        .expect("writing BSM identity to a String cannot fail");
    writeln!(identity, "samples={}", context.runtime.sample_count)
        .expect("writing BSM identity to a String cannot fail");
    writeln!(
        identity,
        "max_events_per_sample={}",
        format_optional_limit(context.execution.max_events_per_sample)
    )
    .expect("writing BSM identity to a String cannot fail");
    stable_bsm_fingerprint(identity.as_bytes())
}

fn write_bsm_stream_metadata(
    output_dir: &Path,
    status: &str,
    completed_samples: usize,
    completed_anagenetic_events: usize,
    context: &BsmRunContext<'_>,
) -> Result<(), CliError> {
    let runtime = context.runtime;
    let parsed_tree = context.parsed_tree;
    let parsed_ranges = context.parsed_ranges;
    let states = context.states;
    let result = context.result;
    let execution = context.execution;
    let run_fingerprint = bsm_run_fingerprint(context);
    let completed_shards = execution.shard_samples.map(|shard_samples| {
        bsm_completed_shards(completed_samples, runtime.sample_count, shard_samples)
    });
    let total_shards = execution
        .shard_samples
        .map(|shard_samples| bsm_total_shards(runtime.sample_count, shard_samples));
    let manifest_file = if execution.shard_samples.is_some() {
        BSM_SHARD_MANIFEST_FILE
    } else {
        "none"
    };
    let reference_tables = if runtime.output_level.is_v2() {
        "areas.tsv,states.tsv,nodes.tsv,edges.tsv,periods.tsv"
    } else {
        "none"
    };
    let path = output_dir.join("metadata.tsv");
    let mut metadata = format!(
        "key\tvalue\n\
format\t{}\n\
output_level\t{}\n\
path_details\t{}\n\
sparse_occupancy\t{}\n\
reference_tables\t{reference_tables}\n\
status\t{status}\n\
completed_samples\t{completed_samples}\n\
completed_anagenetic_events\t{completed_anagenetic_events}\n\
run_fingerprint\t{run_fingerprint}\n\
model\t{}\n\
lnL\t{:.15}\n\
seed\t{}\n\
samples\t{}\n\
rng_protocol\t{}\n\
available_parallelism\t{}\n\
threads\t{}\n\
max_in_flight\t{}\n\
interactive_control\t{}\n\
checkpoint_samples\t{}\n\
shard_samples\t{}\n\
completed_shards\t{}\n\
total_shards\t{}\n\
manifest_file\t{}\n\
max_events_per_sample\t{}\n\
max_events_total\t{}\n\
memory_budget_mb\t{}\n\
memory_budget_scope\tcompleted_history_window\n\
memory_budget_excludes\tworker_sampling_scratch,shared_caches,writer_buffers,allocator_overhead\n\
retained_bytes_per_sample_upper_bound\t{}\n\
buffered_history_bytes_upper_bound\t{}\n\
time_limit_seconds\t{}\n\
states\t{}\n\
areas\t{}\n\
tips\t{}\n\
max_range_size\t{}\n\
include_null_range\t{}\n\
root_prior\t{}\n\
min_branch_length\t{:.17}\n\
direct_ancestor_nodes\t{}\n\
direct_ancestor_hook_edges\t{}\n",
        bsm_stream_format(execution, runtime.output_level),
        runtime.output_level.as_str(),
        runtime.output_level.includes_path_details(),
        runtime.output_level.is_compact(),
        runtime.model_name,
        result.log_likelihood,
        runtime.seed,
        runtime.sample_count,
        biogeo_core::INDEXED_BSM_RNG_PROTOCOL,
        execution.available_parallelism,
        execution.threads,
        execution.max_in_flight,
        runtime.interactive,
        execution.checkpoint_samples,
        format_optional_limit(execution.shard_samples),
        format_optional_estimate(completed_shards),
        format_optional_estimate(total_shards),
        manifest_file,
        format_optional_limit(execution.max_events_per_sample),
        format_optional_limit(execution.max_events_total),
        format_optional_limit(execution.memory_budget_mb),
        format_optional_estimate(execution.retained_bytes_per_sample_upper_bound),
        format_optional_estimate(execution.buffered_history_bytes_upper_bound),
        format_optional_duration(execution.time_limit),
        states.len(),
        parsed_ranges.area_names.len(),
        parsed_tree.tip_labels.len(),
        states.max_range_size(),
        states.include_null_range(),
        runtime.root_prior.as_str(),
        parsed_tree.tree.direct_ancestor_threshold(),
        parsed_tree
            .tree
            .postorder_internal_nodes()
            .iter()
            .filter(|node| parsed_tree.tree.is_direct_ancestor_node(**node))
            .count(),
        parsed_tree.tree.direct_ancestor_hook_edges().len(),
    );
    metadata.push_str(&runtime.parameter_metadata);
    fs::write(&path, metadata).map_err(|source| CliError::OutputIo { path, source })
}

fn map_parallel_bsm_error(error: biogeo_core::StochasticMapParallelError<CliError>) -> CliError {
    match error {
        biogeo_core::StochasticMapParallelError::ZeroThreads => {
            CliError::NonPositiveBsmOption("--bsm-threads")
        }
        biogeo_core::StochasticMapParallelError::ZeroMaxInFlight => {
            CliError::NonPositiveBsmOption("--bsm-max-in-flight")
        }
        biogeo_core::StochasticMapParallelError::MaxInFlightBelowThreads {
            threads,
            max_in_flight,
        } => CliError::BsmMaxInFlightBelowThreads {
            threads,
            max_in_flight,
        },
        biogeo_core::StochasticMapParallelError::Planning(error) => {
            map_bsm_parallel_plan_error(error)
        }
        biogeo_core::StochasticMapParallelError::InvalidSampleRange { start, end } => {
            CliError::InvalidBsmSampleRange { start, end }
        }
        biogeo_core::StochasticMapParallelError::ThreadPoolBuild(error) => {
            CliError::BsmThreadPoolBuild(error.to_string())
        }
        biogeo_core::StochasticMapParallelError::Preparation(error) => error.into(),
        biogeo_core::StochasticMapParallelError::Sampling {
            sample_index,
            source,
        } => CliError::BsmSampling {
            sample_index,
            source,
        },
        biogeo_core::StochasticMapParallelError::TotalAnageneticEventLimitExceeded {
            sample_index,
            limit,
            completed,
            attempted,
        } => CliError::BsmTotalEventLimitExceeded {
            sample_index,
            limit,
            completed,
            attempted,
        },
        biogeo_core::StochasticMapParallelError::Stopped {
            sample_index,
            reason: biogeo_core::StochasticMapStopReason::Cancelled,
        } => CliError::BsmCancelled { sample_index },
        biogeo_core::StochasticMapParallelError::Stopped {
            sample_index,
            reason: biogeo_core::StochasticMapStopReason::DeadlineExceeded,
        } => CliError::BsmTimeLimitExceeded { sample_index },
        biogeo_core::StochasticMapParallelError::Consumer { source, .. } => source,
    }
}

fn map_bsm_parallel_plan_error(error: biogeo_core::StochasticMapParallelPlanError) -> CliError {
    match error {
        biogeo_core::StochasticMapParallelPlanError::ZeroThreads => {
            CliError::NonPositiveBsmOption("--bsm-threads")
        }
        biogeo_core::StochasticMapParallelPlanError::ZeroMaxInFlight => {
            CliError::NonPositiveBsmOption("--bsm-max-in-flight")
        }
        biogeo_core::StochasticMapParallelPlanError::MaxInFlightBelowThreads {
            threads,
            max_in_flight,
        } => CliError::BsmMaxInFlightBelowThreads {
            threads,
            max_in_flight,
        },
        biogeo_core::StochasticMapParallelPlanError::MemoryBudgetRequiresPerMapEventLimit {
            ..
        } => CliError::BsmMemoryBudgetRequiresPerSampleEventLimit,
        biogeo_core::StochasticMapParallelPlanError::MemoryBudgetTooSmall {
            budget_bytes,
            minimum_bytes,
        } => CliError::BsmMemoryBudgetTooSmall {
            budget_bytes,
            minimum_bytes,
        },
        biogeo_core::StochasticMapParallelPlanError::RetainedHistorySizeOverflow => {
            CliError::BsmRetainedHistorySizeOverflow
        }
    }
}

fn bsm_stop_status(error: &CliError) -> Option<&'static str> {
    match error {
        CliError::BsmCancelled { .. } => Some("cancelled"),
        CliError::BsmTimeLimitExceeded { .. } => Some("time_limit"),
        CliError::BsmTotalEventLimitExceeded { .. } => Some("event_limit"),
        _ => None,
    }
}

fn write_stochastic_histories_to_directory(
    output_dir: &Path,
    context: &BsmRunContext<'_>,
) -> Result<ResolvedBsmExecution, CliError> {
    let engine = biogeo_core::LikelihoodEngine::new(
        &context.parsed_tree.tree,
        context.states,
        context.runtime.root_prior.to_core(),
    );
    let sampler = engine.prepare_stochastic_map_sampler(context.model, context.result)?;
    let parallel_options =
        bsm_parallel_options(context.execution, context.execution_control.as_ref(), 0);
    let plan = sampler
        .plan_indexed_parallel(context.runtime.sample_count, &parallel_options)
        .map_err(map_bsm_parallel_plan_error)?;
    let execution = context.execution.with_parallel_plan(plan);
    let mut planned_context = context.clone();
    planned_context.execution = execution;
    if execution.shard_samples.is_some() {
        write_sharded_stochastic_histories_to_directory(output_dir, &planned_context, &sampler)
    } else {
        write_monolithic_stochastic_histories_to_directory(output_dir, &planned_context, &sampler)
    }
}

fn write_monolithic_stochastic_histories_to_directory(
    output_dir: &Path,
    context: &BsmRunContext<'_>,
    sampler: &biogeo_core::StochasticMapSampler<'_>,
) -> Result<ResolvedBsmExecution, CliError> {
    let runtime = context.runtime;
    let execution = context.execution;
    let parsed_tree = context.parsed_tree;
    let parsed_ranges = context.parsed_ranges;
    let states = context.states;
    let run_fingerprint = bsm_run_fingerprint(context);
    let formatting = BsmFormattingContext::new(
        parsed_tree,
        parsed_ranges,
        states,
        context.model,
        context.runtime.output_level,
    )?;
    let (mut writers, mut last_checkpoint) = if runtime.resume {
        if !output_dir.is_dir() {
            return Err(CliError::MissingBsmOutputDirectory(
                output_dir.to_path_buf(),
            ));
        }
        let checkpoint = load_latest_bsm_checkpoint(output_dir, runtime.sample_count)?;
        if checkpoint.run_fingerprint != run_fingerprint {
            return Err(CliError::BsmResumeFingerprintMismatch {
                expected: checkpoint.run_fingerprint,
                actual: run_fingerprint,
            });
        }
        let writers = BsmTableWriters::open_at_checkpoint(output_dir, &checkpoint)?;
        (writers, checkpoint)
    } else {
        prepare_bsm_output_directory(output_dir)?;
        let mut writers = BsmTableWriters::create(output_dir, runtime.output_level)?;
        let checkpoint = commit_bsm_checkpoint(&mut writers, output_dir, 0, 0, &run_fingerprint)?;
        (writers, checkpoint)
    };
    ensure_bsm_reference_tables(output_dir, &formatting, context.model, runtime.resume)?;
    let mut completed_anagenetic_events = last_checkpoint
        .completed_anagenetic_events
        .expect("loaded and new checkpoints must have an event count");
    if let Some(progress) = context.interactive_progress {
        progress.set_completed_samples(last_checkpoint.completed_samples);
    }
    write_bsm_stream_metadata(
        output_dir,
        "incomplete",
        last_checkpoint.completed_samples,
        completed_anagenetic_events,
        context,
    )?;
    if last_checkpoint.completed_samples == runtime.sample_count {
        write_bsm_stream_metadata(
            output_dir,
            "complete",
            last_checkpoint.completed_samples,
            completed_anagenetic_events,
            context,
        )?;
        return Ok(execution);
    }

    let mut completed_samples = last_checkpoint.completed_samples;
    let stream_result = sampler.try_for_each_map_indexed_parallel_range_with_options(
        completed_samples..runtime.sample_count,
        runtime.seed,
        bsm_parallel_options(
            execution,
            context.execution_control.as_ref(),
            completed_anagenetic_events,
        ),
        |sample_index, stochastic_history| {
            debug_assert_eq!(sample_index, completed_samples);
            let event_count = stochastic_history.anagenetic_event_count()?;
            let rows =
                format_stochastic_history_rows(sample_index, stochastic_history, &formatting)?;
            writers.write_sample(&rows)?;
            completed_samples += 1;
            completed_anagenetic_events = completed_anagenetic_events
                .checked_add(event_count)
                .ok_or(biogeo_core::BsmError::AnageneticEventCountOverflow)?;
            if let Some(progress) = context.interactive_progress {
                progress.set_completed_samples(completed_samples);
            }
            if completed_samples == runtime.sample_count
                || completed_samples - last_checkpoint.completed_samples
                    >= execution.checkpoint_samples
            {
                last_checkpoint = commit_bsm_checkpoint(
                    &mut writers,
                    output_dir,
                    completed_samples,
                    completed_anagenetic_events,
                    &run_fingerprint,
                )?;
            }
            Ok(())
        },
    );
    if let Err(error) = stream_result {
        let error = map_parallel_bsm_error(error);
        if let Some(status) = bsm_stop_status(&error) {
            if completed_samples > last_checkpoint.completed_samples {
                match commit_bsm_checkpoint(
                    &mut writers,
                    output_dir,
                    completed_samples,
                    completed_anagenetic_events,
                    &run_fingerprint,
                ) {
                    Ok(checkpoint) => last_checkpoint = checkpoint,
                    Err(commit_error) => {
                        let committed_samples = last_checkpoint.completed_samples;
                        let committed_events = last_checkpoint
                            .completed_anagenetic_events
                            .expect("committed checkpoints must have an event count");
                        let recovery = match writers.rollback(&last_checkpoint) {
                            Ok(()) => commit_error,
                            Err(rollback_error) => CliError::BsmRecoveryFailed {
                                original: Box::new(commit_error),
                                recovery: Box::new(rollback_error),
                            },
                        };
                        let _ = write_bsm_stream_metadata(
                            output_dir,
                            "incomplete",
                            committed_samples,
                            committed_events,
                            context,
                        );
                        return Err(CliError::BsmRecoveryFailed {
                            original: Box::new(error),
                            recovery: Box::new(recovery),
                        });
                    }
                }
            }
            let committed_samples = last_checkpoint.completed_samples;
            let committed_events = last_checkpoint
                .completed_anagenetic_events
                .expect("committed checkpoints must have an event count");
            let final_status = if committed_samples == runtime.sample_count {
                "complete"
            } else {
                status
            };
            write_bsm_stream_metadata(
                output_dir,
                final_status,
                committed_samples,
                committed_events,
                context,
            )?;
            return if final_status == "complete" {
                Ok(execution)
            } else {
                Err(error)
            };
        }
        let committed_samples = last_checkpoint.completed_samples;
        let committed_events = last_checkpoint
            .completed_anagenetic_events
            .expect("committed checkpoints must have an event count");
        if let Err(recovery) = writers.rollback(&last_checkpoint) {
            return Err(CliError::BsmRecoveryFailed {
                original: Box::new(error),
                recovery: Box::new(recovery),
            });
        }
        let _ = write_bsm_stream_metadata(
            output_dir,
            "incomplete",
            committed_samples,
            committed_events,
            context,
        );
        return Err(error);
    }
    debug_assert_eq!(last_checkpoint.completed_samples, runtime.sample_count);
    write_bsm_stream_metadata(
        output_dir,
        "complete",
        completed_samples,
        completed_anagenetic_events,
        context,
    )?;
    Ok(execution)
}

fn node_label(parsed_tree: &biogeo_core::ParsedNewickTree, node: usize) -> String {
    parsed_tree
        .node_label(node)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("node_{node}"))
}

fn parse_selected_tree_input(
    input: &str,
    tree_name: Option<&str>,
) -> Result<biogeo_core::ParsedTreeInput, CliError> {
    parse_selected_tree_input_with_fill(input, tree_name, None)
}

fn parse_selected_tree_input_with_fill(
    input: &str,
    tree_name: Option<&str>,
    missing_branch_length_fill: Option<f64>,
) -> Result<biogeo_core::ParsedTreeInput, CliError> {
    let options = biogeo_core::NewickParseOptions {
        missing_branch_lengths: missing_branch_length_fill.map_or(
            biogeo_core::MissingBranchLengthPolicy::Reject,
            biogeo_core::MissingBranchLengthPolicy::Fill,
        ),
    };
    match tree_name {
        Some(tree_name) => Ok(biogeo_core::parse_tree_input_named_with_options(
            input, tree_name, options,
        )?),
        None => Ok(biogeo_core::parse_tree_input_with_options(input, options)?),
    }
}

fn validate_tip_state_constraints_for_cli(
    parsed_tree: &biogeo_core::ParsedNewickTree,
    states: &biogeo_core::StateSpace,
    model: &biogeo_core::ModelConfig,
    tip_likelihoods: &[biogeo_core::TipLikelihood],
) -> Result<(), CliError> {
    let violations = biogeo_core::tip_likelihood_state_constraint_violations(
        &parsed_tree.tree,
        states,
        model,
        tip_likelihoods,
    )
    .map_err(|error| CliError::Dec(biogeo_core::DecAnalysisError::Anagenesis(error)))?;
    if violations.is_empty() {
        return Ok(());
    }
    let tips = violations
        .into_iter()
        .map(|violation| {
            (
                violation.node,
                node_label(parsed_tree, violation.node),
                violation.stratum_index,
            )
        })
        .collect();
    Err(CliError::TipStateConstraintViolations { tips })
}

fn parse_tip_ranges_input(
    input: &str,
    tree: &biogeo_core::ParsedNewickTree,
    use_ambiguities: bool,
) -> Result<biogeo_core::ParsedTipRanges, CliError> {
    let canonical =
        legacy_import::maybe_import_range_table(input, None)?.map(|table| table.to_tsv());
    let input = canonical.as_deref().unwrap_or(input);
    if use_ambiguities {
        Ok(biogeo_core::parse_tip_ranges_table_with_ambiguities(
            input, tree,
        )?)
    } else {
        Ok(biogeo_core::parse_tip_ranges_table(input, tree)?)
    }
}

fn parse_analysis_tree(
    input: &str,
    tree_name: Option<&str>,
    min_branch_length: f64,
) -> Result<biogeo_core::ParsedNewickTree, CliError> {
    parse_analysis_tree_with_fill(input, tree_name, min_branch_length, None)
}

fn parse_analysis_tree_with_fill(
    input: &str,
    tree_name: Option<&str>,
    min_branch_length: f64,
    missing_branch_length_fill: Option<f64>,
) -> Result<biogeo_core::ParsedNewickTree, CliError> {
    parse_selected_tree_input_with_fill(input, tree_name, missing_branch_length_fill)?
        .parsed_tree
        .with_direct_ancestor_hooks_below(min_branch_length)
        .map_err(|error| CliError::Newick(error.into()))
}

fn clade_label(parsed_tree: &biogeo_core::ParsedNewickTree, node: usize) -> String {
    let mut labels = Vec::new();
    collect_descendant_tip_labels(parsed_tree, node, &mut labels);
    labels.sort();
    labels.join("+")
}

fn collect_descendant_tip_labels(
    parsed_tree: &biogeo_core::ParsedNewickTree,
    node: usize,
    labels: &mut Vec<String>,
) {
    if let Some(tip) = parsed_tree.tip_labels.iter().find(|tip| tip.node == node) {
        labels.push(tip.label.clone());
        return;
    }

    if let Some(children) = parsed_tree.tree.children(node) {
        for child in children {
            collect_descendant_tip_labels(parsed_tree, child.node, labels);
        }
    }
}

fn range_label(state: biogeo_core::AreaSet, area_names: &[String]) -> String {
    if state.is_empty() {
        return "null".to_string();
    }

    let mut names = Vec::new();
    for (area_index, area_name) in area_names.iter().enumerate() {
        if state.contains(area_index as u8) {
            names.push(area_name.as_str());
        }
    }

    names.join("+")
}

fn parse_command(args: Vec<String>) -> Result<Command, CliError> {
    if args.is_empty() || args[0] == "-h" || args[0] == "--help" {
        return Ok(Command::Help);
    }
    if args[0] == "-V" || args[0] == "--version" {
        if args.len() == 1 {
            return Ok(Command::Version);
        }
        return Err(CliError::UnexpectedArgument(args[1].clone()));
    }

    let mut iter = args.into_iter();
    let command = iter
        .next()
        .expect("non-empty args should include command name");
    let command_args = iter.collect::<Vec<_>>();
    if command_args
        .iter()
        .any(|argument| matches!(argument.as_str(), "-h" | "--help"))
    {
        return if cli_help::is_known_command(&command) {
            Ok(Command::TopicHelp(command))
        } else {
            Err(CliError::UnknownCommand(command))
        };
    }
    let ExtractedTreeOptions {
        remaining,
        min_branch_length,
        missing_branch_length_fill,
        tree_name,
        use_ambiguities,
    } = extract_tree_options(command_args)?;
    let parsed = match command.as_str() {
        "engine-info" => parse_no_option_command(remaining, Command::EngineInfo),
        "convert-tree" => parse_convert_tree_args(remaining),
        "convert-ranges" => parse_convert_ranges_args(remaining),
        "convert-biogeobears-strata" => parse_convert_biogeobears_strata_args(remaining),
        "fossil-place" => parse_fossil_place_args(remaining),
        "validate-inputs" => parse_validate_inputs_args(remaining),
        "dec" => parse_fixed_args(remaining, FixedPreset::Dec),
        "divalike" => parse_fixed_args(remaining, FixedPreset::DivaLike),
        "bayarealike" => parse_fixed_args(remaining, FixedPreset::BayAreaLike),
        "dec-optimize" => parse_de_optimize_args(remaining, FixedPreset::Dec),
        "divalike-optimize" => parse_de_optimize_args(remaining, FixedPreset::DivaLike),
        "bayarealike-optimize" => parse_de_optimize_args(remaining, FixedPreset::BayAreaLike),
        "dec-x-optimize" => parse_exponent_optimize_args(remaining, ExponentKind::GeographicX),
        "dec-n-optimize" => parse_exponent_optimize_args(remaining, ExponentKind::EnvironmentN),
        "dec-u-optimize" => parse_exponent_optimize_args(remaining, ExponentKind::AreaSizeU),
        "dec-xnu-optimize" => parse_xnu_optimize_args(remaining),
        "dec-xn-profile" => parse_pair_profile_args(remaining, ProfilePair::Xn),
        "dec-xu-profile" => parse_pair_profile_args(remaining, ProfilePair::Xu),
        "dec-nu-profile" => parse_pair_profile_args(remaining, ProfilePair::Nu),
        "decj-optimize" => parse_decj_optimize_args(remaining, FixedPreset::Dec),
        "divalikej-optimize" => parse_decj_optimize_args(remaining, FixedPreset::DivaLike),
        "bayarealikej-optimize" => parse_decj_optimize_args(remaining, FixedPreset::BayAreaLike),
        "parameter-template" => parse_parameter_template_args(remaining),
        "analysis-template" => parse_analysis_template_args(remaining),
        "analysis-plan" => parse_analysis_plan_args(remaining),
        "analysis-run" => parse_analysis_run_args(remaining),
        "analysis-workflow" => parse_analysis_workflow_args(remaining),
        "model-workflow-plan" => parse_model_workflow_plan_args(remaining),
        "model-workflow" => parse_model_workflow_args(remaining),
        "model-evaluate" => parse_parameter_model_args(remaining, ParameterRunMode::Evaluate),
        "model-optimize" => parse_parameter_model_args(remaining, ParameterRunMode::Optimize),
        "model-batch" => parse_parameter_batch_args(remaining),
        "dataset-batch" => parse_dataset_batch_args(remaining),
        "model-bsm" => parse_parameter_bsm_args(remaining),
        "bsm-inspect" => parse_bsm_inspect_args(remaining),
        "analysis-result-inspect" => parse_analysis_result_inspect_args(remaining),
        "analysis-result-migrate" => parse_analysis_result_migrate_args(remaining),
        "input-bundle-inspect" => parse_input_bundle_inspect_args(remaining),
        _ => Err(CliError::UnknownCommand(command)),
    }?;
    let parsed = match min_branch_length {
        Some(value) => parsed.with_min_branch_length(value),
        None => Ok(parsed),
    }?;
    let parsed = match missing_branch_length_fill {
        Some(value) => parsed.with_missing_branch_length_fill(value),
        None => Ok(parsed),
    }?;
    let parsed = match tree_name {
        Some(tree_name) => parsed.with_tree_name(tree_name),
        None => Ok(parsed),
    }?;
    if use_ambiguities {
        parsed.with_ambiguities()
    } else {
        Ok(parsed)
    }
}

fn parse_no_option_command(args: Vec<String>, command: Command) -> Result<Command, CliError> {
    match args.first().map(String::as_str) {
        None => Ok(command),
        Some("-h" | "--help") if args.len() == 1 => Ok(Command::Help),
        Some(option) if option.starts_with('-') => Err(CliError::UnknownOption(option.to_string())),
        Some(argument) => Err(CliError::UnexpectedArgument(argument.to_string())),
    }
}

fn parse_convert_ranges_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut ranges_path = None;
    let mut input_format = legacy_import::RangeSourceFormat::Auto;
    let mut taxon_column = None;
    let mut taxon_map_path = None;
    let mut area_map_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--input-format" => {
                input_format = legacy_import::RangeSourceFormat::parse(&take_value(
                    &mut iter,
                    "--input-format",
                )?)?
            }
            "--taxon-column" => taxon_column = Some(take_value(&mut iter, "--taxon-column")?),
            "--taxon-map" => {
                taxon_map_path = Some(PathBuf::from(take_value(&mut iter, "--taxon-map")?))
            }
            "--area-map" => {
                area_map_path = Some(PathBuf::from(take_value(&mut iter, "--area-map")?))
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::ConvertRanges(ConvertRangesConfig {
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        input_format,
        taxon_column,
        taxon_map_path,
        area_map_path,
    }))
}

fn parse_convert_biogeobears_strata_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut time_boundaries_path = None;
    let mut dispersal_matrices_path = None;
    let mut adjacency_matrices_path = None;
    let mut adjacency_range_rule = legacy_import::AdjacencyRangeRule::AllPairs;
    let mut max_range_size = None;
    let mut output_dir = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--time-boundaries" => {
                time_boundaries_path =
                    Some(PathBuf::from(take_value(&mut iter, "--time-boundaries")?))
            }
            "--dispersal-matrices" => {
                dispersal_matrices_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-matrices",
                )?))
            }
            "--adjacency-matrices" => {
                adjacency_matrices_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--adjacency-matrices",
                )?))
            }
            "--adjacency-range-rule" => {
                adjacency_range_rule = legacy_import::AdjacencyRangeRule::parse(&take_value(
                    &mut iter,
                    "--adjacency-range-rule",
                )?)?
            }
            "--max-range-size" => {
                max_range_size = Some(parse_usize(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--output-dir" => {
                output_dir = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?))
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::ConvertBioGeoBearsStrata(
        ConvertBioGeoBearsStrataConfig {
            time_boundaries_path: time_boundaries_path
                .ok_or(CliError::MissingRequired("--time-boundaries"))?,
            dispersal_matrices_path,
            adjacency_matrices_path,
            adjacency_range_rule,
            max_range_size,
            output_dir: output_dir.ok_or(CliError::MissingRequired("--output-dir"))?,
        },
    ))
}

fn parse_convert_tree_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::ConvertTree(ConvertTreeConfig {
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        missing_branch_length_fill: None,
    }))
}

fn parse_fossil_place_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut manifest_path = None;
    let mut output_dir = None;
    let mut replicates = 1;
    let mut seed = 1;
    let mut direct_ancestor_hook_length = 1e-7;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--manifest" => {
                manifest_path = Some(PathBuf::from(take_value(&mut iter, "--manifest")?))
            }
            "--output-dir" => {
                output_dir = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?))
            }
            "--replicates" => {
                replicates =
                    parse_positive_usize("--replicates", take_value(&mut iter, "--replicates")?)?
            }
            "--seed" => seed = parse_u64("--seed", take_value(&mut iter, "--seed")?)?,
            "--direct-ancestor-hook-length" => {
                direct_ancestor_hook_length = parse_float(
                    "--direct-ancestor-hook-length",
                    take_value(&mut iter, "--direct-ancestor-hook-length")?,
                )?
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::FossilPlace(
        fossil_placement::FossilPlacementRunConfig {
            tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
            tree_name: None,
            manifest_path: manifest_path.ok_or(CliError::MissingRequired("--manifest"))?,
            output_dir: output_dir.ok_or(CliError::MissingRequired("--output-dir"))?,
            replicates,
            seed,
            direct_ancestor_hook_length,
        },
    ))
}

fn parse_validate_inputs_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut tip_age_tolerance = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--tip-age-tolerance" => {
                let value = parse_float(
                    "--tip-age-tolerance",
                    take_value(&mut iter, "--tip-age-tolerance")?,
                )?;
                if !value.is_finite() || value < 0.0 {
                    return Err(CliError::InvalidTipAgeTolerance(value));
                }
                tip_age_tolerance = Some(value);
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }

    Ok(Command::ValidateInputs(ValidateInputsConfig {
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        min_branch_length: 0.0,
        missing_branch_length_fill: None,
        tip_age_tolerance,
    }))
}

struct ExtractedTreeOptions {
    remaining: Vec<String>,
    min_branch_length: Option<f64>,
    missing_branch_length_fill: Option<f64>,
    tree_name: Option<String>,
    use_ambiguities: bool,
}

fn extract_tree_options(args: Vec<String>) -> Result<ExtractedTreeOptions, CliError> {
    let mut remaining = Vec::with_capacity(args.len());
    let mut min_branch_length = None;
    let mut missing_branch_length_fill = None;
    let mut tree_name = None;
    let mut use_ambiguities = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        if arg == "--min-branch-length" {
            if min_branch_length.is_some() {
                return Err(CliError::DuplicateOption("--min-branch-length"));
            }
            let value = parse_float(
                "--min-branch-length",
                take_value(&mut iter, "--min-branch-length")?,
            )?;
            if !value.is_finite() || value < 0.0 {
                return Err(CliError::InvalidMinBranchLength(value));
            }
            min_branch_length = Some(value);
        } else if arg == "--fill-missing-branch-length" {
            if missing_branch_length_fill.is_some() {
                return Err(CliError::DuplicateOption("--fill-missing-branch-length"));
            }
            let value = parse_float(
                "--fill-missing-branch-length",
                take_value(&mut iter, "--fill-missing-branch-length")?,
            )?;
            if !value.is_finite() || value < 0.0 {
                return Err(CliError::InvalidMissingBranchLengthFill(value));
            }
            missing_branch_length_fill = Some(value);
        } else if arg == "--tree-name" {
            if tree_name.is_some() {
                return Err(CliError::DuplicateOption("--tree-name"));
            }
            tree_name = Some(take_value(&mut iter, "--tree-name")?);
        } else if arg == "--use-ambiguities" {
            if use_ambiguities {
                return Err(CliError::DuplicateOption("--use-ambiguities"));
            }
            use_ambiguities = true;
        } else {
            remaining.push(arg);
        }
    }
    Ok(ExtractedTreeOptions {
        remaining,
        min_branch_length,
        missing_branch_length_fill,
        tree_name,
        use_ambiguities,
    })
}

fn parse_parameter_template_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut preset = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--preset" => {
                preset = Some(parse_parameter_preset(take_value(&mut iter, "--preset")?)?)
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::ParameterTemplate(ParameterTemplateConfig {
        preset: preset.ok_or(CliError::MissingRequired("--preset"))?,
    }))
}

fn parse_analysis_template_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut preset = None;
    let mut mode = None;
    let mut output_dir_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--preset" => {
                if preset.is_some() {
                    return Err(CliError::DuplicateOption("--preset"));
                }
                preset = Some(parse_parameter_preset(take_value(&mut iter, "--preset")?)?);
            }
            "--mode" => {
                if mode.is_some() {
                    return Err(CliError::DuplicateOption("--mode"));
                }
                mode = Some(parse_analysis_request_mode(take_value(
                    &mut iter, "--mode",
                )?)?);
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::AnalysisTemplate(AnalysisTemplateConfig {
        preset: preset.ok_or(CliError::MissingRequired("--preset"))?,
        mode: mode.ok_or(CliError::MissingRequired("--mode"))?,
        output_dir_path: output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?,
    }))
}

fn parse_analysis_request_mode(
    value: String,
) -> Result<analysis_request::AnalysisRequestMode, CliError> {
    match value.as_str() {
        "evaluate" => Ok(analysis_request::AnalysisRequestMode::Evaluate),
        "optimize" => Ok(analysis_request::AnalysisRequestMode::Optimize),
        _ => Err(CliError::InvalidAnalysisRequestMode(value)),
    }
}

fn parse_analysis_plan_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut request_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--request" => {
                if request_path.is_some() {
                    return Err(CliError::DuplicateOption("--request"));
                }
                request_path = Some(PathBuf::from(take_value(&mut iter, "--request")?));
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::AnalysisPlan(AnalysisPlanConfig {
        request_path: request_path.ok_or(CliError::MissingRequired("--request"))?,
    }))
}

fn parse_analysis_run_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut request_path = None;
    let mut output_dir_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--request" => {
                if request_path.is_some() {
                    return Err(CliError::DuplicateOption("--request"));
                }
                request_path = Some(PathBuf::from(take_value(&mut iter, "--request")?));
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::AnalysisRun(AnalysisRunConfig {
        request_path: request_path.ok_or(CliError::MissingRequired("--request"))?,
        output_dir_path: output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?,
    }))
}

fn parse_analysis_workflow_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut request_path = None;
    let mut output_dir_path = None;
    let mut resume = false;
    let mut deep_inspection = false;
    let mut bsm_args = Vec::new();
    let mut has_output_level = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--request" => {
                if request_path.is_some() {
                    return Err(CliError::DuplicateOption("--request"));
                }
                request_path = Some(PathBuf::from(take_value(&mut iter, "--request")?));
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            "--resume" => {
                if resume {
                    return Err(CliError::DuplicateOption("--resume"));
                }
                resume = true;
            }
            "--deep" => {
                if deep_inspection {
                    return Err(CliError::DuplicateOption("--deep"));
                }
                deep_inspection = true;
            }
            "--analysis-result" | "--bsm-output-dir" | "--bsm-resume" => {
                return Err(CliError::AnalysisWorkflowOwnedOption(arg));
            }
            "--bsm-interactive" => bsm_args.push(arg),
            "--bsm-samples"
            | "--bsm-output-level"
            | "--bsm-threads"
            | "--bsm-max-in-flight"
            | "--bsm-max-events-per-sample"
            | "--bsm-max-events-total"
            | "--bsm-memory-budget-mb"
            | "--bsm-shard-samples"
            | "--bsm-checkpoint-samples"
            | "--bsm-time-limit-seconds"
            | "--seed" => {
                has_output_level |= arg == "--bsm-output-level";
                let value = take_value(
                    &mut iter,
                    match arg.as_str() {
                        "--bsm-samples" => "--bsm-samples",
                        "--bsm-output-level" => "--bsm-output-level",
                        "--bsm-threads" => "--bsm-threads",
                        "--bsm-max-in-flight" => "--bsm-max-in-flight",
                        "--bsm-max-events-per-sample" => "--bsm-max-events-per-sample",
                        "--bsm-max-events-total" => "--bsm-max-events-total",
                        "--bsm-memory-budget-mb" => "--bsm-memory-budget-mb",
                        "--bsm-shard-samples" => "--bsm-shard-samples",
                        "--bsm-checkpoint-samples" => "--bsm-checkpoint-samples",
                        "--bsm-time-limit-seconds" => "--bsm-time-limit-seconds",
                        "--seed" => "--seed",
                        _ => unreachable!(),
                    },
                )?;
                bsm_args.extend([arg, value]);
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }

    let request_path = request_path.ok_or(CliError::MissingRequired("--request"))?;
    let output_dir_path = output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?;
    let analysis_result_dir = output_dir_path.join("analysis-result");
    let bsm_result_dir = output_dir_path.join("bsm-result");
    if !has_output_level {
        bsm_args.extend(["--bsm-output-level".to_string(), "compact".to_string()]);
    }
    bsm_args.extend([
        "--analysis-result".to_string(),
        analysis_result_dir.to_string_lossy().into_owned(),
        "--bsm-output-dir".to_string(),
        bsm_result_dir.to_string_lossy().into_owned(),
    ]);
    let bsm = match parse_parameter_bsm_args(bsm_args)? {
        Command::ParameterBsm(config) => config,
        Command::Help => return Ok(Command::Help),
        _ => unreachable!("model-bsm parser returned an unrelated command"),
    };
    Ok(Command::AnalysisWorkflow(AnalysisWorkflowConfig {
        request_path,
        output_dir_path,
        resume,
        deep_inspection,
        bsm,
    }))
}

fn parse_model_workflow_plan_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut request_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--request" => {
                if request_path.is_some() {
                    return Err(CliError::DuplicateOption("--request"));
                }
                request_path = Some(PathBuf::from(take_value(&mut iter, "--request")?));
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::ModelWorkflowPlan(ModelWorkflowPlanConfig {
        request_path: request_path.ok_or(CliError::MissingRequired("--request"))?,
    }))
}

fn parse_model_workflow_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut request_path = None;
    let mut output_dir_path = None;
    let mut resume = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--request" => {
                if request_path.is_some() {
                    return Err(CliError::DuplicateOption("--request"));
                }
                request_path = Some(PathBuf::from(take_value(&mut iter, "--request")?));
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            "--resume" => {
                if resume {
                    return Err(CliError::DuplicateOption("--resume"));
                }
                resume = true;
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    Ok(Command::ModelWorkflow(ModelWorkflowConfig {
        request_path: request_path.ok_or(CliError::MissingRequired("--request"))?,
        output_dir_path: output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?,
        resume,
    }))
}

fn parse_parameter_preset(value: String) -> Result<biogeo_core::BioGeoBearsPreset, CliError> {
    match value.to_ascii_lowercase().as_str() {
        "dec" => Ok(biogeo_core::BioGeoBearsPreset::Dec),
        "dec+j" | "decj" => Ok(biogeo_core::BioGeoBearsPreset::DecJ),
        "divalike" => Ok(biogeo_core::BioGeoBearsPreset::DivaLike),
        "divalike+j" | "divalikej" => Ok(biogeo_core::BioGeoBearsPreset::DivaLikeJ),
        "bayarealike" => Ok(biogeo_core::BioGeoBearsPreset::BayAreaLike),
        "bayarealike+j" | "bayarealikej" => Ok(biogeo_core::BioGeoBearsPreset::BayAreaLikeJ),
        _ => Err(CliError::InvalidParameterPreset(value)),
    }
}

fn parse_parameter_model_args(
    args: Vec<String>,
    mode: ParameterRunMode,
) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut detections_path = None;
    let mut controls_path = None;
    let mut use_detection_model = false;
    let mut parameters_path = None;
    let mut analysis_result_dir_path = None;
    let mut max_range_size = None;
    let mut max_states = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut distance_matrix_path = None;
    let mut environment_distance_matrix_path = None;
    let mut extirpation_multipliers_path = None;
    let mut area_sizes_path = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut ancestral_probs = false;
    let mut split_probs = false;
    let mut optimization = biogeo_core::ParameterOptimizationConfig::default();
    let mut optimization_option_seen = false;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--detections" | "--detects" => {
                detections_path = Some(PathBuf::from(take_value(&mut iter, "--detections")?))
            }
            "--controls" => {
                controls_path = Some(PathBuf::from(take_value(&mut iter, "--controls")?))
            }
            "--use-detection-model" => use_detection_model = true,
            "--parameters" => {
                parameters_path = Some(PathBuf::from(take_value(&mut iter, "--parameters")?))
            }
            "--analysis-result-dir" => {
                analysis_result_dir_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--analysis-result-dir",
                )?))
            }
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--max-states" => {
                max_states = Some(parse_positive_usize(
                    "--max-states",
                    take_value(&mut iter, "--max-states")?,
                )?)
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--extirpation-multipliers" => {
                extirpation_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--extirpation-multipliers",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--ancestral-probs" => ancestral_probs = true,
            "--split-probs" => split_probs = true,
            "--initial-step" => {
                optimization.initial_step =
                    parse_float("--initial-step", take_value(&mut iter, "--initial-step")?)?;
                optimization_option_seen = true;
            }
            "--tolerance" => {
                optimization.tolerance =
                    parse_float("--tolerance", take_value(&mut iter, "--tolerance")?)?;
                optimization_option_seen = true;
            }
            "--max-iterations" => {
                optimization.max_iterations = parse_usize(
                    "--max-iterations",
                    take_value(&mut iter, "--max-iterations")?,
                )?;
                optimization_option_seen = true;
            }
            "--additional-start" => {
                optimization.additional_starts.push(parse_float_list(
                    "--additional-start",
                    take_value(&mut iter, "--additional-start")?,
                )?);
                optimization_option_seen = true;
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_owned()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_owned())),
        }
    }
    if mode == ParameterRunMode::Evaluate && optimization_option_seen {
        return Err(CliError::ParameterOptimizationOptionRequiresOptimize);
    }
    if use_detection_model {
        if ranges_path.is_some() {
            return Err(CliError::ConflictingTipObservationInputs);
        }
        if detections_path.is_none() {
            return Err(CliError::MissingRequired("--detections"));
        }
        if controls_path.is_none() {
            return Err(CliError::MissingRequired("--controls"));
        }
    } else {
        if detections_path.is_some() || controls_path.is_some() {
            return Err(CliError::DetectionInputRequiresModel);
        }
        if ranges_path.is_none() {
            return Err(CliError::MissingRequired("--ranges"));
        }
    }
    Ok(Command::ParameterModel(ParameterModelConfig {
        mode,
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path,
        detections_path,
        controls_path,
        use_detection_model,
        use_ambiguities: false,
        parameters_path: parameters_path.ok_or(CliError::MissingRequired("--parameters"))?,
        source_request_path: None,
        analysis_result_dir_path,
        min_branch_length: 0.0,
        missing_branch_length_fill: None,
        max_range_size,
        max_states,
        dispersal_multipliers_path,
        dispersal_strata_path,
        distance_matrix_path,
        environment_distance_matrix_path,
        extirpation_multipliers_path,
        area_sizes_path,
        include_null_range,
        root_prior,
        ancestral_probs,
        split_probs,
        optimization,
    }))
}

fn parse_parameter_batch_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut manifest_path = None;
    let mut output_dir_path = None;
    let mut resume = false;
    let mut model_args = Vec::new();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--manifest" => {
                if manifest_path.is_some() {
                    return Err(CliError::DuplicateOption("--manifest"));
                }
                manifest_path = Some(PathBuf::from(take_value(&mut iter, "--manifest")?));
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            "--resume" => {
                if resume {
                    return Err(CliError::DuplicateOption("--resume"));
                }
                resume = true;
            }
            "--parameters" | "--analysis-result-dir" | "--ancestral-probs" | "--split-probs" => {
                return Err(CliError::ModelBatchOwnedOption(arg));
            }
            _ => model_args.push(arg),
        }
    }

    let invocation_tokens = model_args.clone();
    model_args.extend([
        "--parameters".to_string(),
        "__model_batch_parameter_placeholder__.tsv".to_string(),
    ]);
    let template = match parse_parameter_model_args(model_args, ParameterRunMode::Optimize)? {
        Command::ParameterModel(config) => config,
        Command::Help => return Ok(Command::Help),
        _ => unreachable!("parameter model parser returned an unrelated command"),
    };
    Ok(Command::ParameterBatch(ParameterBatchConfig {
        manifest_path: manifest_path.ok_or(CliError::MissingRequired("--manifest"))?,
        output_dir_path: output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?,
        resume,
        invocation_tokens,
        template,
    }))
}

fn parse_dataset_batch_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut manifest_path = None;
    let mut output_dir_path = None;
    let mut resume = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--manifest" => {
                if manifest_path.is_some() {
                    return Err(CliError::DuplicateOption("--manifest"));
                }
                manifest_path = Some(PathBuf::from(take_value(&mut iter, "--manifest")?));
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            "--resume" => {
                if resume {
                    return Err(CliError::DuplicateOption("--resume"));
                }
                resume = true;
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_string()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_string())),
        }
    }
    Ok(Command::DatasetBatch(DatasetBatchConfig {
        manifest_path: manifest_path.ok_or(CliError::MissingRequired("--manifest"))?,
        output_dir_path: output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?,
        resume,
    }))
}

fn parse_parameter_bsm_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut analysis_result_dir_path = None;
    let mut bsm_samples = 0;
    let mut bsm_output_dir_path = None;
    let mut bsm_output_level = BsmOutputLevel::Legacy;
    let mut thread_selection = BsmThreadSelection::Auto;
    let mut max_in_flight = None;
    let mut max_events_per_sample = None;
    let mut max_events_total = None;
    let mut memory_budget_mb = None;
    let mut shard_samples = None;
    let mut checkpoint_samples = None;
    let mut resume = false;
    let mut time_limit = None;
    let mut interactive = false;
    let mut seed = 1;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--analysis-result" => {
                analysis_result_dir_path =
                    Some(PathBuf::from(take_value(&mut iter, "--analysis-result")?))
            }
            "--bsm-samples" => {
                bsm_samples = parse_usize("--bsm-samples", take_value(&mut iter, "--bsm-samples")?)?
            }
            "--bsm-output-dir" => {
                bsm_output_dir_path =
                    Some(PathBuf::from(take_value(&mut iter, "--bsm-output-dir")?))
            }
            "--bsm-output-level" => {
                bsm_output_level =
                    parse_bsm_output_level(take_value(&mut iter, "--bsm-output-level")?)?
            }
            "--bsm-threads" => {
                thread_selection = parse_bsm_threads(take_value(&mut iter, "--bsm-threads")?)?;
            }
            "--bsm-max-in-flight" => {
                max_in_flight = Some(parse_positive_usize(
                    "--bsm-max-in-flight",
                    take_value(&mut iter, "--bsm-max-in-flight")?,
                )?)
            }
            "--bsm-max-events-per-sample" => {
                max_events_per_sample = Some(parse_usize(
                    "--bsm-max-events-per-sample",
                    take_value(&mut iter, "--bsm-max-events-per-sample")?,
                )?)
            }
            "--bsm-max-events-total" => {
                max_events_total = Some(parse_usize(
                    "--bsm-max-events-total",
                    take_value(&mut iter, "--bsm-max-events-total")?,
                )?)
            }
            "--bsm-memory-budget-mb" => {
                memory_budget_mb = Some(parse_positive_usize(
                    "--bsm-memory-budget-mb",
                    take_value(&mut iter, "--bsm-memory-budget-mb")?,
                )?)
            }
            "--bsm-shard-samples" => {
                shard_samples = Some(parse_positive_usize(
                    "--bsm-shard-samples",
                    take_value(&mut iter, "--bsm-shard-samples")?,
                )?)
            }
            "--bsm-checkpoint-samples" => {
                checkpoint_samples = Some(parse_positive_usize(
                    "--bsm-checkpoint-samples",
                    take_value(&mut iter, "--bsm-checkpoint-samples")?,
                )?)
            }
            "--bsm-resume" => resume = true,
            "--bsm-time-limit-seconds" => {
                time_limit = Some(parse_bsm_duration(
                    "--bsm-time-limit-seconds",
                    take_value(&mut iter, "--bsm-time-limit-seconds")?,
                )?)
            }
            "--bsm-interactive" => interactive = true,
            "--seed" => seed = parse_u64("--seed", take_value(&mut iter, "--seed")?)?,
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_string()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_string())),
        }
    }

    if bsm_samples == 0 {
        return Err(CliError::BsmExecutionRequiresSamples);
    }
    if bsm_output_dir_path.is_none() && checkpoint_samples.is_some() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-checkpoint-samples",
        ));
    }
    if bsm_output_dir_path.is_none() && resume {
        return Err(CliError::BsmStreamOptionRequiresOutput("--bsm-resume"));
    }
    if bsm_output_dir_path.is_none() && memory_budget_mb.is_some() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-memory-budget-mb",
        ));
    }
    if bsm_output_dir_path.is_none() && shard_samples.is_some() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-shard-samples",
        ));
    }
    if bsm_output_dir_path.is_none() && bsm_output_level.is_v2() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-output-level",
        ));
    }
    if memory_budget_mb.is_some() && max_events_per_sample.is_none() {
        return Err(CliError::BsmMemoryBudgetRequiresPerSampleEventLimit);
    }
    Ok(Command::ParameterBsm(ParameterBsmConfig {
        analysis_result_dir_path: analysis_result_dir_path
            .ok_or(CliError::MissingRequired("--analysis-result"))?,
        bsm_samples,
        bsm_output_dir_path,
        bsm_output_level,
        execution_request: BsmExecutionRequest {
            thread_selection,
            max_in_flight,
            max_events_per_sample,
            max_events_total,
            memory_budget_mb,
            shard_samples,
            checkpoint_samples,
            time_limit,
        },
        bsm_resume: resume,
        bsm_interactive: interactive,
        seed,
    }))
}

fn parse_analysis_result_inspect_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut analysis_result_dir_path = None;
    let mut replay = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--analysis-result" => {
                if analysis_result_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--analysis-result"));
                }
                analysis_result_dir_path =
                    Some(PathBuf::from(take_value(&mut iter, "--analysis-result")?));
            }
            "--replay" => {
                if replay {
                    return Err(CliError::DuplicateOption("--replay"));
                }
                replay = true;
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_string()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_string())),
        }
    }
    Ok(Command::AnalysisResultInspect(
        AnalysisResultInspectConfig {
            analysis_result_dir_path: analysis_result_dir_path
                .ok_or(CliError::MissingRequired("--analysis-result"))?,
            replay,
        },
    ))
}

fn parse_bsm_inspect_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut bsm_result_dir_path = None;
    let mut deep = false;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--bsm-result" => {
                if bsm_result_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--bsm-result"));
                }
                bsm_result_dir_path = Some(PathBuf::from(take_value(&mut iter, "--bsm-result")?));
            }
            "--deep" => {
                if deep {
                    return Err(CliError::DuplicateOption("--deep"));
                }
                deep = true;
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_string()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_string())),
        }
    }
    Ok(Command::BsmInspect(BsmInspectConfig {
        bsm_result_dir_path: bsm_result_dir_path
            .ok_or(CliError::MissingRequired("--bsm-result"))?,
        deep,
    }))
}

fn parse_analysis_result_migrate_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut analysis_result_dir_path = None;
    let mut output_dir_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--analysis-result" => {
                if analysis_result_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--analysis-result"));
                }
                analysis_result_dir_path =
                    Some(PathBuf::from(take_value(&mut iter, "--analysis-result")?));
            }
            "--output-dir" => {
                if output_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--output-dir"));
                }
                output_dir_path = Some(PathBuf::from(take_value(&mut iter, "--output-dir")?));
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_string()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_string())),
        }
    }
    Ok(Command::AnalysisResultMigrate(
        AnalysisResultMigrateConfig {
            analysis_result_dir_path: analysis_result_dir_path
                .ok_or(CliError::MissingRequired("--analysis-result"))?,
            output_dir_path: output_dir_path.ok_or(CliError::MissingRequired("--output-dir"))?,
        },
    ))
}

fn parse_input_bundle_inspect_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut input_bundle_dir_path = None;
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--input-bundle" => {
                if input_bundle_dir_path.is_some() {
                    return Err(CliError::DuplicateOption("--input-bundle"));
                }
                input_bundle_dir_path =
                    Some(PathBuf::from(take_value(&mut iter, "--input-bundle")?));
            }
            option if option.starts_with('-') => {
                return Err(CliError::UnknownOption(option.to_string()));
            }
            argument => return Err(CliError::UnexpectedArgument(argument.to_string())),
        }
    }
    Ok(Command::InputBundleInspect(InputBundleInspectConfig {
        input_bundle_dir_path: input_bundle_dir_path
            .ok_or(CliError::MissingRequired("--input-bundle"))?,
    }))
}

fn parse_fixed_args(args: Vec<String>, preset: FixedPreset) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut d = None;
    let mut e = None;
    let mut j = 0.0;
    let mut mx01 = None;
    let mut mx01y = None;
    let mut mx01s = None;
    let mut mx01v = None;
    let mut mx01j = None;
    let mut max_range_size = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut distance_matrix_path = None;
    let mut distance_exponent = None;
    let mut environment_distance_matrix_path = None;
    let mut environment_distance_exponent = None;
    let mut extirpation_multipliers_path = None;
    let mut area_sizes_path = None;
    let mut area_exponent = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut ancestral_probs = false;
    let mut split_probs = false;
    let mut traceback_samples = 0;
    let mut bsm_samples = 0;
    let mut bsm_output_dir_path = None;
    let mut bsm_output_level = BsmOutputLevel::Legacy;
    let mut bsm_output_level_was_set = false;
    let mut bsm_threads = BsmThreadSelection::Auto;
    let mut bsm_threads_was_set = false;
    let mut bsm_max_in_flight = None;
    let mut bsm_max_events_per_sample = None;
    let mut bsm_max_events_total = None;
    let mut bsm_memory_budget_mb = None;
    let mut bsm_shard_samples = None;
    let mut bsm_checkpoint_samples = None;
    let mut bsm_resume = false;
    let mut bsm_time_limit = None;
    let mut bsm_interactive = false;
    let mut seed = 1;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--d" => d = Some(parse_float("--d", take_value(&mut iter, "--d")?)?),
            "--e" => e = Some(parse_float("--e", take_value(&mut iter, "--e")?)?),
            "--j" => {
                j = parse_float("--j", take_value(&mut iter, "--j")?)?;
            }
            "--mx01" => mx01 = Some(parse_float("--mx01", take_value(&mut iter, "--mx01")?)?),
            "--mx01y" => mx01y = Some(parse_float("--mx01y", take_value(&mut iter, "--mx01y")?)?),
            "--mx01s" => mx01s = Some(parse_float("--mx01s", take_value(&mut iter, "--mx01s")?)?),
            "--mx01v" => mx01v = Some(parse_float("--mx01v", take_value(&mut iter, "--mx01v")?)?),
            "--mx01j" => mx01j = Some(parse_float("--mx01j", take_value(&mut iter, "--mx01j")?)?),
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--distance-exponent" => {
                distance_exponent = Some(parse_float(
                    "--distance-exponent",
                    take_value(&mut iter, "--distance-exponent")?,
                )?)
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--environment-distance-exponent" => {
                environment_distance_exponent = Some(parse_float(
                    "--environment-distance-exponent",
                    take_value(&mut iter, "--environment-distance-exponent")?,
                )?)
            }
            "--extirpation-multipliers" => {
                extirpation_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--extirpation-multipliers",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--area-exponent" => {
                area_exponent = Some(parse_float(
                    "--area-exponent",
                    take_value(&mut iter, "--area-exponent")?,
                )?)
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--ancestral-probs" => ancestral_probs = true,
            "--split-probs" => split_probs = true,
            "--traceback-samples" => {
                traceback_samples = parse_usize(
                    "--traceback-samples",
                    take_value(&mut iter, "--traceback-samples")?,
                )?
            }
            "--bsm-samples" => {
                bsm_samples = parse_usize("--bsm-samples", take_value(&mut iter, "--bsm-samples")?)?
            }
            "--bsm-output-dir" => {
                bsm_output_dir_path =
                    Some(PathBuf::from(take_value(&mut iter, "--bsm-output-dir")?))
            }
            "--bsm-output-level" => {
                bsm_output_level =
                    parse_bsm_output_level(take_value(&mut iter, "--bsm-output-level")?)?;
                bsm_output_level_was_set = true;
            }
            "--bsm-threads" => {
                bsm_threads = parse_bsm_threads(take_value(&mut iter, "--bsm-threads")?)?;
                bsm_threads_was_set = true;
            }
            "--bsm-max-in-flight" => {
                bsm_max_in_flight = Some(parse_positive_usize(
                    "--bsm-max-in-flight",
                    take_value(&mut iter, "--bsm-max-in-flight")?,
                )?)
            }
            "--bsm-max-events-per-sample" => {
                bsm_max_events_per_sample = Some(parse_usize(
                    "--bsm-max-events-per-sample",
                    take_value(&mut iter, "--bsm-max-events-per-sample")?,
                )?)
            }
            "--bsm-max-events-total" => {
                bsm_max_events_total = Some(parse_usize(
                    "--bsm-max-events-total",
                    take_value(&mut iter, "--bsm-max-events-total")?,
                )?)
            }
            "--bsm-memory-budget-mb" => {
                bsm_memory_budget_mb = Some(parse_positive_usize(
                    "--bsm-memory-budget-mb",
                    take_value(&mut iter, "--bsm-memory-budget-mb")?,
                )?)
            }
            "--bsm-shard-samples" => {
                bsm_shard_samples = Some(parse_positive_usize(
                    "--bsm-shard-samples",
                    take_value(&mut iter, "--bsm-shard-samples")?,
                )?)
            }
            "--bsm-checkpoint-samples" => {
                bsm_checkpoint_samples = Some(parse_positive_usize(
                    "--bsm-checkpoint-samples",
                    take_value(&mut iter, "--bsm-checkpoint-samples")?,
                )?)
            }
            "--bsm-resume" => bsm_resume = true,
            "--bsm-time-limit-seconds" => {
                bsm_time_limit = Some(parse_bsm_duration(
                    "--bsm-time-limit-seconds",
                    take_value(&mut iter, "--bsm-time-limit-seconds")?,
                )?)
            }
            "--bsm-interactive" => bsm_interactive = true,
            "--seed" => seed = parse_u64("--seed", take_value(&mut iter, "--seed")?)?,
            _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
            _ => return Err(CliError::UnexpectedArgument(arg)),
        }
    }

    if dispersal_multipliers_path.is_some() && dispersal_strata_path.is_some() {
        return Err(CliError::ConflictingDispersalModifiers);
    }
    if distance_matrix_path.is_some() && distance_exponent.is_none()
        || dispersal_strata_path.is_none()
            && distance_matrix_path.is_none()
            && distance_exponent.is_some()
    {
        return Err(CliError::IncompleteDistanceModifier);
    }
    if environment_distance_matrix_path.is_some() && environment_distance_exponent.is_none()
        || dispersal_strata_path.is_none()
            && environment_distance_matrix_path.is_none()
            && environment_distance_exponent.is_some()
    {
        return Err(CliError::IncompleteEnvironmentDistanceModifier);
    }
    if area_sizes_path.is_some() && area_exponent.is_none()
        || dispersal_strata_path.is_none() && area_sizes_path.is_none() && area_exponent.is_some()
    {
        return Err(CliError::IncompleteAreaSizeModifier);
    }
    if area_sizes_path.is_some() && extirpation_multipliers_path.is_some() {
        return Err(CliError::ConflictingExtirpationModifiers);
    }
    if traceback_samples > 0 && bsm_samples > 0 {
        return Err(CliError::ConflictingHistorySamplingOptions);
    }
    if bsm_output_dir_path.is_some() && bsm_samples == 0 {
        return Err(CliError::BsmOutputRequiresSamples);
    }
    if bsm_output_dir_path.is_none() && bsm_checkpoint_samples.is_some() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-checkpoint-samples",
        ));
    }
    if bsm_output_dir_path.is_none() && bsm_resume {
        return Err(CliError::BsmStreamOptionRequiresOutput("--bsm-resume"));
    }
    if bsm_output_dir_path.is_none() && bsm_memory_budget_mb.is_some() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-memory-budget-mb",
        ));
    }
    if bsm_output_dir_path.is_none() && bsm_shard_samples.is_some() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-shard-samples",
        ));
    }
    if bsm_output_dir_path.is_none() && bsm_output_level.is_v2() {
        return Err(CliError::BsmStreamOptionRequiresOutput(
            "--bsm-output-level",
        ));
    }
    if bsm_memory_budget_mb.is_some() && bsm_max_events_per_sample.is_none() {
        return Err(CliError::BsmMemoryBudgetRequiresPerSampleEventLimit);
    }
    if bsm_samples == 0
        && (bsm_threads_was_set
            || bsm_max_in_flight.is_some()
            || bsm_max_events_per_sample.is_some()
            || bsm_max_events_total.is_some()
            || bsm_memory_budget_mb.is_some()
            || bsm_shard_samples.is_some()
            || bsm_checkpoint_samples.is_some()
            || bsm_output_level_was_set
            || bsm_resume
            || bsm_time_limit.is_some()
            || bsm_interactive)
    {
        return Err(CliError::BsmExecutionRequiresSamples);
    }
    Ok(Command::Fixed(FixedModelConfig {
        preset,
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        d: d.ok_or(CliError::MissingRequired("--d"))?,
        e: e.ok_or(CliError::MissingRequired("--e"))?,
        j,
        range_size: resolve_range_size_config(
            preset.default_range_size(),
            mx01,
            mx01y,
            mx01s,
            mx01v,
            mx01j,
        ),
        min_branch_length: 0.0,
        max_range_size,
        dispersal_multipliers_path,
        dispersal_strata_path,
        distance_matrix_path,
        distance_exponent,
        environment_distance_matrix_path,
        environment_distance_exponent,
        extirpation_multipliers_path,
        area_sizes_path,
        area_exponent,
        include_null_range,
        root_prior,
        ancestral_probs,
        split_probs,
        traceback_samples,
        bsm_samples,
        bsm_output_dir_path,
        bsm_output_level,
        bsm_threads,
        bsm_max_in_flight,
        bsm_max_events_per_sample,
        bsm_max_events_total,
        bsm_memory_budget_mb,
        bsm_shard_samples,
        bsm_checkpoint_samples,
        bsm_resume,
        bsm_time_limit,
        bsm_interactive,
        seed,
    }))
}

fn parse_de_optimize_args(args: Vec<String>, preset: FixedPreset) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut max_range_size = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut distance_matrix_path = None;
    let mut distance_exponent = None;
    let mut environment_distance_matrix_path = None;
    let mut environment_distance_exponent = None;
    let mut extirpation_multipliers_path = None;
    let mut area_sizes_path = None;
    let mut area_exponent = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut ancestral_probs = false;
    let mut split_probs = false;
    let mut optimization = match preset {
        FixedPreset::Dec => biogeo_core::DecOptimizationConfig::default(),
        FixedPreset::DivaLike => biogeo_core::DecOptimizationConfig::for_divalike(),
        FixedPreset::BayAreaLike => biogeo_core::DecOptimizationConfig::for_bayarealike(),
    };
    let mut mx01 = None;
    let mut mx01y = None;
    let mut mx01s = None;
    let mut mx01v = None;
    let mut mx01j = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--distance-exponent" => {
                distance_exponent = Some(parse_float(
                    "--distance-exponent",
                    take_value(&mut iter, "--distance-exponent")?,
                )?)
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--environment-distance-exponent" => {
                environment_distance_exponent = Some(parse_float(
                    "--environment-distance-exponent",
                    take_value(&mut iter, "--environment-distance-exponent")?,
                )?)
            }
            "--extirpation-multipliers" => {
                extirpation_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--extirpation-multipliers",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--area-exponent" => {
                area_exponent = Some(parse_float(
                    "--area-exponent",
                    take_value(&mut iter, "--area-exponent")?,
                )?)
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--ancestral-probs" => ancestral_probs = true,
            "--split-probs" => split_probs = true,
            "--mx01" => mx01 = Some(parse_float("--mx01", take_value(&mut iter, "--mx01")?)?),
            "--mx01y" => mx01y = Some(parse_float("--mx01y", take_value(&mut iter, "--mx01y")?)?),
            "--mx01s" => mx01s = Some(parse_float("--mx01s", take_value(&mut iter, "--mx01s")?)?),
            "--mx01v" => mx01v = Some(parse_float("--mx01v", take_value(&mut iter, "--mx01v")?)?),
            "--mx01j" => mx01j = Some(parse_float("--mx01j", take_value(&mut iter, "--mx01j")?)?),
            "--init-d" => {
                optimization.initial_d =
                    parse_float("--init-d", take_value(&mut iter, "--init-d")?)?
            }
            "--init-e" => {
                optimization.initial_e =
                    parse_float("--init-e", take_value(&mut iter, "--init-e")?)?
            }
            "--min-rate" => {
                optimization.min_rate =
                    parse_float("--min-rate", take_value(&mut iter, "--min-rate")?)?
            }
            "--max-rate" => {
                optimization.max_rate =
                    parse_float("--max-rate", take_value(&mut iter, "--max-rate")?)?
            }
            "--initial-log-step" => {
                optimization.initial_log_step = parse_float(
                    "--initial-log-step",
                    take_value(&mut iter, "--initial-log-step")?,
                )?
            }
            "--tolerance" => {
                optimization.tolerance =
                    parse_float("--tolerance", take_value(&mut iter, "--tolerance")?)?
            }
            "--max-iterations" => {
                optimization.max_iterations = parse_usize(
                    "--max-iterations",
                    take_value(&mut iter, "--max-iterations")?,
                )?
            }
            "--multi-start-points" => {
                optimization.multi_start_points_per_axis = parse_usize(
                    "--multi-start-points",
                    take_value(&mut iter, "--multi-start-points")?,
                )?
            }
            _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
            _ => return Err(CliError::UnexpectedArgument(arg)),
        }
    }

    optimization.range_size =
        resolve_range_size_config(optimization.range_size, mx01, mx01y, mx01s, mx01v, mx01j);
    if dispersal_multipliers_path.is_some() && dispersal_strata_path.is_some() {
        return Err(CliError::ConflictingDispersalModifiers);
    }
    if distance_matrix_path.is_some() && distance_exponent.is_none()
        || dispersal_strata_path.is_none()
            && distance_matrix_path.is_none()
            && distance_exponent.is_some()
    {
        return Err(CliError::IncompleteDistanceModifier);
    }
    if environment_distance_matrix_path.is_some() && environment_distance_exponent.is_none()
        || dispersal_strata_path.is_none()
            && environment_distance_matrix_path.is_none()
            && environment_distance_exponent.is_some()
    {
        return Err(CliError::IncompleteEnvironmentDistanceModifier);
    }
    if area_sizes_path.is_some() && area_exponent.is_none()
        || dispersal_strata_path.is_none() && area_sizes_path.is_none() && area_exponent.is_some()
    {
        return Err(CliError::IncompleteAreaSizeModifier);
    }
    if area_sizes_path.is_some() && extirpation_multipliers_path.is_some() {
        return Err(CliError::ConflictingExtirpationModifiers);
    }

    Ok(Command::DeOptimize(DeOptimizeConfig {
        preset,
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        min_branch_length: 0.0,
        max_range_size,
        dispersal_multipliers_path,
        dispersal_strata_path,
        distance_matrix_path,
        distance_exponent,
        environment_distance_matrix_path,
        environment_distance_exponent,
        extirpation_multipliers_path,
        area_sizes_path,
        area_exponent,
        include_null_range,
        root_prior,
        ancestral_probs,
        split_probs,
        optimization,
    }))
}

fn parse_exponent_optimize_args(
    args: Vec<String>,
    kind: ExponentKind,
) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut distance_matrix_path = None;
    let mut distance_exponent = None;
    let mut environment_distance_matrix_path = None;
    let mut environment_distance_exponent = None;
    let mut area_sizes_path = None;
    let mut area_exponent = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut extirpation_multipliers_path = None;
    let mut max_range_size = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut ancestral_probs = false;
    let mut split_probs = false;
    let mut optimization = kind.optimization_config();
    let mut mx01 = None;
    let mut mx01y = None;
    let mut mx01s = None;
    let mut mx01v = None;
    let mut mx01j = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--distance-exponent" => {
                if kind == ExponentKind::GeographicX {
                    return Err(CliError::OptimizedExponentAlsoFixed { parameter: "x" });
                }
                distance_exponent = Some(parse_float(
                    "--distance-exponent",
                    take_value(&mut iter, "--distance-exponent")?,
                )?)
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--environment-distance-exponent" => {
                if kind == ExponentKind::EnvironmentN {
                    return Err(CliError::OptimizedExponentAlsoFixed { parameter: "n" });
                }
                environment_distance_exponent = Some(parse_float(
                    "--environment-distance-exponent",
                    take_value(&mut iter, "--environment-distance-exponent")?,
                )?)
            }
            "--extirpation-multipliers" => {
                extirpation_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--extirpation-multipliers",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--area-exponent" => {
                if kind == ExponentKind::AreaSizeU {
                    return Err(CliError::OptimizedExponentAlsoFixed { parameter: "u" });
                }
                area_exponent = Some(parse_float(
                    "--area-exponent",
                    take_value(&mut iter, "--area-exponent")?,
                )?)
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--ancestral-probs" => ancestral_probs = true,
            "--split-probs" => split_probs = true,
            "--mx01" => mx01 = Some(parse_float("--mx01", take_value(&mut iter, "--mx01")?)?),
            "--mx01y" => mx01y = Some(parse_float("--mx01y", take_value(&mut iter, "--mx01y")?)?),
            "--mx01s" => mx01s = Some(parse_float("--mx01s", take_value(&mut iter, "--mx01s")?)?),
            "--mx01v" => mx01v = Some(parse_float("--mx01v", take_value(&mut iter, "--mx01v")?)?),
            "--mx01j" => mx01j = Some(parse_float("--mx01j", take_value(&mut iter, "--mx01j")?)?),
            "--init-d" => {
                optimization.de.initial_d =
                    parse_float("--init-d", take_value(&mut iter, "--init-d")?)?
            }
            "--init-e" => {
                optimization.de.initial_e =
                    parse_float("--init-e", take_value(&mut iter, "--init-e")?)?
            }
            "--min-rate" => {
                optimization.de.min_rate =
                    parse_float("--min-rate", take_value(&mut iter, "--min-rate")?)?
            }
            "--max-rate" => {
                optimization.de.max_rate =
                    parse_float("--max-rate", take_value(&mut iter, "--max-rate")?)?
            }
            "--initial-log-step" => {
                optimization.de.initial_log_step = parse_float(
                    "--initial-log-step",
                    take_value(&mut iter, "--initial-log-step")?,
                )?
            }
            "--init-exponent" => {
                optimization.initial_exponent =
                    parse_float("--init-exponent", take_value(&mut iter, "--init-exponent")?)?
            }
            "--min-exponent" => {
                optimization.min_exponent =
                    parse_float("--min-exponent", take_value(&mut iter, "--min-exponent")?)?
            }
            "--max-exponent" => {
                optimization.max_exponent =
                    parse_float("--max-exponent", take_value(&mut iter, "--max-exponent")?)?
            }
            "--initial-exponent-step" => {
                optimization.initial_exponent_step = parse_float(
                    "--initial-exponent-step",
                    take_value(&mut iter, "--initial-exponent-step")?,
                )?
            }
            "--tolerance" => {
                optimization.de.tolerance =
                    parse_float("--tolerance", take_value(&mut iter, "--tolerance")?)?
            }
            "--max-iterations" => {
                optimization.de.max_iterations = parse_usize(
                    "--max-iterations",
                    take_value(&mut iter, "--max-iterations")?,
                )?
            }
            "--multi-start-points" => {
                optimization.de.multi_start_points_per_axis = parse_usize(
                    "--multi-start-points",
                    take_value(&mut iter, "--multi-start-points")?,
                )?
            }
            _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
            _ => return Err(CliError::UnexpectedArgument(arg)),
        }
    }

    if dispersal_multipliers_path.is_some() && dispersal_strata_path.is_some() {
        return Err(CliError::ConflictingDispersalModifiers);
    }
    let has_strata = dispersal_strata_path.is_some();
    match kind {
        ExponentKind::GeographicX => {
            if distance_matrix_path.is_none() && !has_strata {
                return Err(CliError::MissingRequired("--distance-matrix"));
            }
            if environment_distance_matrix_path.is_some() && environment_distance_exponent.is_none()
                || !has_strata
                    && environment_distance_matrix_path.is_none()
                    && environment_distance_exponent.is_some()
            {
                return Err(CliError::IncompleteEnvironmentDistanceModifier);
            }
            if area_sizes_path.is_some() && area_exponent.is_none()
                || !has_strata && area_sizes_path.is_none() && area_exponent.is_some()
            {
                return Err(CliError::IncompleteAreaSizeModifier);
            }
            if area_sizes_path.is_some() && extirpation_multipliers_path.is_some() {
                return Err(CliError::ConflictingExtirpationModifiers);
            }
        }
        ExponentKind::EnvironmentN => {
            if environment_distance_matrix_path.is_none() && !has_strata {
                return Err(CliError::MissingRequired("--environment-distance-matrix"));
            }
            if distance_matrix_path.is_some() && distance_exponent.is_none()
                || !has_strata && distance_matrix_path.is_none() && distance_exponent.is_some()
            {
                return Err(CliError::IncompleteDistanceModifier);
            }
            if area_sizes_path.is_some() && area_exponent.is_none()
                || !has_strata && area_sizes_path.is_none() && area_exponent.is_some()
            {
                return Err(CliError::IncompleteAreaSizeModifier);
            }
            if area_sizes_path.is_some() && extirpation_multipliers_path.is_some() {
                return Err(CliError::ConflictingExtirpationModifiers);
            }
        }
        ExponentKind::AreaSizeU => {
            if area_sizes_path.is_none() && !has_strata {
                return Err(CliError::MissingRequired("--area-sizes"));
            }
            if extirpation_multipliers_path.is_some() {
                return Err(CliError::ConflictingExtirpationModifiers);
            }
            if distance_matrix_path.is_some() && distance_exponent.is_none()
                || !has_strata && distance_matrix_path.is_none() && distance_exponent.is_some()
            {
                return Err(CliError::IncompleteDistanceModifier);
            }
            if environment_distance_matrix_path.is_some() && environment_distance_exponent.is_none()
                || !has_strata
                    && environment_distance_matrix_path.is_none()
                    && environment_distance_exponent.is_some()
            {
                return Err(CliError::IncompleteEnvironmentDistanceModifier);
            }
        }
    }
    optimization.de.range_size =
        resolve_range_size_config(optimization.de.range_size, mx01, mx01y, mx01s, mx01v, mx01j);

    Ok(Command::ExponentOptimize(ExponentOptimizeConfig {
        kind,
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        min_branch_length: 0.0,
        distance_matrix_path,
        distance_exponent,
        environment_distance_matrix_path,
        environment_distance_exponent,
        area_sizes_path,
        area_exponent,
        dispersal_multipliers_path,
        dispersal_strata_path,
        extirpation_multipliers_path,
        max_range_size,
        include_null_range,
        root_prior,
        ancestral_probs,
        split_probs,
        optimization,
    }))
}

fn parse_xnu_optimize_args(args: Vec<String>) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut distance_matrix_path = None;
    let mut environment_distance_matrix_path = None;
    let mut area_sizes_path = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut max_range_size = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut ancestral_probs = false;
    let mut split_probs = false;
    let mut optimization = biogeo_core::DecXnuOptimizationConfig::default();
    let mut mx01 = None;
    let mut mx01y = None;
    let mut mx01s = None;
    let mut mx01v = None;
    let mut mx01j = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-exponent" => {
                return Err(CliError::OptimizedExponentAlsoFixed { parameter: "x" });
            }
            "--environment-distance-exponent" => {
                return Err(CliError::OptimizedExponentAlsoFixed { parameter: "n" });
            }
            "--area-exponent" => {
                return Err(CliError::OptimizedExponentAlsoFixed { parameter: "u" });
            }
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--ancestral-probs" => ancestral_probs = true,
            "--split-probs" => split_probs = true,
            "--mx01" => mx01 = Some(parse_float("--mx01", take_value(&mut iter, "--mx01")?)?),
            "--mx01y" => mx01y = Some(parse_float("--mx01y", take_value(&mut iter, "--mx01y")?)?),
            "--mx01s" => mx01s = Some(parse_float("--mx01s", take_value(&mut iter, "--mx01s")?)?),
            "--mx01v" => mx01v = Some(parse_float("--mx01v", take_value(&mut iter, "--mx01v")?)?),
            "--mx01j" => mx01j = Some(parse_float("--mx01j", take_value(&mut iter, "--mx01j")?)?),
            "--init-d" => {
                optimization.de.initial_d =
                    parse_float("--init-d", take_value(&mut iter, "--init-d")?)?
            }
            "--init-e" => {
                optimization.de.initial_e =
                    parse_float("--init-e", take_value(&mut iter, "--init-e")?)?
            }
            "--min-rate" => {
                optimization.de.min_rate =
                    parse_float("--min-rate", take_value(&mut iter, "--min-rate")?)?
            }
            "--max-rate" => {
                optimization.de.max_rate =
                    parse_float("--max-rate", take_value(&mut iter, "--max-rate")?)?
            }
            "--initial-log-step" => {
                optimization.de.initial_log_step = parse_float(
                    "--initial-log-step",
                    take_value(&mut iter, "--initial-log-step")?,
                )?
            }
            "--init-x" => {
                optimization.initial_x =
                    parse_float("--init-x", take_value(&mut iter, "--init-x")?)?
            }
            "--min-x" => {
                optimization.min_x = parse_float("--min-x", take_value(&mut iter, "--min-x")?)?
            }
            "--max-x" => {
                optimization.max_x = parse_float("--max-x", take_value(&mut iter, "--max-x")?)?
            }
            "--initial-x-step" => {
                optimization.initial_x_step = parse_float(
                    "--initial-x-step",
                    take_value(&mut iter, "--initial-x-step")?,
                )?
            }
            "--init-n" => {
                optimization.initial_n =
                    parse_float("--init-n", take_value(&mut iter, "--init-n")?)?
            }
            "--min-n" => {
                optimization.min_n = parse_float("--min-n", take_value(&mut iter, "--min-n")?)?
            }
            "--max-n" => {
                optimization.max_n = parse_float("--max-n", take_value(&mut iter, "--max-n")?)?
            }
            "--initial-n-step" => {
                optimization.initial_n_step = parse_float(
                    "--initial-n-step",
                    take_value(&mut iter, "--initial-n-step")?,
                )?
            }
            "--init-u" => {
                optimization.initial_u =
                    parse_float("--init-u", take_value(&mut iter, "--init-u")?)?
            }
            "--min-u" => {
                optimization.min_u = parse_float("--min-u", take_value(&mut iter, "--min-u")?)?
            }
            "--max-u" => {
                optimization.max_u = parse_float("--max-u", take_value(&mut iter, "--max-u")?)?
            }
            "--initial-u-step" => {
                optimization.initial_u_step = parse_float(
                    "--initial-u-step",
                    take_value(&mut iter, "--initial-u-step")?,
                )?
            }
            "--tolerance" => {
                optimization.de.tolerance =
                    parse_float("--tolerance", take_value(&mut iter, "--tolerance")?)?
            }
            "--max-iterations" => {
                optimization.de.max_iterations = parse_usize(
                    "--max-iterations",
                    take_value(&mut iter, "--max-iterations")?,
                )?
            }
            "--multi-start-points" => {
                optimization.de.multi_start_points_per_axis = parse_usize(
                    "--multi-start-points",
                    take_value(&mut iter, "--multi-start-points")?,
                )?
            }
            _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
            _ => return Err(CliError::UnexpectedArgument(arg)),
        }
    }

    optimization.de.range_size =
        resolve_range_size_config(optimization.de.range_size, mx01, mx01y, mx01s, mx01v, mx01j);
    if dispersal_strata_path.is_some() {
        if distance_matrix_path.is_some()
            || environment_distance_matrix_path.is_some()
            || area_sizes_path.is_some()
            || dispersal_multipliers_path.is_some()
        {
            return Err(CliError::ConflictingStratifiedRawModifiers);
        }
    } else if distance_matrix_path.is_none() {
        return Err(CliError::MissingRequired("--distance-matrix"));
    } else if environment_distance_matrix_path.is_none() {
        return Err(CliError::MissingRequired("--environment-distance-matrix"));
    } else if area_sizes_path.is_none() {
        return Err(CliError::MissingRequired("--area-sizes"));
    }
    Ok(Command::XnuOptimize(XnuOptimizeConfig {
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        min_branch_length: 0.0,
        distance_matrix_path,
        environment_distance_matrix_path,
        area_sizes_path,
        dispersal_multipliers_path,
        dispersal_strata_path,
        max_range_size,
        include_null_range,
        root_prior,
        ancestral_probs,
        split_probs,
        optimization,
    }))
}

#[derive(Clone, Copy, Debug, Default)]
struct ProfileGridArgs {
    min: Option<f64>,
    max: Option<f64>,
    points: Option<usize>,
}

impl ProfileGridArgs {
    fn is_present(self) -> bool {
        self.min.is_some() || self.max.is_some() || self.points.is_some()
    }
}

fn build_profile_axis(
    kind: ExponentKind,
    grid: ProfileGridArgs,
) -> Result<biogeo_core::DecProfileAxis, CliError> {
    let (min_option, max_option, points_option) = kind.grid_options();
    let min = grid.min.ok_or(CliError::MissingRequired(min_option))?;
    let max = grid.max.ok_or(CliError::MissingRequired(max_option))?;
    let points = grid
        .points
        .ok_or(CliError::MissingRequired(points_option))?;
    if !min.is_finite() || !max.is_finite() || min >= max {
        return Err(CliError::InvalidProfileAxisBounds {
            parameter: kind.parameter_name(),
            min,
            max,
        });
    }
    if points < 2 {
        return Err(CliError::ProfileAxisTooShort {
            parameter: kind.parameter_name(),
            points,
        });
    }

    let denominator = (points - 1) as f64;
    let values = (0..points)
        .map(|index| {
            if index + 1 == points {
                max
            } else {
                min + (max - min) * index as f64 / denominator
            }
        })
        .collect();
    Ok(biogeo_core::DecProfileAxis::new(
        kind.parameter_name(),
        values,
    ))
}

fn parse_pair_profile_args(args: Vec<String>, pair: ProfilePair) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut distance_matrix_path = None;
    let mut environment_distance_matrix_path = None;
    let mut area_sizes_path = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut distance_exponent = None;
    let mut environment_distance_exponent = None;
    let mut area_exponent = None;
    let mut x_grid = ProfileGridArgs::default();
    let mut n_grid = ProfileGridArgs::default();
    let mut u_grid = ProfileGridArgs::default();
    let mut max_range_size = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut optimization = biogeo_core::DecOptimizationConfig {
        max_iterations: 300,
        multi_start_points_per_axis: 2,
        ..biogeo_core::DecOptimizationConfig::default()
    };
    let mut support_delta = biogeo_core::PROFILE_95_SUPPORT_DELTA;
    let mut mx01 = None;
    let mut mx01y = None;
    let mut mx01s = None;
    let mut mx01v = None;
    let mut mx01j = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-exponent" => {
                if pair.fixed() != ExponentKind::GeographicX {
                    return Err(CliError::OptimizedExponentAlsoFixed { parameter: "x" });
                }
                distance_exponent = Some(parse_float(
                    "--distance-exponent",
                    take_value(&mut iter, "--distance-exponent")?,
                )?);
            }
            "--environment-distance-exponent" => {
                if pair.fixed() != ExponentKind::EnvironmentN {
                    return Err(CliError::OptimizedExponentAlsoFixed { parameter: "n" });
                }
                environment_distance_exponent = Some(parse_float(
                    "--environment-distance-exponent",
                    take_value(&mut iter, "--environment-distance-exponent")?,
                )?);
            }
            "--area-exponent" => {
                if pair.fixed() != ExponentKind::AreaSizeU {
                    return Err(CliError::OptimizedExponentAlsoFixed { parameter: "u" });
                }
                area_exponent = Some(parse_float(
                    "--area-exponent",
                    take_value(&mut iter, "--area-exponent")?,
                )?);
            }
            "--x-min" => {
                x_grid.min = Some(parse_float("--x-min", take_value(&mut iter, "--x-min")?)?)
            }
            "--x-max" => {
                x_grid.max = Some(parse_float("--x-max", take_value(&mut iter, "--x-max")?)?)
            }
            "--x-points" => {
                x_grid.points = Some(parse_usize(
                    "--x-points",
                    take_value(&mut iter, "--x-points")?,
                )?)
            }
            "--n-min" => {
                n_grid.min = Some(parse_float("--n-min", take_value(&mut iter, "--n-min")?)?)
            }
            "--n-max" => {
                n_grid.max = Some(parse_float("--n-max", take_value(&mut iter, "--n-max")?)?)
            }
            "--n-points" => {
                n_grid.points = Some(parse_usize(
                    "--n-points",
                    take_value(&mut iter, "--n-points")?,
                )?)
            }
            "--u-min" => {
                u_grid.min = Some(parse_float("--u-min", take_value(&mut iter, "--u-min")?)?)
            }
            "--u-max" => {
                u_grid.max = Some(parse_float("--u-max", take_value(&mut iter, "--u-max")?)?)
            }
            "--u-points" => {
                u_grid.points = Some(parse_usize(
                    "--u-points",
                    take_value(&mut iter, "--u-points")?,
                )?)
            }
            "--support-delta" => {
                support_delta =
                    parse_float("--support-delta", take_value(&mut iter, "--support-delta")?)?
            }
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--mx01" => mx01 = Some(parse_float("--mx01", take_value(&mut iter, "--mx01")?)?),
            "--mx01y" => mx01y = Some(parse_float("--mx01y", take_value(&mut iter, "--mx01y")?)?),
            "--mx01s" => mx01s = Some(parse_float("--mx01s", take_value(&mut iter, "--mx01s")?)?),
            "--mx01v" => mx01v = Some(parse_float("--mx01v", take_value(&mut iter, "--mx01v")?)?),
            "--mx01j" => mx01j = Some(parse_float("--mx01j", take_value(&mut iter, "--mx01j")?)?),
            "--init-d" => {
                optimization.initial_d =
                    parse_float("--init-d", take_value(&mut iter, "--init-d")?)?
            }
            "--init-e" => {
                optimization.initial_e =
                    parse_float("--init-e", take_value(&mut iter, "--init-e")?)?
            }
            "--min-rate" => {
                optimization.min_rate =
                    parse_float("--min-rate", take_value(&mut iter, "--min-rate")?)?
            }
            "--max-rate" => {
                optimization.max_rate =
                    parse_float("--max-rate", take_value(&mut iter, "--max-rate")?)?
            }
            "--initial-log-step" => {
                optimization.initial_log_step = parse_float(
                    "--initial-log-step",
                    take_value(&mut iter, "--initial-log-step")?,
                )?
            }
            "--tolerance" => {
                optimization.tolerance =
                    parse_float("--tolerance", take_value(&mut iter, "--tolerance")?)?
            }
            "--max-iterations" => {
                optimization.max_iterations = parse_usize(
                    "--max-iterations",
                    take_value(&mut iter, "--max-iterations")?,
                )?
            }
            "--multi-start-points" => {
                optimization.multi_start_points_per_axis = parse_usize(
                    "--multi-start-points",
                    take_value(&mut iter, "--multi-start-points")?,
                )?
            }
            _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
            _ => return Err(CliError::UnexpectedArgument(arg)),
        }
    }

    let fixed_grid = match pair.fixed() {
        ExponentKind::GeographicX => x_grid,
        ExponentKind::EnvironmentN => n_grid,
        ExponentKind::AreaSizeU => u_grid,
    };
    if fixed_grid.is_present() {
        return Err(CliError::ProfileGridForFixedExponent {
            parameter: pair.fixed().parameter_name(),
        });
    }
    let grid_for = |kind| match kind {
        ExponentKind::GeographicX => x_grid,
        ExponentKind::EnvironmentN => n_grid,
        ExponentKind::AreaSizeU => u_grid,
    };
    let first = build_profile_axis(pair.first(), grid_for(pair.first()))?;
    let second = build_profile_axis(pair.second(), grid_for(pair.second()))?;
    let fixed_exponent = match pair.fixed() {
        ExponentKind::GeographicX => {
            distance_exponent.ok_or(CliError::MissingRequired("--distance-exponent"))?
        }
        ExponentKind::EnvironmentN => environment_distance_exponent
            .ok_or(CliError::MissingRequired("--environment-distance-exponent"))?,
        ExponentKind::AreaSizeU => {
            area_exponent.ok_or(CliError::MissingRequired("--area-exponent"))?
        }
    };
    optimization.range_size =
        resolve_range_size_config(optimization.range_size, mx01, mx01y, mx01s, mx01v, mx01j);
    if dispersal_strata_path.is_some() {
        if distance_matrix_path.is_some()
            || environment_distance_matrix_path.is_some()
            || area_sizes_path.is_some()
            || dispersal_multipliers_path.is_some()
        {
            return Err(CliError::ConflictingStratifiedRawModifiers);
        }
    } else if distance_matrix_path.is_none() {
        return Err(CliError::MissingRequired("--distance-matrix"));
    } else if environment_distance_matrix_path.is_none() {
        return Err(CliError::MissingRequired("--environment-distance-matrix"));
    } else if area_sizes_path.is_none() {
        return Err(CliError::MissingRequired("--area-sizes"));
    }

    Ok(Command::PairProfile(PairProfileConfig {
        pair,
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        min_branch_length: 0.0,
        distance_matrix_path,
        environment_distance_matrix_path,
        area_sizes_path,
        dispersal_multipliers_path,
        dispersal_strata_path,
        fixed_exponent,
        max_range_size,
        include_null_range,
        root_prior,
        profile: biogeo_core::DecPairProfileConfig {
            de: optimization,
            first,
            second,
            support_delta,
        },
    }))
}

fn parse_decj_optimize_args(args: Vec<String>, preset: FixedPreset) -> Result<Command, CliError> {
    let mut tree_path = None;
    let mut ranges_path = None;
    let mut max_range_size = None;
    let mut dispersal_multipliers_path = None;
    let mut dispersal_strata_path = None;
    let mut distance_matrix_path = None;
    let mut distance_exponent = None;
    let mut environment_distance_matrix_path = None;
    let mut environment_distance_exponent = None;
    let mut extirpation_multipliers_path = None;
    let mut area_sizes_path = None;
    let mut area_exponent = None;
    let mut include_null_range = false;
    let mut root_prior = RootPriorKind::Flat;
    let mut ancestral_probs = false;
    let mut split_probs = false;
    let mut optimization = preset.default_j_optimization();
    let mut mx01 = None;
    let mut mx01y = None;
    let mut mx01s = None;
    let mut mx01v = None;
    let mut mx01j = None;

    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--tree" => tree_path = Some(PathBuf::from(take_value(&mut iter, "--tree")?)),
            "--ranges" => ranges_path = Some(PathBuf::from(take_value(&mut iter, "--ranges")?)),
            "--max-range-size" => {
                max_range_size = Some(parse_u8(
                    "--max-range-size",
                    take_value(&mut iter, "--max-range-size")?,
                )?)
            }
            "--dispersal-multipliers" => {
                dispersal_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--dispersal-multipliers",
                )?))
            }
            "--dispersal-strata" => {
                dispersal_strata_path =
                    Some(PathBuf::from(take_value(&mut iter, "--dispersal-strata")?))
            }
            "--distance-matrix" => {
                distance_matrix_path =
                    Some(PathBuf::from(take_value(&mut iter, "--distance-matrix")?))
            }
            "--distance-exponent" => {
                distance_exponent = Some(parse_float(
                    "--distance-exponent",
                    take_value(&mut iter, "--distance-exponent")?,
                )?)
            }
            "--environment-distance-matrix" => {
                environment_distance_matrix_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--environment-distance-matrix",
                )?))
            }
            "--environment-distance-exponent" => {
                environment_distance_exponent = Some(parse_float(
                    "--environment-distance-exponent",
                    take_value(&mut iter, "--environment-distance-exponent")?,
                )?)
            }
            "--extirpation-multipliers" => {
                extirpation_multipliers_path = Some(PathBuf::from(take_value(
                    &mut iter,
                    "--extirpation-multipliers",
                )?))
            }
            "--area-sizes" => {
                area_sizes_path = Some(PathBuf::from(take_value(&mut iter, "--area-sizes")?))
            }
            "--area-exponent" => {
                area_exponent = Some(parse_float(
                    "--area-exponent",
                    take_value(&mut iter, "--area-exponent")?,
                )?)
            }
            "--include-null-range" => include_null_range = true,
            "--root-prior" => {
                root_prior = parse_root_prior(take_value(&mut iter, "--root-prior")?)?
            }
            "--ancestral-probs" => ancestral_probs = true,
            "--split-probs" => split_probs = true,
            "--mx01" => mx01 = Some(parse_float("--mx01", take_value(&mut iter, "--mx01")?)?),
            "--mx01y" => mx01y = Some(parse_float("--mx01y", take_value(&mut iter, "--mx01y")?)?),
            "--mx01s" => mx01s = Some(parse_float("--mx01s", take_value(&mut iter, "--mx01s")?)?),
            "--mx01v" => mx01v = Some(parse_float("--mx01v", take_value(&mut iter, "--mx01v")?)?),
            "--mx01j" => mx01j = Some(parse_float("--mx01j", take_value(&mut iter, "--mx01j")?)?),
            "--init-d" => {
                optimization.initial_d =
                    parse_float("--init-d", take_value(&mut iter, "--init-d")?)?
            }
            "--init-e" => {
                optimization.initial_e =
                    parse_float("--init-e", take_value(&mut iter, "--init-e")?)?
            }
            "--init-j" => {
                optimization.initial_j =
                    parse_float("--init-j", take_value(&mut iter, "--init-j")?)?
            }
            "--min-rate" => {
                optimization.min_rate =
                    parse_float("--min-rate", take_value(&mut iter, "--min-rate")?)?
            }
            "--max-rate" => {
                optimization.max_rate =
                    parse_float("--max-rate", take_value(&mut iter, "--max-rate")?)?
            }
            "--min-j" => {
                optimization.min_j = parse_float("--min-j", take_value(&mut iter, "--min-j")?)?
            }
            "--max-j" => {
                optimization.max_j = parse_float("--max-j", take_value(&mut iter, "--max-j")?)?
            }
            "--initial-log-step" => {
                optimization.initial_log_step = parse_float(
                    "--initial-log-step",
                    take_value(&mut iter, "--initial-log-step")?,
                )?
            }
            "--tolerance" => {
                optimization.tolerance =
                    parse_float("--tolerance", take_value(&mut iter, "--tolerance")?)?
            }
            "--max-iterations" => {
                optimization.max_iterations = parse_usize(
                    "--max-iterations",
                    take_value(&mut iter, "--max-iterations")?,
                )?
            }
            "--multi-start-points" => {
                optimization.multi_start_points_per_axis = parse_usize(
                    "--multi-start-points",
                    take_value(&mut iter, "--multi-start-points")?,
                )?
            }
            _ if arg.starts_with('-') => return Err(CliError::UnknownOption(arg)),
            _ => return Err(CliError::UnexpectedArgument(arg)),
        }
    }

    if dispersal_multipliers_path.is_some() && dispersal_strata_path.is_some() {
        return Err(CliError::ConflictingDispersalModifiers);
    }
    if distance_matrix_path.is_some() && distance_exponent.is_none()
        || dispersal_strata_path.is_none()
            && distance_matrix_path.is_none()
            && distance_exponent.is_some()
    {
        return Err(CliError::IncompleteDistanceModifier);
    }
    if environment_distance_matrix_path.is_some() && environment_distance_exponent.is_none()
        || dispersal_strata_path.is_none()
            && environment_distance_matrix_path.is_none()
            && environment_distance_exponent.is_some()
    {
        return Err(CliError::IncompleteEnvironmentDistanceModifier);
    }
    if area_sizes_path.is_some() && area_exponent.is_none()
        || dispersal_strata_path.is_none() && area_sizes_path.is_none() && area_exponent.is_some()
    {
        return Err(CliError::IncompleteAreaSizeModifier);
    }
    if area_sizes_path.is_some() && extirpation_multipliers_path.is_some() {
        return Err(CliError::ConflictingExtirpationModifiers);
    }
    let preset_j_upper = preset.j_upper_exclusive();
    if optimization.max_j >= preset_j_upper {
        return Err(CliError::InvalidPresetJUpperBound {
            preset: preset.plus_j_name(),
            max_j: optimization.max_j,
            upper_exclusive: preset_j_upper,
        });
    }

    optimization.range_size = resolve_range_size_config(
        preset.default_range_size(),
        mx01,
        mx01y,
        mx01s,
        mx01v,
        mx01j,
    );

    Ok(Command::DecJOptimize(DecJOptimizeConfig {
        preset,
        tree_path: tree_path.ok_or(CliError::MissingRequired("--tree"))?,
        tree_name: None,
        ranges_path: ranges_path.ok_or(CliError::MissingRequired("--ranges"))?,
        use_ambiguities: false,
        min_branch_length: 0.0,
        max_range_size,
        dispersal_multipliers_path,
        dispersal_strata_path,
        distance_matrix_path,
        distance_exponent,
        environment_distance_matrix_path,
        environment_distance_exponent,
        extirpation_multipliers_path,
        area_sizes_path,
        area_exponent,
        include_null_range,
        root_prior,
        ancestral_probs,
        split_probs,
        optimization,
    }))
}

fn resolve_range_size_config(
    mut config: biogeo_core::CladogenesisRangeSizeConfig,
    mx01: Option<f64>,
    mx01y: Option<f64>,
    mx01s: Option<f64>,
    mx01v: Option<f64>,
    mx01j: Option<f64>,
) -> biogeo_core::CladogenesisRangeSizeConfig {
    if let Some(mx01) = mx01 {
        config = biogeo_core::CladogenesisRangeSizeConfig::linked(mx01);
    }
    config.mx01y = mx01y.unwrap_or(config.mx01y);
    config.mx01s = mx01s.unwrap_or(config.mx01s);
    config.mx01v = mx01v.unwrap_or(config.mx01v);
    config.mx01j = mx01j.unwrap_or(config.mx01j);
    config
}

fn take_value(
    iter: &mut impl Iterator<Item = String>,
    option: &'static str,
) -> Result<String, CliError> {
    iter.next().ok_or(CliError::MissingValue(option))
}

fn parse_float(option: &'static str, value: String) -> Result<f64, CliError> {
    value
        .parse::<f64>()
        .map_err(|source| CliError::InvalidFloat {
            option,
            value,
            source,
        })
}

fn parse_float_list(option: &'static str, value: String) -> Result<Vec<f64>, CliError> {
    value
        .split(',')
        .map(|item| parse_float(option, item.trim().to_owned()))
        .collect()
}

fn parse_u8(option: &'static str, value: String) -> Result<u8, CliError> {
    value.parse::<u8>().map_err(|source| CliError::InvalidU8 {
        option,
        value,
        source,
    })
}

fn parse_usize(option: &'static str, value: String) -> Result<usize, CliError> {
    value
        .parse::<usize>()
        .map_err(|source| CliError::InvalidUsize {
            option,
            value,
            source,
        })
}

fn parse_positive_usize(option: &'static str, value: String) -> Result<usize, CliError> {
    let parsed = parse_usize(option, value)?;
    if parsed == 0 {
        return Err(CliError::NonPositiveBsmOption(option));
    }
    Ok(parsed)
}

fn parse_bsm_duration(option: &'static str, value: String) -> Result<Duration, CliError> {
    let seconds = parse_float(option, value)?;
    Duration::try_from_secs_f64(seconds)
        .map_err(|_| CliError::InvalidBsmTimeLimit { option, seconds })
}

fn parse_bsm_threads(value: String) -> Result<BsmThreadSelection, CliError> {
    if value.eq_ignore_ascii_case("auto") {
        return Ok(BsmThreadSelection::Auto);
    }
    let parsed = value
        .parse::<usize>()
        .map_err(|_| CliError::InvalidBsmThreads(value.clone()))?;
    if parsed == 0 {
        return Err(CliError::NonPositiveBsmOption("--bsm-threads"));
    }
    Ok(BsmThreadSelection::Fixed(parsed))
}

fn parse_bsm_output_level(value: String) -> Result<BsmOutputLevel, CliError> {
    match value.as_str() {
        "legacy" => Ok(BsmOutputLevel::Legacy),
        "full" => Ok(BsmOutputLevel::Full),
        "compact" => Ok(BsmOutputLevel::Compact),
        "summary" => Ok(BsmOutputLevel::Summary),
        _ => Err(CliError::InvalidBsmOutputLevel(value)),
    }
}

fn parse_u64(option: &'static str, value: String) -> Result<u64, CliError> {
    value.parse::<u64>().map_err(|source| CliError::InvalidU64 {
        option,
        value,
        source,
    })
}

fn parse_root_prior(value: String) -> Result<RootPriorKind, CliError> {
    match value.as_str() {
        "flat" => Ok(RootPriorKind::Flat),
        "equal" => Ok(RootPriorKind::Equal),
        _ => Err(CliError::InvalidRootPrior(value)),
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Command {
    Help,
    TopicHelp(String),
    Version,
    EngineInfo,
    ConvertTree(ConvertTreeConfig),
    ConvertRanges(ConvertRangesConfig),
    ConvertBioGeoBearsStrata(ConvertBioGeoBearsStrataConfig),
    FossilPlace(fossil_placement::FossilPlacementRunConfig),
    ValidateInputs(ValidateInputsConfig),
    Fixed(FixedModelConfig),
    DeOptimize(DeOptimizeConfig),
    ExponentOptimize(ExponentOptimizeConfig),
    XnuOptimize(XnuOptimizeConfig),
    PairProfile(PairProfileConfig),
    DecJOptimize(DecJOptimizeConfig),
    ParameterTemplate(ParameterTemplateConfig),
    AnalysisTemplate(AnalysisTemplateConfig),
    AnalysisPlan(AnalysisPlanConfig),
    AnalysisRun(AnalysisRunConfig),
    AnalysisWorkflow(AnalysisWorkflowConfig),
    ModelWorkflowPlan(ModelWorkflowPlanConfig),
    ModelWorkflow(ModelWorkflowConfig),
    ParameterModel(ParameterModelConfig),
    ParameterBatch(ParameterBatchConfig),
    DatasetBatch(DatasetBatchConfig),
    ParameterBsm(ParameterBsmConfig),
    BsmInspect(BsmInspectConfig),
    AnalysisResultInspect(AnalysisResultInspectConfig),
    AnalysisResultMigrate(AnalysisResultMigrateConfig),
    InputBundleInspect(InputBundleInspectConfig),
}

impl Command {
    fn with_min_branch_length(mut self, value: f64) -> Result<Self, CliError> {
        match &mut self {
            Self::ValidateInputs(config) => config.min_branch_length = value,
            Self::Fixed(config) => config.min_branch_length = value,
            Self::DeOptimize(config) => config.min_branch_length = value,
            Self::ExponentOptimize(config) => config.min_branch_length = value,
            Self::XnuOptimize(config) => config.min_branch_length = value,
            Self::PairProfile(config) => config.min_branch_length = value,
            Self::DecJOptimize(config) => config.min_branch_length = value,
            Self::ParameterModel(config) => config.min_branch_length = value,
            Self::ParameterBatch(config) => {
                config.template.min_branch_length = value;
                config.invocation_tokens.extend([
                    "--min-branch-length-bits".to_string(),
                    format!("{:016x}", value.to_bits()),
                ]);
            }
            Self::Help
            | Self::TopicHelp(_)
            | Self::Version
            | Self::EngineInfo
            | Self::ConvertTree(_)
            | Self::ConvertRanges(_)
            | Self::ConvertBioGeoBearsStrata(_)
            | Self::FossilPlace(_)
            | Self::ParameterTemplate(_)
            | Self::AnalysisTemplate(_)
            | Self::AnalysisPlan(_)
            | Self::AnalysisRun(_)
            | Self::AnalysisWorkflow(_)
            | Self::ModelWorkflowPlan(_)
            | Self::ModelWorkflow(_)
            | Self::DatasetBatch(_)
            | Self::ParameterBsm(_)
            | Self::BsmInspect(_)
            | Self::AnalysisResultInspect(_)
            | Self::AnalysisResultMigrate(_)
            | Self::InputBundleInspect(_) => {
                return Err(CliError::UnknownOption("--min-branch-length".to_string()));
            }
        }
        Ok(self)
    }

    fn with_missing_branch_length_fill(mut self, value: f64) -> Result<Self, CliError> {
        match &mut self {
            Self::ConvertTree(config) => config.missing_branch_length_fill = Some(value),
            Self::ValidateInputs(config) => config.missing_branch_length_fill = Some(value),
            Self::ParameterModel(config) => config.missing_branch_length_fill = Some(value),
            Self::ParameterBatch(config) => {
                config.template.missing_branch_length_fill = Some(value);
                config.invocation_tokens.extend([
                    "--fill-missing-branch-length-bits".to_string(),
                    format!("{:016x}", value.to_bits()),
                ]);
            }
            Self::Help
            | Self::TopicHelp(_)
            | Self::Version
            | Self::EngineInfo
            | Self::ConvertRanges(_)
            | Self::ConvertBioGeoBearsStrata(_)
            | Self::FossilPlace(_)
            | Self::Fixed(_)
            | Self::DeOptimize(_)
            | Self::ExponentOptimize(_)
            | Self::XnuOptimize(_)
            | Self::PairProfile(_)
            | Self::DecJOptimize(_)
            | Self::ParameterTemplate(_)
            | Self::AnalysisTemplate(_)
            | Self::AnalysisPlan(_)
            | Self::AnalysisRun(_)
            | Self::AnalysisWorkflow(_)
            | Self::ModelWorkflowPlan(_)
            | Self::ModelWorkflow(_)
            | Self::DatasetBatch(_)
            | Self::ParameterBsm(_)
            | Self::BsmInspect(_)
            | Self::AnalysisResultInspect(_)
            | Self::AnalysisResultMigrate(_)
            | Self::InputBundleInspect(_) => {
                return Err(CliError::UnknownOption(
                    "--fill-missing-branch-length".to_string(),
                ));
            }
        }
        Ok(self)
    }

    fn with_tree_name(mut self, tree_name: String) -> Result<Self, CliError> {
        match &mut self {
            Self::ConvertTree(config) => config.tree_name = Some(tree_name),
            Self::FossilPlace(config) => config.tree_name = Some(tree_name),
            Self::ValidateInputs(config) => config.tree_name = Some(tree_name),
            Self::Fixed(config) => config.tree_name = Some(tree_name),
            Self::DeOptimize(config) => config.tree_name = Some(tree_name),
            Self::ExponentOptimize(config) => config.tree_name = Some(tree_name),
            Self::XnuOptimize(config) => config.tree_name = Some(tree_name),
            Self::PairProfile(config) => config.tree_name = Some(tree_name),
            Self::DecJOptimize(config) => config.tree_name = Some(tree_name),
            Self::ParameterModel(config) => config.tree_name = Some(tree_name),
            Self::ParameterBatch(config) => {
                config.template.tree_name = Some(tree_name.clone());
                config
                    .invocation_tokens
                    .extend(["--tree-name".to_string(), tree_name]);
            }
            Self::Help
            | Self::TopicHelp(_)
            | Self::Version
            | Self::EngineInfo
            | Self::ConvertRanges(_)
            | Self::ConvertBioGeoBearsStrata(_)
            | Self::ParameterTemplate(_)
            | Self::AnalysisTemplate(_)
            | Self::AnalysisPlan(_)
            | Self::AnalysisRun(_)
            | Self::AnalysisWorkflow(_)
            | Self::ModelWorkflowPlan(_)
            | Self::ModelWorkflow(_)
            | Self::DatasetBatch(_)
            | Self::ParameterBsm(_)
            | Self::BsmInspect(_)
            | Self::AnalysisResultInspect(_)
            | Self::AnalysisResultMigrate(_)
            | Self::InputBundleInspect(_) => {
                return Err(CliError::UnknownOption("--tree-name".to_string()));
            }
        }
        Ok(self)
    }

    fn with_ambiguities(mut self) -> Result<Self, CliError> {
        match &mut self {
            Self::ValidateInputs(config) => config.use_ambiguities = true,
            Self::Fixed(config) => config.use_ambiguities = true,
            Self::DeOptimize(config) => config.use_ambiguities = true,
            Self::ExponentOptimize(config) => config.use_ambiguities = true,
            Self::XnuOptimize(config) => config.use_ambiguities = true,
            Self::PairProfile(config) => config.use_ambiguities = true,
            Self::DecJOptimize(config) => config.use_ambiguities = true,
            Self::ParameterModel(config) if !config.use_detection_model => {
                config.use_ambiguities = true;
            }
            Self::ParameterModel(_) => return Err(CliError::ConflictingTipObservationInputs),
            Self::ParameterBatch(config) if !config.template.use_detection_model => {
                config.template.use_ambiguities = true;
                config
                    .invocation_tokens
                    .push("--use-ambiguities".to_string());
            }
            Self::ParameterBatch(_) => return Err(CliError::ConflictingTipObservationInputs),
            Self::Help
            | Self::TopicHelp(_)
            | Self::Version
            | Self::EngineInfo
            | Self::ConvertTree(_)
            | Self::ConvertRanges(_)
            | Self::ConvertBioGeoBearsStrata(_)
            | Self::FossilPlace(_)
            | Self::ParameterTemplate(_)
            | Self::AnalysisTemplate(_)
            | Self::AnalysisPlan(_)
            | Self::AnalysisRun(_)
            | Self::AnalysisWorkflow(_)
            | Self::ModelWorkflowPlan(_)
            | Self::ModelWorkflow(_)
            | Self::DatasetBatch(_)
            | Self::ParameterBsm(_)
            | Self::BsmInspect(_)
            | Self::AnalysisResultInspect(_)
            | Self::AnalysisResultMigrate(_)
            | Self::InputBundleInspect(_) => {
                return Err(CliError::UnknownOption("--use-ambiguities".to_string()));
            }
        }
        Ok(self)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ConvertTreeConfig {
    tree_path: PathBuf,
    tree_name: Option<String>,
    missing_branch_length_fill: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
struct ConvertRangesConfig {
    ranges_path: PathBuf,
    input_format: legacy_import::RangeSourceFormat,
    taxon_column: Option<String>,
    taxon_map_path: Option<PathBuf>,
    area_map_path: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
struct ConvertBioGeoBearsStrataConfig {
    time_boundaries_path: PathBuf,
    dispersal_matrices_path: Option<PathBuf>,
    adjacency_matrices_path: Option<PathBuf>,
    adjacency_range_rule: legacy_import::AdjacencyRangeRule,
    max_range_size: Option<usize>,
    output_dir: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct ValidateInputsConfig {
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    min_branch_length: f64,
    missing_branch_length_fill: Option<f64>,
    tip_age_tolerance: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParameterRunMode {
    Evaluate,
    Optimize,
}

impl ParameterRunMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Evaluate => "evaluate",
            Self::Optimize => "optimize",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ParameterTemplateConfig {
    preset: biogeo_core::BioGeoBearsPreset,
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisTemplateConfig {
    preset: biogeo_core::BioGeoBearsPreset,
    mode: analysis_request::AnalysisRequestMode,
    output_dir_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisPlanConfig {
    request_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisRunConfig {
    request_path: PathBuf,
    output_dir_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisWorkflowConfig {
    request_path: PathBuf,
    output_dir_path: PathBuf,
    resume: bool,
    deep_inspection: bool,
    bsm: ParameterBsmConfig,
}

#[derive(Clone, Debug, PartialEq)]
struct ModelWorkflowPlanConfig {
    request_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct ModelWorkflowConfig {
    request_path: PathBuf,
    output_dir_path: PathBuf,
    resume: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ParameterModelConfig {
    mode: ParameterRunMode,
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: Option<PathBuf>,
    detections_path: Option<PathBuf>,
    controls_path: Option<PathBuf>,
    use_detection_model: bool,
    use_ambiguities: bool,
    parameters_path: PathBuf,
    source_request_path: Option<PathBuf>,
    analysis_result_dir_path: Option<PathBuf>,
    min_branch_length: f64,
    missing_branch_length_fill: Option<f64>,
    max_range_size: Option<u8>,
    max_states: Option<usize>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    distance_matrix_path: Option<PathBuf>,
    environment_distance_matrix_path: Option<PathBuf>,
    extirpation_multipliers_path: Option<PathBuf>,
    area_sizes_path: Option<PathBuf>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    ancestral_probs: bool,
    split_probs: bool,
    optimization: biogeo_core::ParameterOptimizationConfig,
}

#[derive(Clone, Debug, PartialEq)]
struct ParameterBatchConfig {
    manifest_path: PathBuf,
    output_dir_path: PathBuf,
    resume: bool,
    invocation_tokens: Vec<String>,
    template: ParameterModelConfig,
}

#[derive(Clone, Debug, PartialEq)]
struct DatasetBatchConfig {
    manifest_path: PathBuf,
    output_dir_path: PathBuf,
    resume: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct ParameterBsmConfig {
    analysis_result_dir_path: PathBuf,
    bsm_samples: usize,
    bsm_output_dir_path: Option<PathBuf>,
    bsm_output_level: BsmOutputLevel,
    execution_request: BsmExecutionRequest,
    bsm_resume: bool,
    bsm_interactive: bool,
    seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
struct BsmInspectConfig {
    bsm_result_dir_path: PathBuf,
    deep: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisResultInspectConfig {
    analysis_result_dir_path: PathBuf,
    replay: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct AnalysisResultMigrateConfig {
    analysis_result_dir_path: PathBuf,
    output_dir_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct InputBundleInspectConfig {
    input_bundle_dir_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
struct FixedModelConfig {
    preset: FixedPreset,
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    d: f64,
    e: f64,
    j: f64,
    range_size: biogeo_core::CladogenesisRangeSizeConfig,
    min_branch_length: f64,
    max_range_size: Option<u8>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    distance_matrix_path: Option<PathBuf>,
    distance_exponent: Option<f64>,
    environment_distance_matrix_path: Option<PathBuf>,
    environment_distance_exponent: Option<f64>,
    extirpation_multipliers_path: Option<PathBuf>,
    area_sizes_path: Option<PathBuf>,
    area_exponent: Option<f64>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    ancestral_probs: bool,
    split_probs: bool,
    traceback_samples: usize,
    bsm_samples: usize,
    bsm_output_dir_path: Option<PathBuf>,
    bsm_output_level: BsmOutputLevel,
    bsm_threads: BsmThreadSelection,
    bsm_max_in_flight: Option<usize>,
    bsm_max_events_per_sample: Option<usize>,
    bsm_max_events_total: Option<usize>,
    bsm_memory_budget_mb: Option<usize>,
    bsm_shard_samples: Option<usize>,
    bsm_checkpoint_samples: Option<usize>,
    bsm_resume: bool,
    bsm_time_limit: Option<Duration>,
    bsm_interactive: bool,
    seed: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BsmThreadSelection {
    Auto,
    Fixed(usize),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
enum BsmOutputLevel {
    #[default]
    Legacy,
    Full,
    Compact,
    Summary,
}

impl BsmOutputLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Summary => "summary",
        }
    }

    fn is_v2(self) -> bool {
        self != Self::Legacy
    }

    fn is_compact(self) -> bool {
        matches!(self, Self::Compact | Self::Summary)
    }

    fn includes_path_details(self) -> bool {
        self != Self::Summary
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BsmExecutionRequest {
    thread_selection: BsmThreadSelection,
    max_in_flight: Option<usize>,
    max_events_per_sample: Option<usize>,
    max_events_total: Option<usize>,
    memory_budget_mb: Option<usize>,
    shard_samples: Option<usize>,
    checkpoint_samples: Option<usize>,
    time_limit: Option<Duration>,
}

impl BsmExecutionRequest {
    fn from_config(config: &FixedModelConfig) -> Self {
        Self {
            thread_selection: config.bsm_threads,
            max_in_flight: config.bsm_max_in_flight,
            max_events_per_sample: config.bsm_max_events_per_sample,
            max_events_total: config.bsm_max_events_total,
            memory_budget_mb: config.bsm_memory_budget_mb,
            shard_samples: config.bsm_shard_samples,
            checkpoint_samples: config.bsm_checkpoint_samples,
            time_limit: config.bsm_time_limit,
        }
    }
}

impl Default for BsmExecutionRequest {
    fn default() -> Self {
        Self {
            thread_selection: BsmThreadSelection::Auto,
            max_in_flight: None,
            max_events_per_sample: None,
            max_events_total: None,
            memory_budget_mb: None,
            shard_samples: None,
            checkpoint_samples: None,
            time_limit: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedBsmExecution {
    available_parallelism: usize,
    threads: usize,
    max_in_flight: usize,
    checkpoint_samples: usize,
    max_events_per_sample: Option<usize>,
    max_events_total: Option<usize>,
    memory_budget_mb: Option<usize>,
    shard_samples: Option<usize>,
    retained_bytes_per_sample_upper_bound: Option<usize>,
    buffered_history_bytes_upper_bound: Option<usize>,
    time_limit: Option<Duration>,
}

const DEFAULT_BSM_CHECKPOINT_SAMPLES: usize = 1024;

fn format_optional_limit(limit: Option<usize>) -> String {
    limit.map_or_else(|| "unlimited".to_string(), |value| value.to_string())
}

fn format_optional_estimate(estimate: Option<usize>) -> String {
    estimate.map_or_else(|| "not_computed".to_string(), |value| value.to_string())
}

fn bsm_stream_format(
    execution: ResolvedBsmExecution,
    output_level: BsmOutputLevel,
) -> &'static str {
    match (output_level, execution.shard_samples.is_some()) {
        (BsmOutputLevel::Legacy, false) => BSM_STREAM_FORMAT,
        (BsmOutputLevel::Legacy, true) => BSM_SHARDED_STREAM_FORMAT,
        (BsmOutputLevel::Full, false) => BSM_FULL_STREAM_FORMAT_V2,
        (BsmOutputLevel::Full, true) => BSM_FULL_SHARDED_STREAM_FORMAT_V2,
        (BsmOutputLevel::Compact, false) => BSM_COMPACT_STREAM_FORMAT_V2,
        (BsmOutputLevel::Compact, true) => BSM_COMPACT_SHARDED_STREAM_FORMAT_V2,
        (BsmOutputLevel::Summary, false) => BSM_SUMMARY_STREAM_FORMAT_V2,
        (BsmOutputLevel::Summary, true) => BSM_SUMMARY_SHARDED_STREAM_FORMAT_V2,
    }
}

impl ResolvedBsmExecution {
    fn with_parallel_plan(mut self, plan: biogeo_core::StochasticMapParallelPlan) -> Self {
        self.threads = plan.threads;
        self.max_in_flight = plan.max_in_flight;
        self.retained_bytes_per_sample_upper_bound = plan.retained_bytes_per_sample_upper_bound;
        self.buffered_history_bytes_upper_bound = plan.buffered_history_bytes_upper_bound;
        self
    }

    fn memory_budget_bytes(self) -> Result<Option<usize>, CliError> {
        self.memory_budget_mb
            .map(|megabytes| {
                megabytes
                    .checked_mul(1024 * 1024)
                    .ok_or(CliError::BsmMemoryBudgetOverflow { megabytes })
            })
            .transpose()
    }
}

fn format_optional_duration(limit: Option<Duration>) -> String {
    limit.map_or_else(
        || "unlimited".to_string(),
        |duration| duration.as_secs_f64().to_string(),
    )
}

fn resolve_bsm_execution(
    config: &FixedModelConfig,
) -> Result<Option<ResolvedBsmExecution>, CliError> {
    let available_parallelism = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    resolve_bsm_execution_with_available(
        BsmExecutionRequest::from_config(config),
        config.bsm_samples,
        available_parallelism,
    )
}

fn resolve_bsm_execution_with_available(
    request: BsmExecutionRequest,
    sample_count: usize,
    available_parallelism: usize,
) -> Result<Option<ResolvedBsmExecution>, CliError> {
    if sample_count == 0 {
        return Ok(None);
    }

    let available_parallelism = available_parallelism.max(1);
    let requested_threads = match request.thread_selection {
        BsmThreadSelection::Auto => available_parallelism,
        BsmThreadSelection::Fixed(threads) => threads,
    };
    let threads = requested_threads.min(sample_count);
    if threads == 0 {
        return Err(CliError::NonPositiveBsmOption("--bsm-threads"));
    }

    let max_in_flight = request
        .max_in_flight
        .unwrap_or_else(|| threads.saturating_mul(2))
        .min(sample_count);
    if max_in_flight < threads {
        return Err(CliError::BsmMaxInFlightBelowThreads {
            threads,
            max_in_flight,
        });
    }
    let checkpoint_samples = request
        .checkpoint_samples
        .unwrap_or_else(|| max_in_flight.max(DEFAULT_BSM_CHECKPOINT_SAMPLES))
        .min(sample_count);
    let shard_samples = request.shard_samples.map(|value| value.min(sample_count));
    if let Some(megabytes) = request.memory_budget_mb {
        megabytes
            .checked_mul(1024 * 1024)
            .ok_or(CliError::BsmMemoryBudgetOverflow { megabytes })?;
    }

    Ok(Some(ResolvedBsmExecution {
        available_parallelism,
        threads,
        max_in_flight,
        checkpoint_samples,
        max_events_per_sample: request.max_events_per_sample,
        max_events_total: request.max_events_total,
        memory_budget_mb: request.memory_budget_mb,
        shard_samples,
        retained_bytes_per_sample_upper_bound: None,
        buffered_history_bytes_upper_bound: None,
        time_limit: request.time_limit,
    }))
}

fn resolve_bsm_execution_control(
    config: &FixedModelConfig,
    cancellation: Option<biogeo_core::StochasticMapCancellationToken>,
    pause: Option<biogeo_core::StochasticMapPauseToken>,
) -> Result<Option<biogeo_core::StochasticMapExecutionControl>, CliError> {
    resolve_bsm_execution_control_values(
        config.bsm_samples,
        config.bsm_time_limit,
        cancellation,
        pause,
    )
}

fn resolve_bsm_execution_control_values(
    sample_count: usize,
    time_limit: Option<Duration>,
    cancellation: Option<biogeo_core::StochasticMapCancellationToken>,
    pause: Option<biogeo_core::StochasticMapPauseToken>,
) -> Result<Option<biogeo_core::StochasticMapExecutionControl>, CliError> {
    if sample_count == 0 {
        return Ok(None);
    }
    let deadline = time_limit
        .map(|limit| {
            Instant::now()
                .checked_add(limit)
                .ok_or(CliError::BsmTimeLimitOverflow(limit))
        })
        .transpose()?;
    if cancellation.is_none() && deadline.is_none() && pause.is_none() {
        return Ok(None);
    }
    let mut control =
        biogeo_core::StochasticMapExecutionControl::new(cancellation.unwrap_or_default(), deadline);
    if let Some(pause) = pause {
        control = control.with_pause_token(pause);
    }
    Ok(Some(control))
}

fn bsm_parallel_options(
    execution: ResolvedBsmExecution,
    control: Option<&biogeo_core::StochasticMapExecutionControl>,
    completed_anagenetic_events: usize,
) -> biogeo_core::StochasticMapParallelOptions {
    let options =
        biogeo_core::StochasticMapParallelOptions::new(execution.threads, execution.max_in_flight)
            .with_limits(biogeo_core::StochasticMapLimits::new(
                execution.max_events_per_sample,
            ))
            .with_task_limits(biogeo_core::StochasticMapTaskLimits::new(
                execution.max_events_total,
                completed_anagenetic_events,
            ))
            .with_max_buffered_history_bytes(
                execution
                    .memory_budget_bytes()
                    .expect("resolved BSM memory budget must fit usize"),
            );
    match control {
        Some(control) => options.with_execution_control(control.clone()),
        None => options,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FixedPreset {
    Dec,
    DivaLike,
    BayAreaLike,
}

impl FixedPreset {
    fn model_name(self, j: f64) -> &'static str {
        match self {
            Self::Dec if j > 0.0 => "DEC+J",
            Self::DivaLike if j > 0.0 => "DIVALIKE+J",
            Self::BayAreaLike if j > 0.0 => "BAYAREALIKE+J",
            Self::Dec => "DEC",
            Self::DivaLike => "DIVALIKE",
            Self::BayAreaLike => "BAYAREALIKE",
        }
    }

    fn default_range_size(self) -> biogeo_core::CladogenesisRangeSizeConfig {
        match self {
            Self::Dec => biogeo_core::CladogenesisConfig::preset_dec().range_size,
            Self::DivaLike => biogeo_core::CladogenesisConfig::preset_divalike().range_size,
            Self::BayAreaLike => biogeo_core::CladogenesisConfig::preset_bayarealike().range_size,
        }
    }

    fn build_model(
        self,
        d: f64,
        e: f64,
        j: f64,
    ) -> Result<biogeo_core::ModelConfig, biogeo_core::DecModelError> {
        match self {
            Self::Dec => biogeo_core::ModelConfig::preset_dec_j(d, e, j),
            Self::DivaLike => biogeo_core::ModelConfig::preset_divalike_j(d, e, j),
            Self::BayAreaLike => biogeo_core::ModelConfig::preset_bayarealike_j(d, e, j),
        }
    }

    fn default_j_optimization(self) -> biogeo_core::DecJOptimizationConfig {
        match self {
            Self::Dec => biogeo_core::DecJOptimizationConfig::default(),
            Self::DivaLike => biogeo_core::DecJOptimizationConfig::for_divalike(),
            Self::BayAreaLike => biogeo_core::DecJOptimizationConfig::for_bayarealike(),
        }
    }

    fn plus_j_name(self) -> &'static str {
        match self {
            Self::Dec => "DEC+J",
            Self::DivaLike => "DIVALIKE+J",
            Self::BayAreaLike => "BAYAREALIKE+J",
        }
    }

    fn j_upper_exclusive(self) -> f64 {
        match self {
            Self::Dec => 3.0,
            Self::DivaLike => 2.0,
            Self::BayAreaLike => 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct DeOptimizeConfig {
    preset: FixedPreset,
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    min_branch_length: f64,
    max_range_size: Option<u8>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    distance_matrix_path: Option<PathBuf>,
    distance_exponent: Option<f64>,
    environment_distance_matrix_path: Option<PathBuf>,
    environment_distance_exponent: Option<f64>,
    extirpation_multipliers_path: Option<PathBuf>,
    area_sizes_path: Option<PathBuf>,
    area_exponent: Option<f64>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    ancestral_probs: bool,
    split_probs: bool,
    optimization: biogeo_core::DecOptimizationConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ExponentKind {
    GeographicX,
    EnvironmentN,
    AreaSizeU,
}

impl ExponentKind {
    fn model_name(self) -> &'static str {
        match self {
            Self::GeographicX => "DEC+x",
            Self::EnvironmentN => "DEC+n",
            Self::AreaSizeU => "DEC+u",
        }
    }

    fn parameter_name(self) -> &'static str {
        match self {
            Self::GeographicX => "x",
            Self::EnvironmentN => "n",
            Self::AreaSizeU => "u",
        }
    }

    fn optimization_config(self) -> biogeo_core::DecExponentOptimizationConfig {
        match self {
            Self::GeographicX => biogeo_core::DecExponentOptimizationConfig::for_x(),
            Self::EnvironmentN => biogeo_core::DecExponentOptimizationConfig::for_n(),
            Self::AreaSizeU => biogeo_core::DecExponentOptimizationConfig::for_u(),
        }
    }

    fn grid_options(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::GeographicX => ("--x-min", "--x-max", "--x-points"),
            Self::EnvironmentN => ("--n-min", "--n-max", "--n-points"),
            Self::AreaSizeU => ("--u-min", "--u-max", "--u-points"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ExponentOptimizeConfig {
    kind: ExponentKind,
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    min_branch_length: f64,
    distance_matrix_path: Option<PathBuf>,
    distance_exponent: Option<f64>,
    environment_distance_matrix_path: Option<PathBuf>,
    environment_distance_exponent: Option<f64>,
    area_sizes_path: Option<PathBuf>,
    area_exponent: Option<f64>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    extirpation_multipliers_path: Option<PathBuf>,
    max_range_size: Option<u8>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    ancestral_probs: bool,
    split_probs: bool,
    optimization: biogeo_core::DecExponentOptimizationConfig,
}

#[derive(Clone, Debug, PartialEq)]
struct XnuOptimizeConfig {
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    min_branch_length: f64,
    distance_matrix_path: Option<PathBuf>,
    environment_distance_matrix_path: Option<PathBuf>,
    area_sizes_path: Option<PathBuf>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    max_range_size: Option<u8>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    ancestral_probs: bool,
    split_probs: bool,
    optimization: biogeo_core::DecXnuOptimizationConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProfilePair {
    Xn,
    Xu,
    Nu,
}

impl ProfilePair {
    fn first(self) -> ExponentKind {
        match self {
            Self::Xn | Self::Xu => ExponentKind::GeographicX,
            Self::Nu => ExponentKind::EnvironmentN,
        }
    }

    fn second(self) -> ExponentKind {
        match self {
            Self::Xn => ExponentKind::EnvironmentN,
            Self::Xu | Self::Nu => ExponentKind::AreaSizeU,
        }
    }

    fn fixed(self) -> ExponentKind {
        match self {
            Self::Xn => ExponentKind::AreaSizeU,
            Self::Xu => ExponentKind::EnvironmentN,
            Self::Nu => ExponentKind::GeographicX,
        }
    }

    fn exponents(self, first: f64, second: f64, fixed: f64) -> (f64, f64, f64) {
        match self {
            Self::Xn => (first, second, fixed),
            Self::Xu => (first, fixed, second),
            Self::Nu => (fixed, first, second),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct PairProfileConfig {
    pair: ProfilePair,
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    min_branch_length: f64,
    distance_matrix_path: Option<PathBuf>,
    environment_distance_matrix_path: Option<PathBuf>,
    area_sizes_path: Option<PathBuf>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    fixed_exponent: f64,
    max_range_size: Option<u8>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    profile: biogeo_core::DecPairProfileConfig,
}

#[derive(Clone, Debug, PartialEq)]
struct DecJOptimizeConfig {
    preset: FixedPreset,
    tree_path: PathBuf,
    tree_name: Option<String>,
    ranges_path: PathBuf,
    use_ambiguities: bool,
    min_branch_length: f64,
    max_range_size: Option<u8>,
    dispersal_multipliers_path: Option<PathBuf>,
    dispersal_strata_path: Option<PathBuf>,
    distance_matrix_path: Option<PathBuf>,
    distance_exponent: Option<f64>,
    environment_distance_matrix_path: Option<PathBuf>,
    environment_distance_exponent: Option<f64>,
    extirpation_multipliers_path: Option<PathBuf>,
    area_sizes_path: Option<PathBuf>,
    area_exponent: Option<f64>,
    include_null_range: bool,
    root_prior: RootPriorKind,
    ancestral_probs: bool,
    split_probs: bool,
    optimization: biogeo_core::DecJOptimizationConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RootPriorKind {
    Flat,
    Equal,
}

impl RootPriorKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Equal => "equal",
        }
    }

    fn to_core(self) -> biogeo_core::RootPrior<'static> {
        match self {
            Self::Flat => biogeo_core::RootPrior::Flat,
            Self::Equal => biogeo_core::RootPrior::Equal,
        }
    }
}

#[derive(Debug)]
enum CliError {
    UnknownCommand(String),
    UnknownOption(String),
    DuplicateOption(&'static str),
    UnexpectedArgument(String),
    MissingValue(&'static str),
    MissingRequired(&'static str),
    InvalidFloat {
        option: &'static str,
        value: String,
        source: ParseFloatError,
    },
    InvalidU8 {
        option: &'static str,
        value: String,
        source: ParseIntError,
    },
    InvalidUsize {
        option: &'static str,
        value: String,
        source: ParseIntError,
    },
    InvalidU64 {
        option: &'static str,
        value: String,
        source: ParseIntError,
    },
    InvalidRootPrior(String),
    InvalidBsmOutputLevel(String),
    InvalidErrorFormat(String),
    InvalidProgressFormat(String),
    LegacyImport(legacy_import::LegacyImportError),
    ProgressOutput(std::io::Error),
    TaskCancelled {
        operation: &'static str,
        attempt_path: Option<PathBuf>,
    },
    InvalidMinBranchLength(f64),
    InvalidMissingBranchLengthFill(f64),
    InvalidTipAgeTolerance(f64),
    TipStateConstraintViolations {
        tips: Vec<(usize, String, usize)>,
    },
    NonBinaryInputTree {
        nodes: Vec<(usize, usize)>,
    },
    InvalidParameterPreset(String),
    InvalidAnalysisRequestMode(String),
    AnalysisWorkflowOwnedOption(String),
    AnalysisWorkflowOutputExists(PathBuf),
    MissingAnalysisWorkflowOutput(PathBuf),
    InvalidAnalysisWorkflow {
        path: PathBuf,
        message: String,
    },
    AnalysisWorkflowRequestMismatch {
        expected: String,
        actual: String,
    },
    AnalysisRequest(analysis_request::AnalysisRequestError),
    ModelWorkflow(Box<model_workflow::ModelWorkflowError>),
    AnalysisPlanSizeOverflow(&'static str),
    StateSpaceLimitExceeded {
        estimated_states: usize,
        max_states: usize,
        num_areas: u8,
        max_range_size: u8,
        include_null_range: bool,
    },
    AnalysisPlanStrataCoverage {
        oldest_age: f64,
        root_age: f64,
    },
    ParameterOptimizationOptionRequiresOptimize,
    ConflictingTipObservationInputs,
    DetectionInputRequiresModel,
    ParameterTable(biogeo_core::ParameterTableParseError),
    Parameter(biogeo_core::ParameterError),
    ParameterModel(biogeo_core::BioGeoBearsModelError),
    ParameterOptimization(biogeo_core::ParameterOptimizationError),
    AnalysisResult(analysis_result::AnalysisResultError),
    FossilPlacement(fossil_placement::FossilPlacementCliError),
    ModelAverage(model_average::ModelAverageError),
    ModelBatch(Box<model_batch::ModelBatchError>),
    DatasetBatch(Box<dataset_batch::DatasetBatchError>),
    ModelBatchOwnedOption(String),
    ModelBatchJob {
        model_id: String,
        source: Box<CliError>,
    },
    ModelBatchFailures {
        failed: usize,
        attempt_path: PathBuf,
    },
    DatasetBatchJob {
        dataset_id: String,
        source: Box<CliError>,
    },
    DatasetBatchFailures {
        failed: usize,
        attempt_path: PathBuf,
    },
    DatasetBatchUnexpectedHelp {
        dataset_id: String,
    },
    AnalysisReplayMismatch {
        field: &'static str,
        expected: String,
        actual: String,
    },
    MissingParameterTableParameter(&'static str),
    UnknownParameterTableParameter(String),
    UnsupportedParameterSemantics {
        parameter: &'static str,
        required_value: f64,
    },
    StratifiedBranchLengthExponent,
    ParameterInputRequired {
        parameter: &'static str,
        option: &'static str,
    },
    ParameterEvaluateHasFree(Vec<String>),
    ParameterOptimizeHasNoFree,
    UnusedFreeParameter(String),
    ConflictingParameterModifierSources(&'static str),
    ConflictingDispersalModifiers,
    IncompleteDistanceModifier,
    IncompleteEnvironmentDistanceModifier,
    IncompleteAreaSizeModifier,
    ConflictingExtirpationModifiers,
    ConflictingHistorySamplingOptions,
    BsmOutputRequiresSamples,
    BsmExecutionRequiresSamples,
    BsmStreamOptionRequiresOutput(&'static str),
    InvalidBsmThreads(String),
    InvalidBsmTimeLimit {
        option: &'static str,
        seconds: f64,
    },
    BsmTimeLimitOverflow(Duration),
    NonPositiveBsmOption(&'static str),
    BsmMemoryBudgetRequiresPerSampleEventLimit,
    BsmMemoryBudgetOverflow {
        megabytes: usize,
    },
    BsmMemoryBudgetTooSmall {
        budget_bytes: usize,
        minimum_bytes: usize,
    },
    BsmRetainedHistorySizeOverflow,
    BsmMaxInFlightBelowThreads {
        threads: usize,
        max_in_flight: usize,
    },
    BsmThreadPoolBuild(String),
    SignalHandler(String),
    BsmInteractiveControlThread(String),
    InvalidBsmSampleRange {
        start: usize,
        end: usize,
    },
    BsmSampling {
        sample_index: usize,
        source: biogeo_core::BsmError,
    },
    BsmSummary {
        sample_index: usize,
        source: biogeo_core::BsmSummaryError,
    },
    BsmTreeReference {
        sample_index: usize,
        message: String,
    },
    BsmStateConstraintViolation {
        sample_index: usize,
        forbidden_state_transitions: usize,
        forbidden_state_endpoints: usize,
        forbidden_state_time: f64,
    },
    BsmCancelled {
        sample_index: usize,
    },
    BsmTimeLimitExceeded {
        sample_index: usize,
    },
    BsmTotalEventLimitExceeded {
        sample_index: usize,
        limit: usize,
        completed: usize,
        attempted: usize,
    },
    BsmOutputDirectoryExists(PathBuf),
    MissingBsmOutputDirectory(PathBuf),
    MissingBsmCheckpoint(PathBuf),
    InvalidBsmCheckpoint {
        path: PathBuf,
        message: String,
    },
    InvalidBsmShard {
        path: PathBuf,
        message: String,
    },
    InvalidBsmReference {
        path: PathBuf,
        message: String,
    },
    BsmResumeFingerprintMismatch {
        expected: String,
        actual: String,
    },
    BsmTableShorterThanCheckpoint {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },
    BsmRecoveryFailed {
        original: Box<CliError>,
        recovery: Box<CliError>,
    },
    BsmInspection(bsm_inspect::BsmInspectionError),
    ConflictingStratifiedRawModifiers,
    MissingStratifiedModifier(&'static str),
    UnidentifiableAreaExponent,
    OptimizedExponentAlsoFixed {
        parameter: &'static str,
    },
    ProfileGridForFixedExponent {
        parameter: &'static str,
    },
    InvalidProfileAxisBounds {
        parameter: &'static str,
        min: f64,
        max: f64,
    },
    ProfileAxisTooShort {
        parameter: &'static str,
        points: usize,
    },
    InvalidPresetJUpperBound {
        preset: &'static str,
        max_j: f64,
        upper_exclusive: f64,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    OutputIo {
        path: PathBuf,
        source: std::io::Error,
    },
    TreeInput(biogeo_core::TreeInputError),
    Newick(biogeo_core::NewickError),
    Ranges(biogeo_core::RangeParseError),
    RangeLikelihood(biogeo_core::RangeLikelihoodError),
    DetectionData(biogeo_core::DetectionDataParseError),
    DetectionModel(biogeo_core::DetectionModelError),
    DispersalMultipliers(biogeo_core::DispersalMatrixParseError),
    DispersalMatrix(biogeo_core::DispersalMatrixError),
    DispersalStrata(biogeo_core::DispersalStrataParseError),
    AnageneticStrata(biogeo_core::AnageneticStrataParseError),
    StateConstraintParse(biogeo_core::StateConstraintParseError),
    StateConstraint(biogeo_core::StateConstraintError),
    DispersalSchedule(biogeo_core::DispersalScheduleError),
    ExtirpationMultipliers(biogeo_core::ExtirpationMultiplierParseError),
    AreaSizes(biogeo_core::AreaSizeParseError),
    AreaSize(biogeo_core::AreaSizeError),
    StateSpace(biogeo_core::StateSpaceError),
    Dec(biogeo_core::DecAnalysisError),
    Optimize(biogeo_core::DecOptimizationError),
    Traceback(biogeo_core::BsmError),
}

impl CliError {
    fn is_cancelled(&self) -> bool {
        match self {
            Self::TaskCancelled { .. } | Self::BsmCancelled { .. } => true,
            Self::ModelBatchJob { source, .. } | Self::DatasetBatchJob { source, .. } => {
                source.is_cancelled()
            }
            Self::ParameterOptimization(error) => {
                *error == biogeo_core::ParameterOptimizationError::Cancelled
            }
            _ => false,
        }
    }

    fn exit_code(&self) -> i32 {
        match self {
            Self::BsmCancelled { .. } | Self::TaskCancelled { .. } => 130,
            Self::BsmTimeLimitExceeded { .. } => 124,
            Self::BsmTotalEventLimitExceeded { .. } => 3,
            _ => 2,
        }
    }

    fn prints_usage(&self) -> bool {
        !matches!(
            self,
            Self::BsmCancelled { .. }
                | Self::TaskCancelled { .. }
                | Self::BsmTimeLimitExceeded { .. }
                | Self::BsmTotalEventLimitExceeded { .. }
                | Self::SignalHandler(_)
        )
    }

    fn stable_code(&self) -> &'static str {
        match self {
            Self::BsmCancelled { .. } => "bsm_cancelled",
            Self::TaskCancelled { .. } => "task_cancelled",
            Self::BsmTimeLimitExceeded { .. } => "bsm_time_limit",
            Self::BsmTotalEventLimitExceeded { .. } => "bsm_event_limit",
            Self::StateSpaceLimitExceeded { .. } => "resource_limit",
            Self::UnknownCommand(_)
            | Self::UnknownOption(_)
            | Self::DuplicateOption(_)
            | Self::UnexpectedArgument(_)
            | Self::MissingValue(_)
            | Self::MissingRequired(_)
            | Self::InvalidFloat { .. }
            | Self::InvalidU8 { .. }
            | Self::InvalidUsize { .. }
            | Self::InvalidU64 { .. }
            | Self::InvalidRootPrior(_)
            | Self::InvalidBsmOutputLevel(_)
            | Self::InvalidAnalysisRequestMode(_)
            | Self::AnalysisWorkflowOwnedOption(_)
            | Self::InvalidErrorFormat(_)
            | Self::InvalidProgressFormat(_)
            | Self::InvalidMinBranchLength(_)
            | Self::InvalidMissingBranchLengthFill(_)
            | Self::InvalidTipAgeTolerance(_) => "invalid_arguments",
            Self::Io { .. } | Self::OutputIo { .. } | Self::ProgressOutput(_) => "io_error",
            Self::LegacyImport(_) => "invalid_input",
            Self::TipStateConstraintViolations { .. } => "invalid_input",
            Self::TreeInput(_)
            | Self::Newick(_)
            | Self::Ranges(_)
            | Self::RangeLikelihood(_)
            | Self::DetectionData(_)
            | Self::DispersalMultipliers(_)
            | Self::DispersalStrata(_)
            | Self::AnageneticStrata(_)
            | Self::StateConstraintParse(_)
            | Self::ExtirpationMultipliers(_)
            | Self::AreaSizes(_) => "invalid_input",
            Self::ParameterOptimization(_) | Self::Optimize(_) => "optimization_error",
            Self::AnalysisResult(_) | Self::AnalysisReplayMismatch { .. } => {
                "analysis_result_error"
            }
            Self::AnalysisWorkflowOutputExists(_)
            | Self::MissingAnalysisWorkflowOutput(_)
            | Self::InvalidAnalysisWorkflow { .. }
            | Self::AnalysisWorkflowRequestMismatch { .. } => "analysis_workflow_error",
            Self::FossilPlacement(_) => "fossil_placement_error",
            Self::ModelAverage(_) => "model_average_error",
            Self::ModelBatch(_)
            | Self::ModelBatchOwnedOption(_)
            | Self::ModelBatchJob { .. }
            | Self::ModelBatchFailures { .. } => "model_batch_error",
            Self::DatasetBatch(_)
            | Self::DatasetBatchJob { .. }
            | Self::DatasetBatchFailures { .. }
            | Self::DatasetBatchUnexpectedHelp { .. } => "dataset_batch_error",
            Self::BsmSampling { .. }
            | Self::BsmSummary { .. }
            | Self::BsmTreeReference { .. }
            | Self::BsmStateConstraintViolation { .. }
            | Self::BsmOutputDirectoryExists(_)
            | Self::MissingBsmOutputDirectory(_)
            | Self::MissingBsmCheckpoint(_)
            | Self::InvalidBsmCheckpoint { .. }
            | Self::InvalidBsmShard { .. }
            | Self::InvalidBsmReference { .. }
            | Self::BsmResumeFingerprintMismatch { .. }
            | Self::BsmTableShorterThanCheckpoint { .. }
            | Self::BsmRecoveryFailed { .. }
            | Self::BsmInspection(_) => "bsm_error",
            Self::SignalHandler(_) | Self::BsmThreadPoolBuild(_) => "runtime_error",
            _ => "configuration_error",
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand(command) => write!(f, "unknown command {command:?}"),
            Self::UnknownOption(option) => write!(f, "unknown option {option:?}"),
            Self::DuplicateOption(option) => {
                write!(f, "option {option} was provided more than once")
            }
            Self::UnexpectedArgument(argument) => {
                write!(f, "unexpected positional argument {argument:?}")
            }
            Self::MissingValue(option) => write!(f, "missing value for {option}"),
            Self::MissingRequired(option) => write!(f, "missing required option {option}"),
            Self::InvalidFloat {
                option,
                value,
                source,
            } => write!(
                f,
                "invalid floating-point value {value:?} for {option}: {source}"
            ),
            Self::InvalidU8 {
                option,
                value,
                source,
            } => write!(f, "invalid integer value {value:?} for {option}: {source}"),
            Self::InvalidUsize {
                option,
                value,
                source,
            } => write!(f, "invalid integer value {value:?} for {option}: {source}"),
            Self::InvalidU64 {
                option,
                value,
                source,
            } => write!(f, "invalid integer value {value:?} for {option}: {source}"),
            Self::InvalidRootPrior(value) => {
                write!(f, "invalid root prior {value:?}; expected flat or equal")
            }
            Self::InvalidBsmOutputLevel(value) => write!(
                f,
                "invalid BSM output level {value:?}; expected legacy, full, compact, or summary"
            ),
            Self::InvalidErrorFormat(value) => {
                write!(f, "invalid error format {value:?}; expected human or tsv")
            }
            Self::InvalidProgressFormat(value) => {
                write!(f, "invalid progress format {value:?}; expected none or tsv")
            }
            Self::LegacyImport(error) => write!(f, "failed to import legacy input: {error}"),
            Self::ProgressOutput(error) => write!(f, "failed to write live progress: {error}"),
            Self::TaskCancelled {
                operation,
                attempt_path,
            } => {
                write!(f, "{operation} cancelled")?;
                if let Some(path) = attempt_path {
                    write!(f, "; see {}", path.display())?;
                }
                Ok(())
            }
            Self::InvalidMinBranchLength(value) => write!(
                f,
                "invalid --min-branch-length {value}; expected a finite non-negative value"
            ),
            Self::InvalidMissingBranchLengthFill(value) => write!(
                f,
                "invalid --fill-missing-branch-length {value}; expected a finite non-negative value"
            ),
            Self::InvalidTipAgeTolerance(value) => write!(
                f,
                "invalid --tip-age-tolerance {value}; expected a finite non-negative value"
            ),
            Self::TipStateConstraintViolations { tips } => {
                let details = tips
                    .iter()
                    .map(|(node, label, stratum)| {
                        format!("{label} (node {node}, stratum {})", stratum + 1)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "{} tip range(s) conflict with BioGeoBEARS state constraints at their sampling ages: {details}",
                    tips.len()
                )
            }
            Self::NonBinaryInputTree { nodes } => {
                let details = nodes
                    .iter()
                    .map(|(node, child_count)| format!("node {node}: {child_count} children"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "tree is not binary and cannot enter cladogenesis likelihood: {details}"
                )
            }
            Self::InvalidParameterPreset(value) => write!(
                f,
                "invalid parameter preset {value:?}; expected dec, dec+j, divalike, divalike+j, bayarealike, or bayarealike+j"
            ),
            Self::InvalidAnalysisRequestMode(value) => write!(
                f,
                "invalid analysis request mode {value:?}; expected evaluate or optimize"
            ),
            Self::AnalysisWorkflowOwnedOption(option) => write!(
                f,
                "{option} is managed by analysis-workflow; use --output-dir for the workflow and --resume to continue it"
            ),
            Self::AnalysisWorkflowOutputExists(path) => write!(
                f,
                "analysis workflow output directory already exists; use --resume to continue {}",
                path.display()
            ),
            Self::MissingAnalysisWorkflowOutput(path) => write!(
                f,
                "cannot resume because analysis workflow output directory does not exist: {}",
                path.display()
            ),
            Self::InvalidAnalysisWorkflow { path, message } => write!(
                f,
                "invalid analysis workflow directory {}: {message}",
                path.display()
            ),
            Self::AnalysisWorkflowRequestMismatch { expected, actual } => write!(
                f,
                "analysis workflow request does not match the completed analysis result (result {expected}, current {actual})"
            ),
            Self::AnalysisRequest(error) => write!(f, "{error}"),
            Self::ModelWorkflow(error) => write!(f, "model workflow failed: {error}"),
            Self::AnalysisPlanSizeOverflow(quantity) => write!(
                f,
                "analysis plan size overflow while calculating {quantity}"
            ),
            Self::StateSpaceLimitExceeded {
                estimated_states,
                max_states,
                num_areas,
                max_range_size,
                include_null_range,
            } => write!(
                f,
                "estimated state space has {estimated_states} states for {num_areas} areas, max_range_size={max_range_size}, include_null_range={include_null_range}, exceeding --max-states {max_states}; reduce max_range_size or raise/remove the explicit limit"
            ),
            Self::AnalysisPlanStrataCoverage {
                oldest_age,
                root_age,
            } => write!(
                f,
                "analysis strata end at age {oldest_age}, but the tree root age is {root_age}"
            ),
            Self::ParameterOptimizationOptionRequiresOptimize => write!(
                f,
                "--initial-step, --tolerance, and --max-iterations are valid only for model-optimize"
            ),
            Self::ConflictingTipObservationInputs => write!(
                f,
                "--use-detection-model cannot be combined with --ranges or --use-ambiguities"
            ),
            Self::DetectionInputRequiresModel => {
                write!(f, "--detections/--controls require --use-detection-model")
            }
            Self::ParameterTable(error) => {
                write!(f, "failed to parse versioned parameter table: {error}")
            }
            Self::Parameter(error) => write!(f, "invalid parameter configuration: {error}"),
            Self::ParameterModel(error) => {
                write!(f, "failed to build configured biogeographic model: {error}")
            }
            Self::ParameterOptimization(error) => {
                write!(f, "configured model optimization failed: {error}")
            }
            Self::AnalysisResult(error) => write!(f, "analysis result failed: {error}"),
            Self::FossilPlacement(error) => write!(f, "{error}"),
            Self::ModelAverage(error) => write!(f, "model averaging failed: {error}"),
            Self::ModelBatch(error) => write!(f, "model batch failed: {error}"),
            Self::DatasetBatch(error) => write!(f, "dataset batch failed: {error}"),
            Self::ModelBatchOwnedOption(option) => write!(
                f,
                "{option} is managed by model-batch; put parameter tables in --manifest and use --output-dir for the batch result"
            ),
            Self::ModelBatchJob { model_id, source } => {
                write!(f, "model-batch job {model_id:?} failed: {source}")
            }
            Self::ModelBatchFailures {
                failed,
                attempt_path,
            } => write!(
                f,
                "model-batch completed with {failed} failed model(s); see {}",
                attempt_path.display()
            ),
            Self::DatasetBatchJob { dataset_id, source } => {
                write!(f, "dataset-batch job {dataset_id:?} failed: {source}")
            }
            Self::DatasetBatchFailures {
                failed,
                attempt_path,
            } => write!(
                f,
                "dataset-batch completed with {failed} failed dataset(s); see {}",
                attempt_path.display()
            ),
            Self::DatasetBatchUnexpectedHelp { dataset_id } => write!(
                f,
                "dataset-batch config for {dataset_id:?} unexpectedly requested help"
            ),
            Self::AnalysisReplayMismatch {
                field,
                expected,
                actual,
            } => write!(
                f,
                "analysis result replay mismatch for {field}: expected {expected}, got {actual}"
            ),
            Self::MissingParameterTableParameter(parameter) => write!(
                f,
                "parameter table is missing required BioGeoBEARS parameter {parameter:?}"
            ),
            Self::UnknownParameterTableParameter(parameter) => write!(
                f,
                "parameter table contains unknown BioGeoBEARS parameter {parameter:?}"
            ),
            Self::UnsupportedParameterSemantics {
                parameter,
                required_value,
            } => write!(
                f,
                "parameter {parameter} is present for configuration compatibility but its non-default semantics are not implemented; it must remain fixed at {required_value}"
            ),
            Self::StratifiedBranchLengthExponent => write!(
                f,
                "parameter b must remain fixed at 1 when --dispersal-strata is used because BioGeoBEARS defines b as non-stratified only"
            ),
            Self::ParameterInputRequired { parameter, option } => write!(
                f,
                "parameter {parameter} can be nonzero, derived, or free only when its raw modifier input is provided with {option} or --dispersal-strata"
            ),
            Self::ParameterEvaluateHasFree(parameters) => write!(
                f,
                "model-evaluate requires no free parameters; still free: {}",
                parameters.join(", ")
            ),
            Self::ParameterOptimizeHasNoFree => {
                write!(f, "model-optimize requires at least one free parameter")
            }
            Self::UnusedFreeParameter(parameter) => write!(
                f,
                "free parameter {parameter} does not affect any quantity consumed by the configured likelihood model"
            ),
            Self::ConflictingParameterModifierSources(parameter) => write!(
                f,
                "parameter {parameter} has both a static raw input and a per-period raw input in --dispersal-strata"
            ),
            Self::ConflictingDispersalModifiers => write!(
                f,
                "--dispersal-multipliers and --dispersal-strata are mutually exclusive"
            ),
            Self::IncompleteDistanceModifier => write!(
                f,
                "--distance-matrix and --distance-exponent must be provided together"
            ),
            Self::IncompleteEnvironmentDistanceModifier => write!(
                f,
                "--environment-distance-matrix and --environment-distance-exponent must be provided together"
            ),
            Self::IncompleteAreaSizeModifier => write!(
                f,
                "--area-sizes and --area-exponent must be provided together when u is fixed"
            ),
            Self::ConflictingExtirpationModifiers => write!(
                f,
                "--extirpation-multipliers and --area-sizes are mutually exclusive"
            ),
            Self::ConflictingHistorySamplingOptions => write!(
                f,
                "--traceback-samples and --bsm-samples are mutually exclusive; a full BSM already contains the sampled history skeleton"
            ),
            Self::BsmOutputRequiresSamples => {
                write!(
                    f,
                    "--bsm-output-dir requires --bsm-samples greater than zero"
                )
            }
            Self::BsmExecutionRequiresSamples => write!(
                f,
                "BSM execution options require --bsm-samples greater than zero"
            ),
            Self::BsmStreamOptionRequiresOutput(option) => {
                write!(f, "{option} requires --bsm-output-dir")
            }
            Self::InvalidBsmThreads(value) => write!(
                f,
                "invalid BSM thread selection {value:?}; expected auto or a positive integer"
            ),
            Self::InvalidBsmTimeLimit { option, seconds } => write!(
                f,
                "{option} must be a finite non-negative number, got {seconds}"
            ),
            Self::BsmTimeLimitOverflow(limit) => write!(
                f,
                "BSM time limit {} seconds exceeds the platform deadline range",
                limit.as_secs_f64()
            ),
            Self::NonPositiveBsmOption(option) => {
                write!(f, "{option} must be greater than zero")
            }
            Self::BsmMemoryBudgetRequiresPerSampleEventLimit => write!(
                f,
                "--bsm-memory-budget-mb requires --bsm-max-events-per-sample so one completed history has a finite size upper bound"
            ),
            Self::BsmMemoryBudgetOverflow { megabytes } => write!(
                f,
                "--bsm-memory-budget-mb {megabytes} cannot be represented as bytes on this platform"
            ),
            Self::BsmMemoryBudgetTooSmall {
                budget_bytes,
                minimum_bytes,
            } => write!(
                f,
                "BSM completed-history budget {budget_bytes} bytes is below the {minimum_bytes}-byte upper bound required for one sample"
            ),
            Self::BsmRetainedHistorySizeOverflow => write!(
                f,
                "BSM completed-history memory upper-bound calculation overflowed usize"
            ),
            Self::BsmMaxInFlightBelowThreads {
                threads,
                max_in_flight,
            } => write!(
                f,
                "--bsm-max-in-flight {max_in_flight} is below the resolved BSM worker count {threads}"
            ),
            Self::BsmThreadPoolBuild(message) => {
                write!(f, "failed to create BSM worker pool: {message}")
            }
            Self::SignalHandler(message) => {
                write!(f, "failed to install Ctrl+C handler: {message}")
            }
            Self::BsmInteractiveControlThread(message) => {
                write!(
                    f,
                    "failed to create BSM interactive-control thread: {message}"
                )
            }
            Self::InvalidBsmSampleRange { start, end } => {
                write!(f, "invalid BSM sample range {start}..{end}")
            }
            Self::BsmSampling {
                sample_index,
                source,
            } => write!(f, "BSM sample {sample_index} failed: {source}"),
            Self::BsmSummary {
                sample_index,
                source,
            } => write!(
                f,
                "BSM sample {sample_index} failed structural validation: {source}"
            ),
            Self::BsmTreeReference {
                sample_index,
                message,
            } => write!(
                f,
                "BSM sample {sample_index} does not match the output tree or areas: {message}"
            ),
            Self::BsmStateConstraintViolation {
                sample_index,
                forbidden_state_transitions,
                forbidden_state_endpoints,
                forbidden_state_time,
            } => write!(
                f,
                "BSM sample {sample_index} violates time-stratified state constraints: {forbidden_state_transitions} forbidden transition(s), {forbidden_state_endpoints} forbidden segment endpoint(s), and {forbidden_state_time:.15} forbidden occupancy time"
            ),
            Self::BsmInspection(error) => write!(f, "BSM inspection failed: {error}"),
            Self::BsmCancelled { sample_index } => {
                write!(f, "BSM cancelled before sample {sample_index}")
            }
            Self::BsmTimeLimitExceeded { sample_index } => {
                write!(f, "BSM time limit reached before sample {sample_index}")
            }
            Self::BsmTotalEventLimitExceeded {
                sample_index,
                limit,
                completed,
                attempted,
            } => write!(
                f,
                "BSM sample {sample_index} would raise the task anagenetic-event count from {completed} to {attempted}, exceeding --bsm-max-events-total {limit}"
            ),
            Self::BsmOutputDirectoryExists(path) => write!(
                f,
                "BSM output directory already exists; refusing to overwrite {}",
                path.display()
            ),
            Self::MissingBsmOutputDirectory(path) => write!(
                f,
                "cannot resume because BSM output directory does not exist: {}",
                path.display()
            ),
            Self::MissingBsmCheckpoint(path) => write!(
                f,
                "cannot resume because no committed BSM checkpoint exists in {}",
                path.display()
            ),
            Self::InvalidBsmCheckpoint { path, message } => {
                write!(f, "invalid BSM checkpoint {}: {message}", path.display())
            }
            Self::InvalidBsmShard { path, message } => {
                write!(f, "invalid BSM shard {}: {message}", path.display())
            }
            Self::InvalidBsmReference { path, message } => {
                write!(
                    f,
                    "invalid BSM reference table {}: {message}",
                    path.display()
                )
            }
            Self::BsmResumeFingerprintMismatch { expected, actual } => write!(
                f,
                "BSM resume configuration does not match the checkpoint (checkpoint {expected}, current {actual})"
            ),
            Self::BsmTableShorterThanCheckpoint {
                path,
                expected,
                actual,
            } => write!(
                f,
                "BSM table {} is shorter than its checkpoint (expected at least {expected} bytes, found {actual})",
                path.display()
            ),
            Self::BsmRecoveryFailed { original, recovery } => write!(
                f,
                "BSM streaming failed ({original}) and rollback to the last checkpoint also failed ({recovery})"
            ),
            Self::ConflictingStratifiedRawModifiers => write!(
                f,
                "extended --dispersal-strata contains per-period raw modifiers and cannot be combined with static distance, environment, area-size, or manual matrices"
            ),
            Self::MissingStratifiedModifier(column) => write!(
                f,
                "extended --dispersal-strata must contain at least one {column} path for this optimization"
            ),
            Self::UnidentifiableAreaExponent => write!(
                f,
                "free u is unidentifiable when all raw area sizes are equal; provide varying positive area sizes"
            ),
            Self::OptimizedExponentAlsoFixed { parameter } => write!(
                f,
                "parameter {parameter} is being optimized and cannot also be fixed; use --init-exponent to set its starting value"
            ),
            Self::ProfileGridForFixedExponent { parameter } => write!(
                f,
                "parameter {parameter} is fixed in this profile and cannot also have profile-grid options"
            ),
            Self::InvalidProfileAxisBounds {
                parameter,
                min,
                max,
            } => write!(
                f,
                "profile axis {parameter} requires finite bounds with min < max, got min={min}, max={max}"
            ),
            Self::ProfileAxisTooShort { parameter, points } => write!(
                f,
                "profile axis {parameter} needs at least two grid points, got {points}"
            ),
            Self::InvalidPresetJUpperBound {
                preset,
                max_j,
                upper_exclusive,
            } => write!(
                f,
                "{preset} optimization requires --max-j < {upper_exclusive}, got {max_j}"
            ),
            Self::Io { path, source } => write!(f, "failed to read {}: {source}", path.display()),
            Self::OutputIo { path, source } => {
                write!(f, "failed to write {}: {source}", path.display())
            }
            Self::TreeInput(error) => write!(f, "failed to parse tree input: {error}"),
            Self::Newick(error) => write!(f, "failed to parse Newick tree: {error}"),
            Self::Ranges(error) => write!(f, "failed to parse tip ranges: {error}"),
            Self::RangeLikelihood(error) => {
                write!(f, "failed to build tip range likelihoods: {error}")
            }
            Self::DetectionData(error) => {
                write!(f, "failed to parse detection/control counts: {error}")
            }
            Self::DetectionModel(error) => write!(f, "invalid detection model: {error}"),
            Self::DispersalMultipliers(error) => {
                write!(f, "failed to parse dispersal multipliers: {error}")
            }
            Self::DispersalMatrix(error) => write!(f, "invalid dispersal matrix: {error}"),
            Self::DispersalStrata(error) => {
                write!(f, "failed to parse dispersal strata: {error}")
            }
            Self::AnageneticStrata(error) => {
                write!(f, "failed to parse anagenetic strata: {error}")
            }
            Self::StateConstraintParse(error) => {
                write!(f, "failed to parse range-state constraint: {error}")
            }
            Self::StateConstraint(error) => write!(f, "invalid range-state constraint: {error}"),
            Self::DispersalSchedule(error) => {
                write!(f, "invalid dispersal schedule: {error}")
            }
            Self::ExtirpationMultipliers(error) => {
                write!(f, "failed to parse extirpation multipliers: {error}")
            }
            Self::AreaSizes(error) => write!(f, "failed to parse area sizes: {error}"),
            Self::AreaSize(error) => write!(f, "invalid area-size transformation: {error}"),
            Self::StateSpace(error) => write!(f, "failed to build state space: {error}"),
            Self::Dec(error) => write!(f, "DEC likelihood failed: {error}"),
            Self::Optimize(error) => write!(f, "DEC optimization failed: {error}"),
            Self::Traceback(error) => write!(f, "conditional history sampling failed: {error}"),
        }
    }
}

impl Error for CliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidFloat { source, .. } => Some(source),
            Self::InvalidU8 { source, .. } => Some(source),
            Self::InvalidUsize { source, .. } => Some(source),
            Self::InvalidU64 { source, .. } => Some(source),
            Self::ProgressOutput(error) => Some(error),
            Self::LegacyImport(error) => Some(error),
            Self::ParameterTable(error) => Some(error),
            Self::Parameter(error) => Some(error),
            Self::ParameterModel(error) => Some(error),
            Self::ParameterOptimization(error) => Some(error),
            Self::AnalysisRequest(error) => Some(error),
            Self::ModelWorkflow(error) => Some(error.as_ref()),
            Self::AnalysisResult(error) => Some(error),
            Self::FossilPlacement(error) => Some(error),
            Self::ModelAverage(error) => Some(error),
            Self::ModelBatch(error) => Some(error.as_ref()),
            Self::ModelBatchJob { source, .. } => Some(source.as_ref()),
            Self::DatasetBatch(error) => Some(error.as_ref()),
            Self::DatasetBatchJob { source, .. } => Some(source.as_ref()),
            Self::Io { source, .. } => Some(source),
            Self::OutputIo { source, .. } => Some(source),
            Self::TreeInput(error) => Some(error),
            Self::Newick(error) => Some(error),
            Self::Ranges(error) => Some(error),
            Self::RangeLikelihood(error) => Some(error),
            Self::DetectionData(error) => Some(error),
            Self::DetectionModel(error) => Some(error),
            Self::DispersalMultipliers(error) => Some(error),
            Self::DispersalMatrix(error) => Some(error),
            Self::DispersalStrata(error) => Some(error),
            Self::AnageneticStrata(error) => Some(error),
            Self::StateConstraintParse(error) => Some(error),
            Self::StateConstraint(error) => Some(error),
            Self::DispersalSchedule(error) => Some(error),
            Self::ExtirpationMultipliers(error) => Some(error),
            Self::AreaSizes(error) => Some(error),
            Self::AreaSize(error) => Some(error),
            Self::StateSpace(error) => Some(error),
            Self::Dec(error) => Some(error),
            Self::Optimize(error) => Some(error),
            Self::Traceback(error) => Some(error),
            Self::BsmSampling { source, .. } => Some(source),
            Self::BsmSummary { source, .. } => Some(source),
            Self::BsmRecoveryFailed { original, .. } => Some(original.as_ref()),
            Self::BsmInspection(error) => Some(error),
            _ => None,
        }
    }
}

impl From<biogeo_core::NewickError> for CliError {
    fn from(value: biogeo_core::NewickError) -> Self {
        Self::Newick(value)
    }
}

impl From<legacy_import::LegacyImportError> for CliError {
    fn from(error: legacy_import::LegacyImportError) -> Self {
        Self::LegacyImport(error)
    }
}

impl From<biogeo_core::TreeInputError> for CliError {
    fn from(value: biogeo_core::TreeInputError) -> Self {
        Self::TreeInput(value)
    }
}

impl From<biogeo_core::ParameterTableParseError> for CliError {
    fn from(value: biogeo_core::ParameterTableParseError) -> Self {
        Self::ParameterTable(value)
    }
}

impl From<biogeo_core::ParameterError> for CliError {
    fn from(value: biogeo_core::ParameterError) -> Self {
        Self::Parameter(value)
    }
}

impl From<biogeo_core::BioGeoBearsModelError> for CliError {
    fn from(value: biogeo_core::BioGeoBearsModelError) -> Self {
        Self::ParameterModel(value)
    }
}

impl From<biogeo_core::ParameterOptimizationError> for CliError {
    fn from(value: biogeo_core::ParameterOptimizationError) -> Self {
        match value {
            biogeo_core::ParameterOptimizationError::Cancelled => Self::TaskCancelled {
                operation: "model optimization",
                attempt_path: None,
            },
            value => Self::ParameterOptimization(value),
        }
    }
}

impl From<analysis_result::AnalysisResultError> for CliError {
    fn from(value: analysis_result::AnalysisResultError) -> Self {
        Self::AnalysisResult(value)
    }
}

impl From<analysis_request::AnalysisRequestError> for CliError {
    fn from(value: analysis_request::AnalysisRequestError) -> Self {
        Self::AnalysisRequest(value)
    }
}

impl From<model_workflow::ModelWorkflowError> for CliError {
    fn from(value: model_workflow::ModelWorkflowError) -> Self {
        Self::ModelWorkflow(Box::new(value))
    }
}

impl From<fossil_placement::FossilPlacementCliError> for CliError {
    fn from(value: fossil_placement::FossilPlacementCliError) -> Self {
        Self::FossilPlacement(value)
    }
}

impl From<model_average::ModelAverageError> for CliError {
    fn from(value: model_average::ModelAverageError) -> Self {
        Self::ModelAverage(value)
    }
}

impl From<model_batch::ModelBatchError> for CliError {
    fn from(value: model_batch::ModelBatchError) -> Self {
        Self::ModelBatch(Box::new(value))
    }
}

impl From<dataset_batch::DatasetBatchError> for CliError {
    fn from(value: dataset_batch::DatasetBatchError) -> Self {
        Self::DatasetBatch(Box::new(value))
    }
}

impl From<biogeo_core::RangeParseError> for CliError {
    fn from(value: biogeo_core::RangeParseError) -> Self {
        Self::Ranges(value)
    }
}

impl From<biogeo_core::RangeLikelihoodError> for CliError {
    fn from(value: biogeo_core::RangeLikelihoodError) -> Self {
        Self::RangeLikelihood(value)
    }
}

impl From<biogeo_core::DetectionDataParseError> for CliError {
    fn from(value: biogeo_core::DetectionDataParseError) -> Self {
        Self::DetectionData(value)
    }
}

impl From<biogeo_core::DetectionModelError> for CliError {
    fn from(value: biogeo_core::DetectionModelError) -> Self {
        Self::DetectionModel(value)
    }
}

impl From<biogeo_core::DispersalMatrixParseError> for CliError {
    fn from(value: biogeo_core::DispersalMatrixParseError) -> Self {
        Self::DispersalMultipliers(value)
    }
}

impl From<biogeo_core::DispersalMatrixError> for CliError {
    fn from(value: biogeo_core::DispersalMatrixError) -> Self {
        Self::DispersalMatrix(value)
    }
}

impl From<biogeo_core::DispersalStrataParseError> for CliError {
    fn from(value: biogeo_core::DispersalStrataParseError) -> Self {
        Self::DispersalStrata(value)
    }
}

impl From<biogeo_core::AnageneticStrataParseError> for CliError {
    fn from(value: biogeo_core::AnageneticStrataParseError) -> Self {
        Self::AnageneticStrata(value)
    }
}

impl From<biogeo_core::StateConstraintParseError> for CliError {
    fn from(value: biogeo_core::StateConstraintParseError) -> Self {
        Self::StateConstraintParse(value)
    }
}

impl From<biogeo_core::StateConstraintError> for CliError {
    fn from(value: biogeo_core::StateConstraintError) -> Self {
        Self::StateConstraint(value)
    }
}

impl From<biogeo_core::DispersalScheduleError> for CliError {
    fn from(value: biogeo_core::DispersalScheduleError) -> Self {
        Self::DispersalSchedule(value)
    }
}

impl From<biogeo_core::ExtirpationMultiplierParseError> for CliError {
    fn from(value: biogeo_core::ExtirpationMultiplierParseError) -> Self {
        Self::ExtirpationMultipliers(value)
    }
}

impl From<biogeo_core::AreaSizeParseError> for CliError {
    fn from(value: biogeo_core::AreaSizeParseError) -> Self {
        Self::AreaSizes(value)
    }
}

impl From<biogeo_core::AreaSizeError> for CliError {
    fn from(value: biogeo_core::AreaSizeError) -> Self {
        Self::AreaSize(value)
    }
}

impl From<biogeo_core::StateSpaceError> for CliError {
    fn from(value: biogeo_core::StateSpaceError) -> Self {
        Self::StateSpace(value)
    }
}

impl From<biogeo_core::DecAnalysisError> for CliError {
    fn from(value: biogeo_core::DecAnalysisError) -> Self {
        Self::Dec(value)
    }
}

impl From<biogeo_core::DecOptimizationError> for CliError {
    fn from(value: biogeo_core::DecOptimizationError) -> Self {
        Self::Optimize(value)
    }
}

impl From<biogeo_core::BsmError> for CliError {
    fn from(value: biogeo_core::BsmError) -> Self {
        Self::Traceback(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn no_args_show_help() {
        assert_eq!(run(Vec::new()).unwrap(), format!("{USAGE}\n"));
    }

    #[test]
    fn global_tsv_error_format_is_extracted_before_the_command() {
        let (options, args) = extract_global_output_options(vec![
            "--error-format".to_string(),
            "tsv".to_string(),
            "--progress-format".to_string(),
            "tsv".to_string(),
            "validate-inputs".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
        ]);

        assert_eq!(options.error, ErrorOutputFormat::Tsv);
        assert_eq!(options.progress, ProgressOutputFormat::Tsv);
        assert_eq!(args.unwrap(), vec!["validate-inputs", "--tree", "tree.nwk"]);
    }

    #[test]
    fn invalid_global_error_format_remains_human_readable() {
        let (options, args) =
            extract_global_output_options(vec!["--error-format".to_string(), "json".to_string()]);

        assert_eq!(options.error, ErrorOutputFormat::Human);
        assert!(matches!(
            args,
            Err(CliError::InvalidErrorFormat(value)) if value == "json"
        ));
    }

    #[test]
    fn global_progress_and_error_options_accept_either_order() {
        let (options, args) = extract_global_output_options(vec![
            "--progress-format".to_string(),
            "tsv".to_string(),
            "--error-format".to_string(),
            "tsv".to_string(),
            "model-optimize".to_string(),
        ]);

        assert_eq!(options.error, ErrorOutputFormat::Tsv);
        assert_eq!(options.progress, ProgressOutputFormat::Tsv);
        assert_eq!(args.unwrap(), vec!["model-optimize"]);
    }

    #[test]
    fn tsv_error_record_is_versioned_encoded_and_carries_exit_code() {
        let error = CliError::SignalHandler("line one\tline two\n".to_string());

        assert_eq!(
            format_cli_error(&error, ErrorOutputFormat::Tsv),
            "format\tbiogeo-cli-error-v1\n\
             code\truntime_error\n\
             message\tfailed to install Ctrl+C handler: line one%09line two%0A\n\
             exit_code\t2\n"
        );
        assert_eq!(CliError::BsmCancelled { sample_index: 7 }.exit_code(), 130);
        assert_eq!(
            CliError::BsmTimeLimitExceeded { sample_index: 7 }.exit_code(),
            124
        );
        assert_eq!(
            CliError::BsmTotalEventLimitExceeded {
                sample_index: 7,
                limit: 10,
                completed: 8,
                attempted: 12,
            }
            .exit_code(),
            3
        );
    }

    #[test]
    fn parameter_template_emits_parseable_versioned_table() {
        let output = run(vec![
            "parameter-template".to_string(),
            "--preset".to_string(),
            "divalike+j".to_string(),
        ])
        .unwrap();
        let table = biogeo_core::parse_parameter_table(&output).unwrap();

        assert_eq!(table.specs().len(), 23);
        assert_eq!(table.free_parameter_names(), vec!["d", "e", "j"]);
        assert_eq!(table.resolve_initial().unwrap().get("mx01v"), Some(0.5));
    }

    #[test]
    fn analysis_template_evaluate_freezes_preset_and_refuses_overwrite() {
        let temp = TempInputs::new();
        let output_dir = temp.dir.join("request-template");
        let output = run(vec![
            "analysis-template".to_string(),
            "--preset".to_string(),
            "dec+j".to_string(),
            "--mode".to_string(),
            "evaluate".to_string(),
            "--output-dir".to_string(),
            output_dir.display().to_string(),
        ])
        .unwrap();
        assert!(output.contains("format\tbiogeo-analysis-template-v1\n"));
        assert!(output.contains("ready_to_plan\tfalse\n"));

        let parameters = biogeo_core::parse_parameter_table(
            &fs::read_to_string(output_dir.join(analysis_request::PARAMETERS_FILE)).unwrap(),
        )
        .unwrap();
        assert!(parameters.free_parameter_names().is_empty());
        let request_path = output_dir.join(analysis_request::REQUEST_FILE);
        let request = analysis_request::parse_analysis_request(
            &fs::read_to_string(&request_path).unwrap(),
            &request_path,
        )
        .unwrap();
        assert_eq!(
            request.mode,
            analysis_request::AnalysisRequestMode::Evaluate
        );

        let error = run(vec![
            "analysis-template".to_string(),
            "--preset".to_string(),
            "dec".to_string(),
            "--mode".to_string(),
            "optimize".to_string(),
            "--output-dir".to_string(),
            output_dir.display().to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::AnalysisRequest(analysis_request::AnalysisRequestError::OutputExists(path))
                if path == output_dir
        ));
    }

    #[test]
    fn analysis_request_plan_and_run_share_the_parameter_model_engine() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        fs::write(
            temp.dir.join("parameters.tsv"),
            biogeo_core::BioGeoBearsPreset::Dec
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        let request_path = temp.dir.join("analysis.tsv");
        fs::write(
            &request_path,
            "key\tvalue\n\
             format\tbiogeo-analysis-request-v1\n\
             mode\toptimize\n\
             tree\ttree.nwk\n\
             observation\texact_ranges\n\
             ranges\tranges.tsv\n\
             parameters\tparameters.tsv\n\
             max_range_size\tauto\n\
             include_null_range\tfalse\n\
             root_prior\tflat\n\
             min_branch_length\t0\n\
             ancestral_probabilities\ttrue\n\
             split_probabilities\ttrue\n\
             optimization_initial_step\t0.2\n\
             optimization_tolerance\t1e-8\n\
             optimization_max_iterations\t2\n",
        )
        .unwrap();

        let plan = run(vec![
            "analysis-plan".to_string(),
            "--request".to_string(),
            request_path.display().to_string(),
        ])
        .unwrap();
        assert!(plan.contains("format\tbiogeo-analysis-plan-v1\n"));
        assert!(plan.contains("states\t3\n"));
        assert!(plan.contains("stratum_allowed_state_counts\t3\n"));
        assert!(plan.contains("q_off_diagonal_transitions\t4\n"));
        assert!(plan.contains("process_rss_estimate_available\tfalse\n"));

        let result_dir = temp.dir.join("analysis-result");
        let completed = run(vec![
            "analysis-run".to_string(),
            "--request".to_string(),
            request_path.display().to_string(),
            "--output-dir".to_string(),
            result_dir.display().to_string(),
        ])
        .unwrap();
        assert!(completed.contains("format\tbiogeo-analysis-run-v2\n"));
        #[cfg(windows)]
        {
            assert!(completed.contains("telemetry_provider\twindows_process_api\n"));
            assert!(completed.contains("process_telemetry_available\ttrue\n"));
        }
        #[cfg(not(windows))]
        {
            assert!(completed.contains("telemetry_provider\tunavailable\n"));
            assert!(completed.contains("process_telemetry_available\tfalse\n"));
        }
        assert!(completed.contains("analysis_worker_threads\t1\n"));
        assert!(completed.contains("analysis_result_format\tbiogeo-analysis-result-v2\n"));
        let loaded = analysis_result::load_analysis_result(&result_dir).unwrap();
        assert_eq!(loaded.manifest.states, 3);
        assert!(loaded.manifest.inputs.contains_key("analysis_request"));
        assert!(loaded.input_bundle.is_some());

        let workflow_dir = temp.dir.join("analysis-workflow");
        let workflow_args = vec![
            "analysis-workflow".to_string(),
            "--request".to_string(),
            request_path.display().to_string(),
            "--output-dir".to_string(),
            workflow_dir.display().to_string(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--bsm-threads".to_string(),
            "1".to_string(),
            "--seed".to_string(),
            "20260821".to_string(),
            "--deep".to_string(),
        ];
        let workflow = run(workflow_args.clone()).unwrap();
        assert!(workflow.contains("format\tbiogeo-analysis-workflow-v1\n"));
        assert!(workflow.contains("analysis_reused\tfalse\n"));
        assert!(workflow.contains("bsm_output_level\tcompact\n"));
        assert!(workflow.contains("bsm_completed_samples\t2\n"));
        assert!(workflow.contains("bsm_resumed\tfalse\n"));
        assert!(workflow.contains("bsm_validation\tdeep\n"));
        assert!(workflow.contains("bsm_validation_status\tvalid\n"));
        assert!(workflow_dir.join("analysis-result/metadata.tsv").is_file());
        assert!(workflow_dir.join("bsm-result/metadata.tsv").is_file());

        assert!(matches!(
            run(workflow_args.clone()),
            Err(CliError::AnalysisWorkflowOutputExists(path)) if path == workflow_dir
        ));
        let mut resume_args = workflow_args;
        resume_args.push("--resume".to_string());
        let resumed = run(resume_args.clone()).unwrap();
        assert!(resumed.contains("analysis_reused\ttrue\n"));
        assert!(resumed.contains("bsm_resumed\ttrue\n"));

        let interrupted_dir = temp.dir.join("interrupted-analysis-workflow");
        let interrupted_args = vec![
            "analysis-workflow".to_string(),
            "--request".to_string(),
            request_path.display().to_string(),
            "--output-dir".to_string(),
            interrupted_dir.display().to_string(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--bsm-threads".to_string(),
            "1".to_string(),
            "--seed".to_string(),
            "20260822".to_string(),
            "--bsm-time-limit-seconds".to_string(),
            "0".to_string(),
        ];
        assert!(matches!(
            run(interrupted_args.clone()),
            Err(CliError::BsmTimeLimitExceeded { sample_index: 0 })
        ));
        assert!(
            interrupted_dir
                .join("analysis-result/metadata.tsv")
                .is_file()
        );
        let interrupted_metadata =
            fs::read_to_string(interrupted_dir.join("bsm-result/metadata.tsv")).unwrap();
        assert!(interrupted_metadata.contains("status\ttime_limit\n"));
        assert!(interrupted_metadata.contains("completed_samples\t0\n"));
        fs::remove_file(&temp.tree_path).unwrap();
        fs::remove_file(&temp.ranges_path).unwrap();
        fs::remove_file(temp.dir.join("parameters.tsv")).unwrap();
        let mut interrupted_resume = interrupted_args[..interrupted_args.len() - 2].to_vec();
        interrupted_resume.push("--resume".to_string());
        let recovered = run(interrupted_resume).unwrap();
        assert!(recovered.contains("analysis_reused\ttrue\n"));
        assert!(recovered.contains("bsm_resumed\ttrue\n"));
        assert!(recovered.contains("bsm_completed_samples\t2\n"));

        let changed_request = fs::read_to_string(&request_path).unwrap().replace(
            "optimization_max_iterations\t2",
            "optimization_max_iterations\t3",
        );
        fs::write(&request_path, changed_request).unwrap();
        assert!(matches!(
            run(resume_args),
            Err(CliError::AnalysisWorkflowRequestMismatch { .. })
        ));
        assert!(matches!(
            parse_command(vec![
                "analysis-workflow".to_string(),
                "--request".to_string(),
                request_path.display().to_string(),
                "--output-dir".to_string(),
                "workflow".to_string(),
                "--bsm-samples".to_string(),
                "1".to_string(),
                "--analysis-result".to_string(),
                "owned".to_string(),
            ]),
            Err(CliError::AnalysisWorkflowOwnedOption(option)) if option == "--analysis-result"
        ));
    }

    #[test]
    fn model_workflow_recovers_and_requires_explicit_unambiguous_bsm_selection() {
        let temp = TempInputs::new();
        let dec = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .to_versioned_tsv();
        fs::write(temp.dir.join("dec.tsv"), &dec).unwrap();
        fs::write(
            temp.dir.join("decj.tsv"),
            biogeo_core::BioGeoBearsPreset::DecJ
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        let manifest_path = temp.dir.join("models.tsv");
        fs::write(
            &manifest_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\nDEC+J\tdecj.tsv\n",
        )
        .unwrap();
        fs::write(
            temp.dir.join("model-config.tsv"),
            "biogeo-model-batch-config-v1\noption\tvalue\n--tree\ttree.nwk\n--ranges\tranges.tsv\n--max-iterations\t200\n",
        )
        .unwrap();
        let request_path = temp.dir.join("workflow.tsv");
        let explicit_request = "key\tvalue\n\
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
bsm_seed\t20260822\n";
        fs::write(&request_path, explicit_request).unwrap();

        let plan = run(vec![
            "model-workflow-plan".to_string(),
            "--request".to_string(),
            request_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(plan.contains("format\tbiogeo-model-workflow-plan-v1\n"));
        assert!(plan.contains("candidate_models\t2\n"));
        assert!(plan.contains("bsm_requested_model_id\tDEC\n"));

        let output_dir = temp.dir.join("interrupted-model-workflow");
        let args = vec![
            "model-workflow".to_string(),
            "--request".to_string(),
            request_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
        ];
        let cancellation = biogeo_core::ExecutionCancellationToken::new();
        cancellation.cancel();
        assert!(matches!(
            run_with_cancellation(args.clone(), Some(cancellation)),
            Err(CliError::TaskCancelled {
                operation: "model batch",
                ..
            })
        ));
        assert!(output_dir.join("metadata.tsv").is_file());
        assert!(
            output_dir
                .join("model-batch/attempts/attempt-000001.tsv")
                .is_file()
        );
        assert!(!output_dir.join("selection.tsv").exists());

        let mut resume_args = args.clone();
        resume_args.push("--resume".to_string());
        let recovered = run(resume_args.clone()).unwrap();
        assert!(recovered.contains("format\tbiogeo-model-workflow-run-v1\n"));
        assert!(recovered.contains("model_batch_resumed\ttrue\n"));
        assert!(recovered.contains("selected_model_id\tDEC\n"));
        assert!(recovered.contains("bsm_completed_samples\t2\n"));
        assert!(output_dir.join("complete.tsv").is_file());

        let timed_request_path = temp.dir.join("timed-workflow.tsv");
        let timed_request = explicit_request.replace(
            "bsm_deep_inspection\ttrue",
            "bsm_time_limit_seconds\t0\nbsm_deep_inspection\ttrue",
        );
        fs::write(&timed_request_path, &timed_request).unwrap();
        let timed_output_dir = temp.dir.join("timed-model-workflow");
        let timed_args = vec![
            "model-workflow".to_string(),
            "--request".to_string(),
            timed_request_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            timed_output_dir.to_string_lossy().into_owned(),
        ];
        assert!(matches!(
            run(timed_args.clone()),
            Err(CliError::BsmTimeLimitExceeded { sample_index: 0 })
        ));
        assert!(
            fs::read_to_string(timed_output_dir.join("source-request.tsv"))
                .unwrap()
                .contains("bsm_time_limit_seconds\t0\n")
        );
        fs::write(
            &timed_request_path,
            timed_request.replace("bsm_time_limit_seconds\t0", "bsm_time_limit_seconds\t60"),
        )
        .unwrap();
        let mut timed_resume_args = timed_args;
        timed_resume_args.push("--resume".to_string());
        let timed_recovered = run(timed_resume_args).unwrap();
        assert!(timed_recovered.contains("model_batch_resumed\ttrue\n"));
        assert!(timed_recovered.contains("bsm_resumed\ttrue\n"));
        assert!(timed_recovered.contains("bsm_completed_samples\t2\n"));
        let timed_bsm_metadata =
            fs::read_to_string(timed_output_dir.join("bsm-result/metadata.tsv")).unwrap();
        assert!(timed_bsm_metadata.contains("status\tcomplete\n"));
        assert!(timed_bsm_metadata.contains("time_limit_seconds\t60\n"));

        fs::write(
            &request_path,
            format!("{explicit_request}# changed audit note\n"),
        )
        .unwrap();
        assert!(matches!(
            run(resume_args),
            Err(CliError::ModelWorkflow(error))
                if matches!(*error, model_workflow::ModelWorkflowError::IdentityMismatch { .. })
        ));

        let unique_best_request_path = temp.dir.join("unique-best-workflow.tsv");
        fs::write(
            &unique_best_request_path,
            "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tmodel-config.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tbest_by_criterion\n\
bsm_samples\t1\n\
bsm_output_level\tsummary\n\
bsm_threads\t1\n",
        )
        .unwrap();
        let unique_best_output = temp.dir.join("unique-best-model-workflow");
        let unique_best = run(vec![
            "model-workflow".to_string(),
            "--request".to_string(),
            unique_best_request_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            unique_best_output.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(unique_best.contains("bsm_selection\tbest_by_criterion\n"));
        assert!(unique_best.contains("selection_reason\tunique_aic_rank_1\n"));
        assert!(!unique_best.contains("selected_model_id\tnone\n"));
        assert!(unique_best.contains("bsm_completed_samples\t1\n"));

        fs::write(
            &manifest_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC-A\tdec.tsv\nDEC-B\tdec.tsv\n",
        )
        .unwrap();
        let best_request_path = temp.dir.join("best-workflow.tsv");
        fs::write(
            &best_request_path,
            "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tmodel-config.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tbest_by_criterion\n\
bsm_samples\t1\n\
bsm_output_level\tsummary\n\
bsm_threads\t1\n",
        )
        .unwrap();
        let tied_output = temp.dir.join("tied-model-workflow");
        let tied_error = run(vec![
            "model-workflow".to_string(),
            "--request".to_string(),
            best_request_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            tied_output.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            tied_error,
            CliError::ModelWorkflow(error)
                if matches!(*error, model_workflow::ModelWorkflowError::TiedBestModels { .. })
        ));
        assert!(tied_output.join("model-batch/complete.tsv").is_file());
        assert!(!tied_output.join("selection.tsv").exists());

        let no_bsm_request_path = temp.dir.join("no-bsm-workflow.tsv");
        fs::write(
            &no_bsm_request_path,
            "key\tvalue\n\
format\tbiogeo-model-workflow-request-v1\n\
models\tmodels.tsv\n\
config\tmodel-config.tsv\n\
comparison_criterion\taic\n\
bsm_selection\tnone\n",
        )
        .unwrap();
        let no_bsm_output = temp.dir.join("no-bsm-model-workflow");
        let no_bsm = run(vec![
            "model-workflow".to_string(),
            "--request".to_string(),
            no_bsm_request_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            no_bsm_output.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(no_bsm.contains("bsm_status\tskipped\n"));
        assert!(no_bsm.contains("selected_model_id\tnone\n"));
        assert!(no_bsm_output.join("complete.tsv").is_file());
        assert!(!no_bsm_output.join("bsm-result").exists());
    }

    #[test]
    fn model_batch_optimizes_compares_and_resumes_only_missing_models() {
        let temp = TempInputs::new_with_contents(
            "(((A:0.2,B:0.2):0.2,(C:0.2,D:0.2):0.2):0.2,(E:0.4,F:0.4):0.2);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\nD\t1\t0\nE\t0\t1\nF\t1\t1\n",
        );
        let dec_path = temp.dir.join("dec.tsv");
        let decj_path = temp.dir.join("decj.tsv");
        fs::write(
            &dec_path,
            biogeo_core::BioGeoBearsPreset::Dec
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        fs::write(
            &decj_path,
            biogeo_core::BioGeoBearsPreset::DecJ
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        let manifest_path = temp.dir.join("models.tsv");
        fs::write(
            &manifest_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\nDEC+J\tdecj.tsv\n",
        )
        .unwrap();
        let output_dir = temp.dir.join("batch-result");
        let args = vec![
            "model-batch".to_string(),
            "--manifest".to_string(),
            manifest_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "1000".to_string(),
        ];

        let comparison = run(args.clone()).unwrap();
        assert!(comparison.starts_with("format\tbiogeo-model-comparison-v3\n"));
        assert!(
            comparison.contains("models\t2\neligible_models\t2\n"),
            "{comparison}"
        );
        assert!(output_dir.join("models/DEC/metadata.tsv").is_file());
        assert!(output_dir.join("models/DEC+J/metadata.tsv").is_file());
        assert_eq!(
            fs::read_to_string(output_dir.join("comparison.tsv")).unwrap(),
            comparison
        );
        assert!(output_dir.join("complete.tsv").is_file());
        let model_average_path = output_dir.join("model-averaged-ancestral-ranges.tsv");
        let model_average = fs::read_to_string(&model_average_path).unwrap();
        assert!(
            model_average.starts_with(
                "format\tbiogeo-model-averaged-ancestral-ranges-v2\nstatus\tavailable\n"
            )
        );
        assert!(model_average.contains("criteria\t2\n"));
        assert!(model_average.contains("aic_models\t2\n"));
        assert!(model_average.contains("aicc_models\t2\n"));
        let probability_rows = model_average
            .split_once("ancestral_state_probabilities\n")
            .unwrap()
            .1
            .split_once("\nsplit_scenarios\n")
            .unwrap()
            .0
            .lines()
            .skip(1)
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .collect::<Vec<_>>();
        let mut probability_sums = std::collections::BTreeMap::new();
        for fields in &probability_rows {
            *probability_sums
                .entry((fields[0].to_string(), fields[1].to_string()))
                .or_insert(0.0) += fields[3].parse::<f64>().unwrap();
        }
        assert_eq!(probability_sums.len(), 10);
        assert!(
            probability_sums
                .values()
                .all(|sum| (sum - 1.0).abs() < 1e-12)
        );
        let completion = fs::read_to_string(output_dir.join("complete.tsv")).unwrap();
        assert!(completion.contains("format\tbiogeo-model-batch-result-v2\n"));
        assert!(completion.contains("model_average_file\tmodel-averaged-ancestral-ranges.tsv\n"));
        assert!(comparison.contains("DEC\tDEC+J\tnested_boundary\t2\t3\t1\tj\tj\t"));
        assert!(comparison.contains("\nlikelihood_ratio_tests\n"));
        assert!(model_average.contains("\ncladogenetic_split_probabilities\n"));

        let rows = comparison
            .split_once("model_comparison\n")
            .unwrap()
            .1
            .split_once("\nnested_model_relationships\n")
            .unwrap()
            .0
            .lines()
            .skip(1)
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        let aic_weight_sum = rows
            .iter()
            .map(|fields| fields[10].parse::<f64>().unwrap())
            .sum::<f64>();
        let aicc_weight_sum = rows
            .iter()
            .map(|fields| fields[14].parse::<f64>().unwrap())
            .sum::<f64>();
        assert!((aic_weight_sum - 1.0).abs() < 1e-14);
        assert!((aicc_weight_sum - 1.0).abs() < 1e-14);

        assert!(matches!(
            run(args.clone()),
            Err(CliError::ModelBatch(error))
                if matches!(*error, model_batch::ModelBatchError::OutputDirectoryExists(_))
        ));

        let retained_dec_metadata = fs::read(output_dir.join("models/DEC/metadata.tsv")).unwrap();
        fs::remove_file(output_dir.join("complete.tsv")).unwrap();
        fs::remove_file(output_dir.join("comparison.tsv")).unwrap();
        fs::remove_dir_all(output_dir.join("models/DEC+J")).unwrap();
        let mut resume_args = args;
        resume_args.push("--resume".to_string());
        assert_eq!(run(resume_args).unwrap(), comparison);
        assert_eq!(
            fs::read_to_string(&model_average_path).unwrap(),
            model_average
        );
        assert_eq!(
            fs::read(output_dir.join("models/DEC/metadata.tsv")).unwrap(),
            retained_dec_metadata
        );
        assert!(output_dir.join("models/DEC+J/metadata.tsv").is_file());
        assert!(output_dir.join("complete.tsv").is_file());

        fs::write(
            &manifest_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\nDEC+J\tdecj.tsv\n# changed audit note\n",
        )
        .unwrap();
        let mut changed_resume_args = vec![
            "model-batch".to_string(),
            "--manifest".to_string(),
            manifest_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "1000".to_string(),
        ];
        changed_resume_args.push("--resume".to_string());
        assert!(matches!(
            run(changed_resume_args),
            Err(CliError::ModelBatch(error))
                if matches!(*error, model_batch::ModelBatchError::ResumeIdentityMismatch { .. })
        ));
    }

    #[test]
    fn model_batch_rejects_per_job_options_owned_by_the_manifest() {
        let error = parse_command(vec![
            "model-batch".to_string(),
            "--manifest".to_string(),
            "models.tsv".to_string(),
            "--output-dir".to_string(),
            "results".to_string(),
            "--parameters".to_string(),
            "dec.tsv".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::ModelBatchOwnedOption(option) if option == "--parameters"
        ));
    }

    #[test]
    fn model_batch_records_all_job_outcomes_instead_of_stopping_at_first_failure() {
        let temp = TempInputs::new_with_contents(
            "((A:0.2,B:0.2):0.2,C:0.4);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        fs::write(temp.dir.join("bad.tsv"), "not a parameter table\n").unwrap();
        fs::write(
            temp.dir.join("good.tsv"),
            biogeo_core::BioGeoBearsPreset::Dec
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        let manifest_path = temp.dir.join("models.tsv");
        fs::write(
            &manifest_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nBad\tbad.tsv\nGood\tgood.tsv\n",
        )
        .unwrap();
        let output_dir = temp.dir.join("batch-with-failure");
        let error = run(vec![
            "model-batch".to_string(),
            "--manifest".to_string(),
            manifest_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "50".to_string(),
        ])
        .unwrap_err();

        let attempt_path = match error {
            CliError::ModelBatchFailures {
                failed: 1,
                attempt_path,
            } => attempt_path,
            other => panic!("unexpected error: {other}"),
        };
        assert!(output_dir.join("models/Good/metadata.tsv").is_file());
        assert!(!output_dir.join("models/Bad").exists());
        assert!(!output_dir.join("complete.tsv").exists());
        let attempt = fs::read_to_string(attempt_path).unwrap();
        assert!(attempt.contains("format\tbiogeo-model-batch-attempt-v2\n"));
        assert!(attempt.contains("status\tfailed\n"));
        assert!(attempt.contains("Bad\tfailed\tmodels/Bad\tmodel_batch_error\t"));
        assert!(attempt.contains("Good\tcomplete\tmodels/Good\tNA\tNA\n"));
    }

    #[test]
    fn cancelled_model_batch_records_every_unstarted_job_and_stops() {
        let temp = TempInputs::new_with_contents(
            "((A:0.2,B:0.2):0.2,C:0.4);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        for (name, preset) in [
            ("dec.tsv", biogeo_core::BioGeoBearsPreset::Dec),
            ("decj.tsv", biogeo_core::BioGeoBearsPreset::DecJ),
        ] {
            fs::write(
                temp.dir.join(name),
                preset.parameter_table().unwrap().to_versioned_tsv(),
            )
            .unwrap();
        }
        let manifest_path = temp.dir.join("models.tsv");
        fs::write(
            &manifest_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\nDEC+J\tdecj.tsv\n",
        )
        .unwrap();
        let output_dir = temp.dir.join("cancelled-batch");
        let cancellation = biogeo_core::ExecutionCancellationToken::new();
        cancellation.cancel();

        let error = run_with_cancellation(
            vec![
                "model-batch".to_string(),
                "--manifest".to_string(),
                manifest_path.to_string_lossy().into_owned(),
                "--output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
            ],
            Some(cancellation),
        )
        .unwrap_err();

        let attempt_path = match error {
            CliError::TaskCancelled {
                operation: "model batch",
                attempt_path: Some(path),
            } => path,
            other => panic!("unexpected error: {other}"),
        };
        let attempt = fs::read_to_string(attempt_path).unwrap();
        assert!(attempt.contains("status\tcancelled\n"));
        assert!(attempt.contains("cancelled_models\t0\n"));
        assert!(attempt.contains("not_started_models\t2\n"));
        assert!(attempt.contains("DEC\tnot_started\tmodels/DEC\tNA\tNA\n"));
        assert!(attempt.contains("DEC+J\tnot_started\tmodels/DEC+J\tNA\tNA\n"));
        assert!(!output_dir.join("complete.tsv").exists());
        assert_eq!(fs::read_dir(output_dir.join("models")).unwrap().count(), 0);
    }

    #[test]
    fn dataset_batch_runs_distinct_trees_and_resumes_a_failed_dataset() {
        let temp = TempInputs::new_with_contents(
            "((A:0.2,B:0.2):0.2,C:0.4);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        let parameters_path = temp.dir.join("dec.tsv");
        fs::write(
            &parameters_path,
            biogeo_core::BioGeoBearsPreset::Dec
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        let models_path = temp.dir.join("models.tsv");
        fs::write(
            &models_path,
            "biogeo-model-batch-manifest-v1\nmodel_id\tparameters\nDEC\tdec.tsv\n",
        )
        .unwrap();
        fs::write(temp.dir.join("tree-a.nwk"), "((A:0.2,B:0.2):0.2,C:0.4);\n").unwrap();
        fs::write(
            temp.dir.join("tree-b.nex"),
            "#NEXUS\nBEGIN TREES;\nTREE ignored = ((W:0.2,X:0.2):0.2,(Y:0.2,Z:0.2):0.2);\nTREE selected = ((W:0.1,Y:0.1):0.3,(X:0.1,Z:0.1):0.3);\nEND;\n",
        )
        .unwrap();
        fs::write(
            temp.dir.join("ranges-a.tsv"),
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        )
        .unwrap();
        fs::write(
            temp.dir.join("config-a.tsv"),
            "biogeo-model-batch-config-v1\noption\tvalue\n--tree\ttree-a.nwk\n--ranges\tranges-a.tsv\n--max-iterations\t50\n",
        )
        .unwrap();
        fs::write(
            temp.dir.join("config-b.tsv"),
            "biogeo-model-batch-config-v1\noption\tvalue\n--tree\ttree-b.nex\n--tree-name\tselected\n--ranges\tranges-b.tsv\n--max-iterations\t50\n",
        )
        .unwrap();
        let manifest_path = temp.dir.join("datasets.tsv");
        fs::write(
            &manifest_path,
            "biogeo-dataset-batch-manifest-v1\ndataset_id\tmodels\tconfig\nStudyA\tmodels.tsv\tconfig-a.tsv\nStudyB\tmodels.tsv\tconfig-b.tsv\n",
        )
        .unwrap();
        let output_dir = temp.dir.join("dataset-results");
        let args = vec![
            "dataset-batch".to_string(),
            "--manifest".to_string(),
            manifest_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
        ];

        let error = run(args.clone()).unwrap_err();
        assert!(matches!(
            error,
            CliError::DatasetBatchFailures { failed: 1, .. }
        ));
        let study_a_metadata =
            fs::read(output_dir.join("datasets/StudyA/models/DEC/metadata.tsv")).unwrap();
        assert!(
            !output_dir
                .join("datasets/StudyB/models/DEC/metadata.tsv")
                .exists()
        );
        let first_attempt =
            fs::read_to_string(output_dir.join("attempts/attempt-000001.tsv")).unwrap();
        assert!(first_attempt.contains("StudyA\tcomplete\tdatasets/StudyA\t"));
        assert!(first_attempt.contains("StudyB\tfailed\tdatasets/StudyB\tNA\t"));
        let nested_failure =
            fs::read_to_string(output_dir.join("datasets/StudyB/attempts/attempt-000001.tsv"))
                .unwrap();
        assert!(nested_failure.contains("status\tfailed\n"));

        fs::write(
            temp.dir.join("ranges-b.tsv"),
            "tip\tAreaA\tAreaB\nW\t1\t0\nX\t0\t1\nY\t1\t1\nZ\t1\t0\n",
        )
        .unwrap();
        let mut resume_args = args;
        resume_args.push("--resume".to_string());
        let completion = run(resume_args).unwrap();
        assert!(completion.contains("format\tbiogeo-dataset-batch-result-v1\n"));
        assert!(completion.contains("status\tcomplete\n"));
        assert_eq!(
            fs::read(output_dir.join("datasets/StudyA/models/DEC/metadata.tsv")).unwrap(),
            study_a_metadata
        );
        let study_b_metadata =
            fs::read_to_string(output_dir.join("datasets/StudyB/models/DEC/metadata.tsv")).unwrap();
        assert!(study_b_metadata.contains("tree_name\tselected\n"));
        assert!(
            fs::read_to_string(output_dir.join("datasets/StudyA/comparison.tsv"))
                .unwrap()
                .contains("sample_size\t3\n")
        );
        assert!(
            fs::read_to_string(output_dir.join("datasets/StudyB/comparison.tsv"))
                .unwrap()
                .contains("sample_size\t4\n")
        );
        let second_attempt =
            fs::read_to_string(output_dir.join("attempts/attempt-000002.tsv")).unwrap();
        assert!(second_attempt.contains("status\tcomplete\n"));
        assert!(second_attempt.contains("complete_datasets\t2\n"));
        assert!(output_dir.join("complete.tsv").is_file());

        fs::write(
            temp.dir.join("config-a.tsv"),
            "biogeo-model-batch-config-v1\noption\tvalue\n--tree\ttree-a.nwk\n--ranges\tranges-a.tsv\n--max-iterations\t50\n# changed audit note\n",
        )
        .unwrap();
        let changed_resume = vec![
            "dataset-batch".to_string(),
            "--manifest".to_string(),
            manifest_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--resume".to_string(),
        ];
        assert!(matches!(
            run(changed_resume),
            Err(CliError::DatasetBatch(error))
                if matches!(*error, dataset_batch::DatasetBatchError::ResumeIdentityMismatch { .. })
        ));
    }

    #[test]
    fn parameter_model_evaluate_matches_fixed_dec_command() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.05)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        let generic = run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let specialized = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.05".to_string(),
        ])
        .unwrap();

        assert_eq!(
            output_field(&generic, "lnL"),
            output_field(&specialized, "lnL")
        );
        assert!(generic.contains("mode\tevaluate\n"));
        assert!(generic.contains("d\tfixed\t0.100000000000000\t"));
    }

    #[test]
    fn ambiguous_ranges_require_opt_in_and_share_the_fixed_and_parameter_engines() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t?\nB\t?\t1\nC\t1\t0\n",
        );
        let strict_error = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.05".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            strict_error,
            CliError::Ranges(biogeo_core::RangeParseError::InvalidPresenceValue {
                allow_ambiguities: false,
                ..
            })
        ));

        let validation = run(vec![
            "validate-inputs".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--use-ambiguities".to_string(),
        ])
        .unwrap();
        assert_eq!(
            output_field(&validation, "tip_observation_model"),
            "ambiguous_ranges"
        );
        assert_eq!(output_field(&validation, "ambiguous_tips"), "2");
        assert_eq!(output_field(&validation, "unknown_range_cells"), "2");
        assert_eq!(output_field(&validation, "all_unknown_tips"), "0");
        assert_eq!(
            output_field(&validation, "maximum_possible_range_size"),
            "2"
        );

        let fixed = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--use-ambiguities".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.05".to_string(),
        ])
        .unwrap();
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.05)
            .unwrap();
        let parameters_path = temp.dir.join("ambiguity-parameters.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();
        let generic = run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--use-ambiguities".to_string(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert_eq!(output_field(&generic, "lnL"), output_field(&fixed, "lnL"));
        assert_eq!(
            output_field(&generic, "tip_observation_model"),
            "ambiguous_ranges"
        );
        assert_eq!(
            generic
                .matches("tip_observation_model\tambiguous_ranges\n")
                .count(),
            1
        );
        assert_eq!(output_field(&generic, "ambiguous_tips"), "2");
    }

    #[test]
    fn analysis_result_replays_into_the_shared_bsm_engine_with_strict_guards() {
        fn bsm_table_section(output: &str, index: usize) -> &str {
            let spec = &BSM_TABLE_SPECS[index];
            let section = output
                .split_once(&format!("{}\n", spec.section))
                .expect("BSM section should exist")
                .1;
            BSM_TABLE_SPECS.get(index + 1).map_or(section, |next| {
                section
                    .split_once(&format!("{}\n", next.section))
                    .expect("next BSM section should exist")
                    .0
            })
        }

        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 1e-12)
            .unwrap()
            .with_fixed("e", 1.0)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        let analysis_dir = temp.dir.join("analysis-result");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();
        let evaluate_args = vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--include-null-range".to_string(),
            "--analysis-result-dir".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
        ];
        let evaluate_output = run(evaluate_args.clone()).unwrap();
        assert!(evaluate_output.contains("analysis_result_format\tbiogeo-analysis-result-v2\n"));
        let metadata = fs::read_to_string(analysis_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("mode\tevaluate\n"));
        assert!(metadata.contains("optimization_performed\tfalse\n"));
        assert!(matches!(
            run(evaluate_args),
            Err(CliError::AnalysisResult(
                analysis_result::AnalysisResultError::OutputExists(path)
            )) if path == analysis_dir
        ));

        let retained = run(vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "8".to_string(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "4".to_string(),
            "--seed".to_string(),
            "20260717".to_string(),
        ])
        .unwrap();
        let fixed = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.000000000001".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "8".to_string(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "4".to_string(),
            "--seed".to_string(),
            "20260717".to_string(),
        ])
        .unwrap();
        assert!(retained.contains("mode\tbsm\n"));
        for (index, spec) in BSM_TABLE_SPECS.iter().enumerate() {
            assert_eq!(
                bsm_table_section(&retained, index),
                bsm_table_section(&fixed, index),
                "configured and fixed BSM differ in {}",
                spec.section
            );
        }

        let mut baseline = None;
        for threads in [1, 4] {
            let output_dir = temp.dir.join(format!("analysis-bsm-{threads}"));
            let output = run(vec![
                "model-bsm".to_string(),
                "--analysis-result".to_string(),
                analysis_dir.to_string_lossy().into_owned(),
                "--bsm-samples".to_string(),
                "8".to_string(),
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--bsm-threads".to_string(),
                threads.to_string(),
                "--bsm-max-in-flight".to_string(),
                threads.to_string(),
                "--seed".to_string(),
                "20260717".to_string(),
            ])
            .unwrap();
            assert!(output.contains("biogeographic_stochastic_histories\n"));
            let stream_metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
            assert!(
                stream_metadata.contains("analysis_result_format\tbiogeo-analysis-result-v2\n")
            );
            assert!(stream_metadata.contains("status\tcomplete\n"));
            let tables = BSM_TABLE_SPECS
                .iter()
                .map(|spec| fs::read(output_dir.join(spec.file_name)).unwrap())
                .collect::<Vec<_>>();
            if let Some(expected) = &baseline {
                assert_eq!(&tables, expected, "thread count {threads}");
            } else {
                baseline = Some(tables);
            }
        }

        fs::write(&temp.ranges_path, "tip\tAreaA\nA\t1\nB\t1\n").unwrap();
        run(vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "1".to_string(),
        ])
        .unwrap();
        let loaded = analysis_result::load_analysis_result(&analysis_dir).unwrap();
        fs::write(
            loaded.require_input_path("ranges").unwrap(),
            "tip\tAreaA\nA\t1\nB\t1\n",
        )
        .unwrap();
        assert!(matches!(
            run(vec![
                "model-bsm".to_string(),
                "--analysis-result".to_string(),
                analysis_dir.to_string_lossy().into_owned(),
                "--bsm-samples".to_string(),
                "1".to_string(),
            ]),
            Err(CliError::AnalysisResult(
                analysis_result::AnalysisResultError::InputBundle(source)
            )) if matches!(
                source.as_ref(),
                input_bundle::InputBundleError::FileChanged { id, .. } if id == "input:ranges"
            )
        ));
    }

    #[test]
    fn analysis_result_inspection_and_migration_survive_original_input_removal() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let parameters_path = temp.dir.join("migration-parameters.tsv");
        let current_dir = temp.dir.join("current-result");
        let legacy_dir = temp.dir.join("legacy-result");
        let migrated_dir = temp.dir.join("migrated-result");
        let redundant_dir = temp.dir.join("redundant-result");
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.05)
            .unwrap();
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();
        run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--analysis-result-dir".to_string(),
            current_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let current = analysis_result::load_analysis_result(&current_dir).unwrap();
        analysis_result::write_legacy_analysis_result_for_test(
            &legacy_dir,
            &analysis_result::AnalysisResultWriteRequest {
                mode: &current.manifest.mode,
                log_likelihood: current.manifest.log_likelihood,
                model_fingerprint: &current.manifest.model_fingerprint,
                tip_observation_model: &current.manifest.tip_observation_model,
                tree_name: current.manifest.tree_name.as_deref(),
                max_range_size: current.manifest.max_range_size,
                include_null_range: current.manifest.include_null_range,
                root_prior: &current.manifest.root_prior,
                min_branch_length: current.manifest.min_branch_length,
                missing_branch_length_fill: current.manifest.missing_branch_length_fill,
                states: current.manifest.states,
                areas: current.manifest.areas,
                tips: current.manifest.tips,
                optimization: current.manifest.optimization,
                source_parameters: &current.source_parameters,
                resolved_parameters: &current.resolved_parameters,
                inputs: vec![
                    analysis_result::AnalysisInputSpec {
                        role: "tree",
                        path: &temp.tree_path,
                        required_for_replay: true,
                    },
                    analysis_result::AnalysisInputSpec {
                        role: "ranges",
                        path: &temp.ranges_path,
                        required_for_replay: true,
                    },
                    analysis_result::AnalysisInputSpec {
                        role: "source_parameters",
                        path: &parameters_path,
                        required_for_replay: false,
                    },
                ],
            },
        )
        .unwrap();
        fs::remove_file(&parameters_path).unwrap();

        let legacy_inspection = run(vec![
            "analysis-result-inspect".to_string(),
            "--analysis-result".to_string(),
            legacy_dir.to_string_lossy().into_owned(),
            "--replay".to_string(),
        ])
        .unwrap();
        assert!(legacy_inspection.contains("analysis_result_format\tbiogeo-analysis-result-v1\n"));
        assert!(legacy_inspection.contains("portable\tfalse\n"));
        assert!(legacy_inspection.contains("replay_validation\tpassed\n"));

        let migration = run(vec![
            "analysis-result-migrate".to_string(),
            "--analysis-result".to_string(),
            legacy_dir.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            migrated_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(migration.contains("source_format\tbiogeo-analysis-result-v1\n"));
        assert!(migration.contains("target_format\tbiogeo-analysis-result-v2\n"));
        assert!(migration.contains("scientific_replay\tpassed\n"));
        assert!(matches!(
            run(vec![
                "analysis-result-migrate".to_string(),
                "--analysis-result".to_string(),
                legacy_dir.to_string_lossy().into_owned(),
                "--output-dir".to_string(),
                migrated_dir.to_string_lossy().into_owned(),
            ]),
            Err(CliError::AnalysisResult(
                analysis_result::AnalysisResultError::OutputExists(path)
            )) if path == migrated_dir
        ));
        assert!(matches!(
            run(vec![
                "analysis-result-migrate".to_string(),
                "--analysis-result".to_string(),
                migrated_dir.to_string_lossy().into_owned(),
                "--output-dir".to_string(),
                redundant_dir.to_string_lossy().into_owned(),
            ]),
            Err(CliError::AnalysisResult(
                analysis_result::AnalysisResultError::AlreadyCurrentFormat(_)
            ))
        ));
        assert!(!redundant_dir.exists());

        fs::remove_dir_all(current_dir).unwrap();
        fs::remove_file(&temp.tree_path).unwrap();
        fs::remove_file(&temp.ranges_path).unwrap();
        let migrated_inspection = run(vec![
            "analysis-result-inspect".to_string(),
            "--analysis-result".to_string(),
            migrated_dir.to_string_lossy().into_owned(),
            "--replay".to_string(),
        ])
        .unwrap();
        assert!(migrated_inspection.contains("portable\ttrue\n"));
        assert!(migrated_inspection.contains("replay_validation\tpassed\n"));
        let bundle_inspection = run(vec![
            "input-bundle-inspect".to_string(),
            "--input-bundle".to_string(),
            migrated_dir
                .join(analysis_result::INPUT_BUNDLE_DIR)
                .to_string_lossy()
                .into_owned(),
        ])
        .unwrap();
        assert!(bundle_inspection.contains("status\tvalid\n"));
        assert!(bundle_inspection.contains("input_count\t3\n"));
        run(vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            migrated_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "19".to_string(),
        ])
        .unwrap();
    }

    #[test]
    fn ambiguous_analysis_result_replays_into_bsm_without_losing_tip_constraints() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t?\nB\t?\t1\n");
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 1.0)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        let analysis_dir = temp.dir.join("ambiguous-analysis");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        let evaluated = run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--use-ambiguities".to_string(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--include-null-range".to_string(),
            "--analysis-result-dir".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert_eq!(
            output_field(&evaluated, "tip_observation_model"),
            "ambiguous_ranges"
        );
        let analysis_metadata = fs::read_to_string(analysis_dir.join("metadata.tsv")).unwrap();
        assert!(analysis_metadata.contains("tip_observation_model\tambiguous_ranges\n"));

        let replay_dir = temp.dir.join("replay-bsm");
        let replayed = run(vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "32".to_string(),
            "--bsm-output-dir".to_string(),
            replay_dir.to_string_lossy().into_owned(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "4".to_string(),
            "--seed".to_string(),
            "20260722".to_string(),
        ])
        .unwrap();
        assert_eq!(
            output_field(&replayed, "tip_observation_model"),
            "ambiguous_ranges"
        );
        let replay_metadata = fs::read_to_string(replay_dir.join("metadata.tsv")).unwrap();
        assert!(replay_metadata.contains("tip_observation_model\tambiguous_ranges\n"));

        let fixed_dir = temp.dir.join("fixed-bsm");
        run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--use-ambiguities".to_string(),
            "--d".to_string(),
            "1".to_string(),
            "--e".to_string(),
            "0.1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "32".to_string(),
            "--bsm-output-dir".to_string(),
            fixed_dir.to_string_lossy().into_owned(),
            "--bsm-threads".to_string(),
            "1".to_string(),
            "--bsm-max-in-flight".to_string(),
            "1".to_string(),
            "--seed".to_string(),
            "20260722".to_string(),
        ])
        .unwrap();
        for spec in BSM_TABLE_SPECS {
            assert_eq!(
                fs::read(replay_dir.join(spec.file_name)).unwrap(),
                fs::read(fixed_dir.join(spec.file_name)).unwrap(),
                "analysis replay differs in {}",
                spec.file_name
            );
        }

        let node_states = fs::read_to_string(fixed_dir.join("node_states.tsv")).unwrap();
        let mut saw_ambiguous_expansion = false;
        for line in node_states.lines().skip(1) {
            let fields = line.split('\t').collect::<Vec<_>>();
            let label = fields[2];
            let bits = fields[6].parse::<u64>().unwrap();
            match label {
                "A" => {
                    assert_ne!(bits & 0b01, 0, "sampled A violates required AreaA");
                    saw_ambiguous_expansion |= bits == 0b11;
                }
                "B" => {
                    assert_ne!(bits & 0b10, 0, "sampled B violates required AreaB");
                    saw_ambiguous_expansion |= bits == 0b11;
                }
                _ => {}
            }
        }
        assert!(saw_ambiguous_expansion);
    }

    #[test]
    fn bsm_fingerprint_distinguishes_exact_and_ambiguity_observation_modes() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let run_mode = |label: &str, use_ambiguities: bool| {
            let output_dir = temp.dir.join(label);
            let mut args = vec![
                "dec".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0.1".to_string(),
                "--e".to_string(),
                "0.05".to_string(),
                "--bsm-samples".to_string(),
                "1".to_string(),
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--seed".to_string(),
                "7".to_string(),
            ];
            if use_ambiguities {
                args.push("--use-ambiguities".to_string());
            }
            run(args).unwrap();
            output_dir
        };

        let exact_dir = run_mode("exact-bsm", false);
        let ambiguous_dir = run_mode("ambiguity-bsm", true);
        let exact_metadata = fs::read_to_string(exact_dir.join("metadata.tsv")).unwrap();
        let ambiguous_metadata = fs::read_to_string(ambiguous_dir.join("metadata.tsv")).unwrap();
        assert_ne!(
            output_field(&exact_metadata, "run_fingerprint"),
            output_field(&ambiguous_metadata, "run_fingerprint")
        );
        assert!(!exact_metadata.contains("tip_observation_model\t"));
        assert!(ambiguous_metadata.contains("tip_observation_model\tambiguous_ranges\n"));
        for spec in BSM_TABLE_SPECS {
            assert_eq!(
                fs::read(exact_dir.join(spec.file_name)).unwrap(),
                fs::read(ambiguous_dir.join(spec.file_name)).unwrap(),
                "observation-mode identity changed sampled rows in {}",
                spec.file_name
            );
        }
    }

    #[test]
    fn analysis_result_persists_named_nexus_tree_selection_for_replay() {
        let temp = TempInputs::new_with_contents(
            "#NEXUS\nBEGIN TREES;\n\
             TREE first = [&R] (A:0.2,B:0.2);\n\
             TREE selected = [&R] (A:1,B:1);\nEND;\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n",
        );
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.05)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        let analysis_dir = temp.dir.join("named-tree-analysis");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--tree-name".to_string(),
            "selected".to_string(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--analysis-result-dir".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let metadata_path = analysis_dir.join("metadata.tsv");
        let metadata = fs::read_to_string(&metadata_path).unwrap();
        assert!(metadata.contains("tree_name\tselected\n"));

        let replay = run(vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "1".to_string(),
            "--seed".to_string(),
            "17".to_string(),
        ])
        .unwrap();
        assert!(replay.contains("mode\tbsm\n"));

        fs::write(
            &metadata_path,
            metadata.replace("tree_name\tselected\n", "tree_name\tfirst\n"),
        )
        .unwrap();
        assert!(matches!(
            run(vec![
                "model-bsm".to_string(),
                "--analysis-result".to_string(),
                analysis_dir.to_string_lossy().into_owned(),
                "--bsm-samples".to_string(),
                "1".to_string(),
            ]),
            Err(CliError::AnalysisReplayMismatch { field: "lnL", .. })
        ));
    }

    #[test]
    fn analysis_result_replays_detection_and_stratified_modifiers() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let strata_path = write_raw_anagenetic_strata(&temp);
        let detections_path = temp.dir.join("detections.tsv");
        let controls_path = temp.dir.join("controls.tsv");
        fs::write(
            &detections_path,
            "\tAreaA\tAreaB\nA\t2\t0\nB\t0\t2\nC\t1\t0\n",
        )
        .unwrap();
        fs::write(
            &controls_path,
            "\tAreaA\tAreaB\nA\t10\t10\nB\t10\t10\nC\t10\t10\n",
        )
        .unwrap();
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_fixed("x", -0.4)
            .unwrap()
            .with_fixed("n", 0.25)
            .unwrap()
            .with_fixed("u", -0.3)
            .unwrap()
            .with_fixed("mf", 0.2)
            .unwrap()
            .with_fixed("dp", 0.8)
            .unwrap()
            .with_fixed("fdp", 0.05)
            .unwrap();
        let parameters_path = temp.dir.join("detection-strata-parameters.tsv");
        let analysis_dir = temp.dir.join("detection-strata-analysis");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();
        run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--use-detection-model".to_string(),
            "--detections".to_string(),
            detections_path.to_string_lossy().into_owned(),
            "--controls".to_string(),
            controls_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
            "--analysis-result-dir".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();

        let bsm_args = vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-threads".to_string(),
            "2".to_string(),
            "--bsm-max-in-flight".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "17072026".to_string(),
        ];
        let output = run(bsm_args.clone()).unwrap();
        assert_eq!(output, run(bsm_args).unwrap());
        assert!(output.contains("tip_observation_model\tmf_dp_fdp_detection\n"));
        assert!(output.contains("x\t-0.4\n"));
        assert!(output.contains("bsm_sample_period_event_counts\n"));
        assert!(output.contains("bsm_sample_period_state_occupancy\n"));

        fs::write(
            temp.dir.join("young-distance.tsv"),
            "from\tAreaA\tAreaB\nAreaA\t0\t9\nAreaB\t7\t0\n",
        )
        .unwrap();
        run(vec![
            "model-bsm".to_string(),
            "--analysis-result".to_string(),
            analysis_dir.to_string_lossy().into_owned(),
            "--bsm-samples".to_string(),
            "1".to_string(),
        ])
        .unwrap();
        let loaded = analysis_result::load_analysis_result(&analysis_dir).unwrap();
        let bundled_distance = loaded
            .input_bundle
            .as_ref()
            .unwrap()
            .files
            .values()
            .find(|record| record.kind == "dependency" && record.role == "distance_matrix")
            .unwrap()
            .path
            .clone();
        fs::write(
            bundled_distance,
            "from\tAreaA\tAreaB\nAreaA\t0\t9\nAreaB\t7\t0\n",
        )
        .unwrap();
        assert!(matches!(
            run(vec![
                "model-bsm".to_string(),
                "--analysis-result".to_string(),
                analysis_dir.to_string_lossy().into_owned(),
                "--bsm-samples".to_string(),
                "1".to_string(),
            ]),
            Err(CliError::AnalysisResult(
                analysis_result::AnalysisResultError::InputBundle(source)
            )) if matches!(
                source.as_ref(),
                input_bundle::InputBundleError::FileChanged { .. }
            )
        ));
    }

    #[test]
    fn parameter_model_static_xnu_matches_fixed_dec_command() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_fixed("x", -0.4)
            .unwrap()
            .with_fixed("n", 0.25)
            .unwrap()
            .with_fixed("u", -0.3)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        let distance_path = temp.dir.join("distance.tsv");
        let environment_path = temp.dir.join("environment.tsv");
        let area_sizes_path = temp.dir.join("area-sizes.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();
        fs::write(
            &distance_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t2\nAreaB\t4\t0\n",
        )
        .unwrap();
        fs::write(
            &environment_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t1.5\nAreaB\t2.5\t0\n",
        )
        .unwrap();
        fs::write(&area_sizes_path, "area\tsize\nAreaA\t0.5\nAreaB\t2\n").unwrap();

        let common = vec![
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--distance-matrix".to_string(),
            distance_path.to_string_lossy().into_owned(),
            "--environment-distance-matrix".to_string(),
            environment_path.to_string_lossy().into_owned(),
            "--area-sizes".to_string(),
            area_sizes_path.to_string_lossy().into_owned(),
        ];
        let mut generic_args = vec!["model-evaluate".to_string()];
        generic_args.extend(common.clone());
        generic_args.extend([
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ]);
        let generic = run(generic_args).unwrap();

        let mut specialized_args = vec!["dec".to_string()];
        specialized_args.extend(common);
        specialized_args.extend([
            "--d".to_string(),
            "0.08".to_string(),
            "--e".to_string(),
            "0.03".to_string(),
            "--distance-exponent".to_string(),
            "-0.4".to_string(),
            "--environment-distance-exponent".to_string(),
            "0.25".to_string(),
            "--area-exponent".to_string(),
            "-0.3".to_string(),
        ]);
        let specialized = run(specialized_args).unwrap();

        assert_eq!(
            output_field(&generic, "lnL"),
            output_field(&specialized, "lnL")
        );
    }

    #[test]
    fn parameter_model_optimizes_linked_cladogenesis_parameters() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.4):0.2,C:0.6);\n",
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t1\t1\t0\n",
        );
        let bounds = biogeo_core::ParameterBounds::new(0.0001, 0.9999).unwrap();
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.04)
            .unwrap()
            .with_fixed("e", 0.02)
            .unwrap()
            .with_free("y", 0.4, bounds)
            .unwrap()
            .with_derived_from_str("s", "y/2")
            .unwrap()
            .with_free("v", 0.5, bounds)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        let output = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "80".to_string(),
            "--additional-start".to_string(),
            "0.8,0.2".to_string(),
        ])
        .unwrap();

        assert!(output.contains("mode\toptimize\n"));
        assert!(output.contains("y\tfree\t"));
        assert!(output.contains("s\tderived\t"));
        assert!(output.contains("v\tfree\t"));
        assert!(output.contains("evaluations\t"));
        assert!(output.contains("starts\t2\n"));
    }

    #[test]
    fn parameter_model_honors_preexisting_cancellation() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.4):0.2,C:0.6);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        let parameters_path = temp.dir.join("parameters.tsv");
        fs::write(
            &parameters_path,
            biogeo_core::BioGeoBearsPreset::Dec
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        let cancellation = biogeo_core::ExecutionCancellationToken::new();
        cancellation.cancel();

        let error = run_with_cancellation(
            vec![
                "model-optimize".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--parameters".to_string(),
                parameters_path.to_string_lossy().into_owned(),
            ],
            Some(cancellation),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            CliError::TaskCancelled {
                operation: "model optimization",
                attempt_path: None
            }
        ));
        assert_eq!(error.exit_code(), 130);
        assert_eq!(error.stable_code(), "task_cancelled");
    }

    #[test]
    fn parameter_model_time_stratified_xnu_matches_fixed_dec_command() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        let strata_path = write_raw_anagenetic_strata(&temp);
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_fixed("x", -0.4)
            .unwrap()
            .with_fixed("n", 0.25)
            .unwrap()
            .with_fixed("u", -0.3)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        let common = vec![
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ];
        let mut generic_args = vec!["model-evaluate".to_string()];
        generic_args.extend(common.clone());
        generic_args.extend([
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ]);
        let generic = run(generic_args).unwrap();

        let mut specialized_args = vec!["dec".to_string()];
        specialized_args.extend(common);
        specialized_args.extend([
            "--d".to_string(),
            "0.08".to_string(),
            "--e".to_string(),
            "0.03".to_string(),
            "--distance-exponent".to_string(),
            "-0.4".to_string(),
            "--environment-distance-exponent".to_string(),
            "0.25".to_string(),
            "--area-exponent".to_string(),
            "-0.3".to_string(),
        ]);
        let specialized = run(specialized_args).unwrap();

        assert_eq!(
            output_field(&generic, "lnL"),
            output_field(&specialized, "lnL")
        );
    }

    #[test]
    fn parameter_model_applies_a_b_and_manual_multiplier_exponent_w() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.7):0.2,C:0.9);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let powered_table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_fixed("a", 0.04)
            .unwrap()
            .with_fixed("b", 0.6)
            .unwrap()
            .with_fixed("w", 2.0)
            .unwrap();
        let pretransformed_table = powered_table.clone().with_fixed("w", 1.0).unwrap();
        let powered_parameters = temp.dir.join("powered-parameters.tsv");
        let pretransformed_parameters = temp.dir.join("pretransformed-parameters.tsv");
        let manual = temp.dir.join("manual.tsv");
        let manual_squared = temp.dir.join("manual-squared.tsv");
        fs::write(&powered_parameters, powered_table.to_versioned_tsv()).unwrap();
        fs::write(
            &pretransformed_parameters,
            pretransformed_table.to_versioned_tsv(),
        )
        .unwrap();
        fs::write(&manual, "from\tAreaA\tAreaB\nAreaA\t1\t0.5\nAreaB\t2\t1\n").unwrap();
        fs::write(
            &manual_squared,
            "from\tAreaA\tAreaB\nAreaA\t1\t0.25\nAreaB\t4\t1\n",
        )
        .unwrap();

        let evaluate = |parameters: &PathBuf, multipliers: &PathBuf| {
            run(vec![
                "model-evaluate".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--parameters".to_string(),
                parameters.to_string_lossy().into_owned(),
                "--dispersal-multipliers".to_string(),
                multipliers.to_string_lossy().into_owned(),
            ])
            .unwrap()
        };
        let powered = evaluate(&powered_parameters, &manual);
        let pretransformed = evaluate(&pretransformed_parameters, &manual_squared);

        assert_eq!(
            output_field(&powered, "lnL"),
            output_field(&pretransformed, "lnL")
        );
        assert!(powered.contains("a\tfixed\t0.040000000000000\t"));
        assert!(powered.contains("b\tfixed\t0.600000000000000\t"));
        assert!(powered.contains("w\tfixed\t2.000000000000000\t"));
    }

    #[test]
    fn parameter_model_optimizes_a_b_and_w_through_the_generic_engine() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.7):0.2,C:0.9);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_free(
                "a",
                0.04,
                biogeo_core::ParameterBounds::new(1e-6, 0.5).unwrap(),
            )
            .unwrap()
            .with_free(
                "b",
                0.6,
                biogeo_core::ParameterBounds::new(0.1, 1.0).unwrap(),
            )
            .unwrap()
            .with_free(
                "w",
                1.0,
                biogeo_core::ParameterBounds::new(-2.0, 2.0).unwrap(),
            )
            .unwrap();
        let parameters = temp.dir.join("parameters.tsv");
        let manual = temp.dir.join("manual.tsv");
        fs::write(&parameters, table.to_versioned_tsv()).unwrap();
        fs::write(&manual, "from\tAreaA\tAreaB\nAreaA\t1\t0.5\nAreaB\t2\t1\n").unwrap();

        let output = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters.to_string_lossy().into_owned(),
            "--dispersal-multipliers".to_string(),
            manual.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "8".to_string(),
        ])
        .unwrap();

        assert!(output.contains("a\tfree\t"));
        assert!(output.contains("b\tfree\t"));
        assert!(output.contains("w\tfree\t"));
        assert!(output.contains("evaluations\t"));
    }

    #[test]
    fn parameter_model_detection_evaluate_matches_core_tip_likelihoods() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let detections_path = temp.dir.join("detections.tsv");
        let controls_path = temp.dir.join("controls.tsv");
        fs::write(
            &detections_path,
            "\tAreaA\tAreaB\nA\t2\t0\nB\t0\t2\nC\t1\t0\n",
        )
        .unwrap();
        fs::write(
            &controls_path,
            "\tAreaA\tAreaB\nA\t10\t10\nB\t10\t10\nC\t10\t10\n",
        )
        .unwrap();
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_fixed("mf", 0.1)
            .unwrap()
            .with_fixed("dp", 0.8)
            .unwrap()
            .with_fixed("fdp", 0.01)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        let output = run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--use-detection-model".to_string(),
            "--detections".to_string(),
            detections_path.to_string_lossy().into_owned(),
            "--controls".to_string(),
            controls_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--include-null-range".to_string(),
        ])
        .unwrap();

        let tree =
            biogeo_core::parse_newick(&fs::read_to_string(&temp.tree_path).unwrap()).unwrap();
        let data = biogeo_core::parse_detection_data(
            &fs::read_to_string(&detections_path).unwrap(),
            &fs::read_to_string(&controls_path).unwrap(),
            &tree,
        )
        .unwrap();
        let states = biogeo_core::StateSpace::new(2, 2, true).unwrap();
        let resolved = table.resolve_initial().unwrap();
        let model = biogeo_core::ModelConfig::from_biogeobears_core_parameters(&resolved).unwrap();
        let tips = biogeo_core::DetectionModel::new(0.1, 0.8, 0.01)
            .unwrap()
            .tip_likelihoods(&data, &states)
            .unwrap();
        let expected =
            biogeo_core::LikelihoodEngine::new(&tree.tree, &states, biogeo_core::RootPrior::Flat)
                .evaluate(&model, &tips)
                .unwrap();

        assert_eq!(
            output_field(&output, "lnL"),
            format!("{:.15}", expected.log_likelihood)
        );
        assert_eq!(
            output_field(&output, "tip_observation_model"),
            "mf_dp_fdp_detection"
        );
        assert_eq!(output_field(&output, "ranges"), "none");
    }

    #[test]
    fn parameter_model_jointly_optimizes_mf_dp_and_fdp() {
        let temp = TempInputs::new_with_contents(
            "((A:0.3,B:0.3):0.2,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let detections_path = temp.dir.join("detections.tsv");
        let controls_path = temp.dir.join("controls.tsv");
        fs::write(
            &detections_path,
            "AreaA\tAreaB\nA\t2\t0\nB\t0\t2\nC\t1\t0\n",
        )
        .unwrap();
        fs::write(
            &controls_path,
            "AreaA\tAreaB\nA\t10\t10\nB\t10\t10\nC\t10\t10\n",
        )
        .unwrap();
        let probability_bounds = biogeo_core::ParameterBounds::new(0.005, 0.995).unwrap();
        let table = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.08)
            .unwrap()
            .with_fixed("e", 0.03)
            .unwrap()
            .with_free("mf", 0.1, probability_bounds)
            .unwrap()
            .with_free("dp", 0.8, probability_bounds)
            .unwrap()
            .with_free("fdp", 0.01, probability_bounds)
            .unwrap();
        let parameters_path = temp.dir.join("parameters.tsv");
        fs::write(&parameters_path, table.to_versioned_tsv()).unwrap();

        let output = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--use-detection-model".to_string(),
            "--detections".to_string(),
            detections_path.to_string_lossy().into_owned(),
            "--controls".to_string(),
            controls_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--include-null-range".to_string(),
            "--max-iterations".to_string(),
            "12".to_string(),
        ])
        .unwrap();

        assert!(output.contains("mf\tfree\t"));
        assert!(output.contains("dp\tfree\t"));
        assert!(output.contains("fdp\tfree\t"));
        assert!(output.contains("evaluations\t"));
    }

    #[test]
    fn parameter_model_detection_options_are_explicit_and_exclusive() {
        let without_mode = parse_command(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--detections".to_string(),
            "detections.tsv".to_string(),
            "--controls".to_string(),
            "controls.tsv".to_string(),
            "--parameters".to_string(),
            "parameters.tsv".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            without_mode,
            CliError::DetectionInputRequiresModel
        ));

        let mixed = parse_command(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--use-detection-model".to_string(),
            "--detections".to_string(),
            "detections.tsv".to_string(),
            "--controls".to_string(),
            "controls.tsv".to_string(),
            "--parameters".to_string(),
            "parameters.tsv".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(mixed, CliError::ConflictingTipObservationInputs));
    }

    #[test]
    fn parameter_model_keeps_biogeobears_mx01r_as_a_fixed_compatibility_noop() {
        let temp = TempInputs::new();
        let nondefault = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap()
            .with_fixed("mx01r", 0.4)
            .unwrap();
        let nondefault_path = temp.dir.join("mx01r-nondefault.tsv");
        fs::write(&nondefault_path, nondefault.to_versioned_tsv()).unwrap();

        let error = run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            nondefault_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::UnsupportedParameterSemantics {
                parameter: "mx01r",
                required_value: 0.5
            }
        ));

        let released = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap()
            .with_free(
                "mx01r",
                0.5,
                biogeo_core::ParameterBounds::new(0.0001, 0.9999).unwrap(),
            )
            .unwrap();
        let released_path = temp.dir.join("mx01r-released.tsv");
        fs::write(&released_path, released.to_versioned_tsv()).unwrap();

        let error = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            released_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::UnsupportedParameterSemantics {
                parameter: "mx01r",
                required_value: 0.5
            }
        ));
    }

    #[test]
    fn parameter_model_rejects_missing_inputs_and_unused_free_parameters() {
        let temp = TempInputs::new();
        let bounds = biogeo_core::ParameterBounds::new(-2.5, 2.5).unwrap();
        let missing_input = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap()
            .with_free("x", -0.5, bounds)
            .unwrap();
        let parameters_path = temp.dir.join("missing-input.tsv");
        fs::write(&parameters_path, missing_input.to_versioned_tsv()).unwrap();
        let error = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::ParameterInputRequired { parameter: "x", .. }
        ));

        let unused = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap()
            .with_free(
                "ys",
                1.0,
                biogeo_core::ParameterBounds::new(1e-5, 2.0).unwrap(),
            )
            .unwrap();
        let parameters_path = temp.dir.join("unused.tsv");
        fs::write(&parameters_path, unused.to_versioned_tsv()).unwrap();
        let error = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(error, CliError::UnusedFreeParameter(parameter) if parameter == "ys"));

        let missing_manual = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap()
            .with_free(
                "w",
                1.0,
                biogeo_core::ParameterBounds::new(-2.0, 2.0).unwrap(),
            )
            .unwrap();
        let parameters_path = temp.dir.join("missing-manual.tsv");
        fs::write(&parameters_path, missing_manual.to_versioned_tsv()).unwrap();
        let error = run(vec![
            "model-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            &error,
            CliError::ParameterInputRequired { parameter: "w", .. }
        ));
        assert_eq!(
            error.to_string(),
            "parameter w can be nonzero, derived, or free only when its raw modifier input is provided with --dispersal-multipliers or --dispersal-strata"
        );

        let stratified_b = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.1)
            .unwrap()
            .with_fixed("b", 0.5)
            .unwrap();
        let parameters_path = temp.dir.join("stratified-b.tsv");
        fs::write(&parameters_path, stratified_b.to_versioned_tsv()).unwrap();
        let strata_path = write_raw_anagenetic_strata(&temp);
        let error = run(vec![
            "model-evaluate".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--parameters".to_string(),
            parameters_path.to_string_lossy().into_owned(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(error, CliError::StratifiedBranchLengthExponent));
        assert_eq!(
            error.to_string(),
            "parameter b must remain fixed at 1 when --dispersal-strata is used because BioGeoBEARS defines b as non-stratified only"
        );
    }

    fn output_field<'a>(output: &'a str, key: &str) -> &'a str {
        output
            .lines()
            .find_map(|line| line.split_once('\t').filter(|(name, _)| *name == key))
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("output has no {key:?} field"))
    }

    #[test]
    fn parses_dec_command() {
        let command = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "0.3".to_string(),
            "--mx01".to_string(),
            "0.5".to_string(),
            "--mx01y".to_string(),
            "0.8".to_string(),
            "--mx01v".to_string(),
            "0.25".to_string(),
            "--max-range-size".to_string(),
            "2".to_string(),
            "--include-null-range".to_string(),
            "--root-prior".to_string(),
            "equal".to_string(),
            "--min-branch-length".to_string(),
            "0.000001".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
            "--traceback-samples".to_string(),
            "3".to_string(),
            "--seed".to_string(),
            "42".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Fixed(FixedModelConfig {
                preset: FixedPreset::Dec,
                tree_path: PathBuf::from("tree.nwk"),
                tree_name: None,
                ranges_path: PathBuf::from("ranges.tsv"),
                use_ambiguities: false,
                d: 0.1,
                e: 0.2,
                j: 0.3,
                range_size: biogeo_core::CladogenesisRangeSizeConfig {
                    mx01y: 0.8,
                    mx01s: 0.5,
                    mx01v: 0.25,
                    mx01j: 0.5,
                },
                min_branch_length: 1e-6,
                max_range_size: Some(2),
                dispersal_multipliers_path: None,
                dispersal_strata_path: None,
                distance_matrix_path: None,
                distance_exponent: None,
                environment_distance_matrix_path: None,
                environment_distance_exponent: None,
                extirpation_multipliers_path: None,
                area_sizes_path: None,
                area_exponent: None,
                include_null_range: true,
                root_prior: RootPriorKind::Equal,
                ancestral_probs: true,
                split_probs: true,
                traceback_samples: 3,
                bsm_samples: 0,
                bsm_output_dir_path: None,
                bsm_output_level: BsmOutputLevel::Legacy,
                bsm_threads: BsmThreadSelection::Auto,
                bsm_max_in_flight: None,
                bsm_max_events_per_sample: None,
                bsm_max_events_total: None,
                bsm_memory_budget_mb: None,
                bsm_shard_samples: None,
                bsm_checkpoint_samples: None,
                bsm_resume: false,
                bsm_time_limit: None,
                bsm_interactive: false,
                seed: 42,
            })
        );
    }

    #[test]
    fn parses_and_resolves_bsm_execution_options() {
        let command = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--bsm-samples".to_string(),
            "100".to_string(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "12".to_string(),
            "--bsm-max-events-per-sample".to_string(),
            "500".to_string(),
            "--bsm-max-events-total".to_string(),
            "5000".to_string(),
            "--bsm-memory-budget-mb".to_string(),
            "64".to_string(),
            "--bsm-shard-samples".to_string(),
            "25".to_string(),
            "--bsm-output-dir".to_string(),
            "bsm-output".to_string(),
            "--bsm-checkpoint-samples".to_string(),
            "7".to_string(),
            "--bsm-resume".to_string(),
            "--bsm-time-limit-seconds".to_string(),
            "1.5".to_string(),
            "--bsm-interactive".to_string(),
        ])
        .unwrap();
        let Command::Fixed(config) = command else {
            panic!("expected fixed command");
        };
        assert_eq!(config.bsm_threads, BsmThreadSelection::Fixed(4));
        assert_eq!(config.bsm_max_in_flight, Some(12));
        assert_eq!(config.bsm_max_events_per_sample, Some(500));
        assert_eq!(config.bsm_max_events_total, Some(5000));
        assert_eq!(config.bsm_memory_budget_mb, Some(64));
        assert_eq!(config.bsm_shard_samples, Some(25));
        assert_eq!(config.bsm_checkpoint_samples, Some(7));
        assert!(config.bsm_resume);
        assert_eq!(config.bsm_time_limit, Some(Duration::from_millis(1_500)));
        assert!(config.bsm_interactive);
        assert_eq!(
            resolve_bsm_execution_with_available(
                BsmExecutionRequest::from_config(&config),
                config.bsm_samples,
                16,
            )
            .unwrap(),
            Some(ResolvedBsmExecution {
                available_parallelism: 16,
                threads: 4,
                max_in_flight: 12,
                checkpoint_samples: 7,
                max_events_per_sample: Some(500),
                max_events_total: Some(5000),
                memory_budget_mb: Some(64),
                shard_samples: Some(25),
                retained_bytes_per_sample_upper_bound: None,
                buffered_history_bytes_upper_bound: None,
                time_limit: Some(Duration::from_millis(1_500)),
            })
        );

        assert_eq!(
            resolve_bsm_execution_with_available(BsmExecutionRequest::default(), 100, 16,).unwrap(),
            Some(ResolvedBsmExecution {
                available_parallelism: 16,
                threads: 16,
                max_in_flight: 32,
                checkpoint_samples: 100,
                max_events_per_sample: None,
                max_events_total: None,
                memory_budget_mb: None,
                shard_samples: None,
                retained_bytes_per_sample_upper_bound: None,
                buffered_history_bytes_upper_bound: None,
                time_limit: None,
            })
        );
        assert_eq!(
            resolve_bsm_execution_with_available(
                BsmExecutionRequest {
                    thread_selection: BsmThreadSelection::Fixed(64),
                    ..BsmExecutionRequest::default()
                },
                5,
                16,
            )
            .unwrap(),
            Some(ResolvedBsmExecution {
                available_parallelism: 16,
                threads: 5,
                max_in_flight: 5,
                checkpoint_samples: 5,
                max_events_per_sample: None,
                max_events_total: None,
                memory_budget_mb: None,
                shard_samples: None,
                retained_bytes_per_sample_upper_bound: None,
                buffered_history_bytes_upper_bound: None,
                time_limit: None,
            })
        );
        assert!(matches!(
            resolve_bsm_execution_with_available(
                BsmExecutionRequest {
                    thread_selection: BsmThreadSelection::Fixed(4),
                    max_in_flight: Some(3),
                    ..BsmExecutionRequest::default()
                },
                100,
                16
            ),
            Err(CliError::BsmMaxInFlightBelowThreads {
                threads: 4,
                max_in_flight: 3
            })
        ));
    }

    #[test]
    fn interactive_bsm_commands_control_pause_resume_progress_and_cancel() {
        let pause = biogeo_core::StochasticMapPauseToken::new();
        let cancellation = biogeo_core::StochasticMapCancellationToken::new();
        let progress = BsmInteractiveProgress::new(100);
        progress.set_completed_samples(7);
        let mut output = Vec::new();
        run_bsm_interactive_commands(
            std::io::Cursor::new(
                b"pause\nstatus\nresume\nstatus\nunknown\nhelp\ncancel\n".as_slice(),
            ),
            &mut output,
            pause.clone(),
            cancellation.clone(),
            progress,
        )
        .unwrap();

        let output = String::from_utf8(output).unwrap();
        assert!(output.contains("BSM interactive control enabled"));
        assert!(output.contains("BSM status: pause requested; completed_samples=7/100"));
        assert!(output.contains("BSM status: resumed; completed_samples=7/100"));
        assert!(output.contains("BSM status: running; completed_samples=7/100"));
        assert!(output.contains("BSM status: unknown command; enter help"));
        assert!(output.contains("BSM status: commands: pause, resume, status, cancel"));
        assert!(output.contains("BSM status: cancellation requested"));
        assert!(!pause.is_paused());
        assert!(cancellation.is_cancelled());

        let eof_pause = biogeo_core::StochasticMapPauseToken::new();
        let eof_cancellation = biogeo_core::StochasticMapCancellationToken::new();
        let mut eof_output = Vec::new();
        run_bsm_interactive_commands(
            std::io::Cursor::new(b"pause\n".as_slice()),
            &mut eof_output,
            eof_pause.clone(),
            eof_cancellation.clone(),
            BsmInteractiveProgress::new(10),
        )
        .unwrap();
        assert!(!eof_pause.is_paused());
        assert!(!eof_cancellation.is_cancelled());
        assert!(
            String::from_utf8(eof_output)
                .unwrap()
                .contains("standard input closed; resumed")
        );
    }

    #[test]
    fn rejects_invalid_or_inactive_bsm_execution_options() {
        let base = vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
        ];

        for (option, value) in [
            ("--bsm-threads", "0"),
            ("--bsm-max-in-flight", "0"),
            ("--bsm-memory-budget-mb", "0"),
            ("--bsm-shard-samples", "0"),
            ("--bsm-checkpoint-samples", "0"),
        ] {
            let mut args = base.clone();
            args.extend([option.to_string(), value.to_string()]);
            assert!(matches!(
                parse_command(args),
                Err(CliError::NonPositiveBsmOption(found)) if found == option
            ));
        }

        let mut invalid = base.clone();
        invalid.extend(["--bsm-threads".to_string(), "many".to_string()]);
        assert!(matches!(
            parse_command(invalid),
            Err(CliError::InvalidBsmThreads(value)) if value == "many"
        ));

        let mut inactive = base.clone();
        inactive.extend(["--bsm-threads".to_string(), "auto".to_string()]);
        assert!(matches!(
            parse_command(inactive),
            Err(CliError::BsmExecutionRequiresSamples)
        ));

        let mut inactive_interactive = base.clone();
        inactive_interactive.push("--bsm-interactive".to_string());
        assert!(matches!(
            parse_command(inactive_interactive),
            Err(CliError::BsmExecutionRequiresSamples)
        ));

        for option in ["--bsm-max-events-per-sample", "--bsm-max-events-total"] {
            let mut inactive_budget = base.clone();
            inactive_budget.extend([option.to_string(), "0".to_string()]);
            assert!(matches!(
                parse_command(inactive_budget),
                Err(CliError::BsmExecutionRequiresSamples)
            ));
        }

        for value in ["-1", "NaN", "inf"] {
            let mut args = base.clone();
            args.extend([
                "--bsm-samples".to_string(),
                "1".to_string(),
                "--bsm-time-limit-seconds".to_string(),
                value.to_string(),
            ]);
            assert!(matches!(
                parse_command(args),
                Err(CliError::InvalidBsmTimeLimit {
                    option: "--bsm-time-limit-seconds",
                    ..
                })
            ));
        }

        let mut inactive_time_limit = base.clone();
        inactive_time_limit.extend(["--bsm-time-limit-seconds".to_string(), "1".to_string()]);
        assert!(matches!(
            parse_command(inactive_time_limit),
            Err(CliError::BsmExecutionRequiresSamples)
        ));

        for option in ["--bsm-checkpoint-samples", "--bsm-resume"] {
            let mut without_output = base.clone();
            without_output.extend(["--bsm-samples".to_string(), "1".to_string()]);
            without_output.push(option.to_string());
            if option == "--bsm-checkpoint-samples" {
                without_output.push("1".to_string());
            }
            assert!(matches!(
                parse_command(without_output),
                Err(CliError::BsmStreamOptionRequiresOutput(found)) if found == option
            ));
        }

        let mut memory_without_output = base.clone();
        memory_without_output.extend([
            "--bsm-samples".to_string(),
            "1".to_string(),
            "--bsm-max-events-per-sample".to_string(),
            "100".to_string(),
            "--bsm-memory-budget-mb".to_string(),
            "1".to_string(),
        ]);
        assert!(matches!(
            parse_command(memory_without_output),
            Err(CliError::BsmStreamOptionRequiresOutput(
                "--bsm-memory-budget-mb"
            ))
        ));

        let mut memory_without_event_limit = base.clone();
        memory_without_event_limit.extend([
            "--bsm-samples".to_string(),
            "1".to_string(),
            "--bsm-output-dir".to_string(),
            "bsm-output".to_string(),
            "--bsm-memory-budget-mb".to_string(),
            "1".to_string(),
        ]);
        assert!(matches!(
            parse_command(memory_without_event_limit),
            Err(CliError::BsmMemoryBudgetRequiresPerSampleEventLimit)
        ));

        let mut shard_without_output = base.clone();
        shard_without_output.extend([
            "--bsm-samples".to_string(),
            "1".to_string(),
            "--bsm-shard-samples".to_string(),
            "1".to_string(),
        ]);
        assert!(matches!(
            parse_command(shard_without_output),
            Err(CliError::BsmStreamOptionRequiresOutput(
                "--bsm-shard-samples"
            ))
        ));

        assert!(matches!(
            resolve_bsm_execution_with_available(
                BsmExecutionRequest {
                    thread_selection: BsmThreadSelection::Fixed(1),
                    max_in_flight: Some(1),
                    max_events_per_sample: Some(0),
                    memory_budget_mb: Some(usize::MAX),
                    ..BsmExecutionRequest::default()
                },
                1,
                1,
            ),
            Err(CliError::BsmMemoryBudgetOverflow {
                megabytes: usize::MAX
            })
        ));
    }

    #[test]
    fn parses_divalike_command_with_preset_range_size_defaults() {
        let command = parse_command(vec![
            "divalike".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::Fixed(FixedModelConfig {
                preset: FixedPreset::DivaLike,
                tree_path: PathBuf::from("tree.nwk"),
                tree_name: None,
                ranges_path: PathBuf::from("ranges.tsv"),
                use_ambiguities: false,
                d: 0.1,
                e: 0.2,
                j: 0.0,
                range_size: biogeo_core::CladogenesisRangeSizeConfig {
                    mx01y: 0.0001,
                    mx01s: 0.0001,
                    mx01v: 0.5,
                    mx01j: 0.0001,
                },
                min_branch_length: 0.0,
                max_range_size: None,
                dispersal_multipliers_path: None,
                dispersal_strata_path: None,
                distance_matrix_path: None,
                distance_exponent: None,
                environment_distance_matrix_path: None,
                environment_distance_exponent: None,
                extirpation_multipliers_path: None,
                area_sizes_path: None,
                area_exponent: None,
                include_null_range: false,
                root_prior: RootPriorKind::Flat,
                ancestral_probs: false,
                split_probs: false,
                traceback_samples: 0,
                bsm_samples: 0,
                bsm_output_dir_path: None,
                bsm_output_level: BsmOutputLevel::Legacy,
                bsm_threads: BsmThreadSelection::Auto,
                bsm_max_in_flight: None,
                bsm_max_events_per_sample: None,
                bsm_max_events_total: None,
                bsm_memory_budget_mb: None,
                bsm_shard_samples: None,
                bsm_checkpoint_samples: None,
                bsm_resume: false,
                bsm_time_limit: None,
                bsm_interactive: false,
                seed: 1,
            })
        );
    }

    #[test]
    fn parses_bayarealike_command_with_preset_range_size_defaults() {
        let command = parse_command(vec![
            "bayarealike".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
        ])
        .unwrap();

        let Command::Fixed(config) = command else {
            panic!("expected a fixed model command");
        };
        assert_eq!(config.preset, FixedPreset::BayAreaLike);
        assert_eq!(config.range_size.mx01y, 0.9999);
        assert_eq!(config.range_size.mx01s, 0.0001);
        assert_eq!(config.range_size.mx01v, 0.0001);
        assert_eq!(config.range_size.mx01j, 0.0001);
    }

    #[test]
    fn parses_plus_j_fixed_commands_with_preset_semantics() {
        let divalike = parse_command(vec![
            "divalike".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "0.4".to_string(),
        ])
        .unwrap();
        let Command::Fixed(divalike) = divalike else {
            panic!("expected a fixed DIVALIKE+J command");
        };
        let divalike_model = divalike
            .preset
            .build_model(divalike.d, divalike.e, divalike.j)
            .unwrap();

        let bayarealike = parse_command(vec![
            "bayarealike".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "0.4".to_string(),
        ])
        .unwrap();
        let Command::Fixed(bayarealike) = bayarealike else {
            panic!("expected a fixed BAYAREALIKE+J command");
        };
        let bayarealike_model = bayarealike
            .preset
            .build_model(bayarealike.d, bayarealike.e, bayarealike.j)
            .unwrap();

        assert_eq!(divalike.preset.model_name(divalike.j), "DIVALIKE+J");
        assert_eq!(divalike.range_size.mx01v, 0.5);
        assert_eq!(
            divalike_model.cladogenesis.event_weights,
            biogeo_core::CladogeneticEventWeights {
                sympatry: 0.8,
                subset_sympatry: 0.0,
                vicariance: 0.8,
                founder_event: 0.4,
            }
        );
        assert_eq!(
            bayarealike.preset.model_name(bayarealike.j),
            "BAYAREALIKE+J"
        );
        assert_eq!(bayarealike.range_size.mx01y, 0.9999);
        assert_eq!(
            bayarealike_model.cladogenesis.event_weights,
            biogeo_core::CladogeneticEventWeights {
                sympatry: 0.6,
                subset_sympatry: 0.0,
                vicariance: 0.0,
                founder_event: 0.4,
            }
        );
    }

    #[test]
    fn parses_anagenetic_dispersal_multiplier_path() {
        let command = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-multipliers".to_string(),
            "multipliers.tsv".to_string(),
        ])
        .unwrap();

        let Command::Fixed(config) = command else {
            panic!("expected a fixed model command");
        };
        assert_eq!(
            config.dispersal_multipliers_path,
            Some(PathBuf::from("multipliers.tsv"))
        );
    }

    #[test]
    fn accepts_pairwise_multipliers_with_founder_event() {
        let command = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "0.5".to_string(),
            "--dispersal-multipliers".to_string(),
            "multipliers.tsv".to_string(),
        ])
        .unwrap();

        let Command::Fixed(config) = command else {
            panic!("expected fixed DEC+J command");
        };
        assert_eq!(config.j, 0.5);
        assert_eq!(
            config.dispersal_multipliers_path,
            Some(PathBuf::from("multipliers.tsv"))
        );
    }

    #[test]
    fn requires_distance_matrix_and_exponent_together() {
        let error = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--distance-matrix".to_string(),
            "distances.tsv".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::IncompleteDistanceModifier));
    }

    #[test]
    fn requires_environment_distance_matrix_and_exponent_together() {
        let error = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--environment-distance-matrix".to_string(),
            "environment.tsv".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            CliError::IncompleteEnvironmentDistanceModifier
        ));
    }

    #[test]
    fn parses_dec_x_n_and_u_optimization_commands_with_biogeobears_bounds() {
        let x_command = parse_command(vec![
            "dec-x-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--distance-matrix".to_string(),
            "distances.tsv".to_string(),
        ])
        .unwrap();
        let Command::ExponentOptimize(x_config) = x_command else {
            panic!("expected exponent optimization command");
        };
        assert_eq!(x_config.kind, ExponentKind::GeographicX);
        assert_eq!(
            x_config.distance_matrix_path,
            Some(PathBuf::from("distances.tsv"))
        );
        assert_eq!(x_config.optimization.min_exponent, -2.5);
        assert_eq!(x_config.optimization.max_exponent, 2.5);
        assert_eq!(x_config.optimization.de.multi_start_points_per_axis, 2);

        let n_command = parse_command(vec![
            "dec-n-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--environment-distance-matrix".to_string(),
            "environment.tsv".to_string(),
        ])
        .unwrap();
        let Command::ExponentOptimize(n_config) = n_command else {
            panic!("expected exponent optimization command");
        };
        assert_eq!(n_config.kind, ExponentKind::EnvironmentN);
        assert_eq!(n_config.optimization.min_exponent, -10.0);
        assert_eq!(n_config.optimization.max_exponent, 10.0);

        let u_command = parse_command(vec![
            "dec-u-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--area-sizes".to_string(),
            "areas.tsv".to_string(),
            "--dispersal-strata".to_string(),
            "strata.tsv".to_string(),
        ])
        .unwrap();
        let Command::ExponentOptimize(u_config) = u_command else {
            panic!("expected exponent optimization command");
        };
        assert_eq!(u_config.kind, ExponentKind::AreaSizeU);
        assert_eq!(u_config.area_sizes_path, Some(PathBuf::from("areas.tsv")));
        assert_eq!(
            u_config.dispersal_strata_path,
            Some(PathBuf::from("strata.tsv"))
        );
        assert_eq!(u_config.optimization.min_exponent, -10.0);
        assert_eq!(u_config.optimization.max_exponent, 10.0);
    }

    #[test]
    fn parses_joint_xnu_optimization_command() {
        let command = parse_command(vec![
            "dec-xnu-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--distance-matrix".to_string(),
            "distance.tsv".to_string(),
            "--environment-distance-matrix".to_string(),
            "environment.tsv".to_string(),
            "--area-sizes".to_string(),
            "areas.tsv".to_string(),
            "--init-x".to_string(),
            "-0.5".to_string(),
            "--min-n".to_string(),
            "-4".to_string(),
            "--max-n".to_string(),
            "4".to_string(),
            "--multi-start-points".to_string(),
            "2".to_string(),
        ])
        .unwrap();
        let Command::XnuOptimize(config) = command else {
            panic!("expected joint x/n/u optimization command");
        };

        assert_eq!(config.optimization.initial_x, -0.5);
        assert_eq!(config.optimization.min_x, -2.5);
        assert_eq!(config.optimization.max_x, 2.5);
        assert_eq!(config.optimization.min_n, -4.0);
        assert_eq!(config.optimization.max_n, 4.0);
        assert_eq!(config.optimization.min_u, -10.0);
        assert_eq!(config.optimization.max_u, 10.0);
        assert_eq!(config.optimization.de.multi_start_points_per_axis, 2);
    }

    #[test]
    fn rejects_fixed_exponent_in_joint_xnu_optimization() {
        let error = parse_command(vec![
            "dec-xnu-optimize".to_string(),
            "--distance-exponent".to_string(),
            "-1".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            CliError::OptimizedExponentAlsoFixed { parameter: "x" }
        ));
    }

    #[test]
    fn parses_pair_profile_grid_and_fixed_exponent() {
        let command = parse_command(vec![
            "dec-xn-profile".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--distance-matrix".to_string(),
            "distance.tsv".to_string(),
            "--environment-distance-matrix".to_string(),
            "environment.tsv".to_string(),
            "--area-sizes".to_string(),
            "areas.tsv".to_string(),
            "--area-exponent".to_string(),
            "0.5".to_string(),
            "--x-min".to_string(),
            "-2".to_string(),
            "--x-max".to_string(),
            "0".to_string(),
            "--x-points".to_string(),
            "3".to_string(),
            "--n-min".to_string(),
            "-1".to_string(),
            "--n-max".to_string(),
            "1".to_string(),
            "--n-points".to_string(),
            "5".to_string(),
        ])
        .unwrap();
        let Command::PairProfile(config) = command else {
            panic!("expected pair-profile command");
        };

        assert_eq!(config.pair, ProfilePair::Xn);
        assert_eq!(config.fixed_exponent, 0.5);
        assert_eq!(config.profile.first.parameter, "x");
        assert_eq!(config.profile.first.values, vec![-2.0, -1.0, 0.0]);
        assert_eq!(config.profile.second.parameter, "n");
        assert_eq!(config.profile.second.values.len(), 5);
        assert_eq!(config.profile.second.values[2], 0.0);
        assert_eq!(config.profile.de.multi_start_points_per_axis, 2);
        assert_eq!(
            config.profile.support_delta,
            biogeo_core::PROFILE_95_SUPPORT_DELTA
        );
    }

    #[test]
    fn rejects_profile_grid_for_the_fixed_exponent() {
        let error = parse_command(vec![
            "dec-xu-profile".to_string(),
            "--n-min".to_string(),
            "-1".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            CliError::ProfileGridForFixedExponent { parameter: "n" }
        ));
    }

    #[test]
    fn validates_fixed_and_free_area_size_options() {
        let incomplete = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--area-sizes".to_string(),
            "areas.tsv".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(incomplete, CliError::IncompleteAreaSizeModifier));

        let conflict = parse_command(vec![
            "dec-u-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--area-sizes".to_string(),
            "areas.tsv".to_string(),
            "--extirpation-multipliers".to_string(),
            "multipliers.tsv".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            conflict,
            CliError::ConflictingExtirpationModifiers
        ));

        let fixed_u = parse_command(vec![
            "dec-u-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--area-sizes".to_string(),
            "areas.tsv".to_string(),
            "--area-exponent".to_string(),
            "-1".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            fixed_u,
            CliError::OptimizedExponentAlsoFixed { parameter: "u" }
        ));
    }

    #[test]
    fn rejects_fixed_but_accepts_stratified_input_for_the_optimized_exponent() {
        let fixed_error = parse_command(vec![
            "dec-x-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--distance-matrix".to_string(),
            "distances.tsv".to_string(),
            "--distance-exponent".to_string(),
            "-1".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            fixed_error,
            CliError::OptimizedExponentAlsoFixed { parameter: "x" }
        ));

        let stratified = parse_command(vec![
            "dec-n-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--environment-distance-matrix".to_string(),
            "environment.tsv".to_string(),
            "--dispersal-strata".to_string(),
            "strata.tsv".to_string(),
        ])
        .unwrap();
        let Command::ExponentOptimize(config) = stratified else {
            panic!("expected exponent optimization command");
        };
        assert_eq!(
            config.dispersal_strata_path,
            Some(PathBuf::from("strata.tsv"))
        );
    }

    #[test]
    fn accepts_distance_modifier_with_founder_event() {
        let command = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "0.1".to_string(),
            "--distance-matrix".to_string(),
            "distances.tsv".to_string(),
            "--distance-exponent".to_string(),
            "-1".to_string(),
        ])
        .unwrap();

        let Command::Fixed(config) = command else {
            panic!("expected fixed DEC+J command");
        };
        assert_eq!(
            config.distance_matrix_path,
            Some(PathBuf::from("distances.tsv"))
        );
        assert_eq!(config.distance_exponent, Some(-1.0));
    }

    #[test]
    fn accepts_environment_distance_modifier_with_founder_event() {
        let command = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "0.1".to_string(),
            "--environment-distance-matrix".to_string(),
            "environment.tsv".to_string(),
            "--environment-distance-exponent".to_string(),
            "0.5".to_string(),
        ])
        .unwrap();

        let Command::Fixed(config) = command else {
            panic!("expected fixed DEC+J command");
        };
        assert_eq!(
            config.environment_distance_matrix_path,
            Some(PathBuf::from("environment.tsv"))
        );
        assert_eq!(config.environment_distance_exponent, Some(0.5));
    }

    #[test]
    fn rejects_static_and_stratified_dispersal_together() {
        let error = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-multipliers".to_string(),
            "matrix.tsv".to_string(),
            "--dispersal-strata".to_string(),
            "strata.tsv".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::ConflictingDispersalModifiers));
    }

    #[test]
    fn parses_dec_optimize_command() {
        let command = parse_command(vec![
            "dec-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--max-range-size".to_string(),
            "2".to_string(),
            "--include-null-range".to_string(),
            "--root-prior".to_string(),
            "equal".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
            "--mx01".to_string(),
            "0.5".to_string(),
            "--mx01v".to_string(),
            "0.25".to_string(),
            "--init-d".to_string(),
            "0.02".to_string(),
            "--init-e".to_string(),
            "0.03".to_string(),
            "--min-rate".to_string(),
            "1e-9".to_string(),
            "--max-rate".to_string(),
            "5".to_string(),
            "--initial-log-step".to_string(),
            "0.25".to_string(),
            "--tolerance".to_string(),
            "1e-7".to_string(),
            "--max-iterations".to_string(),
            "50".to_string(),
            "--multi-start-points".to_string(),
            "3".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::DeOptimize(DeOptimizeConfig {
                preset: FixedPreset::Dec,
                tree_path: PathBuf::from("tree.nwk"),
                tree_name: None,
                ranges_path: PathBuf::from("ranges.tsv"),
                use_ambiguities: false,
                min_branch_length: 0.0,
                max_range_size: Some(2),
                dispersal_multipliers_path: None,
                dispersal_strata_path: None,
                distance_matrix_path: None,
                distance_exponent: None,
                environment_distance_matrix_path: None,
                environment_distance_exponent: None,
                extirpation_multipliers_path: None,
                area_sizes_path: None,
                area_exponent: None,
                include_null_range: true,
                root_prior: RootPriorKind::Equal,
                ancestral_probs: true,
                split_probs: true,
                optimization: biogeo_core::DecOptimizationConfig {
                    initial_d: 0.02,
                    initial_e: 0.03,
                    min_rate: 1e-9,
                    max_rate: 5.0,
                    initial_log_step: 0.25,
                    tolerance: 1e-7,
                    max_iterations: 50,
                    multi_start_points_per_axis: 3,
                    range_size: biogeo_core::CladogenesisRangeSizeConfig {
                        mx01y: 0.5,
                        mx01s: 0.5,
                        mx01v: 0.25,
                        mx01j: 0.5,
                    },
                },
            })
        );
    }

    #[test]
    fn parses_divalike_optimize_command_with_preset_defaults() {
        let command = parse_command(vec![
            "divalike-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
        ])
        .unwrap();

        let Command::DeOptimize(config) = command else {
            panic!("expected a two-rate optimization command");
        };
        assert_eq!(config.preset, FixedPreset::DivaLike);
        assert_eq!(
            config.optimization,
            biogeo_core::DecOptimizationConfig::for_divalike()
        );
        assert_eq!(config.optimization.range_size.mx01v, 0.5);
    }

    #[test]
    fn parses_bayarealike_optimize_command_with_preset_defaults() {
        let command = parse_command(vec![
            "bayarealike-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
        ])
        .unwrap();

        let Command::DeOptimize(config) = command else {
            panic!("expected a two-rate optimization command");
        };
        assert_eq!(config.preset, FixedPreset::BayAreaLike);
        assert_eq!(
            config.optimization,
            biogeo_core::DecOptimizationConfig::for_bayarealike()
        );
        assert_eq!(config.optimization.range_size.mx01y, 0.9999);
    }

    #[test]
    fn parses_decj_optimize_command() {
        let command = parse_command(vec![
            "decj-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--max-range-size".to_string(),
            "2".to_string(),
            "--include-null-range".to_string(),
            "--root-prior".to_string(),
            "equal".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
            "--mx01".to_string(),
            "0.6".to_string(),
            "--mx01j".to_string(),
            "0.8".to_string(),
            "--init-d".to_string(),
            "0.02".to_string(),
            "--init-e".to_string(),
            "0.03".to_string(),
            "--init-j".to_string(),
            "0.04".to_string(),
            "--min-rate".to_string(),
            "1e-9".to_string(),
            "--max-rate".to_string(),
            "5".to_string(),
            "--min-j".to_string(),
            "1e-4".to_string(),
            "--max-j".to_string(),
            "2.5".to_string(),
            "--initial-log-step".to_string(),
            "0.25".to_string(),
            "--tolerance".to_string(),
            "1e-7".to_string(),
            "--max-iterations".to_string(),
            "50".to_string(),
            "--multi-start-points".to_string(),
            "3".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            Command::DecJOptimize(DecJOptimizeConfig {
                preset: FixedPreset::Dec,
                tree_path: PathBuf::from("tree.nwk"),
                tree_name: None,
                ranges_path: PathBuf::from("ranges.tsv"),
                use_ambiguities: false,
                min_branch_length: 0.0,
                max_range_size: Some(2),
                dispersal_multipliers_path: None,
                dispersal_strata_path: None,
                distance_matrix_path: None,
                distance_exponent: None,
                environment_distance_matrix_path: None,
                environment_distance_exponent: None,
                extirpation_multipliers_path: None,
                area_sizes_path: None,
                area_exponent: None,
                include_null_range: true,
                root_prior: RootPriorKind::Equal,
                ancestral_probs: true,
                split_probs: true,
                optimization: biogeo_core::DecJOptimizationConfig {
                    initial_d: 0.02,
                    initial_e: 0.03,
                    initial_j: 0.04,
                    min_rate: 1e-9,
                    max_rate: 5.0,
                    min_j: 1e-4,
                    max_j: 2.5,
                    initial_log_step: 0.25,
                    tolerance: 1e-7,
                    max_iterations: 50,
                    multi_start_points_per_axis: 3,
                    range_size: biogeo_core::CladogenesisRangeSizeConfig {
                        mx01y: 0.6,
                        mx01s: 0.6,
                        mx01v: 0.6,
                        mx01j: 0.8,
                    },
                },
            })
        );
    }

    #[test]
    fn parses_other_plus_j_optimizers_with_biogeobears_defaults() {
        let divalike = parse_command(vec![
            "divalikej-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
        ])
        .unwrap();
        let Command::DecJOptimize(divalike) = divalike else {
            panic!("expected a DIVALIKE+J optimization command");
        };

        let bayarealike = parse_command(vec![
            "bayarealikej-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
        ])
        .unwrap();
        let Command::DecJOptimize(bayarealike) = bayarealike else {
            panic!("expected a BAYAREALIKE+J optimization command");
        };

        assert_eq!(divalike.preset, FixedPreset::DivaLike);
        assert_eq!(divalike.optimization.max_j, 1.99999);
        assert_eq!(divalike.optimization.range_size.mx01v, 0.5);
        assert_eq!(bayarealike.preset, FixedPreset::BayAreaLike);
        assert_eq!(bayarealike.optimization.max_j, 0.99999);
        assert_eq!(bayarealike.optimization.range_size.mx01y, 0.9999);
    }

    #[test]
    fn rejects_plus_j_optimization_bounds_above_the_preset_weight_sum() {
        let error = parse_command(vec![
            "bayarealikej-optimize".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--max-j".to_string(),
            "1.0".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            CliError::InvalidPresetJUpperBound {
                preset: "BAYAREALIKE+J",
                max_j: 1.0,
                upper_exclusive: 1.0,
            }
        ));
    }

    #[test]
    fn rejects_missing_required_dec_arg() {
        let error = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::MissingRequired("--ranges")));
    }

    #[test]
    fn runs_dec_from_files() {
        let temp = TempInputs::new();

        let output = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDEC\n"));
        assert!(output.contains("lnL\t-1.791759469228055\n"));
        assert!(output.contains("states\t3\n"));
        assert!(output.contains("root_prior\tflat\n"));
        assert!(output.contains("mx01y\t0.0001\n"));
        assert!(output.contains("ancestral_state_probabilities\n"));
        assert!(
            output.contains(
                "node\tlabel\tkind\tclade\tstate_index\trange_bits\trange\tprobability\n"
            )
        );
        assert!(output.contains("2\tnode_2\troot\tA+B\t2\t3\tAreaA+AreaB\t1.000000000000000\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
        assert!(output.contains("node\tlabel\tkind\tclade\tleft_clade\tright_clade\tancestor_state_index\tancestor_range_bits\tancestor_range\tleft_state_index\tleft_range_bits\tleft_range\tright_state_index\tright_range_bits\tright_range\tscenario_weight\tprobability\n"));
        assert!(output.contains("2\tnode_2\troot\tA+B\tA\tB\t2\t3\tAreaA+AreaB\t0\t1\tAreaA\t1\t2\tAreaB\t0.166666666666667\t1.000000000000000\n"));
    }

    #[test]
    fn validates_official_style_nexus_tree_ranges_and_fossil_tip_age() {
        let temp = TempInputs::new_with_contents(
            "#NEXUS\nBEGIN TAXA; DIMENSIONS NTAX=3; TAXLABELS human chimp gorilla; END;\n\
             BEGIN TREES; TRANSLATE 1 human, 2 chimp, 3 gorilla;\n\
             TREE * UNTITLED = [&R] ((1:0.91,2:1):1,3:2); END;\n",
            "tip\tA\tB\tC\nhuman\t0\t0\t1\nchimp\t0\t0\t1\ngorilla\t1\t1\t0\n",
        );

        let output = run(vec![
            "validate-inputs".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--min-branch-length".to_string(),
            "0.000001".to_string(),
        ])
        .unwrap();

        assert!(output.starts_with("format\tbiogeo-input-validation-v1\nstatus\tvalid\n"));
        assert_eq!(output_field(&output, "tree_input_format"), "nexus");
        assert_eq!(output_field(&output, "tree_name"), "UNTITLED");
        assert_eq!(output_field(&output, "tips"), "3");
        assert_eq!(output_field(&output, "binary"), "true");
        assert_eq!(output_field(&output, "ultrametric"), "false");
        assert_eq!(output_field(&output, "ancient_tips"), "1");
        assert_eq!(output_field(&output, "areas"), "3");
        assert_eq!(output_field(&output, "maximum_observed_range_size"), "2");
        assert_eq!(output_field(&output, "direct_ancestor_nodes"), "0");
        assert!(output.contains("ancient_tip_ages\nnode\tlabel\tage\n0\thuman\t"));
        let human_age = output
            .lines()
            .find(|line| line.starts_with("0\thuman\t"))
            .unwrap()
            .split('\t')
            .nth(2)
            .unwrap()
            .parse::<f64>()
            .unwrap();
        assert!((human_age - 0.09).abs() < 1e-12);
    }

    #[test]
    fn accepts_lagrange_data_directly_and_ignores_only_rounding_scale_tip_ages() {
        let temp = TempInputs::new_with_contents(
            "((A:0.5,B:0.5000000005):0.5,C:1.0000000005);\n",
            "3 2 (West East)\nA 10\nB 01\nC 11\n",
        );

        let base_args = vec![
            "validate-inputs".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
        ];
        let output = run(base_args.clone()).unwrap();
        assert_eq!(output_field(&output, "ultrametric"), "true");
        assert_eq!(output_field(&output, "ancient_tips"), "0");
        assert_eq!(output_field(&output, "tip_age_tolerance_mode"), "auto");
        assert_eq!(output_field(&output, "areas"), "2");

        let mut strict_args = base_args;
        strict_args.extend(["--tip-age-tolerance".to_string(), "0".to_string()]);
        let strict = run(strict_args).unwrap();
        assert_eq!(output_field(&strict, "ultrametric"), "false");
        assert_eq!(output_field(&strict, "ancient_tips"), "1");
        assert_eq!(output_field(&strict, "tip_age_tolerance_mode"), "explicit");
    }

    #[test]
    fn converts_rasp_csv_range_matrix() {
        let temp = TempInputs::new_with_ranges("ID,Name,West,East\n1,Taxon_A,1,0\n2,Taxon_B,0,1\n");
        let output = run(vec![
            "convert-ranges".to_string(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(output.starts_with("# format\tbiogeo-range-table-v1\n"));
        assert!(output.contains("tip\tWest\tEast\nTaxon_A\t1\t0\nTaxon_B\t0\t1\n"));
    }

    #[test]
    fn converts_ranges_with_an_explicit_taxon_map() {
        let temp = TempInputs::new_with_ranges(
            "Name,West,East\nNewGenus_bucki_EX2455_DZUP549431,1,0\nOther,0,1\n",
        );
        let map = temp.dir.join("taxon-map.tsv");
        fs::write(
            &map,
            "source_taxon\ttarget_taxon\nNewGenus_bucki_EX2455_DZUP549431\tNeoponera_bucki\n",
        )
        .unwrap();
        let output = run(vec![
            "convert-ranges".to_string(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--taxon-map".to_string(),
            map.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(output.contains("# taxon_map_applied\t1\n"));
        assert!(output.contains("Neoponera_bucki\t1\t0\n"));
    }

    #[test]
    fn converts_biogeobears_block_strata_to_engine_inputs() {
        let temp = TempInputs::new();
        let boundaries = temp.dir.join("boundaries.txt");
        let dispersal = temp.dir.join("dispersal.txt");
        let adjacency = temp.dir.join("adjacency.txt");
        let output_dir = temp.dir.join("converted-strata");
        fs::write(&boundaries, "1\n2\n").unwrap();
        fs::write(&dispersal, "A B\n1 0.5\n0.25 1\n\nA B\n1 0.1\n0.2 1\n").unwrap();
        fs::write(&adjacency, "A B\n1 1\n1 1\n\nA B\n1 0\n0 1\n").unwrap();

        let summary = run(vec![
            "convert-biogeobears-strata".to_string(),
            "--time-boundaries".to_string(),
            boundaries.to_string_lossy().into_owned(),
            "--dispersal-matrices".to_string(),
            dispersal.to_string_lossy().into_owned(),
            "--adjacency-matrices".to_string(),
            adjacency.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(summary.contains("format\tbiogeo-biogeobears-strata-import-v1\n"));
        let strata = fs::read_to_string(output_dir.join("strata.tsv")).unwrap();
        let specs = biogeo_core::parse_anagenetic_strata_table(&strata).unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[1].oldest_age, 2.0);
        assert_eq!(
            specs[1].areas_adjacency_path.as_deref(),
            Some("adjacency-002.tsv")
        );
        let matrix = fs::read_to_string(output_dir.join("dispersal-001.tsv")).unwrap();
        let parsed = biogeo_core::parse_dispersal_multipliers_table(
            &matrix,
            &["A".to_string(), "B".to_string()],
        )
        .unwrap();
        assert_eq!(parsed.get(0, 1), 0.5);
    }

    #[test]
    fn converts_edge_covered_adjacency_without_dispersal_multipliers() {
        let temp = TempInputs::new();
        let boundaries = temp.dir.join("boundaries.txt");
        let adjacency = temp.dir.join("adjacency.txt");
        let output_dir = temp.dir.join("converted-edge-covered");
        fs::write(&boundaries, "2\n").unwrap();
        fs::write(&adjacency, "A B C\n1 1 0\n1 1 1\n0 1 1\n").unwrap();

        let summary = run(vec![
            "convert-biogeobears-strata".to_string(),
            "--time-boundaries".to_string(),
            boundaries.to_string_lossy().into_owned(),
            "--adjacency-matrices".to_string(),
            adjacency.to_string_lossy().into_owned(),
            "--adjacency-range-rule".to_string(),
            "edge-covered".to_string(),
            "--max-range-size".to_string(),
            "3".to_string(),
            "--output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(summary.contains("has_dispersal\tfalse\n"));
        assert!(summary.contains("allowed_range_counts\t7\n"));
        let strata = fs::read_to_string(output_dir.join("strata.tsv")).unwrap();
        let specs = biogeo_core::parse_anagenetic_strata_table(&strata).unwrap();
        assert_eq!(specs[0].dispersal_matrix_path, None);
        assert_eq!(specs[0].areas_adjacency_path, None);
        assert_eq!(
            specs[0].allowed_ranges_path.as_deref(),
            Some("allowed-ranges-001.tsv")
        );
        let ranges = fs::read_to_string(output_dir.join("allowed-ranges-001.tsv")).unwrap();
        let allowed = biogeo_core::parse_allowed_range_states(
            &ranges,
            &["A".to_string(), "B".to_string(), "C".to_string()],
        )
        .unwrap();
        assert!(allowed.contains(biogeo_core::AreaSet::from_bits(0b111)));
        assert!(!allowed.contains(biogeo_core::AreaSet::from_bits(0b101)));
    }

    #[test]
    fn validation_reports_direct_ancestor_hooks_and_rejects_polytomies() {
        let direct_ancestor = TempInputs::new_with_contents(
            "((A:0.5,F:0.0000001):0.5,B:1);\n",
            "tip\tX\tY\nA\t1\t0\nF\t1\t0\nB\t0\t1\n",
        );
        let output = run(vec![
            "validate-inputs".to_string(),
            "--tree".to_string(),
            direct_ancestor.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            direct_ancestor.ranges_path.to_string_lossy().into_owned(),
            "--min-branch-length".to_string(),
            "0.000001".to_string(),
        ])
        .unwrap();
        assert_eq!(output_field(&output, "direct_ancestor_nodes"), "1");
        assert_eq!(output_field(&output, "direct_ancestor_hook_edges"), "1");

        let polytomy = TempInputs::new_with_contents(
            "(A:1,B:1,C:1);\n",
            "tip\tX\tY\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let error = run(vec![
            "validate-inputs".to_string(),
            "--tree".to_string(),
            polytomy.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            polytomy.ranges_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::NonBinaryInputTree { nodes } if nodes == vec![(3, 3)]
        ));
    }

    #[test]
    fn missing_branch_length_fill_is_explicit_portable_and_replayable() {
        let temp = TempInputs::new_with_contents(
            "('Taxon A','O''Brien');\n",
            "tip\tAreaA\tAreaB\nTaxon A\t1\t0\nO'Brien\t0\t1\n",
        );
        let strict_error = run(vec![
            "convert-tree".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            strict_error,
            CliError::TreeInput(biogeo_core::TreeInputError::Newick(
                biogeo_core::NewickError::RequiredBranchLengthMissing { .. }
            ))
        ));

        let filled = run(vec![
            "convert-tree".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--fill-missing-branch-length".to_string(),
            "0.25".to_string(),
        ])
        .unwrap();
        assert_eq!(filled, "('Taxon A':0.25,'O''Brien':0.25);\n");

        let nexus_path = temp.dir.join("quoted tree.nex");
        fs::write(
            &nexus_path,
            "\u{feff}#nExUs\nBEGIN TAXA; DIMENSIONS NTAX=2; END;\n\
             begin trees;\n\
             [producer[metadata]] translate 1 'Taxon A', 2 'O''Brien';\n\
             tree * 'analysis tree' = [&R] (1,2);\n\
             endblock;\n",
        )
        .unwrap();
        let nexus_filled = run(vec![
            "convert-tree".to_string(),
            "--tree".to_string(),
            nexus_path.to_string_lossy().into_owned(),
            "--tree-name".to_string(),
            "analysis tree".to_string(),
            "--fill-missing-branch-length".to_string(),
            "0.25".to_string(),
        ])
        .unwrap();
        assert_eq!(nexus_filled, filled);

        let validation = run(vec![
            "validate-inputs".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--fill-missing-branch-length".to_string(),
            "0.25".to_string(),
        ])
        .unwrap();
        assert_eq!(
            output_field(&validation, "missing_branch_length_fill"),
            "0.25000000000000000"
        );
        assert_eq!(
            output_field(&validation, "minimum_branch_length"),
            "0.25000000000000000"
        );

        let parameters_path = temp.dir.join("parameters.tsv");
        let parameters = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.2)
            .unwrap();
        fs::write(&parameters_path, parameters.to_versioned_tsv()).unwrap();
        let request_path = temp.dir.join("analysis.tsv");
        fs::write(
            &request_path,
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
        let plan = run(vec![
            "analysis-plan".to_string(),
            "--request".to_string(),
            request_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert_eq!(
            output_field(&plan, "missing_branch_length_fill"),
            "0.25000000000000000"
        );

        let result_dir = temp.dir.join("filled-result");
        run(vec![
            "analysis-run".to_string(),
            "--request".to_string(),
            request_path.to_string_lossy().into_owned(),
            "--output-dir".to_string(),
            result_dir.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let loaded = analysis_result::load_analysis_result(&result_dir).unwrap();
        assert_eq!(
            loaded.manifest.missing_branch_length_fill.map(f64::to_bits),
            Some(0.25_f64.to_bits())
        );
        fs::remove_file(&temp.tree_path).unwrap();
        fs::remove_file(&temp.ranges_path).unwrap();
        fs::remove_file(&parameters_path).unwrap();
        fs::remove_file(&request_path).unwrap();
        let replay = run(vec![
            "analysis-result-inspect".to_string(),
            "--analysis-result".to_string(),
            result_dir.to_string_lossy().into_owned(),
            "--replay".to_string(),
        ])
        .unwrap();
        assert_eq!(output_field(&replay, "replay_validation"), "passed");
    }

    #[test]
    fn nexus_translate_is_end_to_end_equivalent_to_newick() {
        let temp = TempInputs::new();
        let nexus_path = temp.dir.join("tree.nex");
        fs::write(
            &nexus_path,
            "#NEXUS\n\
             BEGIN TAXA; DIMENSIONS NTAX=2; TAXLABELS A B; END;\n\
             BEGIN TREES;\n\
               TRANSLATE 1 A, 2 B;\n\
               TREE analysis = [&R] (1:0,2:0);\n\
             END;\n",
        )
        .unwrap();

        let evaluate = |tree_path: &Path| {
            run(vec![
                "dec".to_string(),
                "--tree".to_string(),
                tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0.1".to_string(),
                "--e".to_string(),
                "0.2".to_string(),
                "--ancestral-probs".to_string(),
                "--split-probs".to_string(),
            ])
            .unwrap()
        };

        assert_eq!(evaluate(&nexus_path), evaluate(&temp.tree_path));
    }

    #[test]
    fn converts_and_analyzes_only_an_explicitly_named_nexus_tree() {
        let multi = TempInputs::new_with_contents(
            "#NEXUS\nBEGIN TREES;\n\
             TRANSLATE 1 A, 2 B, 3 C;\n\
             TREE first = [&R] ((1:0.2,2:0.2):0.2,3:0.4);\n\
             TREE selected = [&R] ((1:0.5,2:0.5):0.5,3:1);\nEND;\n",
            "tip\tX\tY\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );
        let expected = TempInputs::new_with_contents(
            "((A:0.5,B:0.5):0.5,C:1);\n",
            "tip\tX\tY\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
        );

        let converted = run(vec![
            "convert-tree".to_string(),
            "--tree".to_string(),
            multi.tree_path.to_string_lossy().into_owned(),
            "--tree-name".to_string(),
            "selected".to_string(),
        ])
        .unwrap();
        assert_eq!(converted, "((A:0.5,B:0.5):0.5,C:1);\n");

        let evaluate = |tree: &Path, tree_name: Option<&str>| {
            let mut args = vec![
                "dec".to_string(),
                "--tree".to_string(),
                tree.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                multi.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0.1".to_string(),
                "--e".to_string(),
                "0.05".to_string(),
                "--ancestral-probs".to_string(),
                "--split-probs".to_string(),
            ];
            if let Some(tree_name) = tree_name {
                args.extend(["--tree-name".to_string(), tree_name.to_string()]);
            }
            run(args)
        };
        let selected_output = evaluate(&multi.tree_path, Some("selected")).unwrap();
        assert!(selected_output.contains("tree_name\tselected\n"));
        assert_eq!(
            selected_output.replace("tree_name\tselected\n", ""),
            evaluate(&expected.tree_path, None).unwrap()
        );
        assert!(matches!(
            evaluate(&multi.tree_path, None),
            Err(CliError::TreeInput(biogeo_core::TreeInputError::Nexus(
                biogeo_core::NexusError::MultipleTrees { .. }
            )))
        ));

        let mut fingerprints = Vec::new();
        for tree_name in ["first", "selected"] {
            let output_dir = multi.dir.join(format!("bsm-{tree_name}"));
            run(vec![
                "dec".to_string(),
                "--tree".to_string(),
                multi.tree_path.to_string_lossy().into_owned(),
                "--tree-name".to_string(),
                tree_name.to_string(),
                "--ranges".to_string(),
                multi.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0.1".to_string(),
                "--e".to_string(),
                "0.05".to_string(),
                "--bsm-samples".to_string(),
                "1".to_string(),
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--bsm-threads".to_string(),
                "1".to_string(),
            ])
            .unwrap();
            let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
            fingerprints.push(output_field(&metadata, "run_fingerprint").to_string());
        }
        assert_ne!(fingerprints[0], fingerprints[1]);
    }

    #[test]
    fn direct_ancestor_cli_omits_the_hook_from_split_and_bsm_event_tables() {
        let temp = TempInputs::new_with_contents(
            "((A:0.5,F:0.0000001):0.5,B:1);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nF\t1\t0\nB\t0\t1\n",
        );

        let output = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "0".to_string(),
            "--min-branch-length".to_string(),
            "0.000001".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "17".to_string(),
            "--bsm-threads".to_string(),
            "1".to_string(),
        ])
        .unwrap();

        assert!(output.contains("min_branch_length\t0.00000100000000000\n"));
        assert!(output.contains("direct_ancestor_nodes\t1\n"));
        assert!(output.contains("direct_ancestor_hook_edges\t1\n"));
        assert!(output.contains("internal\tA+F\t0\t1\tAreaA\t1.000000000000000\n"));

        let split_probabilities = output
            .split_once("split_scenario_probabilities\n")
            .unwrap()
            .1
            .split_once("biogeographic_stochastic_maps\n")
            .unwrap()
            .0;
        assert!(
            !split_probabilities
                .lines()
                .skip(1)
                .any(|line| line.split('\t').nth(3) == Some("A+F"))
        );

        let sampled_splits = output
            .split_once("bsm_cladogenetic_splits\n")
            .unwrap()
            .1
            .split_once("bsm_branch_segments\n")
            .unwrap()
            .0
            .lines()
            .skip(1)
            .collect::<Vec<_>>();
        assert_eq!(sampled_splits.len(), 2);
        assert!(
            sampled_splits
                .iter()
                .all(|line| line.split('\t').nth(4) != Some("A+F"))
        );

        let event_counts = output
            .split_once("bsm_sample_event_counts\n")
            .unwrap()
            .1
            .split_once("bsm_sample_period_event_counts\n")
            .unwrap()
            .0
            .lines()
            .skip(1)
            .collect::<Vec<_>>();
        assert_eq!(event_counts.len(), 2);
        assert!(
            event_counts
                .iter()
                .all(|line| { line.split('\t').nth(4) == Some("1") })
        );
    }

    #[test]
    fn runs_reproducible_conditional_history_traceback_from_files() {
        let temp = TempInputs::new();
        let args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--traceback-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ];

        let output = run(args.clone()).unwrap();
        assert_eq!(output, run(args).unwrap());
        assert!(output.contains("conditional_history_skeletons\n"));
        assert!(output.contains("traceback_seed\t9\ntraceback_samples\t2\n"));
        assert!(output.contains("traceback_node_states\n"));
        assert!(output.contains("0\t2\tnode_2\troot\tA+B\t2\t3\tAreaA+AreaB\n"));
        assert!(output.contains("traceback_splits\n"));
        assert!(output.contains("0\t2\tnode_2\troot\tA+B\tA\tB\t2\t3\tAreaA+AreaB\t0\t1\tAreaA\t1\t2\tAreaB\t0.166666666666667\n"));
        assert!(output.contains("traceback_branch_endpoints\n"));
        assert!(
            output.contains("0\t0\t2\tA+B\t0\tA\t0.000000000000000\t0\t1\tAreaA\t0\t1\tAreaA\n")
        );
    }

    #[test]
    fn runs_reproducible_full_stochastic_maps_from_files() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ];

        let output = run(args.clone()).unwrap();
        assert_eq!(output, run(args).unwrap());
        assert!(output.contains("biogeographic_stochastic_maps\n"));
        assert!(output.contains("bsm_seed\t9\nbsm_samples\t2\n"));
        assert!(output.contains("bsm_max_events_per_sample\tunlimited\n"));
        assert!(output.contains("bsm_node_states\n"));
        assert!(output.contains("bsm_cladogenetic_splits\n"));
        assert!(output.contains("bsm_branch_segments\n"));
        assert!(output.contains("bsm_sample_event_counts\n"));
        assert!(output.contains("0\t1\t0\t1\t1\t1\t0\t0\t0\t2.000000000000000\n"));
        assert!(output.contains("bsm_sample_period_event_counts\n"));
        assert!(output.contains("0\t0\t1\t1.000000000000000\n"));
        assert!(output.contains("bsm_sample_state_occupancy\n"));
        assert!(output.contains("bsm_sample_period_state_occupancy\n"));
        assert!(output.contains("bsm_anagenetic_events\n"));

        let event_table = output
            .split_once("bsm_anagenetic_events\n")
            .expect("BSM event section should exist")
            .1;
        let event_rows = event_table.lines().skip(1).collect::<Vec<_>>();
        assert_eq!(event_rows.len(), 2);
        for (sample_index, row) in event_rows.iter().enumerate() {
            let fields = row.split('\t').collect::<Vec<_>>();
            assert_eq!(fields[0], sample_index.to_string());
            let event_time = fields[8].parse::<f64>().unwrap();
            assert!((0.0..=1.0).contains(&event_time));
            assert_eq!(fields[9], "local_extirpation");
            assert_eq!(fields[10], "e");
            assert_eq!(fields[11], "0");
            assert_eq!(fields[12], "AreaA");
            assert_eq!(fields[15], "AreaA");
            assert_eq!(fields[18], "null");
        }
    }

    #[test]
    fn streams_stochastic_histories_to_versioned_tables_without_overwrite() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-stream");
        let mut base_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ];
        let batch_output = run(base_args.clone()).unwrap();
        base_args.extend([
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
        ]);

        let streamed_output = run(base_args.clone()).unwrap();
        assert!(streamed_output.contains("biogeographic_stochastic_histories\n"));
        assert!(streamed_output.contains("bsm_format\tbiogeo-bsm-tsv-v1\n"));
        assert!(streamed_output.contains("bsm_seed\t9\nbsm_samples\t2\n"));
        assert!(!streamed_output.contains("bsm_node_states\n"));

        let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("format\tbiogeo-bsm-tsv-v1\n"));
        assert!(metadata.contains("status\tcomplete\n"));
        assert!(metadata.contains("completed_samples\t2\n"));
        assert!(metadata.contains("completed_anagenetic_events\t2\n"));
        assert!(metadata.contains("samples\t2\n"));
        assert!(metadata.contains("rng_protocol\tindexed-chacha12-v1\n"));
        let expected_threads = std::thread::available_parallelism()
            .map(|value| value.get())
            .unwrap_or(1)
            .min(2);
        assert!(metadata.contains(&format!("threads\t{expected_threads}\n")));
        assert!(metadata.contains("max_in_flight\t2\n"));
        assert!(metadata.contains("checkpoint_samples\t2\n"));
        assert!(metadata.contains("max_events_per_sample\tunlimited\n"));
        assert!(metadata.contains("max_events_total\tunlimited\n"));
        assert!(metadata.contains("memory_budget_mb\tunlimited\n"));
        assert!(metadata.contains("retained_bytes_per_sample_upper_bound\tnot_computed\n"));
        assert!(metadata.contains("buffered_history_bytes_upper_bound\tnot_computed\n"));
        assert!(metadata.contains("run_fingerprint\t"));
        let inspection = bsm_inspect::inspect(&output_dir, true).unwrap();
        assert_eq!(inspection.bsm_format, BSM_STREAM_FORMAT);
        assert_eq!(inspection.path_validation, "passed");

        let initial_checkpoint = parse_bsm_checkpoint(&checkpoint_path(&output_dir, 0)).unwrap();
        let final_checkpoint = parse_bsm_checkpoint(&checkpoint_path(&output_dir, 2)).unwrap();
        assert_eq!(initial_checkpoint.completed_samples, 0);
        assert_eq!(final_checkpoint.completed_samples, 2);
        assert_eq!(initial_checkpoint.completed_anagenetic_events, Some(0));
        assert_eq!(final_checkpoint.completed_anagenetic_events, Some(2));
        assert_eq!(
            initial_checkpoint.run_fingerprint,
            final_checkpoint.run_fingerprint
        );

        for (index, spec) in BSM_TABLE_SPECS.iter().enumerate() {
            let section_marker = format!("{}\n", spec.section);
            let section = batch_output
                .split_once(&section_marker)
                .expect("batch BSM section should exist")
                .1;
            let expected = if let Some(next) = BSM_TABLE_SPECS.get(index + 1) {
                section
                    .split_once(&format!("{}\n", next.section))
                    .expect("next batch BSM section should exist")
                    .0
            } else {
                section
            };
            let streamed = fs::read_to_string(output_dir.join(spec.file_name)).unwrap();
            assert_eq!(streamed, expected, "mismatch in {}", spec.file_name);
        }

        assert!(matches!(
            run(base_args),
            Err(CliError::BsmOutputDirectoryExists(path)) if path == output_dir
        ));
    }

    #[test]
    fn bsm_v2_output_levels_share_sampling_but_change_only_the_storage_layout() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let common = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.05".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-threads".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "20260811".to_string(),
        ];

        let mut event_counts = None;
        for (level, expected_format) in [
            ("full", BSM_FULL_STREAM_FORMAT_V2),
            ("compact", BSM_COMPACT_STREAM_FORMAT_V2),
            ("summary", BSM_SUMMARY_STREAM_FORMAT_V2),
        ] {
            let output_dir = temp.dir.join(format!("bsm-v2-{level}"));
            let mut args = common.clone();
            args.extend([
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--bsm-output-level".to_string(),
                level.to_string(),
            ]);
            let output = run(args).unwrap();
            assert!(output.contains(&format!("bsm_format\t{expected_format}\n")));
            assert!(output.contains(&format!("bsm_output_level\t{level}\n")));

            let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
            assert_eq!(output_field(&metadata, "format"), expected_format);
            assert_eq!(output_field(&metadata, "output_level"), level);
            assert_eq!(
                output_field(&metadata, "path_details"),
                if level == "summary" { "false" } else { "true" }
            );
            assert_eq!(
                output_field(&metadata, "sparse_occupancy"),
                if level == "full" { "false" } else { "true" }
            );
            for file_name in [
                "areas.tsv",
                "states.tsv",
                "nodes.tsv",
                "edges.tsv",
                "periods.tsv",
            ] {
                assert!(output_dir.join(file_name).is_file(), "{level}: {file_name}");
            }

            let events = fs::read_to_string(output_dir.join("sample_event_counts.tsv")).unwrap();
            assert!(events.starts_with(BSM_V2_SAMPLE_EVENT_COUNTS_HEADER));
            assert_eq!(events.lines().count(), 5);
            if let Some(expected) = &event_counts {
                assert_eq!(&events, expected, "{level} changed sampled summaries");
            } else {
                event_counts = Some(events);
            }

            let node_states = fs::read_to_string(output_dir.join("node_states.tsv")).unwrap();
            let branch_segments =
                fs::read_to_string(output_dir.join("branch_segments.tsv")).unwrap();
            let anagenetic_events =
                fs::read_to_string(output_dir.join("anagenetic_events.tsv")).unwrap();
            if level == "summary" {
                assert_eq!(node_states.lines().count(), 1);
                assert_eq!(branch_segments.lines().count(), 1);
                assert_eq!(anagenetic_events.lines().count(), 1);
            } else {
                assert!(node_states.lines().count() > 1);
                assert!(branch_segments.lines().count() > 1);
            }
            if level == "compact" {
                assert_eq!(node_states.lines().next(), Some(BSM_V2_COMPACT_HEADERS[0]));
                assert!(!branch_segments.contains("AreaA"));
                assert!(!anagenetic_events.contains("AreaA"));
            }
        }

        assert!(matches!(
            parse_command(vec![
                "model-bsm".to_string(),
                "--analysis-result".to_string(),
                "analysis".to_string(),
                "--bsm-samples".to_string(),
                "1".to_string(),
                "--bsm-output-level".to_string(),
                "summary".to_string(),
            ]),
            Err(CliError::BsmStreamOptionRequiresOutput(
                "--bsm-output-level"
            ))
        ));
        assert!(matches!(
            parse_bsm_output_level("everything".to_string()),
            Err(CliError::InvalidBsmOutputLevel(value)) if value == "everything"
        ));
    }

    #[test]
    fn bsm_inspect_parses_options_and_deep_scan_detects_same_length_corruption() {
        let command = parse_command(vec![
            "bsm-inspect".to_string(),
            "--bsm-result".to_string(),
            "result with spaces".to_string(),
            "--deep".to_string(),
        ])
        .unwrap();
        assert_eq!(
            command,
            Command::BsmInspect(BsmInspectConfig {
                bsm_result_dir_path: PathBuf::from("result with spaces"),
                deep: true,
            })
        );
        assert!(matches!(
            parse_command(vec!["bsm-inspect".to_string()]),
            Err(CliError::MissingRequired("--bsm-result"))
        ));

        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-inspection-corruption");
        run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-output-level".to_string(),
            "summary".to_string(),
        ])
        .unwrap();

        let quick = bsm_inspect::inspect(&output_dir, false).unwrap();
        assert_eq!(quick.completed_samples, 2);
        assert_eq!(quick.event_count_validation, "not_requested");
        assert_eq!(
            bsm_inspect::inspect(&output_dir, true)
                .unwrap()
                .event_count_validation,
            "passed"
        );

        let counts_path = output_dir.join("sample_event_counts.tsv");
        let original = fs::read_to_string(&counts_path).unwrap();
        let original_length = original.len();
        let mut lines = original.lines().map(str::to_string).collect::<Vec<_>>();
        let mut fields = lines[1].split('\t').map(str::to_string).collect::<Vec<_>>();
        assert_eq!(fields[2], "0");
        fields[2] = "1".to_string();
        lines[1] = fields.join("\t");
        let corrupted = format!("{}\n", lines.join("\n"));
        assert_eq!(corrupted.len(), original_length);
        fs::write(&counts_path, corrupted).unwrap();

        assert!(bsm_inspect::inspect(&output_dir, false).is_ok());
        let error = bsm_inspect::inspect(&output_dir, true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("anagenetic event components disagree")
        );
    }

    #[test]
    fn compact_bsm_v2_is_shard_invariant_and_resume_rejects_layout_changes() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let monolithic_dir = temp.dir.join("compact-monolithic");
        let sharded_dir = temp.dir.join("compact-sharded");
        let common = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.05".to_string(),
            "--bsm-samples".to_string(),
            "6".to_string(),
            "--bsm-threads".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "20260811".to_string(),
            "--bsm-output-level".to_string(),
            "compact".to_string(),
        ];

        let mut monolithic_args = common.clone();
        monolithic_args.extend([
            "--bsm-output-dir".to_string(),
            monolithic_dir.to_string_lossy().into_owned(),
        ]);
        run(monolithic_args).unwrap();

        let mut sharded_args = common.clone();
        sharded_args.extend([
            "--bsm-output-dir".to_string(),
            sharded_dir.to_string_lossy().into_owned(),
            "--bsm-shard-samples".to_string(),
            "2".to_string(),
        ]);
        let output = run(sharded_args.clone()).unwrap();
        assert!(output.contains(&format!(
            "bsm_format\t{BSM_COMPACT_SHARDED_STREAM_FORMAT_V2}\n"
        )));

        for spec in BSM_TABLE_SPECS {
            let mut combined = String::new();
            for shard_start in [0, 2, 4] {
                let range = BsmShardRange::for_start(shard_start, 2, 6).unwrap();
                let shard = fs::read_to_string(
                    sharded_dir
                        .join(BSM_SHARD_DIRECTORY)
                        .join(range.directory_name())
                        .join(spec.file_name),
                )
                .unwrap();
                if shard_start == 0 {
                    combined.push_str(&shard);
                } else {
                    combined.push_str(shard.split_once('\n').unwrap().1);
                }
            }
            assert_eq!(
                combined,
                fs::read_to_string(monolithic_dir.join(spec.file_name)).unwrap(),
                "sharding changed compact {}",
                spec.file_name
            );
        }
        for file_name in [
            "areas.tsv",
            "states.tsv",
            "nodes.tsv",
            "edges.tsv",
            "periods.tsv",
        ] {
            assert_eq!(
                fs::read(sharded_dir.join(file_name)).unwrap(),
                fs::read(monolithic_dir.join(file_name)).unwrap(),
                "sharding changed reference {file_name}"
            );
        }

        let mut resume_args = sharded_args.clone();
        resume_args.push("--bsm-resume".to_string());
        run(resume_args).unwrap();

        let mut incompatible = common;
        let level_index = incompatible
            .iter()
            .position(|value| value == "compact")
            .unwrap();
        incompatible[level_index] = "full".to_string();
        incompatible.extend([
            "--bsm-output-dir".to_string(),
            sharded_dir.to_string_lossy().into_owned(),
            "--bsm-shard-samples".to_string(),
            "2".to_string(),
            "--bsm-resume".to_string(),
        ]);
        assert!(matches!(
            run(incompatible),
            Err(CliError::BsmResumeFingerprintMismatch { .. })
        ));
    }

    #[test]
    fn memory_budget_reduces_the_history_window_without_changing_streamed_tables() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let baseline_dir = temp.dir.join("bsm-memory-baseline");
        let budgeted_dir = temp.dir.join("bsm-memory-budgeted");
        let common_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "8".to_string(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "4".to_string(),
            "--bsm-max-events-per-sample".to_string(),
            "10000".to_string(),
            "--seed".to_string(),
            "20260716".to_string(),
        ];

        let mut baseline_args = common_args.clone();
        baseline_args.extend([
            "--bsm-output-dir".to_string(),
            baseline_dir.to_string_lossy().into_owned(),
        ]);
        run(baseline_args).unwrap();

        let mut budgeted_args = common_args.clone();
        budgeted_args.extend([
            "--bsm-output-dir".to_string(),
            budgeted_dir.to_string_lossy().into_owned(),
            "--bsm-memory-budget-mb".to_string(),
            "1".to_string(),
        ]);
        let output = run(budgeted_args).unwrap();

        for spec in BSM_TABLE_SPECS {
            assert_eq!(
                fs::read(budgeted_dir.join(spec.file_name)).unwrap(),
                fs::read(baseline_dir.join(spec.file_name)).unwrap(),
                "memory planning changed {}",
                spec.file_name
            );
        }

        let metadata = fs::read_to_string(budgeted_dir.join("metadata.tsv")).unwrap();
        let values = metadata
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once('\t'))
            .collect::<std::collections::HashMap<_, _>>();
        let threads = values["threads"].parse::<usize>().unwrap();
        let max_in_flight = values["max_in_flight"].parse::<usize>().unwrap();
        let bytes_per_sample = values["retained_bytes_per_sample_upper_bound"]
            .parse::<usize>()
            .unwrap();
        let buffered_bytes = values["buffered_history_bytes_upper_bound"]
            .parse::<usize>()
            .unwrap();
        assert_eq!(values["memory_budget_mb"], "1");
        assert_eq!(values["memory_budget_scope"], "completed_history_window");
        assert!(threads < 4);
        assert_eq!(threads, max_in_flight);
        assert!(bytes_per_sample <= 1024 * 1024);
        assert!(buffered_bytes <= 1024 * 1024);
        assert_eq!(buffered_bytes, bytes_per_sample * max_in_flight);
        assert!(output.contains(&format!("bsm_threads\t{threads}\n")));
        assert!(output.contains(&format!("bsm_max_in_flight\t{max_in_flight}\n")));
        assert!(output.contains("bsm_memory_budget_mb\t1\n"));

        let too_small_dir = temp.dir.join("bsm-memory-too-small");
        let mut too_small_args = common_args;
        too_small_args.extend([
            "--bsm-max-events-per-sample".to_string(),
            "100000".to_string(),
            "--bsm-output-dir".to_string(),
            too_small_dir.to_string_lossy().into_owned(),
            "--bsm-memory-budget-mb".to_string(),
            "1".to_string(),
        ]);
        assert!(matches!(
            run(too_small_args),
            Err(CliError::BsmMemoryBudgetTooSmall {
                budget_bytes: 1_048_576,
                minimum_bytes,
            }) if minimum_bytes > 1_048_576
        ));
        assert!(!too_small_dir.exists());
    }

    #[test]
    fn sharded_output_matches_monolithic_tables_and_resumes_inside_a_shard() {
        fn concatenate_sharded_table(
            output_dir: &Path,
            ranges: &[BsmShardRange],
            file_name: &str,
        ) -> String {
            let mut combined = String::new();
            for (index, range) in ranges.iter().enumerate() {
                let contents = fs::read_to_string(
                    output_dir
                        .join(BSM_SHARD_DIRECTORY)
                        .join(range.directory_name())
                        .join(file_name),
                )
                .unwrap();
                if index == 0 {
                    combined.push_str(&contents);
                } else {
                    combined.push_str(
                        contents
                            .split_once('\n')
                            .expect("shard table must contain a header")
                            .1,
                    );
                }
            }
            combined
        }

        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let monolithic_dir = temp.dir.join("bsm-shard-monolithic");
        let baseline_dir = temp.dir.join("bsm-shard-baseline");
        let resumed_dir = temp.dir.join("bsm-shard-resumed");
        let common_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "10".to_string(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "4".to_string(),
            "--seed".to_string(),
            "20260716".to_string(),
        ];

        let mut monolithic_args = common_args.clone();
        monolithic_args.extend([
            "--bsm-output-dir".to_string(),
            monolithic_dir.to_string_lossy().into_owned(),
        ]);
        run(monolithic_args).unwrap();

        let mut baseline_args = common_args.clone();
        baseline_args.extend([
            "--bsm-output-dir".to_string(),
            baseline_dir.to_string_lossy().into_owned(),
            "--bsm-shard-samples".to_string(),
            "4".to_string(),
        ]);
        let baseline_output = run(baseline_args).unwrap();
        assert!(baseline_output.contains("bsm_format\tbiogeo-bsm-sharded-tsv-v1\n"));
        assert!(baseline_output.contains("bsm_shard_samples\t4\n"));
        let inspection = bsm_inspect::inspect(&baseline_dir, true).unwrap();
        assert_eq!(inspection.bsm_format, BSM_SHARDED_STREAM_FORMAT);
        assert_eq!(inspection.path_validation, "passed");

        let ranges = [
            BsmShardRange {
                index: 0,
                start: 0,
                end_exclusive: 4,
            },
            BsmShardRange {
                index: 1,
                start: 4,
                end_exclusive: 8,
            },
            BsmShardRange {
                index: 2,
                start: 8,
                end_exclusive: 10,
            },
        ];
        for spec in BSM_TABLE_SPECS {
            assert_eq!(
                concatenate_sharded_table(&baseline_dir, &ranges, spec.file_name),
                fs::read_to_string(monolithic_dir.join(spec.file_name)).unwrap(),
                "sharded data differs from monolithic {}",
                spec.file_name
            );
        }
        assert_eq!(
            fs::read_dir(baseline_dir.join(BSM_SHARD_IN_PROGRESS_DIRECTORY))
                .unwrap()
                .count(),
            0
        );
        let baseline_metadata = fs::read_to_string(baseline_dir.join("metadata.tsv")).unwrap();
        assert!(baseline_metadata.contains("format\tbiogeo-bsm-sharded-tsv-v1\n"));
        assert!(baseline_metadata.contains("completed_shards\t3\n"));
        assert!(baseline_metadata.contains("total_shards\t3\n"));
        assert!(baseline_metadata.contains("manifest_file\tmanifest.tsv\n"));
        let baseline_manifest = fs::read(baseline_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap();
        let baseline_manifest_text = String::from_utf8(baseline_manifest.clone()).unwrap();
        assert!(baseline_manifest_text.contains("format\tbiogeo-bsm-shard-manifest-v1\n"));
        assert!(baseline_manifest_text.contains("completed_shards\t3\n"));
        assert!(baseline_manifest_text.contains("0\t0\t4\t4\t4\t4\tshards/"));
        assert!(baseline_manifest_text.contains("1\t4\t8\t4\t4\t8\tshards/"));
        assert!(baseline_manifest_text.contains("2\t8\t10\t2\t2\t10\tshards/"));

        let mut limited_args = common_args.clone();
        limited_args.extend([
            "--bsm-output-dir".to_string(),
            resumed_dir.to_string_lossy().into_owned(),
            "--bsm-shard-samples".to_string(),
            "4".to_string(),
            "--bsm-max-events-total".to_string(),
            "6".to_string(),
        ]);
        assert!(matches!(
            run(limited_args),
            Err(CliError::BsmTotalEventLimitExceeded {
                sample_index: 6,
                limit: 6,
                completed: 6,
                attempted: 7,
            })
        ));
        assert_eq!(
            fs::read_dir(resumed_dir.join(BSM_SHARD_DIRECTORY))
                .unwrap()
                .count(),
            1
        );
        let in_progress = resumed_dir
            .join(BSM_SHARD_IN_PROGRESS_DIRECTORY)
            .join(ranges[1].directory_name());
        let checkpoint = load_latest_bsm_checkpoint(&in_progress, 10).unwrap();
        assert_eq!(checkpoint.completed_samples, 6);
        assert_eq!(checkpoint.completed_anagenetic_events, Some(6));
        let stopped_metadata = fs::read_to_string(resumed_dir.join("metadata.tsv")).unwrap();
        assert!(stopped_metadata.contains("status\tevent_limit\n"));
        assert!(stopped_metadata.contains("completed_samples\t6\n"));
        assert!(stopped_metadata.contains("completed_shards\t1\n"));

        let mut resume_args = common_args;
        resume_args.extend([
            "--bsm-output-dir".to_string(),
            resumed_dir.to_string_lossy().into_owned(),
            "--bsm-shard-samples".to_string(),
            "4".to_string(),
            "--bsm-max-events-total".to_string(),
            "10".to_string(),
            "--bsm-threads".to_string(),
            "1".to_string(),
            "--bsm-resume".to_string(),
        ]);
        run(resume_args.clone()).unwrap();

        for range in ranges {
            for spec in BSM_TABLE_SPECS {
                assert_eq!(
                    fs::read(
                        resumed_dir
                            .join(BSM_SHARD_DIRECTORY)
                            .join(range.directory_name())
                            .join(spec.file_name)
                    )
                    .unwrap(),
                    fs::read(
                        baseline_dir
                            .join(BSM_SHARD_DIRECTORY)
                            .join(range.directory_name())
                            .join(spec.file_name)
                    )
                    .unwrap(),
                    "resumed shard differs in {}",
                    spec.file_name
                );
            }
        }
        assert_eq!(
            fs::read(resumed_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap(),
            baseline_manifest
        );
        assert_eq!(
            fs::read_dir(resumed_dir.join(BSM_SHARD_IN_PROGRESS_DIRECTORY))
                .unwrap()
                .count(),
            0
        );

        let root_metadata = resumed_dir.join("metadata.tsv");
        fs::write(&root_metadata, b"key\tvalue\nformat\t").unwrap();
        run(resume_args.clone()).unwrap();
        let recovered_metadata = fs::read_to_string(&root_metadata).unwrap();
        assert!(recovered_metadata.contains("format\tbiogeo-bsm-sharded-tsv-v1\n"));
        assert!(recovered_metadata.contains("status\tcomplete\n"));
        assert_eq!(
            fs::read(resumed_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap(),
            baseline_manifest
        );

        let protected_table = resumed_dir
            .join(BSM_SHARD_DIRECTORY)
            .join(ranges[0].directory_name())
            .join(BSM_TABLE_SPECS[BSM_NODE_STATES].file_name);
        let protected_bytes = fs::read(&protected_table).unwrap();
        let mut changed_shard_args = resume_args.clone();
        changed_shard_args.extend(["--bsm-shard-samples".to_string(), "5".to_string()]);
        assert!(matches!(
            run(changed_shard_args),
            Err(CliError::BsmResumeFingerprintMismatch { .. })
        ));
        assert_eq!(fs::read(&protected_table).unwrap(), protected_bytes);
        assert_eq!(
            fs::read(resumed_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap(),
            baseline_manifest
        );

        let final_range = ranges[2];
        let final_path = resumed_dir
            .join(BSM_SHARD_DIRECTORY)
            .join(final_range.directory_name());
        let recovered_path = resumed_dir
            .join(BSM_SHARD_IN_PROGRESS_DIRECTORY)
            .join(final_range.directory_name());
        fs::rename(&final_path, &recovered_path).unwrap();
        fs::remove_file(resumed_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap();
        run(resume_args.clone()).unwrap();
        assert!(final_path.is_dir());
        assert!(!recovered_path.exists());
        assert_eq!(
            fs::read(resumed_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap(),
            baseline_manifest
        );

        fs::remove_dir_all(&final_path).unwrap();
        fs::create_dir(&recovered_path).unwrap();
        fs::write(
            recovered_path.join(BSM_TABLE_SPECS[BSM_NODE_STATES].file_name),
            b"partial initialization",
        )
        .unwrap();
        run(resume_args.clone()).unwrap();
        assert!(final_path.is_dir());
        assert!(!recovered_path.exists());
        for spec in BSM_TABLE_SPECS {
            assert_eq!(
                fs::read(final_path.join(spec.file_name)).unwrap(),
                fs::read(
                    baseline_dir
                        .join(BSM_SHARD_DIRECTORY)
                        .join(final_range.directory_name())
                        .join(spec.file_name)
                )
                .unwrap(),
                "reinitialized shard differs in {}",
                spec.file_name
            );
        }
        assert_eq!(
            fs::read(resumed_dir.join(BSM_SHARD_MANIFEST_FILE)).unwrap(),
            baseline_manifest
        );

        let published_table = final_path.join(BSM_TABLE_SPECS[BSM_NODE_STATES].file_name);
        let mut corrupted = fs::read(&published_table).unwrap();
        corrupted.extend_from_slice(b"uncommitted-tail\n");
        fs::write(&published_table, &corrupted).unwrap();
        assert!(matches!(
            run(resume_args),
            Err(CliError::InvalidBsmShard { path, .. }) if path == published_table
        ));
        assert_eq!(fs::read(&published_table).unwrap(), corrupted);
    }

    #[test]
    fn resume_truncates_uncommitted_cross_table_tail_and_matches_full_run() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-resume");
        let base_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "16".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-threads".to_string(),
            "4".to_string(),
            "--bsm-max-in-flight".to_string(),
            "4".to_string(),
            "--bsm-checkpoint-samples".to_string(),
            "4".to_string(),
            "--seed".to_string(),
            "20260716".to_string(),
        ];
        run(base_args).unwrap();
        let expected_tables = BSM_TABLE_SPECS
            .iter()
            .map(|spec| fs::read(output_dir.join(spec.file_name)).unwrap())
            .collect::<Vec<_>>();

        let recovery_checkpoint = parse_bsm_checkpoint(&checkpoint_path(&output_dir, 4)).unwrap();
        for entry in fs::read_dir(output_dir.join(BSM_CHECKPOINT_DIRECTORY)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) == Some("tsv")
                && parse_bsm_checkpoint(&path).unwrap().completed_samples > 4
            {
                fs::remove_file(path).unwrap();
            }
        }
        for (index, (spec, length)) in BSM_TABLE_SPECS
            .iter()
            .zip(recovery_checkpoint.table_lengths.iter().copied())
            .enumerate()
        {
            let path = output_dir.join(spec.file_name);
            let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
            file.set_len(length).unwrap();
            if index < 3 {
                let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
                file.write_all(b"uncommitted-partial-row").unwrap();
                file.sync_all().unwrap();
            }
        }

        let resumed_output = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "16".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-threads".to_string(),
            "2".to_string(),
            "--bsm-max-in-flight".to_string(),
            "2".to_string(),
            "--bsm-checkpoint-samples".to_string(),
            "4".to_string(),
            "--bsm-resume".to_string(),
            "--seed".to_string(),
            "20260716".to_string(),
        ])
        .unwrap();
        assert!(resumed_output.contains("bsm_resume\ttrue\n"));

        for (spec, expected) in BSM_TABLE_SPECS.iter().zip(expected_tables) {
            assert_eq!(
                fs::read(output_dir.join(spec.file_name)).unwrap(),
                expected,
                "resumed bytes differ in {}",
                spec.file_name
            );
        }
        let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tcomplete\n"));
        assert!(metadata.contains("completed_samples\t16\n"));
        assert_eq!(
            parse_bsm_checkpoint(&checkpoint_path(&output_dir, 16))
                .unwrap()
                .completed_samples,
            16
        );
    }

    #[test]
    fn resume_migrates_v1_checkpoint_event_count_from_the_committed_table() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-v1-checkpoint");
        let mut args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-checkpoint-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ];
        run(args.clone()).unwrap();
        let expected_tables = BSM_TABLE_SPECS
            .iter()
            .map(|spec| fs::read(output_dir.join(spec.file_name)).unwrap())
            .collect::<Vec<_>>();

        let checkpoint_path = checkpoint_path(&output_dir, 2);
        let checkpoint = parse_bsm_checkpoint(&checkpoint_path).unwrap();
        assert_eq!(checkpoint.completed_anagenetic_events, Some(2));
        for entry in fs::read_dir(output_dir.join(BSM_CHECKPOINT_DIRECTORY)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|value| value.to_str()) == Some("tsv")
                && parse_bsm_checkpoint(&path).unwrap().completed_samples > 2
            {
                fs::remove_file(path).unwrap();
            }
        }
        for (spec, length) in BSM_TABLE_SPECS
            .iter()
            .zip(checkpoint.table_lengths.iter().copied())
        {
            fs::OpenOptions::new()
                .write(true)
                .open(output_dir.join(spec.file_name))
                .unwrap()
                .set_len(length)
                .unwrap();
        }
        let legacy_contents = format_bsm_checkpoint(&checkpoint)
            .replace(BSM_CHECKPOINT_FORMAT, BSM_CHECKPOINT_FORMAT_V1)
            .lines()
            .filter(|line| !line.starts_with("completed_anagenetic_events\t"))
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&checkpoint_path, legacy_contents).unwrap();
        let parsed_legacy = parse_bsm_checkpoint(&checkpoint_path).unwrap();
        assert_eq!(parsed_legacy.completed_anagenetic_events, None);
        let hydrated = load_latest_bsm_checkpoint(&output_dir, 4).unwrap();
        assert_eq!(hydrated.completed_samples, 2);
        assert_eq!(hydrated.completed_anagenetic_events, Some(2));

        args.extend([
            "--bsm-resume".to_string(),
            "--bsm-max-events-total".to_string(),
            "4".to_string(),
        ]);
        run(args).unwrap();
        for (spec, expected) in BSM_TABLE_SPECS.iter().zip(expected_tables) {
            assert_eq!(fs::read(output_dir.join(spec.file_name)).unwrap(), expected);
        }
    }

    #[test]
    fn resume_rejects_changed_model_before_touching_committed_tables() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-resume-mismatch");
        let args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-checkpoint-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ];
        run(args).unwrap();
        let expected = BSM_TABLE_SPECS
            .iter()
            .map(|spec| fs::read(output_dir.join(spec.file_name)).unwrap())
            .collect::<Vec<_>>();

        let error = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.25".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-resume".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::BsmResumeFingerprintMismatch { .. }
        ));
        for (spec, expected) in BSM_TABLE_SPECS.iter().zip(expected) {
            assert_eq!(fs::read(output_dir.join(spec.file_name)).unwrap(), expected);
        }
    }

    #[test]
    fn resume_rejects_a_table_shorter_than_the_latest_checkpoint() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-resume-short-table");
        let mut args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-checkpoint-samples".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ];
        run(args.clone()).unwrap();
        let checkpoint = parse_bsm_checkpoint(&checkpoint_path(&output_dir, 4)).unwrap();
        let short_path = output_dir.join(BSM_TABLE_SPECS[0].file_name);
        let untouched_path = output_dir.join(BSM_TABLE_SPECS[1].file_name);
        let untouched = fs::read(&untouched_path).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&short_path)
            .unwrap()
            .set_len(checkpoint.table_lengths[0] - 1)
            .unwrap();
        args.push("--bsm-resume".to_string());

        assert!(matches!(
            run(args),
            Err(CliError::BsmTableShorterThanCheckpoint { path, .. }) if path == short_path
        ));
        assert_eq!(fs::read(untouched_path).unwrap(), untouched);
    }

    #[test]
    fn rollback_discards_buffered_rows_and_truncates_already_flushed_rows() {
        let temp = TempInputs::new();
        let output_dir = temp.dir.join("bsm-writer-rollback");
        prepare_bsm_output_directory(&output_dir).unwrap();
        let mut writers = BsmTableWriters::create(&output_dir, BsmOutputLevel::Legacy).unwrap();
        let checkpoint =
            commit_bsm_checkpoint(&mut writers, &output_dir, 0, 0, "test-run").unwrap();
        let rows = BsmSampleTableRows {
            tables: std::array::from_fn(|index| {
                let payload = if index == 0 {
                    "x".repeat(16 * 1024)
                } else {
                    "buffered".to_string()
                };
                format!("{index}\t{payload}\n")
            }),
        };
        writers.write_sample(&rows).unwrap();
        writers.tables[0].writer.flush().unwrap();
        assert!(fs::metadata(&writers.tables[0].path).unwrap().len() > checkpoint.table_lengths[0]);
        assert_eq!(
            fs::metadata(&writers.tables[1].path).unwrap().len(),
            checkpoint.table_lengths[1]
        );

        writers.rollback(&checkpoint).unwrap();
        for (index, spec) in BSM_TABLE_SPECS.iter().enumerate() {
            assert_eq!(
                fs::metadata(output_dir.join(spec.file_name)).unwrap().len(),
                checkpoint.table_lengths[index],
                "rollback length mismatch in {}",
                spec.file_name
            );
        }
    }

    #[test]
    fn storage_full_during_row_or_checkpoint_write_rolls_back_and_resumes_exactly() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let fault_cases = [
            ("row", Some(3), None, None),
            (
                "table-flush",
                None,
                Some(BSM_TABLE_SPECS.len().saturating_add(3)),
                None,
            ),
            ("checkpoint-file", None, None, Some(1)),
        ];

        for (
            name,
            writes_before_failure,
            flushes_before_failure,
            checkpoint_writes_before_failure,
        ) in fault_cases
        {
            let baseline_dir = temp.dir.join(format!("bsm-storage-full-{name}-baseline"));
            let failed_dir = temp.dir.join(format!("bsm-storage-full-{name}-failed"));
            let make_args = |output_dir: &Path| {
                vec![
                    "dec".to_string(),
                    "--tree".to_string(),
                    temp.tree_path.to_string_lossy().into_owned(),
                    "--ranges".to_string(),
                    temp.ranges_path.to_string_lossy().into_owned(),
                    "--d".to_string(),
                    "0".to_string(),
                    "--e".to_string(),
                    "1".to_string(),
                    "--include-null-range".to_string(),
                    "--bsm-samples".to_string(),
                    "8".to_string(),
                    "--bsm-output-dir".to_string(),
                    output_dir.to_string_lossy().into_owned(),
                    "--bsm-threads".to_string(),
                    "4".to_string(),
                    "--bsm-max-in-flight".to_string(),
                    "4".to_string(),
                    "--bsm-checkpoint-samples".to_string(),
                    "1".to_string(),
                    "--seed".to_string(),
                    "20260824".to_string(),
                ]
            };

            run(make_args(&baseline_dir)).unwrap();
            let expected_tables = BSM_TABLE_SPECS
                .iter()
                .map(|spec| fs::read(baseline_dir.join(spec.file_name)).unwrap())
                .collect::<Vec<_>>();

            let fault = inject_bsm_storage_full(
                writes_before_failure,
                flushes_before_failure,
                checkpoint_writes_before_failure,
            );
            let error = run(make_args(&failed_dir)).unwrap_err();
            drop(fault);
            assert!(matches!(
                error,
                CliError::OutputIo { ref source, .. }
                    if source.kind() == io::ErrorKind::StorageFull
            ));

            let checkpoint = load_latest_bsm_checkpoint(&failed_dir, 8).unwrap();
            assert_eq!(checkpoint.completed_samples, 0);
            let metadata = fs::read_to_string(failed_dir.join("metadata.tsv")).unwrap();
            assert!(metadata.contains("status\tincomplete\n"));
            assert!(metadata.contains("completed_samples\t0\n"));
            for (index, spec) in BSM_TABLE_SPECS.iter().enumerate() {
                assert_eq!(
                    fs::metadata(failed_dir.join(spec.file_name)).unwrap().len(),
                    checkpoint.table_lengths[index],
                    "{name} fault left uncommitted bytes in {}",
                    spec.file_name
                );
            }
            assert_eq!(
                fs::read_dir(failed_dir.join(BSM_CHECKPOINT_DIRECTORY))
                    .unwrap()
                    .filter_map(Result::ok)
                    .filter(|entry| entry.file_name().to_string_lossy().starts_with('.'))
                    .count(),
                0,
                "{name} fault left a temporary checkpoint"
            );

            let mut resume_args = make_args(&failed_dir);
            resume_args.push("--bsm-resume".to_string());
            run(resume_args).unwrap();
            for (spec, expected) in BSM_TABLE_SPECS.iter().zip(&expected_tables) {
                assert_eq!(
                    fs::read(failed_dir.join(spec.file_name))
                        .unwrap()
                        .as_slice(),
                    expected.as_slice(),
                    "{name} recovery differs in {}",
                    spec.file_name
                );
            }
        }
    }

    #[test]
    fn event_limit_failure_keeps_stream_incomplete_without_partial_sample_rows() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-limited");
        let error = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--bsm-threads".to_string(),
            "1".to_string(),
            "--bsm-max-in-flight".to_string(),
            "1".to_string(),
            "--bsm-max-events-per-sample".to_string(),
            "0".to_string(),
            "--seed".to_string(),
            "9".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            CliError::BsmSampling {
                sample_index: 0,
                source: biogeo_core::BsmError::AnageneticEventLimitExceeded {
                    limit: 0,
                    attempted: 1,
                }
            }
        ));
        let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tincomplete\n"));
        assert!(metadata.contains("completed_samples\t0\n"));
        assert!(metadata.contains("samples\t4\n"));
        assert!(metadata.contains("max_events_per_sample\t0\n"));
        let checkpoint = parse_bsm_checkpoint(&checkpoint_path(&output_dir, 0)).unwrap();
        assert_eq!(checkpoint.completed_samples, 0);
        for spec in BSM_TABLE_SPECS {
            let contents = fs::read_to_string(output_dir.join(spec.file_name)).unwrap();
            assert_eq!(
                contents.lines().count(),
                1,
                "partial rows in {}",
                spec.file_name
            );
            assert_eq!(
                fs::metadata(output_dir.join(spec.file_name)).unwrap().len(),
                checkpoint.table_lengths[BSM_TABLE_SPECS
                    .iter()
                    .position(|candidate| candidate.file_name == spec.file_name)
                    .unwrap()]
            );
        }
    }

    #[test]
    fn total_event_limit_commits_an_exact_prefix_and_resumes_with_a_higher_limit() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let baseline_dir = temp.dir.join("bsm-total-limit-baseline");
        let limited_dir = temp.dir.join("bsm-total-limited");
        let make_args = |output_dir: &Path| {
            vec![
                "dec".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0".to_string(),
                "--e".to_string(),
                "1".to_string(),
                "--include-null-range".to_string(),
                "--bsm-samples".to_string(),
                "4".to_string(),
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--bsm-threads".to_string(),
                "4".to_string(),
                "--bsm-max-in-flight".to_string(),
                "4".to_string(),
                "--bsm-checkpoint-samples".to_string(),
                "4".to_string(),
                "--seed".to_string(),
                "9".to_string(),
            ]
        };

        run(make_args(&baseline_dir)).unwrap();
        let expected_tables = BSM_TABLE_SPECS
            .iter()
            .map(|spec| fs::read(baseline_dir.join(spec.file_name)).unwrap())
            .collect::<Vec<_>>();

        let mut limited_args = make_args(&limited_dir);
        limited_args.extend(["--bsm-max-events-total".to_string(), "2".to_string()]);
        let error = run(limited_args).unwrap_err();
        assert!(matches!(
            error,
            CliError::BsmTotalEventLimitExceeded {
                sample_index: 2,
                limit: 2,
                completed: 2,
                attempted: 3,
            }
        ));
        let metadata = fs::read_to_string(limited_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tevent_limit\n"));
        assert!(metadata.contains("completed_samples\t2\n"));
        assert!(metadata.contains("completed_anagenetic_events\t2\n"));
        assert!(metadata.contains("max_events_total\t2\n"));
        let checkpoint = parse_bsm_checkpoint(&checkpoint_path(&limited_dir, 2)).unwrap();
        assert_eq!(checkpoint.completed_samples, 2);
        assert_eq!(checkpoint.completed_anagenetic_events, Some(2));

        let mut resume_args = make_args(&limited_dir);
        resume_args.extend([
            "--bsm-resume".to_string(),
            "--bsm-max-events-total".to_string(),
            "4".to_string(),
        ]);
        run(resume_args).unwrap();
        for (spec, expected) in BSM_TABLE_SPECS.iter().zip(expected_tables) {
            assert_eq!(
                fs::read(limited_dir.join(spec.file_name)).unwrap(),
                expected,
                "resumed bytes differ in {}",
                spec.file_name
            );
        }
        let metadata = fs::read_to_string(limited_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tcomplete\n"));
        assert!(metadata.contains("completed_samples\t4\n"));
        assert!(metadata.contains("completed_anagenetic_events\t4\n"));
        assert!(metadata.contains("max_events_total\t4\n"));
    }

    #[test]
    fn cancelled_stream_can_resume_to_byte_identical_tables() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let baseline_dir = temp.dir.join("bsm-cancel-baseline");
        let cancelled_dir = temp.dir.join("bsm-cancelled");
        let make_args = |output_dir: &Path| {
            vec![
                "dec".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0".to_string(),
                "--e".to_string(),
                "1".to_string(),
                "--include-null-range".to_string(),
                "--bsm-samples".to_string(),
                "16".to_string(),
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--bsm-threads".to_string(),
                "4".to_string(),
                "--bsm-max-in-flight".to_string(),
                "4".to_string(),
                "--bsm-checkpoint-samples".to_string(),
                "16".to_string(),
                "--seed".to_string(),
                "20260716".to_string(),
            ]
        };

        run(make_args(&baseline_dir)).unwrap();
        let expected_tables = BSM_TABLE_SPECS
            .iter()
            .map(|spec| fs::read(baseline_dir.join(spec.file_name)).unwrap())
            .collect::<Vec<_>>();

        let cancellation = biogeo_core::StochasticMapCancellationToken::new();
        cancellation.cancel();
        let error =
            run_with_cancellation(make_args(&cancelled_dir), Some(cancellation)).unwrap_err();
        assert!(matches!(error, CliError::BsmCancelled { sample_index: 0 }));
        let metadata = fs::read_to_string(cancelled_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tcancelled\n"));
        assert!(metadata.contains("completed_samples\t0\n"));
        assert_eq!(
            parse_bsm_checkpoint(&checkpoint_path(&cancelled_dir, 0))
                .unwrap()
                .completed_samples,
            0
        );

        let mut resume_args = make_args(&cancelled_dir);
        resume_args.push("--bsm-resume".to_string());
        run(resume_args).unwrap();
        for (spec, expected) in BSM_TABLE_SPECS.iter().zip(expected_tables) {
            assert_eq!(
                fs::read(cancelled_dir.join(spec.file_name)).unwrap(),
                expected,
                "resumed bytes differ in {}",
                spec.file_name
            );
        }
        let metadata = fs::read_to_string(cancelled_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tcomplete\n"));
        assert!(metadata.contains("completed_samples\t16\n"));
    }

    #[test]
    fn zero_time_limit_stops_cleanly_and_can_resume_with_a_new_limit() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let output_dir = temp.dir.join("bsm-time-limit");
        let base_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "1".to_string(),
            "--include-null-range".to_string(),
            "--bsm-samples".to_string(),
            "4".to_string(),
            "--bsm-output-dir".to_string(),
            output_dir.to_string_lossy().into_owned(),
            "--seed".to_string(),
            "9".to_string(),
        ];
        let mut timed_args = base_args.clone();
        timed_args.extend(["--bsm-time-limit-seconds".to_string(), "0".to_string()]);

        assert!(matches!(
            run(timed_args),
            Err(CliError::BsmTimeLimitExceeded { sample_index: 0 })
        ));
        let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\ttime_limit\n"));
        assert!(metadata.contains("completed_samples\t0\n"));
        assert!(metadata.contains("time_limit_seconds\t0\n"));

        let mut resume_args = base_args;
        resume_args.extend([
            "--bsm-resume".to_string(),
            "--bsm-time-limit-seconds".to_string(),
            "60".to_string(),
        ]);
        run(resume_args).unwrap();
        let metadata = fs::read_to_string(output_dir.join("metadata.tsv")).unwrap();
        assert!(metadata.contains("status\tcomplete\n"));
        assert!(metadata.contains("completed_samples\t4\n"));
        assert!(metadata.contains("time_limit_seconds\t60\n"));
    }

    #[test]
    fn streamed_bsm_tables_are_byte_identical_across_thread_counts() {
        let temp = TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\nA\t0\nB\t1\n");
        let mut baseline = None;

        for threads in [1, 2, 4, 8, 16] {
            let output_dir = temp.dir.join(format!("bsm-{threads}"));
            let output = run(vec![
                "dec".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--d".to_string(),
                "0".to_string(),
                "--e".to_string(),
                "1".to_string(),
                "--include-null-range".to_string(),
                "--bsm-samples".to_string(),
                "16".to_string(),
                "--bsm-output-dir".to_string(),
                output_dir.to_string_lossy().into_owned(),
                "--bsm-threads".to_string(),
                threads.to_string(),
                "--bsm-max-in-flight".to_string(),
                threads.to_string(),
                "--seed".to_string(),
                "20260716".to_string(),
            ])
            .unwrap();
            assert!(output.contains("bsm_rng_protocol\tindexed-chacha12-v1\n"));
            assert!(output.contains(&format!("bsm_threads\t{threads}\n")));

            let tables = BSM_TABLE_SPECS
                .iter()
                .map(|spec| fs::read_to_string(output_dir.join(spec.file_name)).unwrap())
                .collect::<Vec<_>>();
            if let Some(expected) = &baseline {
                assert_eq!(&tables, expected, "thread count {threads}");
            } else {
                baseline = Some(tables);
            }
        }
    }

    #[test]
    fn rejects_bsm_output_directory_without_samples() {
        let error = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--bsm-output-dir".to_string(),
            "bsm-output".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::BsmOutputRequiresSamples));
    }

    #[test]
    fn rejects_duplicate_history_sampling_modes() {
        let error = parse_command(vec![
            "dec".to_string(),
            "--tree".to_string(),
            "tree.nwk".to_string(),
            "--ranges".to_string(),
            "ranges.tsv".to_string(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--traceback-samples".to_string(),
            "1".to_string(),
            "--bsm-samples".to_string(),
            "1".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::ConflictingHistorySamplingOptions));
    }

    #[test]
    fn runs_fixed_dec_j_from_files() {
        let temp = TempInputs::new();

        let output = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--j".to_string(),
            "1.0".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDEC+J\n"));
        assert!(output.contains("j\t1\n"));
    }

    #[test]
    fn runs_fixed_dec_j_with_directional_founder_event_weights() {
        let temp = TempInputs::new_with_contents(
            "((A:0.5,B:0.5):0.25,C:0.75);\n",
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
        );
        let matrix_path = temp.dir.join("directional.tsv");
        fs::write(
            &matrix_path,
            "from\tAreaA\tAreaB\tAreaC\nAreaA\t1\t0.25\t0\nAreaB\t2\t1\t0.5\nAreaC\t0.1\t3\t1\n",
        )
        .unwrap();

        let output = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0".to_string(),
            "--e".to_string(),
            "0".to_string(),
            "--j".to_string(),
            "1".to_string(),
            "--max-range-size".to_string(),
            "2".to_string(),
            "--include-null-range".to_string(),
            "--dispersal-multipliers".to_string(),
            matrix_path.to_string_lossy().into_owned(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("lnL\t-1.366617906611146\n"));
        assert!(output.contains("\t0.214285714285714\t"));
        assert!(output.contains(&format!(
            "dispersal_multipliers\t{}\n",
            matrix_path.display()
        )));
    }

    #[test]
    fn runs_fixed_divalike_from_files() {
        let temp = TempInputs::new();

        let output = run(vec![
            "divalike".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDIVALIKE\n"));
        assert!(output.contains("lnL\t-0.693147180559945\n"));
        assert!(output.contains("mx01v\t0.5\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_fixed_bayarealike_from_files() {
        let temp = TempInputs::new_with_ranges("tip\tAreaA\tAreaB\nA\t1\t0\nB\t1\t0\n");

        let output = run(vec![
            "bayarealike".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tBAYAREALIKE\n"));
        assert!(output.contains("mx01y\t0.9999\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_fixed_and_optimized_models_with_directional_dispersal() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let multipliers_path = temp.dir.join("multipliers.tsv");
        fs::write(
            &multipliers_path,
            "from\tAreaA\tAreaB\nAreaA\t1\t0\nAreaB\t0.25\t1\n",
        )
        .unwrap();

        let fixed = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-multipliers".to_string(),
            multipliers_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let optimized = run(vec![
            "dec-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--dispersal-multipliers".to_string(),
            multipliers_path.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(fixed.contains("model\tDEC\n"));
        assert!(fixed.contains(&format!(
            "dispersal_multipliers\t{}\n",
            multipliers_path.display()
        )));
        assert!(optimized.contains("model\tDEC\nmode\toptimize\n"));
        assert!(optimized.contains("converged\ttrue\n"));
    }

    #[test]
    fn runs_fixed_and_optimized_models_with_composed_anagenetic_modifiers() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let manual_path = temp.dir.join("manual.tsv");
        fs::write(
            &manual_path,
            "from\tAreaA\tAreaB\nAreaA\t1\t0.5\nAreaB\t0.25\t1\n",
        )
        .unwrap();
        let distance_path = temp.dir.join("distances.tsv");
        fs::write(
            &distance_path,
            "from\tAreaA\tAreaB\nAreaA\t1\t2\nAreaB\t4\t1\n",
        )
        .unwrap();
        let environment_path = temp.dir.join("environment.tsv");
        fs::write(
            &environment_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t0.25\nAreaB\t4\t0\n",
        )
        .unwrap();
        let extirpation_path = temp.dir.join("extirpation.tsv");
        fs::write(
            &extirpation_path,
            "area\tmultiplier\nAreaA\t0.5\nAreaB\t2\n",
        )
        .unwrap();

        let modifier_args = vec![
            "--dispersal-multipliers".to_string(),
            manual_path.to_string_lossy().into_owned(),
            "--distance-matrix".to_string(),
            distance_path.to_string_lossy().into_owned(),
            "--distance-exponent".to_string(),
            "-1".to_string(),
            "--environment-distance-matrix".to_string(),
            environment_path.to_string_lossy().into_owned(),
            "--environment-distance-exponent".to_string(),
            "0.5".to_string(),
            "--extirpation-multipliers".to_string(),
            extirpation_path.to_string_lossy().into_owned(),
        ];
        let mut fixed_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
        ];
        fixed_args.extend(modifier_args.clone());
        let fixed = run(fixed_args).unwrap();

        let mut optimized_args = vec![
            "dec-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
        ];
        optimized_args.extend(modifier_args);
        let optimized = run(optimized_args).unwrap();

        assert!(fixed.contains(&format!("distance_matrix\t{}\n", distance_path.display())));
        assert!(fixed.contains("distance_exponent\t-1\n"));
        assert!(fixed.contains(&format!(
            "environment_distance_matrix\t{}\n",
            environment_path.display()
        )));
        assert!(fixed.contains("environment_distance_exponent\t0.5\n"));
        assert!(fixed.contains(&format!(
            "extirpation_multipliers\t{}\n",
            extirpation_path.display()
        )));
        assert!(optimized.contains("mode\toptimize\n"));
        assert!(optimized.contains("converged\ttrue\n"));
    }

    #[test]
    fn runs_x_and_n_optimization_with_posteriors() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let distance_path = temp.dir.join("distances.tsv");
        fs::write(
            &distance_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t0.5\nAreaB\t2\t0\n",
        )
        .unwrap();
        let environment_path = temp.dir.join("environment.tsv");
        fs::write(
            &environment_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t0.25\nAreaB\t4\t0\n",
        )
        .unwrap();

        let common = vec![
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--multi-start-points".to_string(),
            "1".to_string(),
            "--max-iterations".to_string(),
            "100".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ];
        let mut x_args = vec!["dec-x-optimize".to_string()];
        x_args.extend(common.clone());
        x_args.extend([
            "--distance-matrix".to_string(),
            distance_path.to_string_lossy().into_owned(),
        ]);
        let x_output = run(x_args).unwrap();

        let mut n_args = vec!["dec-n-optimize".to_string()];
        n_args.extend(common);
        n_args.extend([
            "--environment-distance-matrix".to_string(),
            environment_path.to_string_lossy().into_owned(),
        ]);
        let n_output = run(n_args).unwrap();

        assert!(x_output.contains("model\tDEC+x\nmode\toptimize\n"));
        assert!(x_output.contains("exponent_parameter\tx\n"));
        assert!(x_output.contains("ancestral_state_probabilities\n"));
        assert!(x_output.contains("split_scenario_probabilities\n"));
        assert!(n_output.contains("model\tDEC+n\nmode\toptimize\n"));
        assert!(n_output.contains("exponent_parameter\tn\n"));
        assert!(n_output.contains("starts\t1\n"));
    }

    #[test]
    fn runs_fixed_de_and_u_optimization_with_raw_area_sizes() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let area_sizes_path = temp.dir.join("area-sizes.tsv");
        fs::write(&area_sizes_path, "area\tsize\nAreaA\t0.5\nAreaB\t2\n").unwrap();

        let fixed = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--area-sizes".to_string(),
            area_sizes_path.to_string_lossy().into_owned(),
            "--area-exponent".to_string(),
            "-1".to_string(),
        ])
        .unwrap();
        let de_optimized = run(vec![
            "dec-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--area-sizes".to_string(),
            area_sizes_path.to_string_lossy().into_owned(),
            "--area-exponent".to_string(),
            "-1".to_string(),
        ])
        .unwrap();
        let u_optimized = run(vec![
            "dec-u-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--area-sizes".to_string(),
            area_sizes_path.to_string_lossy().into_owned(),
            "--multi-start-points".to_string(),
            "1".to_string(),
            "--max-iterations".to_string(),
            "100".to_string(),
            "--ancestral-probs".to_string(),
        ])
        .unwrap();

        assert!(fixed.contains(&format!("area_sizes\t{}\n", area_sizes_path.display())));
        assert!(fixed.contains("area_exponent\t-1\n"));
        assert!(de_optimized.contains("converged\ttrue\n"));
        assert!(u_optimized.contains("model\tDEC+u\nmode\toptimize\n"));
        assert!(u_optimized.contains("exponent_parameter\tu\n"));
        assert!(u_optimized.contains("ancestral_state_probabilities\n"));

        let uniform_sizes_path = temp.dir.join("uniform-area-sizes.tsv");
        fs::write(&uniform_sizes_path, "area\tsize\nAreaA\t1\nAreaB\t1\n").unwrap();
        let error = run(vec![
            "dec-u-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--area-sizes".to_string(),
            uniform_sizes_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(error, CliError::UnidentifiableAreaExponent));
    }

    #[test]
    fn runs_fixed_and_optimized_models_with_time_stratified_dispersal() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        fs::write(
            temp.dir.join("young.tsv"),
            "from\tAreaA\tAreaB\nAreaA\t1\t1\nAreaB\t1\t1\n",
        )
        .unwrap();
        fs::write(
            temp.dir.join("old.tsv"),
            "from\tAreaA\tAreaB\nAreaA\t1\t0\nAreaB\t0.5\t1\n",
        )
        .unwrap();
        let strata_path = temp.dir.join("strata.tsv");
        fs::write(
            &strata_path,
            "oldest_age\tmatrix\n0.4\tyoung.tsv\n1.0\told.tsv\n",
        )
        .unwrap();

        let fixed = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let optimized = run(vec![
            "dec-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ])
        .unwrap();

        assert!(fixed.contains(&format!(
            "dispersal_multipliers\tstratified:{}\n",
            strata_path.display()
        )));
        assert!(optimized.contains("mode\toptimize\n"));
        assert!(optimized.contains("converged\ttrue\n"));
    }

    #[test]
    fn runs_state_constraints_from_extended_anagenetic_strata() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        for (name, values) in [
            ("young-allowed.tsv", "AreaA\t1\t1\nAreaB\t1\t1\n"),
            ("old-allowed.tsv", "AreaA\t1\t1\nAreaB\t1\t1\n"),
            ("young-adjacency.tsv", "AreaA\t1\t1\nAreaB\t1\t1\n"),
            ("old-adjacency.tsv", "AreaA\t1\t0\nAreaB\t0\t1\n"),
        ] {
            fs::write(temp.dir.join(name), format!("from\tAreaA\tAreaB\n{values}")).unwrap();
        }
        let strata_path = temp.dir.join("constraint-strata.tsv");
        fs::write(
            &strata_path,
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\n\
             0.4\t-\t-\t-\t-\tyoung-allowed.tsv\tyoung-adjacency.tsv\n\
             1.0\t-\t-\t-\t-\told-allowed.tsv\told-adjacency.tsv\n",
        )
        .unwrap();

        let fixed = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
            "--ancestral-probs".to_string(),
            "--bsm-samples".to_string(),
            "8".to_string(),
            "--bsm-threads".to_string(),
            "2".to_string(),
            "--seed".to_string(),
            "20260811".to_string(),
        ])
        .unwrap();

        assert!(fixed.contains("model\tDEC\n"));
        assert!(fixed.contains("ancestral_state_probabilities\n"));
        assert!(fixed.contains("bsm_sample_period_state_occupancy\n"));
        assert!(fixed.contains(&format!(
            "dispersal_multipliers\tstratified:{}\n",
            strata_path.display()
        )));

        fs::write(
            temp.dir.join("young-adjacency.tsv"),
            "from\tAreaA\tAreaB\nAreaA\t1\t0\nAreaB\t0\t1\n",
        )
        .unwrap();
        fs::write(&temp.ranges_path, "tip\tAreaA\tAreaB\nA\t1\t1\nB\t1\t0\n").unwrap();
        let error = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ])
        .unwrap_err();
        assert!(matches!(
            error,
            CliError::TipStateConstraintViolations { tips }
                if tips == vec![(0, "A".to_string(), 0)]
        ));
    }

    #[test]
    fn explicit_allowed_ranges_reach_fixed_and_generic_preflight_paths() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t1\nB\t1\t0\n");
        let allowed_path = temp.dir.join("allowed-ranges.tsv");
        fs::write(
            &allowed_path,
            "range\tAreaA\tAreaB\nAreaA\t1\t0\nAreaB\t0\t1\nAreaA+AreaB\t1\t1\n",
        )
        .unwrap();
        let strata_path = temp.dir.join("explicit-strata.tsv");
        fs::write(
            &strata_path,
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\tallowed_ranges\n\
             1\tnone\tnone\tnone\tnone\tnone\tnone\tallowed-ranges.tsv\n",
        )
        .unwrap();

        let fixed_args = vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ];
        assert!(run(fixed_args.clone()).unwrap().contains("model\tDEC\n"));

        fs::write(
            &allowed_path,
            "range\tAreaA\tAreaB\nAreaA\t1\t0\nAreaB\t0\t1\n",
        )
        .unwrap();
        let assert_tip_a = |error: CliError| {
            assert!(matches!(
                error,
                CliError::TipStateConstraintViolations { tips }
                    if tips == vec![(0, "A".to_string(), 0)]
            ));
        };
        assert_tip_a(run(fixed_args).unwrap_err());

        let fixed_parameters = biogeo_core::BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.1)
            .unwrap()
            .with_fixed("e", 0.2)
            .unwrap();
        let fixed_parameters_path = temp.dir.join("fixed-parameters.tsv");
        fs::write(&fixed_parameters_path, fixed_parameters.to_versioned_tsv()).unwrap();
        assert_tip_a(
            run(vec![
                "model-evaluate".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--parameters".to_string(),
                fixed_parameters_path.to_string_lossy().into_owned(),
                "--dispersal-strata".to_string(),
                strata_path.to_string_lossy().into_owned(),
            ])
            .unwrap_err(),
        );

        let free_parameters_path = temp.dir.join("free-parameters.tsv");
        fs::write(
            &free_parameters_path,
            biogeo_core::BioGeoBearsPreset::Dec
                .parameter_table()
                .unwrap()
                .to_versioned_tsv(),
        )
        .unwrap();
        assert_tip_a(
            run(vec![
                "model-optimize".to_string(),
                "--tree".to_string(),
                temp.tree_path.to_string_lossy().into_owned(),
                "--ranges".to_string(),
                temp.ranges_path.to_string_lossy().into_owned(),
                "--parameters".to_string(),
                free_parameters_path.to_string_lossy().into_owned(),
                "--dispersal-strata".to_string(),
                strata_path.to_string_lossy().into_owned(),
                "--max-iterations".to_string(),
                "1".to_string(),
            ])
            .unwrap_err(),
        );

        let request_path = temp.dir.join("constraint-analysis.tsv");
        fs::write(
            &request_path,
            "key\tvalue\n\
             format\tbiogeo-analysis-request-v1\n\
             mode\tevaluate\n\
             tree\ttree.nwk\n\
             observation\texact_ranges\n\
             ranges\tranges.tsv\n\
             parameters\tfixed-parameters.tsv\n\
             dispersal_strata\texplicit-strata.tsv\n\
             max_range_size\tauto\n\
             include_null_range\tfalse\n\
             root_prior\tflat\n\
             min_branch_length\t0\n\
             ancestral_probabilities\tfalse\n\
             split_probabilities\tfalse\n",
        )
        .unwrap();
        assert_tip_a(
            run(vec![
                "analysis-plan".to_string(),
                "--request".to_string(),
                request_path.to_string_lossy().into_owned(),
            ])
            .unwrap_err(),
        );
    }

    #[test]
    fn runs_fixed_profiles_and_all_optimizers_with_raw_anagenetic_strata() {
        let temp =
            TempInputs::new_with_contents("(A:1,B:1);\n", "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n");
        let strata_path = write_raw_anagenetic_strata(&temp);
        let common = vec![
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--dispersal-strata".to_string(),
            strata_path.to_string_lossy().into_owned(),
        ];

        let mut fixed_args = vec!["dec".to_string()];
        fixed_args.extend(common.clone());
        fixed_args.extend([
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--distance-exponent".to_string(),
            "-0.5".to_string(),
            "--environment-distance-exponent".to_string(),
            "0.25".to_string(),
            "--area-exponent".to_string(),
            "-0.5".to_string(),
        ]);
        let fixed = run(fixed_args).unwrap();
        assert!(fixed.contains(&format!(
            "dispersal_multipliers\tstratified:{}\n",
            strata_path.display()
        )));
        assert!(fixed.contains("distance_exponent\t-0.5\n"));

        let mut de_args = vec!["dec-optimize".to_string()];
        de_args.extend(common.clone());
        de_args.extend([
            "--distance-exponent".to_string(),
            "-0.5".to_string(),
            "--environment-distance-exponent".to_string(),
            "0.25".to_string(),
            "--area-exponent".to_string(),
            "-0.5".to_string(),
        ]);
        assert!(run(de_args).unwrap().contains("mode\toptimize\n"));

        for (command, fixed_options, parameter) in [
            (
                "dec-x-optimize",
                vec![
                    "--environment-distance-exponent",
                    "0.25",
                    "--area-exponent",
                    "-0.5",
                ],
                "x",
            ),
            (
                "dec-n-optimize",
                vec!["--distance-exponent", "-0.5", "--area-exponent", "-0.5"],
                "n",
            ),
            (
                "dec-u-optimize",
                vec![
                    "--distance-exponent",
                    "-0.5",
                    "--environment-distance-exponent",
                    "0.25",
                ],
                "u",
            ),
        ] {
            let mut args = vec![command.to_string()];
            args.extend(common.clone());
            args.extend(
                fixed_options
                    .into_iter()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            );
            args.extend([
                "--multi-start-points".to_string(),
                "1".to_string(),
                "--max-iterations".to_string(),
                "80".to_string(),
            ]);
            let output = run(args).unwrap();
            assert!(output.contains(&format!("exponent_parameter\t{parameter}\n")));
        }

        let mut joint_args = vec!["dec-xnu-optimize".to_string()];
        joint_args.extend(common.clone());
        joint_args.extend([
            "--multi-start-points".to_string(),
            "1".to_string(),
            "--max-iterations".to_string(),
            "100".to_string(),
        ]);
        let joint = run(joint_args).unwrap();
        assert!(joint.contains("optimization_parameters\td,e,x,n,u\n"));

        let mut profile_args = vec!["dec-xn-profile".to_string()];
        profile_args.extend(common);
        profile_args.extend([
            "--area-exponent".to_string(),
            "-0.5".to_string(),
            "--x-min".to_string(),
            "-1".to_string(),
            "--x-max".to_string(),
            "0".to_string(),
            "--x-points".to_string(),
            "2".to_string(),
            "--n-min".to_string(),
            "0".to_string(),
            "--n-max".to_string(),
            "1".to_string(),
            "--n-points".to_string(),
            "2".to_string(),
            "--multi-start-points".to_string(),
            "1".to_string(),
            "--max-iterations".to_string(),
            "30".to_string(),
        ]);
        assert!(run(profile_args).unwrap().contains("mode\tpair-profile\n"));
    }

    #[test]
    fn runs_fixed_model_with_custom_range_size_constraints() {
        let temp = TempInputs::new();

        let output = run(vec![
            "dec".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--d".to_string(),
            "0.1".to_string(),
            "--e".to_string(),
            "0.2".to_string(),
            "--mx01".to_string(),
            "0.5".to_string(),
            "--mx01y".to_string(),
            "0.8".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("mx01y\t0.8\n"));
        assert!(output.contains("mx01s\t0.5\n"));
        assert!(output.contains("mx01v\t0.5\n"));
        assert!(output.contains("mx01j\t0.5\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_dec_optimize_from_files() {
        let temp = TempInputs::new();

        let output = run(vec![
            "dec-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "10".to_string(),
            "--mx01".to_string(),
            "0.5".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDEC\n"));
        assert!(output.contains("mode\toptimize\n"));
        assert!(output.contains("lnL\t"));
        assert!(output.contains("d\t"));
        assert!(output.contains("e\t"));
        assert!(output.contains("evaluations\t"));
        assert!(output.contains("starts\t"));
        assert!(output.contains("mx01v\t0.5\n"));
        assert!(output.contains("ancestral_state_probabilities\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_divalike_optimize_from_files() {
        let temp = TempInputs::new();

        let output = run(vec![
            "divalike-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "10".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDIVALIKE\n"));
        assert!(output.contains("mode\toptimize\n"));
        assert!(output.contains("mx01v\t0.5\n"));
        assert!(output.contains("ancestral_state_probabilities\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_bayarealike_optimize_from_files() {
        let temp = TempInputs::new_with_ranges("tip\tAreaA\tAreaB\nA\t1\t0\nB\t1\t0\n");

        let output = run(vec![
            "bayarealike-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "10".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tBAYAREALIKE\n"));
        assert!(output.contains("mode\toptimize\n"));
        assert!(output.contains("mx01y\t0.9999\n"));
        assert!(output.contains("ancestral_state_probabilities\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_decj_optimize_from_files() {
        let temp = TempInputs::new();
        let matrix_path = temp.dir.join("directional.tsv");
        fs::write(
            &matrix_path,
            "from\tAreaA\tAreaB\nAreaA\t1\t0.25\nAreaB\t2\t1\n",
        )
        .unwrap();

        let output = run(vec![
            "decj-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--dispersal-multipliers".to_string(),
            matrix_path.to_string_lossy().into_owned(),
            "--max-iterations".to_string(),
            "10".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDEC+J\n"));
        assert!(output.contains("mode\toptimize\n"));
        assert!(output.contains("lnL\t"));
        assert!(output.contains("d\t"));
        assert!(output.contains("e\t"));
        assert!(output.contains("j\t"));
        assert!(output.contains("init_j\t"));
        assert!(output.contains("min_j\t"));
        assert!(output.contains("max_j\t"));
        assert!(output.contains(&format!(
            "dispersal_multipliers\t{}\n",
            matrix_path.display()
        )));
        assert!(output.contains("ancestral_state_probabilities\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_joint_xnu_optimization_from_files() {
        let temp = TempInputs::new_with_contents(
            "((A:0.2,B:0.3):0.4,C:0.5);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
        );
        let distance_path = temp.dir.join("joint-distance.tsv");
        let environment_path = temp.dir.join("joint-environment.tsv");
        let area_sizes_path = temp.dir.join("joint-areas.tsv");
        fs::write(
            &distance_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t2\nAreaB\t3\t0\n",
        )
        .unwrap();
        fs::write(
            &environment_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t0.5\nAreaB\t1.5\t0\n",
        )
        .unwrap();
        fs::write(&area_sizes_path, "area\tsize\nAreaA\t0.5\nAreaB\t2\n").unwrap();

        let output = run(vec![
            "dec-xnu-optimize".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--distance-matrix".to_string(),
            distance_path.to_string_lossy().into_owned(),
            "--environment-distance-matrix".to_string(),
            environment_path.to_string_lossy().into_owned(),
            "--area-sizes".to_string(),
            area_sizes_path.to_string_lossy().into_owned(),
            "--include-null-range".to_string(),
            "--max-iterations".to_string(),
            "80".to_string(),
            "--ancestral-probs".to_string(),
            "--split-probs".to_string(),
        ])
        .unwrap();

        assert!(output.contains("optimization_parameters\td,e,x,n,u\n"));
        assert!(output.contains("x_bound\t"));
        assert!(output.contains("n_bound\t"));
        assert!(output.contains("u_bound\t"));
        assert!(output.contains("starts\t1\n"));
        assert!(output.contains("ancestral_state_probabilities\n"));
        assert!(output.contains("split_scenario_probabilities\n"));
    }

    #[test]
    fn runs_pair_profile_from_files() {
        let temp = TempInputs::new_with_contents(
            "(A:0.4,B:0.6);\n",
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n",
        );
        let distance_path = temp.dir.join("distance.tsv");
        let environment_path = temp.dir.join("environment.tsv");
        let area_sizes_path = temp.dir.join("areas.tsv");
        fs::write(
            &distance_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t2\nAreaB\t3\t0\n",
        )
        .unwrap();
        fs::write(
            &environment_path,
            "from\tAreaA\tAreaB\nAreaA\t0\t0.5\nAreaB\t1.5\t0\n",
        )
        .unwrap();
        fs::write(&area_sizes_path, "area\tsize\nAreaA\t0.5\nAreaB\t2\n").unwrap();

        let output = run(vec![
            "dec-xu-profile".to_string(),
            "--tree".to_string(),
            temp.tree_path.to_string_lossy().into_owned(),
            "--ranges".to_string(),
            temp.ranges_path.to_string_lossy().into_owned(),
            "--distance-matrix".to_string(),
            distance_path.to_string_lossy().into_owned(),
            "--environment-distance-matrix".to_string(),
            environment_path.to_string_lossy().into_owned(),
            "--environment-distance-exponent".to_string(),
            "0.25".to_string(),
            "--area-sizes".to_string(),
            area_sizes_path.to_string_lossy().into_owned(),
            "--x-min".to_string(),
            "-1".to_string(),
            "--x-max".to_string(),
            "0".to_string(),
            "--x-points".to_string(),
            "2".to_string(),
            "--u-min".to_string(),
            "-0.5".to_string(),
            "--u-max".to_string(),
            "0.5".to_string(),
            "--u-points".to_string(),
            "2".to_string(),
            "--max-iterations".to_string(),
            "20".to_string(),
            "--multi-start-points".to_string(),
            "1".to_string(),
        ])
        .unwrap();

        assert!(output.contains("model\tDEC\nmode\tpair-profile\n"));
        assert!(output.contains("first_parameter\tx\n"));
        assert!(output.contains("second_parameter\tu\n"));
        assert!(output.contains("fixed_parameter\tn\n"));
        assert!(output.contains("total_points\t4\n"));
        assert!(output.contains("profile_points\nx\tn\tu\td\te\tlnL\tdelta_lnL\tfinite\t"));
    }

    struct TempInputs {
        dir: PathBuf,
        tree_path: PathBuf,
        ranges_path: PathBuf,
    }

    fn write_raw_anagenetic_strata(temp: &TempInputs) -> PathBuf {
        for (name, values) in [
            ("young-manual.tsv", "AreaA\t1\t1\nAreaB\t1\t1\n"),
            ("old-manual.tsv", "AreaA\t1\t0.5\nAreaB\t0.75\t1\n"),
            ("young-distance.tsv", "AreaA\t0\t2\nAreaB\t4\t0\n"),
            ("old-distance.tsv", "AreaA\t0\t3\nAreaB\t5\t0\n"),
            ("young-environment.tsv", "AreaA\t0\t1.5\nAreaB\t2.5\t0\n"),
            ("old-environment.tsv", "AreaA\t0\t2\nAreaB\t3\t0\n"),
        ] {
            fs::write(temp.dir.join(name), format!("from\tAreaA\tAreaB\n{values}")).unwrap();
        }
        fs::write(
            temp.dir.join("young-area.tsv"),
            "area\tsize\nAreaA\t0.5\nAreaB\t2\n",
        )
        .unwrap();
        fs::write(
            temp.dir.join("old-area.tsv"),
            "area\tsize\nAreaA\t1.5\nAreaB\t0.75\n",
        )
        .unwrap();
        let path = temp.dir.join("raw-strata.tsv");
        fs::write(
            &path,
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\n\
             0.4\tyoung-manual.tsv\tyoung-distance.tsv\tyoung-environment.tsv\tyoung-area.tsv\n\
             1.0\told-manual.tsv\told-distance.tsv\told-environment.tsv\told-area.tsv\n",
        )
        .unwrap();
        path
    }

    impl TempInputs {
        fn new() -> Self {
            Self::new_with_ranges("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n")
        }

        fn new_with_ranges(ranges: &str) -> Self {
            Self::new_with_contents("(A:0,B:0);\n", ranges)
        }

        fn new_with_contents(tree: &str, ranges: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let sequence = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let dir = env::temp_dir().join(format!(
                "biogeo-cli-test-{}-{unique}-{sequence}",
                process::id()
            ));
            fs::create_dir(&dir).unwrap();

            let tree_path = dir.join("tree.nwk");
            let ranges_path = dir.join("ranges.tsv");
            fs::write(&tree_path, tree).unwrap();
            fs::write(&ranges_path, ranges).unwrap();

            Self {
                dir,
                tree_path,
                ranges_path,
            }
        }
    }

    impl Drop for TempInputs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}
