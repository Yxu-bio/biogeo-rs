use std::fmt::Write;

pub const FORMAT: &str = "biogeo-optimization-starts-v1";
pub const FILE: &str = "optimization-starts.tsv";

pub fn format(result: &biogeo_core::ParameterOptimizationResult) -> String {
    let mut out = format!(
        "format\t{FORMAT}\n\nstarts\nstage\tstart\tselected\tstop_reason\tlnL\titerations\tevaluations\tfailed_evaluations\tparameter\tinitial\tfinal\tat_boundary\n"
    );
    for start in &result.start_diagnostics {
        let likelihood = start
            .log_likelihood
            .map_or_else(|| "NA".to_string(), |x| format!("{x:.17}"));
        for ((name, initial), (final_name, value)) in
            start.initial_values.iter().zip(&start.final_values)
        {
            debug_assert_eq!(name, final_name);
            writeln!(
                out,
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                start.stage,
                start.start,
                start.selected,
                start.stop_reason.as_str(),
                likelihood,
                start.iterations,
                start.evaluations,
                start.failed_evaluations,
                name,
                initial,
                value,
                start.boundary_parameters.contains(name)
            )
            .unwrap();
        }
    }
    out
}
