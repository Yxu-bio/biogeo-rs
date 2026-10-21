use crate::branch_process::BranchPropagator;
use crate::engine::{LikelihoodEngine, build_cladogenetic_process};
use crate::propagation::{PropagationError, audit_propagation};
use crate::pruning::{
    RootPrior, cladogenetic_node_state_posteriors_by_branch, prune_with_cladogenesis_by_branch,
};
use crate::{
    ModelConfig, SparseQ, StateSpace, parse_newick, parse_parameter_table, parse_tip_ranges_table,
};
use std::fmt::Write;
use std::fs;

struct TightPropagation {
    q: SparseQ,
    tolerance: f64,
}
impl BranchPropagator for TightPropagation {
    fn state_count(&self) -> usize {
        self.q.size()
    }
    fn propagate(
        &self,
        _: usize,
        length: f64,
        values: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        audit_propagation(&self.q, length, values, self.tolerance, false)
    }
    fn propagate_transpose(
        &self,
        _: usize,
        length: f64,
        values: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        audit_propagation(&self.q, length, values, self.tolerance, true)
    }
}

#[test]
#[ignore = "requires BIOGEO_PRECISION_MANIFEST and BIOGEO_PRECISION_OUTPUT; fixed homogeneous audit inputs"]
fn fixed_parameter_precision_audit() {
    let manifest = fs::read_to_string(std::env::var("BIOGEO_PRECISION_MANIFEST").unwrap()).unwrap();
    let mut output = String::from(
        "case\ttolerance\tdefault_lnL\ttight_lnL\tdelta_lnL\tmax_posterior_delta\tkahan_log_sum_delta\treference_r_lnL\n",
    );
    for row in manifest.lines().skip(1) {
        let fields: Vec<_> = row.split('\t').collect();
        assert_eq!(fields.len(), 8);
        let tree = parse_newick(&fs::read_to_string(fields[1]).unwrap())
            .unwrap()
            .with_direct_ancestor_hooks_below(fields[6].parse().unwrap())
            .unwrap();
        let ranges =
            parse_tip_ranges_table(&fs::read_to_string(fields[2]).unwrap(), &tree).unwrap();
        let parameters = parse_parameter_table(&fs::read_to_string(fields[3]).unwrap())
            .unwrap()
            .resolve_initial()
            .unwrap();
        // This audit intentionally covers homogeneous w=1 cases only.
        assert_eq!(parameters.get("w"), Some(1.0));
        let model = ModelConfig::from_biogeobears_core_parameters(&parameters).unwrap();
        let states = StateSpace::new(
            ranges.area_names.len() as u8,
            fields[4].parse().unwrap(),
            fields[5].parse().unwrap(),
        )
        .unwrap();
        let tips = ranges.tip_likelihoods(&states).unwrap();
        let engine = LikelihoodEngine::new(&tree.tree, &states, RootPrior::Flat);
        let ordinary = engine.evaluate(&model, &tips).unwrap();
        let ordinary_posteriors = engine.node_state_posteriors(&model, &ordinary).unwrap();
        let cladogenesis = build_cladogenetic_process(&model, &tree.tree, &states).unwrap();
        let logs: Vec<f64> = ordinary
            .scale_factors
            .iter()
            .filter(|x| **x > 0.0)
            .map(|x| x.ln())
            .collect();
        let (mut sum, mut compensation) = (0.0, 0.0);
        for value in &logs {
            let adjusted = value - compensation;
            let next = sum + adjusted;
            compensation = (next - sum) - adjusted;
            sum = next;
        }
        let kahan_delta = sum - logs.iter().sum::<f64>();
        assert!(kahan_delta.abs() < 1e-9);
        for tolerance in [1e-14, 1e-15] {
            let propagator = TightPropagation {
                q: model.build_q(&states).unwrap(),
                tolerance,
            };
            let precise = prune_with_cladogenesis_by_branch(
                &tree.tree,
                &propagator,
                &cladogenesis,
                &tips,
                RootPrior::Flat,
            )
            .unwrap();
            let posteriors = cladogenetic_node_state_posteriors_by_branch(
                &tree.tree,
                &propagator,
                &cladogenesis,
                &precise,
                RootPrior::Flat,
            )
            .unwrap();
            let posterior_delta = posteriors
                .iter()
                .zip(&ordinary_posteriors)
                .flat_map(|(a, b)| {
                    assert_eq!(a.node, b.node);
                    a.probabilities
                        .iter()
                        .zip(&b.probabilities)
                        .map(|(a, b)| (a - b).abs())
                })
                .fold(0.0, f64::max);
            let delta = precise.log_likelihood - ordinary.log_likelihood;
            writeln!(output, "{}\t{tolerance:e}\t{:.17}\t{:.17}\t{delta:e}\t{posterior_delta:e}\t{kahan_delta:e}\t{}",
                fields[0], ordinary.log_likelihood, precise.log_likelihood, fields[7]).unwrap();
            assert!(delta.abs() < 1e-8, "{}: delta={delta}", fields[0]);
            assert!(
                posterior_delta < 1e-8,
                "{}: posterior delta={posterior_delta}",
                fields[0]
            );
        }
    }
    fs::write(std::env::var("BIOGEO_PRECISION_OUTPUT").unwrap(), output).unwrap();
}
