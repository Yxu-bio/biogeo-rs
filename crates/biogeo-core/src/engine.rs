use std::error::Error;
use std::fmt;
use std::ops::Range;

use crate::bsm::{
    BiogeographicStochasticMap, BsmError, HistorySkeleton, HistorySkeletonSampler,
    StochasticMapParallelError, StochasticMapParallelOptions, StochasticMapSampler,
    StochasticMapStreamError,
};
use crate::cladogenesis::{CladogenesisError, OwnedCladogeneticProcess};
use crate::model::{AnagenesisError, ModelConfig};
use crate::pruning::{
    NodeStatePosterior, PruningError, PruningResult, RootPrior, SplitScenarioPosterior,
    TipLikelihood, cladogenetic_node_state_posteriors_by_branch,
    cladogenetic_split_scenario_posteriors_by_branch, prune_with_cladogenesis_by_branch,
};
use crate::state::StateSpace;
use crate::tree::Tree;

#[derive(Clone, Copy, Debug)]
pub struct LikelihoodEngine<'a> {
    tree: &'a Tree,
    states: &'a StateSpace,
    root_prior: RootPrior<'a>,
}

impl<'a> LikelihoodEngine<'a> {
    pub fn new(tree: &'a Tree, states: &'a StateSpace, root_prior: RootPrior<'a>) -> Self {
        Self {
            tree,
            states,
            root_prior,
        }
    }

    pub fn evaluate(
        &self,
        model: &ModelConfig,
        tip_likelihoods: &[TipLikelihood],
    ) -> Result<PruningResult, LikelihoodEngineError> {
        let violations = tip_likelihood_state_constraint_violations(
            self.tree,
            self.states,
            model,
            tip_likelihoods,
        )?;
        if !violations.is_empty() {
            return Err(LikelihoodEngineError::Pruning(
                PruningError::TipLikelihoodsExcludedByStateConstraint {
                    violations: violations
                        .into_iter()
                        .map(|violation| (violation.node, violation.stratum_index))
                        .collect(),
                },
            ));
        }
        let propagator = model.build_branch_propagator(self.tree, self.states)?;
        let cladogenesis = build_cladogenetic_process(model, self.tree, self.states)?;

        Ok(prune_with_cladogenesis_by_branch(
            self.tree,
            &propagator,
            &cladogenesis,
            tip_likelihoods,
            self.root_prior,
        )?)
    }

    pub fn node_state_posteriors(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
    ) -> Result<Vec<NodeStatePosterior>, LikelihoodEngineError> {
        let propagator = model.build_branch_propagator(self.tree, self.states)?;
        let cladogenesis = build_cladogenetic_process(model, self.tree, self.states)?;

        Ok(cladogenetic_node_state_posteriors_by_branch(
            self.tree,
            &propagator,
            &cladogenesis,
            pruning,
            self.root_prior,
        )?)
    }

    pub fn split_scenario_posteriors(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
    ) -> Result<Vec<SplitScenarioPosterior>, LikelihoodEngineError> {
        let propagator = model.build_branch_propagator(self.tree, self.states)?;
        let cladogenesis = build_cladogenetic_process(model, self.tree, self.states)?;

        Ok(cladogenetic_split_scenario_posteriors_by_branch(
            self.tree,
            &propagator,
            &cladogenesis,
            pruning,
            self.root_prior,
        )?)
    }

