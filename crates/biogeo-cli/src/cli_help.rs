use std::fmt::Write as _;

pub const RECOMMENDED_COMMANDS: &[&str] = &[
    "engine-info",
    "convert-tree",
    "convert-ranges",
    "convert-biogeobears-strata",
    "fossil-place",
    "validate-inputs",
    "parameter-template",
    "analysis-template",
    "analysis-plan",
    "analysis-run",
    "analysis-workflow",
    "model-workflow-plan",
    "model-workflow",
    "model-evaluate",
    "model-optimize",
    "model-batch",
    "dataset-batch",
    "model-bsm",
    "bsm-inspect",
    "analysis-result-inspect",
    "analysis-result-migrate",
    "input-bundle-inspect",
];

pub const COMPATIBILITY_COMMANDS: &[&str] = &[
    "dec",
    "divalike",
    "bayarealike",
    "dec-optimize",
    "divalike-optimize",
    "bayarealike-optimize",
    "dec-x-optimize",
    "dec-n-optimize",
    "dec-u-optimize",
    "dec-xnu-optimize",
    "dec-xn-profile",
    "dec-xu-profile",
    "dec-nu-profile",
    "decj-optimize",
    "divalikej-optimize",
    "bayarealikej-optimize",
];

const TREE: &[&str] = &["--tree", "--tree-name", "--min-branch-length"];
const TREE_WITH_FILL: &[&str] = &[
    "--tree",
    "--tree-name",
    "--min-branch-length",
    "--fill-missing-branch-length",
];
const RANGE_OBSERVATIONS: &[&str] = &["--ranges", "--use-ambiguities"];
const DETECTION_OBSERVATIONS: &[&str] = &["--use-detection-model", "--detections", "--controls"];
const STATE_SPACE: &[&str] = &[
    "--max-range-size",
    "--max-states",
    "--include-null-range",
    "--root-prior",
];
const MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--distance-exponent",
    "--environment-distance-matrix",
    "--environment-distance-exponent",
    "--extirpation-multipliers",
    "--area-sizes",
    "--area-exponent",
];
const RAW_PARAMETER_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--environment-distance-matrix",
    "--extirpation-multipliers",
    "--area-sizes",
];
const X_OPTIMIZATION_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--environment-distance-matrix",
    "--environment-distance-exponent",
    "--extirpation-multipliers",
    "--area-sizes",
    "--area-exponent",
];
const N_OPTIMIZATION_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--distance-exponent",
    "--environment-distance-matrix",
    "--extirpation-multipliers",
    "--area-sizes",
    "--area-exponent",
];
const U_OPTIMIZATION_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--distance-exponent",
    "--environment-distance-matrix",
    "--environment-distance-exponent",
    "--area-sizes",
];
const XNU_OPTIMIZATION_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--environment-distance-matrix",
    "--area-sizes",
];
const XN_PROFILE_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--environment-distance-matrix",
    "--area-sizes",
    "--area-exponent",
];
const XU_PROFILE_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--environment-distance-matrix",
    "--environment-distance-exponent",
    "--area-sizes",
];
const NU_PROFILE_MODIFIERS: &[&str] = &[
    "--dispersal-multipliers",
    "--dispersal-strata",
    "--distance-matrix",
    "--distance-exponent",
    "--environment-distance-matrix",
    "--area-sizes",
];
const XN_PROFILE_AXES: &[&str] = &[
    "--x-min",
    "--x-max",
    "--x-points",
    "--n-min",
    "--n-max",
    "--n-points",
    "--support-delta",
];
const XU_PROFILE_AXES: &[&str] = &[
    "--x-min",
    "--x-max",
    "--x-points",
    "--u-min",
    "--u-max",
    "--u-points",
    "--support-delta",
];
const NU_PROFILE_AXES: &[&str] = &[
    "--n-min",
    "--n-max",
    "--n-points",
    "--u-min",
    "--u-max",
    "--u-points",
    "--support-delta",
];
const SPLIT_CONTROLS: &[&str] = &["--mx01", "--mx01y", "--mx01s", "--mx01v", "--mx01j"];
const POSTERIORS: &[&str] = &["--ancestral-probs", "--split-probs"];
const DE_OPTIMIZATION: &[&str] = &[
    "--init-d",
    "--init-e",
    "--min-rate",
    "--max-rate",
    "--initial-log-step",
    "--tolerance",
    "--max-iterations",
    "--multi-start-points",
];
const GENERIC_OPTIMIZATION: &[&str] = &[
    "--initial-step",
    "--tolerance",
    "--max-iterations",
    "--additional-start",
];
const BSM: &[&str] = &[
    "--bsm-samples",
    "--bsm-output-dir",
    "--bsm-output-level",
    "--bsm-threads",
    "--bsm-max-in-flight",
    "--bsm-max-events-per-sample",
    "--bsm-max-events-total",
    "--bsm-memory-budget-mb",
    "--bsm-shard-samples",
    "--bsm-checkpoint-samples",
    "--bsm-resume",
    "--bsm-time-limit-seconds",
    "--bsm-interactive",
    "--seed",
];
const WORKFLOW_BSM: &[&str] = &[
    "--bsm-samples",
    "--bsm-output-level",
    "--bsm-threads",
    "--bsm-max-in-flight",
    "--bsm-max-events-per-sample",
    "--bsm-max-events-total",
    "--bsm-memory-budget-mb",
    "--bsm-shard-samples",
    "--bsm-checkpoint-samples",
    "--bsm-time-limit-seconds",
    "--bsm-interactive",
    "--seed",
];

