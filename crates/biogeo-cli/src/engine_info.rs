use std::fmt::Write as _;

use crate::cli_help;

pub const ENGINE_CAPABILITIES_FORMAT: &str = "biogeo-engine-capabilities-v1";
pub const SCHEMA_REGISTRY_FORMAT: &str = "biogeo-schema-registry-v1";
pub const COMPATIBILITY_POLICY_VERSION: &str = "biogeo-compatibility-policy-v1";

pub const PUBLIC_FORMATS: &[&str] = &[
    "biogeo-analysis-result-v2",
    "biogeo-input-bundle-v1",
    "biogeo-analysis-result-inspection-v1",
    "biogeo-input-bundle-inspection-v1",
    "biogeo-bsm-inspection-v1",
    "biogeo-analysis-result-migration-v1",
    "biogeo-cli-error-v1",
    "biogeo-cli-progress-v1",
    "biogeo-windows-package-v1",
    "biogeo-windows-installation-v1",
    "biogeo-windows-package-v3",
    "biogeo-windows-installation-v3",
    "biogeo-fossil-placement-set-v1",
    "biogeo-model-comparison-v3",
    "biogeo-model-averaged-ancestral-ranges-v2",
    "biogeo-analysis-request-v1",
    "biogeo-analysis-template-v1",
    "biogeo-analysis-plan-v1",
    "biogeo-analysis-run-v1",
    "biogeo-analysis-run-v2",
    "biogeo-analysis-workflow-v1",
    "biogeo-model-workflow-request-v1",
    "biogeo-model-workflow-plan-v1",
    "biogeo-model-workflow-run-v1",
    "biogeo-model-workflow-result-v1",
    ENGINE_CAPABILITIES_FORMAT,
    "biogeo-bsm-full-tsv-v2",
    "biogeo-bsm-full-sharded-tsv-v2",
    "biogeo-bsm-compact-tsv-v2",
    "biogeo-bsm-compact-sharded-tsv-v2",
    "biogeo-bsm-summary-tsv-v2",
    "biogeo-bsm-summary-sharded-tsv-v2",
];

const PRESETS: &str = "dec,dec+j,divalike,divalike+j,bayarealike,bayarealike+j";
const BSM_DIRECTORY_FORMATS: &str = "biogeo-bsm-tsv-v1,biogeo-bsm-sharded-tsv-v1,biogeo-bsm-full-tsv-v2,biogeo-bsm-full-sharded-tsv-v2,biogeo-bsm-compact-tsv-v2,biogeo-bsm-compact-sharded-tsv-v2,biogeo-bsm-summary-tsv-v2,biogeo-bsm-summary-sharded-tsv-v2";
const DEPRECATED_FORMATS: &str = "biogeo-analysis-result-v1,biogeo-bsm-tsv-v1,biogeo-bsm-sharded-tsv-v1,biogeo-windows-package-v1,biogeo-windows-installation-v1";

pub fn version_output() -> String {
    format!("biogeo-cli {}\n", env!("CARGO_PKG_VERSION"))
}