    pub fn prepare_history_skeleton_sampler<'b>(
        &'b self,
        model: &ModelConfig,
        pruning: &'b PruningResult,
    ) -> Result<HistorySkeletonSampler<'b>, BsmError> {
        let propagator = model.build_branch_propagator(self.tree, self.states)?;
        let cladogenesis = build_cladogenetic_process(model, self.tree, self.states)?;

        HistorySkeletonSampler::new(
            self.tree,
            self.states,
            pruning,
            propagator,
            cladogenesis,
            self.root_prior,
        )
    }

    pub fn sample_history_skeletons_seeded(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
        sample_count: usize,
        seed: u64,
    ) -> Result<Vec<HistorySkeleton>, BsmError> {
        let sampler = self.prepare_history_skeleton_sampler(model, pruning)?;
        (0..sample_count)
            .map(|sample_index| {
                let sample_index = u64::try_from(sample_index)
                    .map_err(|_| BsmError::SampleIndexOutOfRange { sample_index })?;
                sampler.sample_indexed(seed, sample_index)
            })
            .collect()
    }

    pub fn prepare_stochastic_map_sampler<'b>(
        &'b self,
        model: &ModelConfig,
        pruning: &'b PruningResult,
    ) -> Result<StochasticMapSampler<'b>, BsmError> {
        self.prepare_history_skeleton_sampler(model, pruning)
    }

    pub fn sample_stochastic_maps_seeded(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
        sample_count: usize,
        seed: u64,
    ) -> Result<Vec<BiogeographicStochasticMap>, BsmError> {
        let sampler = self.prepare_stochastic_map_sampler(model, pruning)?;
        (0..sample_count)
            .map(|sample_index| {
                let sample_index = u64::try_from(sample_index)
                    .map_err(|_| BsmError::SampleIndexOutOfRange { sample_index })?;
                sampler.sample_map_indexed(seed, sample_index)
            })
            .collect()
    }

    pub fn try_for_each_stochastic_map_seeded<E, F>(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
        sample_count: usize,
        seed: u64,
        consumer: F,
    ) -> Result<(), StochasticMapStreamError<E>>
    where
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        let sampler = self
            .prepare_stochastic_map_sampler(model, pruning)
            .map_err(StochasticMapStreamError::Sampling)?;
        let mut consumer = consumer;
        for sample_index in 0..sample_count {
            let indexed = u64::try_from(sample_index).map_err(|_| {
                StochasticMapStreamError::Sampling(BsmError::SampleIndexOutOfRange { sample_index })
            })?;
            let map = sampler
                .sample_map_indexed(seed, indexed)
                .map_err(StochasticMapStreamError::Sampling)?;
            consumer(sample_index, &map).map_err(StochasticMapStreamError::Consumer)?;
        }
        Ok(())
    }

    pub fn try_for_each_stochastic_map_parallel_seeded<E, F>(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
        sample_count: usize,
        seed: u64,
        options: StochasticMapParallelOptions,
        consumer: F,
    ) -> Result<(), StochasticMapParallelError<E>>
    where
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        self.try_for_each_stochastic_map_parallel_seeded_range(
            model,
            pruning,
            0..sample_count,
            seed,
            options,
            consumer,
        )
    }

    pub fn try_for_each_stochastic_map_parallel_seeded_range<E, F>(
        &self,
        model: &ModelConfig,
        pruning: &PruningResult,
        sample_range: Range<usize>,
        seed: u64,
        options: StochasticMapParallelOptions,
        consumer: F,
    ) -> Result<(), StochasticMapParallelError<E>>
    where
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        self.prepare_stochastic_map_sampler(model, pruning)
            .map_err(StochasticMapParallelError::Preparation)?
            .try_for_each_map_indexed_parallel_range_with_options(
                sample_range,
                seed,
                options,
                consumer,
            )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TipStateConstraintViolation {
    pub node: usize,
    pub stratum_index: usize,
}

pub fn tip_likelihood_state_constraint_violations(
    tree: &Tree,
    states: &StateSpace,
    model: &ModelConfig,
    tip_likelihoods: &[TipLikelihood],
) -> Result<Vec<TipStateConstraintViolation>, AnagenesisError> {
    let Some(schedule) = model.anagenesis.time_stratified_anagenesis() else {
        return Ok(Vec::new());
    };
    let Some(masks) = model.anagenesis.stratified_state_masks(states)? else {
        return Ok(Vec::new());
    };
    let node_ages = tree.node_ages_from_present();
    let mut violations = Vec::new();
    for tip in tip_likelihoods {
        if tip.node >= node_ages.len() || tip.likelihoods.len() != states.len() {
            continue;
        }
        if tip
            .likelihoods
            .iter()
            .any(|value| !value.is_finite() || *value < 0.0)
        {
            continue;
        }
        let has_positive_mass = tip.likelihoods.iter().any(|value| *value > 0.0);
        if !has_positive_mass {
            continue;
        }
        let stratum_index = schedule.stratum_index_at_age(node_ages[tip.node]);
        let has_allowed_mass = tip
            .likelihoods
            .iter()
            .zip(masks[stratum_index].values())
            .any(|(likelihood, allowed)| *allowed && *likelihood > 0.0);
        if !has_allowed_mass {
            violations.push(TipStateConstraintViolation {
                node: tip.node,
                stratum_index,
            });
        }
    }
    Ok(violations)
}