#[derive(Debug)]
struct HelpSpec {
    usage: String,
    summary: String,
    options: Vec<&'static str>,
    output: String,
    progress: bool,
    cancellation: bool,
    bsm_limits: bool,
    compatibility: bool,
}

pub fn is_known_command(command: &str) -> bool {
    RECOMMENDED_COMMANDS.contains(&command) || COMPATIBILITY_COMMANDS.contains(&command)
}

pub fn render_command_help(command: &str) -> Option<String> {
    let spec = command_spec(command)?;
    let mut output = String::new();
    writeln!(output, "Command: {command}").unwrap();
    writeln!(output, "\nUsage:\n  {}", spec.usage).unwrap();
    writeln!(output, "\nSummary:\n  {}", spec.summary).unwrap();
    output.push_str("\nOptions:\n");
    for option in &spec.options {
        let (syntax, description) = option_help(option)
            .unwrap_or_else(|| panic!("missing help catalog entry for {option}"));
        writeln!(output, "  {syntax:<39} {description}").unwrap();
    }
    output.push_str("  -h, --help                              Show help for this command.\n");
    output.push_str("\nGlobal prefix options:\n");
    output.push_str(
        "  --error-format <human|tsv>              Error output; default human. Place before the command.\n",
    );
    if spec.progress {
        output.push_str(
            "  --progress-format <none|tsv>            Live progress; default none. Place before the command.\n",
        );
    }
    writeln!(output, "\nOutput:\n  {}", spec.output).unwrap();
    output.push_str("\nExit codes:\n");
    output.push_str("  0    Completed successfully.\n");
    output.push_str("  2    Invalid arguments, input, configuration, I/O, or analysis failure.\n");
    if spec.bsm_limits {
        output.push_str("  3    Biogeographic stochastic-history event budget reached.\n");
        output.push_str("  124  Biogeographic stochastic-history time limit reached.\n");
    }
    if spec.cancellation || spec.bsm_limits {
        output.push_str("  130  Task cancelled cooperatively.\n");
    }
    if spec.compatibility {
        output.push_str(
            "\nCompatibility:\n  Supported low-level entry point. New integrations should prefer versioned analysis requests.\n",
        );
    }
    Some(output)
}