pub fn capabilities_output() -> String {
    let available_parallelism = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    let build_profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut output = String::new();
    writeln!(output, "format\t{ENGINE_CAPABILITIES_FORMAT}").unwrap();
    output.push_str("status\tready\n");
    output.push_str("engine\tbiogeo-cli\n");
    writeln!(output, "engine_version\t{}", env!("CARGO_PKG_VERSION")).unwrap();
    writeln!(
        output,
        "compatibility_policy_version\t{COMPATIBILITY_POLICY_VERSION}"
    )
    .unwrap();
    output.push_str("format_compatibility_policy\tstrict_versioned_schema\n");
    output.push_str("unknown_format_policy\treject\n");
    output.push_str("unknown_field_policy\treject\n");
    output.push_str("minimum_deprecation_minor_releases\t1\n");
    writeln!(output, "deprecated_formats\t{DEPRECATED_FORMATS}").unwrap();
    output.push_str("deprecated_commands\tnone\n");
    writeln!(output, "build_os\t{}", std::env::consts::OS).unwrap();
    writeln!(output, "build_arch\t{}", std::env::consts::ARCH).unwrap();
    writeln!(output, "build_family\t{}", std::env::consts::FAMILY).unwrap();
    writeln!(output, "build_profile\t{build_profile}").unwrap();
    writeln!(output, "pointer_width\t{}", usize::BITS).unwrap();
    writeln!(output, "available_parallelism\t{available_parallelism}").unwrap();
    writeln!(output, "schema_registry_format\t{SCHEMA_REGISTRY_FORMAT}").unwrap();
    writeln!(output, "public_format_count\t{}", PUBLIC_FORMATS.len()).unwrap();
    writeln!(output, "public_formats\t{}", PUBLIC_FORMATS.join(",")).unwrap();
    writeln!(output, "supported_presets\t{PRESETS}").unwrap();
    writeln!(
        output,
        "recommended_commands\t{}",
        cli_help::RECOMMENDED_COMMANDS.join(",")
    )
    .unwrap();
    writeln!(
        output,
        "compatibility_commands\t{}",
        cli_help::COMPATIBILITY_COMMANDS.join(",")
    )
    .unwrap();
    output.push_str("tree_input_formats\tnewick,nexus\n");
    output.push_str("range_input_formats\tcanonical_tsv,lagrange_data,rasp_csv\n");
    output.push_str("tip_observation_models\texact_ranges,ambiguous_ranges,mf_dp_fdp_detection\n");
    output.push_str("bsm_output_levels\tlegacy,full,compact,summary\n");
    writeln!(output, "bsm_directory_formats\t{BSM_DIRECTORY_FORMATS}").unwrap();
    output.push_str("supports_parameter_table\ttrue\n");
    output.push_str("supports_parameter_optimization\ttrue\n");
    output.push_str("supports_ancestral_probabilities\ttrue\n");
    output.push_str("supports_split_probabilities\ttrue\n");
    output.push_str("supports_model_batch\ttrue\n");
    output.push_str("supports_dataset_batch\ttrue\n");
    output.push_str("supports_model_comparison\ttrue\n");
    output.push_str("supports_model_averaging\ttrue\n");
    output.push_str("supports_fossil_placement\ttrue\n");
    output.push_str("supports_bsm\ttrue\n");
    output.push_str("supports_bsm_resume\ttrue\n");
    output.push_str("supports_bsm_sharding\ttrue\n");
    output.push_str("supports_analysis_workflow\ttrue\n");
    output.push_str("supports_subcommand_help\ttrue\n");
    writeln!(
        output,
        "supports_windows_process_telemetry\t{}",
        cfg!(windows)
    )
    .unwrap();
    output.push_str("supports_linux_resource_detection\tfalse\n");
    output.push_str("supports_slurm_resource_detection\tfalse\n");
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn public_capability_formats_are_unique_and_self_describing() {
        let unique = PUBLIC_FORMATS.iter().copied().collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), PUBLIC_FORMATS.len());
        assert!(unique.contains(ENGINE_CAPABILITIES_FORMAT));
        assert_eq!(PUBLIC_FORMATS.len(), 32);
    }

    #[test]
    fn outputs_are_stable_key_value_records() {
        assert_eq!(
            version_output(),
            format!("biogeo-cli {}\n", env!("CARGO_PKG_VERSION"))
        );
        let output = capabilities_output();
        assert!(output.starts_with("format\tbiogeo-engine-capabilities-v1\nstatus\tready\n"));
        assert!(output.contains(
            "supported_presets\tdec,dec+j,divalike,divalike+j,bayarealike,bayarealike+j\n"
        ));
        assert!(output.contains("public_format_count\t32\n"));
        assert!(output.contains("compatibility_policy_version\tbiogeo-compatibility-policy-v1\n"));
        assert!(output.contains("supports_subcommand_help\ttrue\n"));
    }
}