pub(crate) fn build_cladogenetic_process(
    model: &ModelConfig,
    tree: &Tree,
    states: &StateSpace,
) -> Result<OwnedCladogeneticProcess, LikelihoodEngineError> {
    let base = model.build_cladogenetic_table(states)?;
    let state_masks = model.anagenesis.stratified_state_masks(states)?;
    let Some(schedule) = model.anagenesis.time_stratified_anagenesis() else {
        return Ok(OwnedCladogeneticProcess::homogeneous(base));
    };
    let has_founder_modifiers = model.cladogenesis.event_weights.founder_event > 0.0
        && schedule
            .strata()
            .iter()
            .any(|stratum| stratum.dispersal_multipliers.is_some());
    if state_masks.is_none() && !has_founder_modifiers {
        return Ok(OwnedCladogeneticProcess::homogeneous(base));
    }

    let tables = schedule
        .strata()
        .iter()
        .enumerate()
        .map(|(index, stratum)| {
            let table = if has_founder_modifiers {
                model
                    .cladogenesis
                    .build_table_with_dispersal(states, stratum.dispersal_multipliers.as_ref())?
            } else {
                base.clone()
            };
            match &state_masks {
                Some(masks) => table.constrained(&masks[index]),
                None => Ok(table),
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    let table_index_by_node = tree
        .node_ages_from_present()
        .into_iter()
        .map(|age| schedule.stratum_index_at_age(age))
        .collect();

    Ok(OwnedCladogeneticProcess::stratified(
        tables,
        table_index_by_node,
        state_masks,
    ))
}

#[derive(Clone, Debug, PartialEq)]
pub enum LikelihoodEngineError {
    Anagenesis(AnagenesisError),
    Cladogenesis(CladogenesisError),
    Pruning(PruningError),
}

impl fmt::Display for LikelihoodEngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anagenesis(error) => write!(f, "anagenesis setup failed: {error}"),
            Self::Cladogenesis(error) => write!(f, "cladogenesis setup failed: {error}"),
            Self::Pruning(error) => write!(f, "likelihood pruning failed: {error}"),
        }
    }
}

impl Error for LikelihoodEngineError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Anagenesis(error) => Some(error),
            Self::Cladogenesis(error) => Some(error),
            Self::Pruning(error) => Some(error),
        }
    }
}

impl From<AnagenesisError> for LikelihoodEngineError {
    fn from(value: AnagenesisError) -> Self {
        Self::Anagenesis(value)
    }
}

impl From<CladogenesisError> for LikelihoodEngineError {
    fn from(value: CladogenesisError) -> Self {
        Self::Cladogenesis(value)
    }
}