fn command_spec(command: &str) -> Option<HelpSpec> {
    let spec = match command {
        "engine-info" => HelpSpec {
            usage: "biogeo-cli engine-info".to_string(),
            summary: "Report the executable version, platform, public schemas, commands, and supported scientific capabilities.".to_string(),
            options: vec![],
            output: "Versioned key/value TSV: biogeo-engine-capabilities-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "convert-tree" => HelpSpec {
            usage: "biogeo-cli convert-tree --tree <tree.newick|tree.nex> [--tree-name <name>]".to_string(),
            summary: "Parse rooted Newick or NEXUS input and emit canonical Newick.".to_string(),
            options: vec!["--tree", "--tree-name", "--fill-missing-branch-length"],
            output: "Canonical rooted Newick on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "convert-ranges" => HelpSpec {
            usage: "biogeo-cli convert-ranges --ranges <ranges.data|ranges.csv> [options]".to_string(),
            summary: "Convert LAGRANGE/BioGeoBEARS geography or a RASP-style CSV matrix to canonical range TSV.".to_string(),
            options: vec![
                "--ranges",
                "--input-format",
                "--taxon-column",
                "--taxon-map",
                "--area-map",
            ],
            output: "Canonical biogeo-range-table-v1 TSV on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "convert-biogeobears-strata" => HelpSpec {
            usage: "biogeo-cli convert-biogeobears-strata --time-boundaries <file> [matrix options] --output-dir <dir>".to_string(),
            summary: "Convert BioGeoBEARS time boundaries and block matrices into a portable stratified-input directory.".to_string(),
            options: vec![
                "--time-boundaries",
                "--dispersal-matrices",
                "--adjacency-matrices",
                "--adjacency-range-rule",
                "--max-range-size",
                "--output-dir",
            ],
            output: "biogeo-biogeobears-strata-import-v1 summary plus a new portable strata directory.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "fossil-place" => HelpSpec {
            usage: "biogeo-cli fossil-place --tree <tree> --manifest <fossils.tsv> --output-dir <dir> [options]".to_string(),
            summary: "Generate reproducible fossil placements under age, stem/crown, and clade constraints.".to_string(),
            options: vec![
                "--tree",
                "--tree-name",
                "--manifest",
                "--output-dir",
                "--replicates",
                "--seed",
                "--direct-ancestor-hook-length",
            ],
            output: "New biogeo-fossil-placement-set-v1 directory and versioned completion summary.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "validate-inputs" => HelpSpec {
            usage: "biogeo-cli validate-inputs --tree <tree> --ranges <ranges> [options]".to_string(),
            summary: "Validate tree/range correspondence, sampling ages, ambiguity constraints, and direct-ancestor hooks before analysis.".to_string(),
            options: options(&[
                TREE_WITH_FILL,
                RANGE_OBSERVATIONS,
                &["--tip-age-tolerance"],
            ]),
            output: "Versioned key/value TSV: biogeo-input-validation-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "parameter-template" => HelpSpec {
            usage: "biogeo-cli parameter-template --preset <preset>".to_string(),
            summary: "Emit a complete BioGeoBEARS-like parameter table for one of the six presets.".to_string(),
            options: vec!["--preset"],
            output: "Versioned biogeo-parameter-table-v1 TSV on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "analysis-template" => HelpSpec {
            usage: "biogeo-cli analysis-template --preset <preset> --mode <evaluate|optimize> --output-dir <dir>".to_string(),
            summary: "Create a non-overwriting request directory containing editable analysis and parameter templates.".to_string(),
            options: vec!["--preset", "--mode", "--output-dir"],
            output: "biogeo-analysis-template-v1 summary and a biogeo-analysis-request-v1 template directory.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "analysis-plan" => HelpSpec {
            usage: "biogeo-cli analysis-plan --request <analysis.tsv>".to_string(),
            summary: "Fully parse and preflight a versioned request without running likelihood optimization.".to_string(),
            options: vec!["--request"],
            output: "Versioned key/value TSV: biogeo-analysis-plan-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "analysis-run" => HelpSpec {
            usage: "biogeo-cli analysis-run --request <analysis.tsv> --output-dir <dir>".to_string(),
            summary: "Evaluate or optimize one versioned request and write a portable fitted-model result.".to_string(),
            options: vec!["--request", "--output-dir"],
            output: "biogeo-analysis-run-v2 summary and a new biogeo-analysis-result-v2 directory.".to_string(),
            progress: true,
            cancellation: true,
            bsm_limits: false,
            compatibility: false,
        },
        "analysis-workflow" => HelpSpec {
            usage: "biogeo-cli analysis-workflow --request <analysis.tsv> --output-dir <dir> --bsm-samples <n> [options]".to_string(),
            summary: "Run request preflight, fitting, streamed biogeographic stochastic histories, and result inspection as one resumable task.".to_string(),
            options: options(&[
                &["--request", "--output-dir", "--resume", "--deep"],
                WORKFLOW_BSM,
            ]),
            output: "biogeo-analysis-workflow-v1 summary with authoritative analysis-result/ and bsm-result/ subdirectories.".to_string(),
            progress: true,
            cancellation: true,
            bsm_limits: true,
            compatibility: false,
        },
        "model-workflow-plan" => HelpSpec {
            usage: "biogeo-cli model-workflow-plan --request <workflow.tsv>".to_string(),
            summary: "Preflight one versioned multi-model request, every candidate parameter table, and its optional stochastic-history resource plan without fitting models.".to_string(),
            options: vec!["--model-workflow-request"],
            output: "Versioned key/value TSV: biogeo-model-workflow-plan-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "model-workflow" => HelpSpec {
            usage: "biogeo-cli model-workflow --request <workflow.tsv> --output-dir <dir> [--resume]".to_string(),
            summary: "Fit and compare candidate models, model-average ancestral results, and optionally sample stochastic histories from an explicitly selected model.".to_string(),
            options: vec!["--model-workflow-request", "--output-dir", "--resume"],
            output: "biogeo-model-workflow-run-v1 summary and a resumable biogeo-model-workflow-result-v1 directory.".to_string(),
            progress: true,
            cancellation: true,
            bsm_limits: true,
            compatibility: false,
        },
        "model-evaluate" | "model-optimize" => {
            let optimize = command == "model-optimize";
            let mut groups: Vec<&[&str]> = vec![
                TREE_WITH_FILL,
                RANGE_OBSERVATIONS,
                DETECTION_OBSERVATIONS,
                &["--parameters", "--analysis-result-dir"],
                STATE_SPACE,
                RAW_PARAMETER_MODIFIERS,
                POSTERIORS,
            ];
            if optimize {
                groups.push(GENERIC_OPTIMIZATION);
            }
            HelpSpec {
                usage: format!(
                    "biogeo-cli {command} --tree <tree> (--ranges <ranges> | --use-detection-model --detections <counts> --controls <counts>) --parameters <parameters.tsv> [options]"
                ),
                summary: if optimize {
                    "Optimize every free parameter in a versioned table through the unified likelihood engine."
                } else {
                    "Evaluate a fully fixed versioned parameter table through the unified likelihood engine."
                }
                .to_string(),
                options: options(&groups),
                output: "Likelihood summary on stdout; --analysis-result-dir additionally writes biogeo-analysis-result-v2.".to_string(),
                progress: optimize,
                cancellation: optimize,
                bsm_limits: false,
                compatibility: false,
            }
        }
        "model-batch" => HelpSpec {
            usage: "biogeo-cli model-batch --manifest <models.tsv> --output-dir <dir> --tree <tree> (--ranges <ranges> | detection options) [options]".to_string(),
            summary: "Fit a manifest of parameter tables to one dataset, compare models, and optionally model-average ancestral results.".to_string(),
            options: options(&[
                &["--manifest", "--output-dir", "--resume"],
                TREE_WITH_FILL,
                RANGE_OBSERVATIONS,
                DETECTION_OBSERVATIONS,
                STATE_SPACE,
                RAW_PARAMETER_MODIFIERS,
                GENERIC_OPTIMIZATION,
            ]),
            output: "Versioned model-batch directory including attempts, model comparison v3, and requested model averages.".to_string(),
            progress: true,
            cancellation: true,
            bsm_limits: false,
            compatibility: false,
        },
        "dataset-batch" => HelpSpec {
            usage: "biogeo-cli dataset-batch --manifest <datasets.tsv> --output-dir <dir> [--resume]".to_string(),
            summary: "Run independent model batches across multiple datasets with durable per-dataset outcomes.".to_string(),
            options: vec!["--manifest", "--output-dir", "--resume"],
            output: "Versioned dataset-batch result directory with per-dataset model-batch subdirectories.".to_string(),
            progress: true,
            cancellation: true,
            bsm_limits: false,
            compatibility: false,
        },
        "model-bsm" => HelpSpec {
            usage: "biogeo-cli model-bsm --analysis-result <dir> --bsm-samples <n> [options]".to_string(),
            summary: "Sample full biogeographic stochastic histories from a completed portable fitted-model result.".to_string(),
            options: options(&[&["--analysis-result"], BSM]),
            output: "Sampling summary plus a streamed v1/v2 BSM directory when --bsm-output-dir is supplied.".to_string(),
            progress: false,
            cancellation: true,
            bsm_limits: true,
            compatibility: false,
        },
        "bsm-inspect" => HelpSpec {
            usage: "biogeo-cli bsm-inspect --bsm-result <dir> [--deep]".to_string(),
            summary: "Validate BSM metadata, checkpoints, shards, row counts, event chains, and state constraints.".to_string(),
            options: vec!["--bsm-result", "--deep"],
            output: "Versioned key/value TSV: biogeo-bsm-inspection-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "analysis-result-inspect" => HelpSpec {
            usage: "biogeo-cli analysis-result-inspect --analysis-result <dir> [--replay]".to_string(),
            summary: "Validate a fitted-model result and optionally replay its likelihood identity from bundled inputs.".to_string(),
            options: vec!["--analysis-result", "--replay"],
            output: "Versioned key/value TSV: biogeo-analysis-result-inspection-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "analysis-result-migrate" => HelpSpec {
            usage: "biogeo-cli analysis-result-migrate --analysis-result <v1-dir> --output-dir <v2-dir>".to_string(),
            summary: "Migrate a legacy analysis-result-v1 directory into a portable, non-overwriting v2 result.".to_string(),
            options: vec!["--analysis-result", "--output-dir"],
            output: "biogeo-analysis-result-migration-v1 summary and a new biogeo-analysis-result-v2 directory.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "input-bundle-inspect" => HelpSpec {
            usage: "biogeo-cli input-bundle-inspect --input-bundle <dir>".to_string(),
            summary: "Validate the manifest, paths, file sizes, and fingerprints of a portable input bundle.".to_string(),
            options: vec!["--input-bundle"],
            output: "Versioned key/value TSV: biogeo-input-bundle-inspection-v1.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: false,
        },
        "dec" | "divalike" | "bayarealike" => HelpSpec {
            usage: format!(
                "biogeo-cli {command} --tree <tree> --ranges <ranges> --d <rate> --e <rate> [options]"
            ),
            summary: format!(
                "Evaluate a fixed {}-family model and optionally calculate posteriors or sample histories.",
                family_name(command)
            ),
            options: options(&[
                TREE,
                RANGE_OBSERVATIONS,
                &["--d", "--e", "--j"],
                STATE_SPACE,
                MODIFIERS,
                SPLIT_CONTROLS,
                POSTERIORS,
                &["--traceback-samples"],
                BSM,
            ]),
            output: "Likelihood and requested posterior/history summaries on stdout; optional streamed BSM directory.".to_string(),
            progress: false,
            cancellation: true,
            bsm_limits: true,
            compatibility: true,
        },
        "dec-optimize" | "divalike-optimize" | "bayarealike-optimize" => HelpSpec {
            usage: format!(
                "biogeo-cli {command} --tree <tree> --ranges <ranges> [options]"
            ),
            summary: format!(
                "Optimize d and e for the {} family with the specialized compatibility interface.",
                family_name(command)
            ),
            options: options(&[
                TREE,
                RANGE_OBSERVATIONS,
                STATE_SPACE,
                MODIFIERS,
                SPLIT_CONTROLS,
                POSTERIORS,
                DE_OPTIMIZATION,
            ]),
            output: "Optimized likelihood, parameters, and requested posterior summaries on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: true,
        },
        "dec-x-optimize" | "dec-n-optimize" | "dec-u-optimize" => HelpSpec {
            usage: format!("biogeo-cli {command} --tree <tree> --ranges <ranges> {} [options]", exponent_input_usage(command)),
            summary: format!(
                "Jointly optimize d, e, and the {} exponent using static or stratified raw modifiers.",
                exponent_name(command)
            ),
            options: options(&[
                TREE,
                RANGE_OBSERVATIONS,
                STATE_SPACE,
                exponent_modifier_options(command),
                SPLIT_CONTROLS,
                POSTERIORS,
                DE_OPTIMIZATION,
                &[
                    "--init-exponent",
                    "--min-exponent",
                    "--max-exponent",
                    "--initial-exponent-step",
                ],
            ]),
            output: "Optimized likelihood, d/e, exponent, and requested posteriors on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: true,
        },
        "dec-xnu-optimize" => HelpSpec {
            usage: "biogeo-cli dec-xnu-optimize --tree <tree> --ranges <ranges> (--distance-matrix <path> --environment-distance-matrix <path> --area-sizes <path> | --dispersal-strata <path>) [options]".to_string(),
            summary: "Jointly optimize d, e, geographic distance x, environmental distance n, and area-size u.".to_string(),
            options: options(&[
                TREE,
                RANGE_OBSERVATIONS,
                STATE_SPACE,
                XNU_OPTIMIZATION_MODIFIERS,
                SPLIT_CONTROLS,
                POSTERIORS,
                DE_OPTIMIZATION,
                &[
                    "--init-x",
                    "--min-x",
                    "--max-x",
                    "--initial-x-step",
                    "--init-n",
                    "--min-n",
                    "--max-n",
                    "--initial-n-step",
                    "--init-u",
                    "--min-u",
                    "--max-u",
                    "--initial-u-step",
                ],
            ]),
            output: "Optimized five-dimensional likelihood and parameter summary on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: true,
        },
        "dec-xn-profile" | "dec-xu-profile" | "dec-nu-profile" => HelpSpec {
            usage: format!("biogeo-cli {command} --tree <tree> --ranges <ranges> {} [profile options]", profile_input_usage(command)),
            summary: format!(
                "Profile the {} exponent pair on a fixed grid while optimizing d/e at each point.",
                profile_pair_name(command)
            ),
            options: options(&[
                TREE,
                RANGE_OBSERVATIONS,
                STATE_SPACE,
                profile_modifier_options(command),
                SPLIT_CONTROLS,
                DE_OPTIMIZATION,
                profile_axis_options(command),
            ]),
            output: "Profile grid, optimized d/e values, and likelihood-support classification on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: true,
        },
        "decj-optimize" | "divalikej-optimize" | "bayarealikej-optimize" => HelpSpec {
            usage: format!(
                "biogeo-cli {command} --tree <tree> --ranges <ranges> [options]"
            ),
            summary: format!(
                "Optimize d, e, and founder-event j for the {} family.",
                family_name(command)
            ),
            options: options(&[
                TREE,
                RANGE_OBSERVATIONS,
                STATE_SPACE,
                MODIFIERS,
                SPLIT_CONTROLS,
                POSTERIORS,
                DE_OPTIMIZATION,
                &["--init-j", "--min-j", "--max-j"],
            ]),
            output: "Optimized likelihood, d/e/j, and requested posterior summaries on stdout.".to_string(),
            progress: false,
            cancellation: false,
            bsm_limits: false,
            compatibility: true,
        },
        _ => return None,
    };
    Some(spec)
}

fn options(groups: &[&[&'static str]]) -> Vec<&'static str> {
    groups
        .iter()
        .flat_map(|group| group.iter().copied())
        .collect()
}

fn family_name(command: &str) -> &'static str {
    if command.starts_with("divalike") {
        "DIVALIKE"
    } else if command.starts_with("bayarealike") {
        "BAYAREALIKE"
    } else {
        "DEC"
    }
}

fn exponent_name(command: &str) -> &'static str {
    if command.contains("-x-") {
        "geographic-distance x"
    } else if command.contains("-n-") {
        "environmental-distance n"
    } else {
        "area-size u"
    }
}

fn exponent_input_usage(command: &str) -> &'static str {
    if command.contains("-x-") {
        "(--distance-matrix <path> | --dispersal-strata <path>)"
    } else if command.contains("-n-") {
        "(--environment-distance-matrix <path> | --dispersal-strata <path>)"
    } else {
        "(--area-sizes <path> | --dispersal-strata <path>)"
    }
}

fn exponent_modifier_options(command: &str) -> &'static [&'static str] {
    if command.contains("-x-") {
        X_OPTIMIZATION_MODIFIERS
    } else if command.contains("-n-") {
        N_OPTIMIZATION_MODIFIERS
    } else {
        U_OPTIMIZATION_MODIFIERS
    }
}