impl From<PruningError> for LikelihoodEngineError {
    fn from(value: PruningError) -> Self {
        Self::Pruning(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cladogenesis::CladogeneticProcess;
    use crate::constraints::{BinaryAreaMatrix, RangeStateConstraint};
    use crate::dispersal::{
        AnageneticTimeStratum, DispersalMultiplierMatrix, DispersalScheduleError,
        DispersalTimeStratum, ExtirpationMultiplierVector, TimeStratifiedAnagenesis,
        TimeStratifiedDispersal,
    };
    use crate::model::ModelConfig;
    use crate::state::AreaSet;
    use crate::tree::Edge;

    #[test]
    fn evaluates_dec_and_posteriors_through_one_engine() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 0.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 0.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b11)).unwrap();
        let mut left = vec![0.0; states.len()];
        let mut right = vec![0.0; states.len()];
        left[a] = 1.0;
        right[b] = 1.0;
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: left,
            },
            TipLikelihood {
                node: 1,
                likelihoods: right,
            },
        ];
        let model = ModelConfig::preset_dec(0.1, 0.2).unwrap();
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);

        let pruning = engine.evaluate(&model, &tips).unwrap();
        let node_posteriors = engine.node_state_posteriors(&model, &pruning).unwrap();
        let split_posteriors = engine.split_scenario_posteriors(&model, &pruning).unwrap();

        assert!((pruning.log_likelihood - (1.0_f64 / 6.0).ln()).abs() < 1e-12);
        assert!((node_posteriors[2].probabilities[ab] - 1.0).abs() < 1e-12);
        let supported_split = split_posteriors
            .iter()
            .find(|scenario| {
                scenario.node == 2
                    && scenario.ancestor == ab
                    && scenario.left == a
                    && scenario.right == b
            })
            .unwrap();
        assert!((supported_split.probability - 1.0).abs() < 1e-12);
    }

    #[test]
    fn equal_anagenetic_strata_match_homogeneous_likelihood_and_posteriors() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let mut left = vec![0.0; states.len()];
        let mut right = vec![0.0; states.len()];
        left[a] = 1.0;
        right[b] = 1.0;
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: left,
            },
            TipLikelihood {
                node: 1,
                likelihoods: right,
            },
        ];
        let matrix = DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap();
        let extirpation = ExtirpationMultiplierVector::new(vec![0.5, 2.0]).unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![
            AnageneticTimeStratum::new(0.4, Some(matrix.clone()), Some(extirpation.clone()))
                .unwrap(),
            AnageneticTimeStratum::new(1.0, Some(matrix), Some(extirpation.clone())).unwrap(),
        ])
        .unwrap();
        let homogeneous = ModelConfig::preset_dec(0.1, 0.2)
            .unwrap()
            .with_extirpation_multipliers(extirpation);
        let stratified = homogeneous
            .clone()
            .with_time_stratified_anagenesis(schedule);
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);

        let homogeneous_result = engine.evaluate(&homogeneous, &tips).unwrap();
        let stratified_result = engine.evaluate(&stratified, &tips).unwrap();
        assert!(
            (homogeneous_result.log_likelihood - stratified_result.log_likelihood).abs() < 1e-12
        );

        let homogeneous_posteriors = engine
            .node_state_posteriors(&homogeneous, &homogeneous_result)
            .unwrap();
        let stratified_posteriors = engine
            .node_state_posteriors(&stratified, &stratified_result)
            .unwrap();
        for (homogeneous, stratified) in homogeneous_posteriors.iter().zip(&stratified_posteriors) {
            for (left, right) in homogeneous
                .probabilities
                .iter()
                .zip(&stratified.probabilities)
            {
                assert!((left - right).abs() < 1e-12);
            }
        }

        let homogeneous_splits = engine
            .split_scenario_posteriors(&homogeneous, &homogeneous_result)
            .unwrap();
        let stratified_splits = engine
            .split_scenario_posteriors(&stratified, &stratified_result)
            .unwrap();
        assert_eq!(homogeneous_splits.len(), stratified_splits.len());
        for (homogeneous, stratified) in homogeneous_splits.iter().zip(&stratified_splits) {
            assert_eq!(
                (
                    homogeneous.node,
                    homogeneous.ancestor,
                    homogeneous.left,
                    homogeneous.right,
                ),
                (
                    stratified.node,
                    stratified.ancestor,
                    stratified.left,
                    stratified.right,
                )
            );
            assert!((homogeneous.weight - stratified.weight).abs() < 1e-12);
            assert!((homogeneous.probability - stratified.probability).abs() < 1e-12);
        }
    }

    #[test]
    fn state_constraints_apply_to_cladogenesis_and_equal_root_prior() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 0.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 0.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b11)).unwrap();
        let adjacency = BinaryAreaMatrix::new(2, vec![true, false, false, true]).unwrap();
        let constraint = RangeStateConstraint::new(None, Some(adjacency)).unwrap();
        let stratum = AnageneticTimeStratum::new(1.0, None, None)
            .unwrap()
            .with_state_constraint(constraint)
            .unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![stratum]).unwrap();
        let model = ModelConfig::preset_dec(0.1, 0.2)
            .unwrap()
            .with_time_stratified_anagenesis(schedule);
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Equal);

        let one_hot = |node, state| {
            let mut likelihoods = vec![0.0; states.len()];
            likelihoods[state] = 1.0;
            TipLikelihood { node, likelihoods }
        };
        let matching_tips = vec![one_hot(0, a), one_hot(1, a)];
        let pruning = engine.evaluate(&model, &matching_tips).unwrap();
        assert!((pruning.log_likelihood - 0.5_f64.ln()).abs() < 1e-12);

        let posteriors = engine.node_state_posteriors(&model, &pruning).unwrap();
        assert_eq!(posteriors[tree.root()].probabilities[ab], 0.0);
        assert!((posteriors[tree.root()].probabilities[a] - 1.0).abs() < 1e-12);

        let incompatible_tips = vec![one_hot(0, a), one_hot(1, b)];
        assert!(matches!(
            engine.evaluate(&model, &incompatible_tips),
            Err(LikelihoodEngineError::Pruning(
                PruningError::NonPositiveNodeLikelihood { node: 2, .. }
            ))
        ));

        let forbidden_tip = vec![one_hot(0, ab), one_hot(1, a)];
        assert_eq!(
            engine.evaluate(&model, &forbidden_tip).unwrap_err(),
            LikelihoodEngineError::Pruning(PruningError::TipLikelihoodsExcludedByStateConstraint {
                violations: vec![(0, 0)],
            })
        );
    }

    #[test]
    fn founder_event_uses_pairwise_matrix_for_each_node_age() {
        let tree = Tree::new(
            4,
            5,
            vec![
                Edge {
                    parent: 3,
                    child: 0,
                    length: 0.2,
                },
                Edge {
                    parent: 3,
                    child: 1,
                    length: 0.2,
                },
                Edge {
                    parent: 4,
                    child: 3,
                    length: 0.8,
                },
                Edge {
                    parent: 4,
                    child: 2,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(3, 2, false).unwrap();
        let young = DispersalMultiplierMatrix::new(3, vec![1.0; 9]).unwrap();
        let old =
            DispersalMultiplierMatrix::new(3, vec![1.0, 10.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0])
                .unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![
            AnageneticTimeStratum::new(0.5, Some(young), None).unwrap(),
            AnageneticTimeStratum::new(1.1, Some(old), None).unwrap(),
        ])
        .unwrap();
        let model = ModelConfig::preset_dec_j(0.1, 0.2, 1.0)
            .unwrap()
            .with_time_stratified_anagenesis(schedule);
        let process = build_cladogenetic_process(&model, &tree, &states).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b001)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b010)).unwrap();
        let young_weight = process
            .table_for_node(3)
            .row(a)
            .unwrap()
            .iter()
            .find(|scenario| scenario.left == a && scenario.right == b)
            .unwrap()
            .weight;
        let old_weight = process
            .table_for_node(4)
            .row(a)
            .unwrap()
            .iter()
            .find(|scenario| scenario.left == a && scenario.right == b)
            .unwrap()
            .weight;

        assert!(old_weight > young_weight);
        assert!(process.state_mask_for_node(3).is_none());
        assert!(process.state_mask_for_node(4).is_none());
    }

    #[test]
    fn constrained_split_posteriors_sum_to_node_state_posteriors() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 0.5,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 0.5,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(3, 3, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b001)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b010)).unwrap();
        let adjacency = BinaryAreaMatrix::new(
            3,
            vec![
                true, true, false, // A connects to A and B.
                true, true, false, // B connects to A and B.
                false, false, true, // C is an isolated singleton.
            ],
        )
        .unwrap();
        let constraint = RangeStateConstraint::new(None, Some(adjacency)).unwrap();
        let mask = constraint.state_mask(&states).unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![
            AnageneticTimeStratum::new(1.0, None, None)
                .unwrap()
                .with_state_constraint(constraint)
                .unwrap(),
        ])
        .unwrap();
        let model = ModelConfig::preset_dec(0.1, 0.2)
            .unwrap()
            .with_time_stratified_anagenesis(schedule);
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let one_hot = |node, state| {
            let mut likelihoods = vec![0.0; states.len()];
            likelihoods[state] = 1.0;
            TipLikelihood { node, likelihoods }
        };
        let pruning = engine
            .evaluate(&model, &[one_hot(0, a), one_hot(1, b)])
            .unwrap();
        let node_posteriors = engine.node_state_posteriors(&model, &pruning).unwrap();
        let split_posteriors = engine.split_scenario_posteriors(&model, &pruning).unwrap();

        let mut split_mass_by_ancestor = vec![0.0; states.len()];
        for split in split_posteriors
            .iter()
            .filter(|split| split.node == tree.root())
        {
            assert!(mask.is_allowed(split.ancestor));
            assert!(mask.is_allowed(split.left));
            assert!(mask.is_allowed(split.right));
            split_mass_by_ancestor[split.ancestor] += split.probability;
        }
        for (state, split_mass) in split_mass_by_ancestor.iter().enumerate() {
            assert!(
                (split_mass - node_posteriors[tree.root()].probabilities[state]).abs() < 1e-12,
                "state {state}: split mass {split_mass} != node posterior {}",
                node_posteriors[tree.root()].probabilities[state]
            );
        }
    }

    #[test]
    fn time_strata_must_cover_the_tree_root() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let matrix = DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap();
        let schedule =
            TimeStratifiedDispersal::new(vec![DispersalTimeStratum::new(0.5, matrix).unwrap()])
                .unwrap();
        let model = ModelConfig::preset_dec(0.1, 0.2)
            .unwrap()
            .with_time_stratified_dispersal(schedule);
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);

        let error = engine.evaluate(&model, &[]).unwrap_err();
        assert_eq!(
            error,
            LikelihoodEngineError::Anagenesis(crate::model::AnagenesisError::DispersalSchedule(
                DispersalScheduleError::DoesNotCoverRoot {
                    oldest_age: 0.5,
                    root_age: 1.0,
                }
            ))
        );
    }
}