fn profile_pair_name(command: &str) -> &'static str {
    if command.contains("-xn-") {
        "x/n"
    } else if command.contains("-xu-") {
        "x/u"
    } else {
        "n/u"
    }
}

fn profile_input_usage(command: &str) -> &'static str {
    if command.contains("-xn-") {
        "(--distance-matrix <path> --environment-distance-matrix <path> --area-sizes <path> | --dispersal-strata <path>) --area-exponent <u>"
    } else if command.contains("-xu-") {
        "(--distance-matrix <path> --environment-distance-matrix <path> --area-sizes <path> | --dispersal-strata <path>) --environment-distance-exponent <n>"
    } else {
        "(--distance-matrix <path> --environment-distance-matrix <path> --area-sizes <path> | --dispersal-strata <path>) --distance-exponent <x>"
    }
}

fn profile_modifier_options(command: &str) -> &'static [&'static str] {
    if command.contains("-xn-") {
        XN_PROFILE_MODIFIERS
    } else if command.contains("-xu-") {
        XU_PROFILE_MODIFIERS
    } else {
        NU_PROFILE_MODIFIERS
    }
}

fn profile_axis_options(command: &str) -> &'static [&'static str] {
    if command.contains("-xn-") {
        XN_PROFILE_AXES
    } else if command.contains("-xu-") {
        XU_PROFILE_AXES
    } else {
        NU_PROFILE_AXES
    }
}

fn option_help(option: &str) -> Option<(&'static str, &'static str)> {
    let value = match option {
        "--tree" => ("--tree <path>", "Rooted Newick or NEXUS tree."),
        "--tree-name" => (
            "--tree-name <name>",
            "Required selection for a multi-tree NEXUS file.",
        ),
        "--min-branch-length" => (
            "--min-branch-length <x>",
            "Direct-ancestor hook threshold; default 0 (disabled).",
        ),
        "--fill-missing-branch-length" => (
            "--fill-missing-branch-length <x>",
            "Explicit fill for omitted non-root branch lengths; default reject.",
        ),
        "--ranges" => (
            "--ranges <path>",
            "Canonical TSV, LAGRANGE .data, or supported CSV matrix.",
        ),
        "--use-ambiguities" => (
            "--use-ambiguities",
            "Interpret '?' as BioGeoBEARS range ambiguity.",
        ),
        "--input-format" => (
            "--input-format <kind>",
            "auto, lagrange, or csv; default auto.",
        ),
        "--taxon-column" => (
            "--taxon-column <name>",
            "CSV taxon column; common names are auto-detected.",
        ),
        "--taxon-map" => (
            "--taxon-map <path>",
            "Explicit source_taxon to target_taxon mapping TSV.",
        ),
        "--area-map" => (
            "--area-map <path>",
            "Explicit source_area to target_area mapping TSV.",
        ),
        "--time-boundaries" => (
            "--time-boundaries <path>",
            "BioGeoBEARS time-period boundary file.",
        ),
        "--dispersal-matrices" => (
            "--dispersal-matrices <path>",
            "Optional block dispersal-multiplier matrices.",
        ),
        "--adjacency-matrices" => (
            "--adjacency-matrices <path>",
            "Optional block area-adjacency matrices.",
        ),
        "--adjacency-range-rule" => (
            "--adjacency-range-rule <kind>",
            "all-pairs or edge-covered; default all-pairs.",
        ),
        "--manifest" => (
            "--manifest <path>",
            "Versioned manifest appropriate to this command.",
        ),
        "--output-dir" => (
            "--output-dir <dir>",
            "New output directory; existing directories are rejected.",
        ),
        "--replicates" => (
            "--replicates <n>",
            "Fossil-placement tree replicates; default 1.",
        ),
        "--direct-ancestor-hook-length" => (
            "--direct-ancestor-hook-length <x>",
            "Fossil hook length; default 1e-7.",
        ),
        "--tip-age-tolerance" => (
            "--tip-age-tolerance <x>",
            "Present-day age tolerance; default root_age*1e-9+1e-12.",
        ),
        "--preset" => (
            "--preset <name>",
            "dec, dec+j, divalike, divalike+j, bayarealike, or bayarealike+j.",
        ),
        "--mode" => ("--mode <evaluate|optimize>", "Analysis request mode."),
        "--request" => (
            "--request <analysis.tsv>",
            "Versioned biogeo-analysis-request-v1 file.",
        ),
        "--model-workflow-request" => (
            "--request <workflow.tsv>",
            "Versioned biogeo-model-workflow-request-v1 file.",
        ),
        "--parameters" => (
            "--parameters <path>",
            "Versioned biogeo-parameter-table-v1 file.",
        ),
        "--analysis-result-dir" => (
            "--analysis-result-dir <dir>",
            "Write a portable, non-overwriting fitted-model result.",
        ),
        "--analysis-result" => (
            "--analysis-result <dir>",
            "Existing completed fitted-model result directory.",
        ),
        "--input-bundle" => (
            "--input-bundle <dir>",
            "Existing portable input-bundle directory.",
        ),
        "--bsm-result" => (
            "--bsm-result <dir>",
            "Existing streamed BSM result directory.",
        ),
        "--replay" => (
            "--replay",
            "Recompute model identity and likelihood from bundled inputs.",
        ),
        "--deep" => (
            "--deep",
            "Scan every stochastic-history row and cross-table invariant.",
        ),
        "--resume" => (
            "--resume",
            "Continue the identical workflow or batch without overwriting completed work.",
        ),
        "--use-detection-model" => (
            "--use-detection-model",
            "Use mf/dp/fdp observation likelihoods instead of fixed ranges.",
        ),
        "--detections" => (
            "--detections <path>",
            "Target-OTU detection counts; --detects is accepted as an alias.",
        ),
        "--controls" => (
            "--controls <path>",
            "Inclusive taphonomic-control counts by tip and area.",
        ),
        "--max-range-size" => (
            "--max-range-size <n>",
            "Maximum occupied areas per state; preset/data default when omitted.",
        ),
        "--max-states" => (
            "--max-states <n>",
            "Reject before state allocation when the combinatorial state count exceeds n.",
        ),
        "--include-null-range" => (
            "--include-null-range",
            "Include the null range as an absorbing state.",
        ),
        "--root-prior" => ("--root-prior <flat|equal>", "Root prior; default flat."),
        "--dispersal-multipliers" => (
            "--dispersal-multipliers <path>",
            "Static directional multiplier matrix for d and j.",
        ),
        "--dispersal-strata" => (
            "--dispersal-strata <path>",
            "Piecewise matrices, raw modifiers, and range constraints.",
        ),
        "--distance-matrix" => (
            "--distance-matrix <path>",
            "Static pairwise geographic distances.",
        ),
        "--distance-exponent" => (
            "--distance-exponent <x>",
            "Fixed geographic-distance exponent x.",
        ),
        "--environment-distance-matrix" => (
            "--environment-distance-matrix <path>",
            "Static pairwise environmental distances.",
        ),
        "--environment-distance-exponent" => (
            "--environment-distance-exponent <n>",
            "Fixed environmental-distance exponent n.",
        ),
        "--extirpation-multipliers" => (
            "--extirpation-multipliers <path>",
            "Static area-specific multipliers applied to e.",
        ),
        "--area-sizes" => ("--area-sizes <path>", "Positive raw area sizes."),
        "--area-exponent" => ("--area-exponent <u>", "Fixed area-size exponent u."),
        "--d" => ("--d <rate>", "Fixed range-expansion/dispersal rate."),
        "--e" => ("--e <rate>", "Fixed local-extinction/extirpation rate."),
        "--j" => ("--j <weight>", "Fixed founder-event weight; default 0."),
        "--mx01" => (
            "--mx01 <value>",
            "Linked daughter-size control; default 0.0001.",
        ),
        "--mx01y" => (
            "--mx01y <value>",
            "Override range-copying daughter-size control.",
        ),
        "--mx01s" => (
            "--mx01s <value>",
            "Override subset-sympatry daughter-size control.",
        ),
        "--mx01v" => (
            "--mx01v <value>",
            "Override vicariance daughter-size control.",
        ),
        "--mx01j" => (
            "--mx01j <value>",
            "Override founder-event daughter-size control.",
        ),
        "--ancestral-probs" => (
            "--ancestral-probs",
            "Calculate internal-node range posteriors.",
        ),
        "--split-probs" => (
            "--split-probs",
            "Calculate cladogenetic split-scenario posteriors.",
        ),
        "--traceback-samples" => (
            "--traceback-samples <n>",
            "Sample n conditional history skeletons; default 0.",
        ),
        "--bsm-samples" => (
            "--bsm-samples <n>",
            "Number of full biogeographic stochastic histories; must be positive.",
        ),
        "--bsm-output-dir" => (
            "--bsm-output-dir <dir>",
            "Stream histories to a new result directory.",
        ),
        "--bsm-output-level" => (
            "--bsm-output-level <level>",
            "legacy, full, compact, or summary; default legacy (workflow: compact).",
        ),
        "--bsm-threads" => (
            "--bsm-threads <auto|n>",
            "Worker count; default auto, capped by sample count.",
        ),
        "--bsm-max-in-flight" => (
            "--bsm-max-in-flight <n>",
            "Ordered result window; default twice the worker count.",
        ),
        "--bsm-max-events-per-sample" => (
            "--bsm-max-events-per-sample <n>",
            "Per-history anagenetic event limit; default unlimited.",
        ),
        "--bsm-max-events-total" => (
            "--bsm-max-events-total <n>",
            "Ordered-task event budget; default unlimited.",
        ),
        "--bsm-memory-budget-mb" => (
            "--bsm-memory-budget-mb <n>",
            "Completed-history memory budget in MiB.",
        ),
        "--bsm-shard-samples" => (
            "--bsm-shard-samples <n>",
            "Samples per fixed output shard; unsharded when omitted.",
        ),
        "--bsm-checkpoint-samples" => (
            "--bsm-checkpoint-samples <n>",
            "Checkpoint interval; adaptive default based on the task window.",
        ),
        "--bsm-resume" => (
            "--bsm-resume",
            "Resume a compatible streamed BSM checkpoint.",
        ),
        "--bsm-time-limit-seconds" => (
            "--bsm-time-limit-seconds <seconds>",
            "Cooperative task time limit; default unlimited.",
        ),
        "--bsm-interactive" => (
            "--bsm-interactive",
            "Read pause, resume, status, and cancel from stdin.",
        ),
        "--seed" => ("--seed <u64>", "Deterministic master seed; default 1."),
        "--init-d" => ("--init-d <rate>", "Initial d; default 0.01."),
        "--init-e" => ("--init-e <rate>", "Initial e; default 0.01."),
        "--init-j" => ("--init-j <weight>", "Initial j; default 0.01."),
        "--min-rate" => (
            "--min-rate <rate>",
            "Lower positive d/e bound; default 1e-12.",
        ),
        "--max-rate" => ("--max-rate <rate>", "Upper d/e bound; default 10."),
        "--min-j" => ("--min-j <weight>", "Lower j bound; default 1e-5."),
        "--max-j" => (
            "--max-j <weight>",
            "Upper j bound; preset-dependent BioGeoBEARS limit.",
        ),
        "--initial-log-step" => (
            "--initial-log-step <x>",
            "Initial log-rate simplex step; default 0.5.",
        ),
        "--initial-step" => (
            "--initial-step <x>",
            "Generic transformed-coordinate simplex step; table-aware default.",
        ),
        "--tolerance" => (
            "--tolerance <x>",
            "Optimization convergence tolerance; default 1e-8.",
        ),
        "--max-iterations" => (
            "--max-iterations <n>",
            "Maximum optimizer iterations; default 200.",
        ),
        "--multi-start-points" => (
            "--multi-start-points <n>",
            "Log-spaced starts per axis; default 1.",
        ),
        "--additional-start" => (
            "--additional-start <v1,v2,...>",
            "Additional free-parameter start; repeatable.",
        ),
        "--init-exponent" => (
            "--init-exponent <x>",
            "Initial optimized x, n, or u; default 0.",
        ),
        "--min-exponent" => (
            "--min-exponent <x>",
            "Lower exponent bound; parameter-specific BioGeoBEARS default.",
        ),
        "--max-exponent" => (
            "--max-exponent <x>",
            "Upper exponent bound; parameter-specific BioGeoBEARS default.",
        ),
        "--initial-exponent-step" => (
            "--initial-exponent-step <x>",
            "Initial exponent simplex step; default 0.5.",
        ),
        "--init-x" => ("--init-x <x>", "Initial geographic exponent; default 0."),
        "--min-x" => ("--min-x <x>", "Lower x bound; default -2.5."),
        "--max-x" => ("--max-x <x>", "Upper x bound; default 2.5."),
        "--initial-x-step" => (
            "--initial-x-step <x>",
            "Initial x simplex step; default 0.5.",
        ),
        "--init-n" => ("--init-n <n>", "Initial environmental exponent; default 0."),
        "--min-n" => ("--min-n <n>", "Lower n bound; default -10."),
        "--max-n" => ("--max-n <n>", "Upper n bound; default 10."),
        "--initial-n-step" => (
            "--initial-n-step <x>",
            "Initial n simplex step; default 0.5.",
        ),
        "--init-u" => ("--init-u <u>", "Initial area-size exponent; default 0."),
        "--min-u" => ("--min-u <u>", "Lower u bound; default -10."),
        "--max-u" => ("--max-u <u>", "Upper u bound; default 10."),
        "--initial-u-step" => (
            "--initial-u-step <x>",
            "Initial u simplex step; default 0.5.",
        ),
        "--x-min" => ("--x-min <x>", "Geographic profile-grid lower bound."),
        "--x-max" => ("--x-max <x>", "Geographic profile-grid upper bound."),
        "--x-points" => (
            "--x-points <n>",
            "Number of x grid values including endpoints.",
        ),
        "--n-min" => ("--n-min <n>", "Environmental profile-grid lower bound."),
        "--n-max" => ("--n-max <n>", "Environmental profile-grid upper bound."),
        "--n-points" => (
            "--n-points <n>",
            "Number of n grid values including endpoints.",
        ),
        "--u-min" => ("--u-min <u>", "Area-size profile-grid lower bound."),
        "--u-max" => ("--u-max <u>", "Area-size profile-grid upper bound."),
        "--u-points" => (
            "--u-points <n>",
            "Number of u grid values including endpoints.",
        ),
        "--support-delta" => (
            "--support-delta <x>",
            "Delta-lnL support cutoff; default 2.995732.",
        ),
        _ => return None,
    };
    Some(value)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn every_advertised_command_has_complete_scoped_help() {
        let commands = RECOMMENDED_COMMANDS
            .iter()
            .chain(COMPATIBILITY_COMMANDS)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            commands.iter().copied().collect::<BTreeSet<_>>().len(),
            commands.len()
        );
        for command in commands {
            let help = render_command_help(command)
                .unwrap_or_else(|| panic!("{command} should have a help specification"));
            assert!(help.starts_with(&format!("Command: {command}\n")));
            assert!(help.contains("\nUsage:\n"));
            assert!(help.contains("\nOutput:\n"));
            assert!(help.contains("\nExit codes:\n"));
        }
    }

    #[test]
    fn help_is_scoped_to_the_selected_command() {
        let bsm = render_command_help("model-bsm").unwrap();
        assert!(bsm.contains("--analysis-result <dir>"));
        assert!(bsm.contains("--bsm-threads <auto|n>"));
        assert!(!bsm.contains("--tree <path>"));
        assert!(!bsm.contains("--d <rate>"));

        let convert = render_command_help("convert-tree").unwrap();
        assert!(convert.contains("--tree <path>"));
        assert!(!convert.contains("--ranges <path>"));
        assert!(!convert.contains("--bsm-samples <n>"));

        let workflow = render_command_help("analysis-workflow").unwrap();
        assert!(workflow.contains("--resume"));
        assert!(!workflow.contains("--bsm-output-dir"));
        assert!(!workflow.contains("--bsm-resume"));

        let model_workflow = render_command_help("model-workflow").unwrap();
        assert!(model_workflow.contains("--request <workflow.tsv>"));
        assert!(!model_workflow.contains("--request <analysis.tsv>"));

        let x = render_command_help("dec-x-optimize").unwrap();
        assert!(x.contains("--distance-matrix <path>"));
        assert!(!x.contains("--distance-exponent <x>"));
        let xn = render_command_help("dec-xn-profile").unwrap();
        assert!(xn.contains("--x-points <n>"));
        assert!(xn.contains("--n-points <n>"));
        assert!(!xn.contains("--u-points <n>"));
    }
}
