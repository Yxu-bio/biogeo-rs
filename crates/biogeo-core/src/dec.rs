use std::error::Error;
use std::fmt;

use crate::cladogenesis::{CladogenesisConfig, CladogenesisError, CladogenesisRangeSizeConfig};
use crate::engine::{LikelihoodEngine, LikelihoodEngineError};
use crate::model::{AnagenesisError, DecModelError, ModelConfig};
use crate::pruning::{
    NodeStatePosterior, PruningError, PruningResult, RootPrior, SplitScenarioPosterior,
    TipLikelihood,
};
use crate::state::{AreaSet, StateSpace};
use crate::tree::Tree;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TipRange {
    pub node: usize,
    pub range: AreaSet,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecOptimizationConfig {
    pub initial_d: f64,
    pub initial_e: f64,
    pub min_rate: f64,
    pub max_rate: f64,
    pub initial_log_step: f64,
    pub tolerance: f64,
    pub max_iterations: usize,
    pub multi_start_points_per_axis: usize,
    pub range_size: CladogenesisRangeSizeConfig,
}

impl Default for DecOptimizationConfig {
    fn default() -> Self {
        Self {
            initial_d: 0.01,
            initial_e: 0.01,
            min_rate: 1e-12,
            max_rate: 10.0,
            initial_log_step: 0.5,
            tolerance: 1e-8,
            max_iterations: 200,
            multi_start_points_per_axis: 1,
            range_size: CladogenesisRangeSizeConfig::default(),
        }
    }
}

impl DecOptimizationConfig {
    pub fn for_divalike() -> Self {
        Self {
            range_size: CladogenesisConfig::preset_divalike().range_size,
            ..Self::default()
        }
    }

    pub fn for_bayarealike() -> Self {
        Self {
            range_size: CladogenesisConfig::preset_bayarealike().range_size,
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecExponentOptimizationConfig {
    pub de: DecOptimizationConfig,
    pub initial_exponent: f64,
    pub min_exponent: f64,
    pub max_exponent: f64,
    pub initial_exponent_step: f64,
}

impl DecExponentOptimizationConfig {
    pub fn for_x() -> Self {
        let de = DecOptimizationConfig {
            max_iterations: 300,
            multi_start_points_per_axis: 2,
            ..DecOptimizationConfig::default()
        };
        Self {
            de,
            initial_exponent: 0.0,
            min_exponent: -2.5,
            max_exponent: 2.5,
            initial_exponent_step: 0.5,
        }
    }

    pub fn for_n() -> Self {
        Self {
            min_exponent: -10.0,
            max_exponent: 10.0,
            ..Self::for_x()
        }
    }

    pub fn for_u() -> Self {
        Self::for_n()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecXnuOptimizationConfig {
    pub de: DecOptimizationConfig,
    pub initial_x: f64,
    pub min_x: f64,
    pub max_x: f64,
    pub initial_x_step: f64,
    pub initial_n: f64,
    pub min_n: f64,
    pub max_n: f64,
    pub initial_n_step: f64,
    pub initial_u: f64,
    pub min_u: f64,
    pub max_u: f64,
    pub initial_u_step: f64,
}

impl Default for DecXnuOptimizationConfig {
    fn default() -> Self {
        Self {
            de: DecOptimizationConfig {
                max_iterations: 500,
                ..DecOptimizationConfig::default()
            },
            initial_x: 0.0,
            min_x: -2.5,
            max_x: 2.5,
            initial_x_step: 0.5,
            initial_n: 0.0,
            min_n: -10.0,
            max_n: 10.0,
            initial_n_step: 0.5,
            initial_u: 0.0,
            min_u: -10.0,
            max_u: 10.0,
            initial_u_step: 0.5,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecJOptimizationConfig {
    pub initial_d: f64,
    pub initial_e: f64,
    pub initial_j: f64,
    pub min_rate: f64,
    pub max_rate: f64,
    pub min_j: f64,
    pub max_j: f64,
    pub initial_log_step: f64,
    pub tolerance: f64,
    pub max_iterations: usize,
    pub multi_start_points_per_axis: usize,
    pub range_size: CladogenesisRangeSizeConfig,
}

impl Default for DecJOptimizationConfig {
    fn default() -> Self {
        Self {
            initial_d: 0.01,
            initial_e: 0.01,
            initial_j: 0.01,
            min_rate: 1e-12,
            max_rate: 10.0,
            min_j: 1e-5,
            max_j: 2.99999,
            initial_log_step: 0.5,
            tolerance: 1e-8,
            max_iterations: 250,
            multi_start_points_per_axis: 1,
            range_size: CladogenesisRangeSizeConfig::default(),
        }
    }
}

impl DecJOptimizationConfig {
    pub fn for_divalike() -> Self {
        Self {
            max_j: 1.99999,
            range_size: CladogenesisConfig::preset_divalike().range_size,
            ..Self::default()
        }
    }

    pub fn for_bayarealike() -> Self {
        Self {
            max_j: 0.99999,
            range_size: CladogenesisConfig::preset_bayarealike().range_size,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecOptimizationResult {
    pub d: f64,
    pub e: f64,
    pub log_likelihood: f64,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub starts: usize,
    pub pruning: PruningResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecJOptimizationResult {
    pub d: f64,
    pub e: f64,
    pub j: f64,
    pub log_likelihood: f64,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub starts: usize,
    pub pruning: PruningResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OptimizationBound {
    Lower,
    Upper,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecExponentOptimizationResult {
    pub d: f64,
    pub e: f64,
    pub exponent: f64,
    pub exponent_bound: Option<OptimizationBound>,
    pub log_likelihood: f64,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub converged_starts: usize,
    pub starts: usize,
    pub pruning: PruningResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecXnuOptimizationResult {
    pub d: f64,
    pub e: f64,
    pub x: f64,
    pub n: f64,
    pub u: f64,
    pub x_bound: Option<OptimizationBound>,
    pub n_bound: Option<OptimizationBound>,
    pub u_bound: Option<OptimizationBound>,
    pub log_likelihood: f64,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub converged_starts: usize,
    pub starts: usize,
    pub pruning: PruningResult,
}

pub const PROFILE_95_SUPPORT_DELTA: f64 = 2.995_732_273_553_991;

#[derive(Clone, Debug, PartialEq)]
pub struct DecProfileAxis {
    pub parameter: String,
    pub values: Vec<f64>,
}

impl DecProfileAxis {
    pub fn new(parameter: impl Into<String>, values: Vec<f64>) -> Self {
        Self {
            parameter: parameter.into(),
            values,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecPairProfileConfig {
    pub de: DecOptimizationConfig,
    pub first: DecProfileAxis,
    pub second: DecProfileAxis,
    pub support_delta: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecPairProfilePoint {
    pub first: f64,
    pub second: f64,
    pub d: f64,
    pub e: f64,
    pub log_likelihood: f64,
    pub delta_log_likelihood: f64,
    pub finite: bool,
    pub within_support: bool,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub starts: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecProfileSupportSpan {
    pub min: f64,
    pub max: f64,
    pub grid_values: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecPairProfileResult {
    pub first_parameter: String,
    pub second_parameter: String,
    pub support_delta: f64,
    pub points: Vec<DecPairProfilePoint>,
    pub best_point_index: usize,
    pub best_first_grid_bound: Option<OptimizationBound>,
    pub best_second_grid_bound: Option<OptimizationBound>,
    pub first_support: DecProfileSupportSpan,
    pub second_support: DecProfileSupportSpan,
    pub support_points: usize,
    pub finite_points: usize,
    pub failed_points: usize,
    pub converged_points: usize,
    pub likelihood_weighted_correlation: Option<f64>,
    pub total_iterations: usize,
    pub total_evaluations: usize,
}

impl DecPairProfileResult {
    pub fn best_point(&self) -> &DecPairProfilePoint {
        &self.points[self.best_point_index]
    }
}

pub fn run_fixed_dec(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    d: f64,
    e: f64,
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, DecAnalysisError> {
    let model = ModelConfig::preset_dec(d, e)?;

    run_fixed_model(tree, states, tip_ranges, &model, root_prior)
}

pub fn run_fixed_dec_j(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    d: f64,
    e: f64,
    j: f64,
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, DecAnalysisError> {
    let model = ModelConfig::preset_dec_j(d, e, j)?;

    run_fixed_model(tree, states, tip_ranges, &model, root_prior)
}

pub fn run_fixed_divalike_j(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    d: f64,
    e: f64,
    j: f64,
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, DecAnalysisError> {
    let model = ModelConfig::preset_divalike_j(d, e, j)?;

    run_fixed_model(tree, states, tip_ranges, &model, root_prior)
}

pub fn run_fixed_bayarealike_j(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    d: f64,
    e: f64,
    j: f64,
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, DecAnalysisError> {
    let model = ModelConfig::preset_bayarealike_j(d, e, j)?;

    run_fixed_model(tree, states, tip_ranges, &model, root_prior)
}

pub fn run_fixed_model(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    model: &ModelConfig,
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, DecAnalysisError> {
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    run_fixed_model_likelihoods(tree, states, &tip_likelihoods, model, root_prior)
}

pub fn run_fixed_model_likelihoods(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    model: &ModelConfig,
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, DecAnalysisError> {
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    Ok(engine.evaluate(model, tip_likelihoods)?)
}

pub fn dec_node_state_posteriors(
    tree: &Tree,
    states: &StateSpace,
    pruning: &PruningResult,
    d: f64,
    e: f64,
    root_prior: RootPrior<'_>,
) -> Result<Vec<NodeStatePosterior>, DecAnalysisError> {
    let model = ModelConfig::preset_dec(d, e)?;

    model_node_state_posteriors(tree, states, pruning, &model, root_prior)
}

pub fn model_node_state_posteriors(
    tree: &Tree,
    states: &StateSpace,
    pruning: &PruningResult,
    model: &ModelConfig,
    root_prior: RootPrior<'_>,
) -> Result<Vec<NodeStatePosterior>, DecAnalysisError> {
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    Ok(engine.node_state_posteriors(model, pruning)?)
}

pub fn dec_split_scenario_posteriors(
    tree: &Tree,
    states: &StateSpace,
    pruning: &PruningResult,
    d: f64,
    e: f64,
    root_prior: RootPrior<'_>,
) -> Result<Vec<SplitScenarioPosterior>, DecAnalysisError> {
    let model = ModelConfig::preset_dec(d, e)?;

    model_split_scenario_posteriors(tree, states, pruning, &model, root_prior)
}

pub fn model_split_scenario_posteriors(
    tree: &Tree,
    states: &StateSpace,
    pruning: &PruningResult,
    model: &ModelConfig,
    root_prior: RootPrior<'_>,
) -> Result<Vec<SplitScenarioPosterior>, DecAnalysisError> {
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    Ok(engine.split_scenario_posteriors(model, pruning)?)
}

pub fn dec_j_node_state_posteriors(
    tree: &Tree,
    states: &StateSpace,
    pruning: &PruningResult,
    d: f64,
    e: f64,
    j: f64,
    root_prior: RootPrior<'_>,
) -> Result<Vec<NodeStatePosterior>, DecAnalysisError> {
    let model = ModelConfig::preset_dec_j(d, e, j)?;

    model_node_state_posteriors(tree, states, pruning, &model, root_prior)
}

pub fn dec_j_split_scenario_posteriors(
    tree: &Tree,
    states: &StateSpace,
    pruning: &PruningResult,
    d: f64,
    e: f64,
    j: f64,
    root_prior: RootPrior<'_>,
) -> Result<Vec<SplitScenarioPosterior>, DecAnalysisError> {
    let model = ModelConfig::preset_dec_j(d, e, j)?;

    model_split_scenario_posteriors(tree, states, pruning, &model, root_prior)
}

pub fn optimize_dec_de(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecOptimizationConfig,
) -> Result<DecOptimizationResult, DecOptimizationError> {
    let range_size = config.range_size;
    optimize_de_with_model(tree, states, tip_ranges, root_prior, config, move |d, e| {
        Ok(ModelConfig::preset_dec(d, e)?.with_range_size_config(range_size))
    })
}

pub fn optimize_divalike_de(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecOptimizationConfig,
) -> Result<DecOptimizationResult, DecOptimizationError> {
    let range_size = config.range_size;
    optimize_de_with_model(tree, states, tip_ranges, root_prior, config, move |d, e| {
        Ok(ModelConfig::preset_divalike(d, e)?.with_range_size_config(range_size))
    })
}

pub fn optimize_bayarealike_de(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecOptimizationConfig,
) -> Result<DecOptimizationResult, DecOptimizationError> {
    let range_size = config.range_size;
    optimize_de_with_model(tree, states, tip_ranges, root_prior, config, move |d, e| {
        Ok(ModelConfig::preset_bayarealike(d, e)?.with_range_size_config(range_size))
    })
}

pub fn optimize_de_with_model<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecOptimizationConfig,
    model_factory: F,
) -> Result<DecOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    optimize_de_with_model_likelihoods(
        tree,
        states,
        &tip_likelihoods,
        root_prior,
        config,
        model_factory,
    )
}

pub fn optimize_de_with_model_likelihoods<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    config: DecOptimizationConfig,
    model_factory: F,
) -> Result<DecOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    validate_optimization_config(config)?;
    config
        .range_size
        .validate()
        .map_err(DecAnalysisError::from)?;
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    optimize_de_with_engine(&engine, tip_likelihoods, config, &model_factory)
}

fn optimize_de_with_engine<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    config: DecOptimizationConfig,
    model_factory: &F,
) -> Result<DecOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let min_log_rate = config.min_rate.ln();
    let max_log_rate = config.max_rate.ln();
    let primary_start = [
        config.initial_d.ln().clamp(min_log_rate, max_log_rate),
        config.initial_e.ln().clamp(min_log_rate, max_log_rate),
    ];

    let starts = optimization_starts(config, primary_start, min_log_rate, max_log_rate);
    let mut total_iterations = 0;
    let mut total_evaluations = 0;
    let mut best_run: Option<SingleOptimizationRun<2>> = None;

    for start in &starts {
        let run = optimize_de_from_start(
            engine,
            tip_likelihoods,
            *start,
            min_log_rate,
            max_log_rate,
            config,
            model_factory,
        );
        total_iterations += run.iterations;
        total_evaluations += run.evaluations;

        let is_best = match &best_run {
            Some(best) => run.best.objective < best.best.objective,
            None => true,
        };
        if is_best {
            best_run = Some(run);
        }
    }

    let best_run = best_run.expect("optimization_starts should produce at least one start");
    let best = best_run.best;
    if !best.objective.is_finite() {
        return Err(DecOptimizationError::NoFiniteLikelihood);
    }

    let d = best.coordinates[0].exp();
    let e = best.coordinates[1].exp();
    let model = model_factory(d, e)?;
    let pruning = engine
        .evaluate(&model, tip_likelihoods)
        .map_err(DecAnalysisError::from)?;

    Ok(DecOptimizationResult {
        d,
        e,
        log_likelihood: pruning.log_likelihood,
        iterations: total_iterations,
        evaluations: total_evaluations,
        converged: best_run.converged,
        starts: starts.len(),
        pruning,
    })
}

pub fn profile_de_pair_with_model<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecPairProfileConfig,
    model_factory: F,
) -> Result<DecPairProfileResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    profile_de_pair_with_model_likelihoods(
        tree,
        states,
        &tip_likelihoods,
        root_prior,
        config,
        model_factory,
    )
}

pub fn profile_de_pair_with_model_likelihoods<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    config: DecPairProfileConfig,
    model_factory: F,
) -> Result<DecPairProfileResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    validate_pair_profile_config(&config)?;
    config
        .de
        .range_size
        .validate()
        .map_err(DecAnalysisError::from)?;
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    let mut points = Vec::with_capacity(config.first.values.len() * config.second.values.len());
    let mut total_iterations = 0;
    let mut total_evaluations = 0;
    let mut converged_points = 0;

    for &first in &config.first.values {
        for &second in &config.second.values {
            let point_factory = |d, e| model_factory(d, e, first, second);
            let optimized = match optimize_de_with_engine(
                &engine,
                tip_likelihoods,
                config.de,
                &point_factory,
            ) {
                Ok(result) => result,
                Err(DecOptimizationError::NoFiniteLikelihood) => {
                    points.push(DecPairProfilePoint {
                        first,
                        second,
                        d: f64::NAN,
                        e: f64::NAN,
                        log_likelihood: f64::NEG_INFINITY,
                        delta_log_likelihood: f64::INFINITY,
                        finite: false,
                        within_support: false,
                        iterations: 0,
                        evaluations: 0,
                        converged: false,
                        starts: 0,
                    });
                    continue;
                }
                Err(error) => return Err(error),
            };

            total_iterations += optimized.iterations;
            total_evaluations += optimized.evaluations;
            if optimized.converged {
                converged_points += 1;
            }
            points.push(DecPairProfilePoint {
                first,
                second,
                d: optimized.d,
                e: optimized.e,
                log_likelihood: optimized.log_likelihood,
                delta_log_likelihood: 0.0,
                finite: true,
                within_support: false,
                iterations: optimized.iterations,
                evaluations: optimized.evaluations,
                converged: optimized.converged,
                starts: optimized.starts,
            });
        }
    }

    let best_point_index = points
        .iter()
        .enumerate()
        .filter(|(_, point)| point.finite)
        .max_by(|(_, left), (_, right)| left.log_likelihood.total_cmp(&right.log_likelihood))
        .map(|(index, _)| index)
        .ok_or(DecOptimizationError::NoFiniteLikelihood)?;
    let best_log_likelihood = points[best_point_index].log_likelihood;
    for point in &mut points {
        if point.finite {
            point.delta_log_likelihood = (best_log_likelihood - point.log_likelihood).max(0.0);
            point.within_support = point.delta_log_likelihood <= config.support_delta;
        }
    }

    let best = &points[best_point_index];
    let first_support = profile_support_span(&points, true);
    let second_support = profile_support_span(&points, false);
    let support_points = points.iter().filter(|point| point.within_support).count();
    let finite_points = points.iter().filter(|point| point.finite).count();
    let likelihood_weighted_correlation = likelihood_weighted_correlation(&points);
    let best_first_grid_bound = classify_grid_bound(best.first, &config.first.values);
    let best_second_grid_bound = classify_grid_bound(best.second, &config.second.values);

    Ok(DecPairProfileResult {
        first_parameter: config.first.parameter,
        second_parameter: config.second.parameter,
        support_delta: config.support_delta,
        best_point_index,
        best_first_grid_bound,
        best_second_grid_bound,
        first_support,
        second_support,
        support_points,
        finite_points,
        failed_points: points.len() - finite_points,
        converged_points,
        likelihood_weighted_correlation,
        total_iterations,
        total_evaluations,
        points,
    })
}

pub fn optimize_de_exponent_with_model<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecExponentOptimizationConfig,
    model_factory: F,
) -> Result<DecExponentOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    optimize_de_exponent_with_model_likelihoods(
        tree,
        states,
        &tip_likelihoods,
        root_prior,
        config,
        model_factory,
    )
}

pub fn optimize_de_exponent_with_model_likelihoods<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    config: DecExponentOptimizationConfig,
    model_factory: F,
) -> Result<DecExponentOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    validate_exponent_optimization_config(config)?;
    config
        .de
        .range_size
        .validate()
        .map_err(DecAnalysisError::from)?;
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    let min_log_rate = config.de.min_rate.ln();
    let max_log_rate = config.de.max_rate.ln();
    let primary_start = [
        config.de.initial_d.ln().clamp(min_log_rate, max_log_rate),
        config.de.initial_e.ln().clamp(min_log_rate, max_log_rate),
        config.initial_exponent,
    ];

    let starts = exponent_optimization_starts(config, primary_start, min_log_rate, max_log_rate);
    let mut total_iterations = 0;
    let mut total_evaluations = 0;
    let mut converged_starts = 0;
    let mut best_run: Option<SingleOptimizationRun<3>> = None;

    for start in &starts {
        let run = optimize_de_exponent_from_start(
            &engine,
            tip_likelihoods,
            *start,
            min_log_rate,
            max_log_rate,
            config,
            &model_factory,
        );
        total_iterations += run.iterations;
        total_evaluations += run.evaluations;
        if run.converged {
            converged_starts += 1;
        }

        let is_best = match &best_run {
            Some(best) => run.best.objective < best.best.objective,
            None => true,
        };
        if is_best {
            best_run = Some(run);
        }
    }

    let best_run =
        best_run.expect("exponent_optimization_starts should produce at least one start");
    let best = best_run.best;
    if !best.objective.is_finite() {
        return Err(DecOptimizationError::NoFiniteLikelihood);
    }

    let d = best.coordinates[0].exp();
    let e = best.coordinates[1].exp();
    let exponent = best.coordinates[2];
    let model = model_factory(d, e, exponent)?;
    let pruning = engine
        .evaluate(&model, tip_likelihoods)
        .map_err(DecAnalysisError::from)?;

    Ok(DecExponentOptimizationResult {
        d,
        e,
        exponent,
        exponent_bound: classify_optimization_bound(
            exponent,
            config.min_exponent,
            config.max_exponent,
        ),
        log_likelihood: pruning.log_likelihood,
        iterations: total_iterations,
        evaluations: total_evaluations,
        converged: best_run.converged,
        converged_starts,
        starts: starts.len(),
        pruning,
    })
}

pub fn optimize_de_xnu_with_model<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecXnuOptimizationConfig,
    model_factory: F,
) -> Result<DecXnuOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    optimize_de_xnu_with_model_likelihoods(
        tree,
        states,
        &tip_likelihoods,
        root_prior,
        config,
        model_factory,
    )
}

pub fn optimize_de_xnu_with_model_likelihoods<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    config: DecXnuOptimizationConfig,
    model_factory: F,
) -> Result<DecXnuOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    validate_xnu_optimization_config(config)?;
    config
        .de
        .range_size
        .validate()
        .map_err(DecAnalysisError::from)?;
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    let min_log_rate = config.de.min_rate.ln();
    let max_log_rate = config.de.max_rate.ln();
    let primary_start = [
        config.de.initial_d.ln().clamp(min_log_rate, max_log_rate),
        config.de.initial_e.ln().clamp(min_log_rate, max_log_rate),
        config.initial_x,
        config.initial_n,
        config.initial_u,
    ];
    let starts = xnu_optimization_starts(config, primary_start, min_log_rate, max_log_rate);
    let mut total_iterations = 0;
    let mut total_evaluations = 0;
    let mut converged_starts = 0;
    let mut best_run: Option<SingleOptimizationRun<5>> = None;

    for start in &starts {
        let run = optimize_de_xnu_from_start(
            &engine,
            tip_likelihoods,
            *start,
            min_log_rate,
            max_log_rate,
            config,
            &model_factory,
        );
        total_iterations += run.iterations;
        total_evaluations += run.evaluations;
        if run.converged {
            converged_starts += 1;
        }

        let is_best = match &best_run {
            Some(best) => run.best.objective < best.best.objective,
            None => true,
        };
        if is_best {
            best_run = Some(run);
        }
    }

    let best_run = best_run.expect("xnu_optimization_starts should produce at least one start");
    let best = best_run.best;
    if !best.objective.is_finite() {
        return Err(DecOptimizationError::NoFiniteLikelihood);
    }

    let d = best.coordinates[0].exp();
    let e = best.coordinates[1].exp();
    let x = best.coordinates[2];
    let n = best.coordinates[3];
    let u = best.coordinates[4];
    let model = model_factory(d, e, x, n, u)?;
    let pruning = engine
        .evaluate(&model, tip_likelihoods)
        .map_err(DecAnalysisError::from)?;

    Ok(DecXnuOptimizationResult {
        d,
        e,
        x,
        n,
        u,
        x_bound: classify_optimization_bound(x, config.min_x, config.max_x),
        n_bound: classify_optimization_bound(n, config.min_n, config.max_n),
        u_bound: classify_optimization_bound(u, config.min_u, config.max_u),
        log_likelihood: pruning.log_likelihood,
        iterations: total_iterations,
        evaluations: total_evaluations,
        converged: best_run.converged,
        converged_starts,
        starts: starts.len(),
        pruning,
    })
}

pub fn optimize_decj_dej(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecJOptimizationConfig,
) -> Result<DecJOptimizationResult, DecOptimizationError> {
    validate_preset_j_upper_bound(config, "DEC+J", 3.0)?;
    let range_size = config.range_size;
    optimize_decj_dej_with_model(
        tree,
        states,
        tip_ranges,
        root_prior,
        config,
        move |d, e, j| Ok(ModelConfig::preset_dec_j(d, e, j)?.with_range_size_config(range_size)),
    )
}

pub fn optimize_divalikej_dej(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecJOptimizationConfig,
) -> Result<DecJOptimizationResult, DecOptimizationError> {
    validate_preset_j_upper_bound(config, "DIVALIKE+J", 2.0)?;
    let range_size = config.range_size;
    optimize_decj_dej_with_model(
        tree,
        states,
        tip_ranges,
        root_prior,
        config,
        move |d, e, j| {
            Ok(ModelConfig::preset_divalike_j(d, e, j)?.with_range_size_config(range_size))
        },
    )
}

pub fn optimize_bayarealikej_dej(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecJOptimizationConfig,
) -> Result<DecJOptimizationResult, DecOptimizationError> {
    validate_preset_j_upper_bound(config, "BAYAREALIKE+J", 1.0)?;
    let range_size = config.range_size;
    optimize_decj_dej_with_model(
        tree,
        states,
        tip_ranges,
        root_prior,
        config,
        move |d, e, j| {
            Ok(ModelConfig::preset_bayarealike_j(d, e, j)?.with_range_size_config(range_size))
        },
    )
}

pub fn optimize_decj_dej_with_model<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    config: DecJOptimizationConfig,
    model_factory: F,
) -> Result<DecJOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    optimize_decj_dej_with_model_likelihoods(
        tree,
        states,
        &tip_likelihoods,
        root_prior,
        config,
        model_factory,
    )
}

pub fn optimize_decj_dej_with_model_likelihoods<F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    config: DecJOptimizationConfig,
    model_factory: F,
) -> Result<DecJOptimizationResult, DecOptimizationError>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    validate_decj_optimization_config(config)?;
    config
        .range_size
        .validate()
        .map_err(DecAnalysisError::from)?;
    let engine = LikelihoodEngine::new(tree, states, root_prior);

    let min_log_rate = config.min_rate.ln();
    let max_log_rate = config.max_rate.ln();
    let primary_start = [
        config.initial_d.ln().clamp(min_log_rate, max_log_rate),
        config.initial_e.ln().clamp(min_log_rate, max_log_rate),
        bounded_to_unbounded(config.initial_j, config.min_j, config.max_j),
    ];

    let starts = decj_optimization_starts(config, primary_start, min_log_rate, max_log_rate);
    let mut total_iterations = 0;
    let mut total_evaluations = 0;
    let mut best_run: Option<SingleOptimizationRun<3>> = None;

    for start in &starts {
        let run = optimize_decj_dej_from_start(
            &engine,
            tip_likelihoods,
            *start,
            min_log_rate,
            max_log_rate,
            config,
            &model_factory,
        );
        total_iterations += run.iterations;
        total_evaluations += run.evaluations;

        let is_best = match &best_run {
            Some(best) => run.best.objective < best.best.objective,
            None => true,
        };
        if is_best {
            best_run = Some(run);
        }
    }

    let best_run = best_run.expect("decj_optimization_starts should produce at least one start");
    let best = best_run.best;
    if !best.objective.is_finite() {
        return Err(DecOptimizationError::NoFiniteLikelihood);
    }

    let d = best.coordinates[0].exp();
    let e = best.coordinates[1].exp();
    let j = unbounded_to_bounded(best.coordinates[2], config.min_j, config.max_j);
    let model = model_factory(d, e, j)?;
    let pruning = engine
        .evaluate(&model, tip_likelihoods)
        .map_err(DecAnalysisError::from)?;

    Ok(DecJOptimizationResult {
        d,
        e,
        j,
        log_likelihood: pruning.log_likelihood,
        iterations: total_iterations,
        evaluations: total_evaluations,
        converged: best_run.converged,
        starts: starts.len(),
        pruning,
    })
}

pub fn tip_ranges_to_likelihoods(
    states: &StateSpace,
    tip_ranges: &[TipRange],
) -> Result<Vec<TipLikelihood>, DecAnalysisError> {
    tip_ranges
        .iter()
        .map(|tip_range| {
            let state_index = states.index_of(tip_range.range).ok_or(
                DecAnalysisError::TipRangeNotInStateSpace {
                    node: tip_range.node,
                    bits: tip_range.range.bits(),
                },
            )?;

            let mut likelihoods = vec![0.0; states.len()];
            likelihoods[state_index] = 1.0;

            Ok(TipLikelihood {
                node: tip_range.node,
                likelihoods,
            })
        })
        .collect()
}

fn validate_optimization_config(config: DecOptimizationConfig) -> Result<(), DecOptimizationError> {
    validate_positive_finite("initial_d", config.initial_d)?;
    validate_positive_finite("initial_e", config.initial_e)?;
    validate_positive_finite("min_rate", config.min_rate)?;
    validate_positive_finite("max_rate", config.max_rate)?;
    validate_positive_finite("initial_log_step", config.initial_log_step)?;
    validate_positive_finite("tolerance", config.tolerance)?;

    if config.min_rate >= config.max_rate {
        return Err(DecOptimizationError::InvalidRateBounds {
            min_rate: config.min_rate,
            max_rate: config.max_rate,
        });
    }
    if config.initial_d < config.min_rate || config.initial_d > config.max_rate {
        return Err(DecOptimizationError::InitialRateOutOfBounds {
            name: "initial_d",
            value: config.initial_d,
            min_rate: config.min_rate,
            max_rate: config.max_rate,
        });
    }
    if config.initial_e < config.min_rate || config.initial_e > config.max_rate {
        return Err(DecOptimizationError::InitialRateOutOfBounds {
            name: "initial_e",
            value: config.initial_e,
            min_rate: config.min_rate,
            max_rate: config.max_rate,
        });
    }
    if config.max_iterations == 0 {
        return Err(DecOptimizationError::ZeroMaxIterations);
    }
    if config.multi_start_points_per_axis == 0 {
        return Err(DecOptimizationError::ZeroMultiStartPoints);
    }

    Ok(())
}

fn validate_exponent_optimization_config(
    config: DecExponentOptimizationConfig,
) -> Result<(), DecOptimizationError> {
    validate_optimization_config(config.de)?;
    validate_positive_finite("initial_exponent_step", config.initial_exponent_step)?;

    if !config.min_exponent.is_finite()
        || !config.max_exponent.is_finite()
        || config.min_exponent >= config.max_exponent
    {
        return Err(DecOptimizationError::InvalidExponentBounds {
            min_exponent: config.min_exponent,
            max_exponent: config.max_exponent,
        });
    }
    if !config.initial_exponent.is_finite()
        || config.initial_exponent <= config.min_exponent
        || config.initial_exponent >= config.max_exponent
    {
        return Err(DecOptimizationError::InitialExponentOutOfBounds {
            value: config.initial_exponent,
            min_exponent: config.min_exponent,
            max_exponent: config.max_exponent,
        });
    }

    Ok(())
}

fn validate_xnu_optimization_config(
    config: DecXnuOptimizationConfig,
) -> Result<(), DecOptimizationError> {
    validate_optimization_config(config.de)?;
    validate_named_exponent(
        "x",
        config.initial_x,
        config.min_x,
        config.max_x,
        config.initial_x_step,
    )?;
    validate_named_exponent(
        "n",
        config.initial_n,
        config.min_n,
        config.max_n,
        config.initial_n_step,
    )?;
    validate_named_exponent(
        "u",
        config.initial_u,
        config.min_u,
        config.max_u,
        config.initial_u_step,
    )?;
    Ok(())
}

fn validate_named_exponent(
    name: &'static str,
    initial: f64,
    min: f64,
    max: f64,
    initial_step: f64,
) -> Result<(), DecOptimizationError> {
    validate_positive_finite(name_for_exponent_step(name), initial_step)?;
    if !min.is_finite() || !max.is_finite() || min >= max {
        return Err(DecOptimizationError::InvalidNamedExponentBounds { name, min, max });
    }
    if !initial.is_finite() || initial <= min || initial >= max {
        return Err(DecOptimizationError::InitialNamedExponentOutOfBounds {
            name,
            value: initial,
            min,
            max,
        });
    }
    Ok(())
}

fn name_for_exponent_step(name: &'static str) -> &'static str {
    match name {
        "x" => "initial_x_step",
        "n" => "initial_n_step",
        "u" => "initial_u_step",
        _ => "initial_exponent_step",
    }
}

fn validate_pair_profile_config(config: &DecPairProfileConfig) -> Result<(), DecOptimizationError> {
    validate_optimization_config(config.de)?;
    validate_profile_axis(&config.first)?;
    validate_profile_axis(&config.second)?;

    if config.first.parameter == config.second.parameter {
        return Err(DecOptimizationError::DuplicateProfileParameter(
            config.first.parameter.clone(),
        ));
    }
    if !config.support_delta.is_finite() || config.support_delta <= 0.0 {
        return Err(DecOptimizationError::InvalidProfileSupportDelta(
            config.support_delta,
        ));
    }

    Ok(())
}

fn validate_profile_axis(axis: &DecProfileAxis) -> Result<(), DecOptimizationError> {
    if axis.parameter.trim().is_empty() {
        return Err(DecOptimizationError::EmptyProfileParameter);
    }
    if axis.values.len() < 2 {
        return Err(DecOptimizationError::ProfileAxisTooShort {
            parameter: axis.parameter.clone(),
            values: axis.values.len(),
        });
    }
    for (index, &value) in axis.values.iter().enumerate() {
        if !value.is_finite() {
            return Err(DecOptimizationError::InvalidProfileAxisValue {
                parameter: axis.parameter.clone(),
                index,
                value,
            });
        }
        if index > 0 && value <= axis.values[index - 1] {
            return Err(DecOptimizationError::NonIncreasingProfileAxis {
                parameter: axis.parameter.clone(),
                index,
                previous: axis.values[index - 1],
                value,
            });
        }
    }

    Ok(())
}

fn profile_support_span(points: &[DecPairProfilePoint], use_first: bool) -> DecProfileSupportSpan {
    let mut values: Vec<f64> = points
        .iter()
        .filter(|point| point.within_support)
        .map(|point| if use_first { point.first } else { point.second })
        .collect();
    values.sort_by(f64::total_cmp);
    values.dedup_by(|left, right| left.total_cmp(right).is_eq());

    DecProfileSupportSpan {
        min: values[0],
        max: values[values.len() - 1],
        grid_values: values.len(),
    }
}

fn classify_grid_bound(value: f64, values: &[f64]) -> Option<OptimizationBound> {
    if value.total_cmp(&values[0]).is_eq() {
        Some(OptimizationBound::Lower)
    } else if value.total_cmp(&values[values.len() - 1]).is_eq() {
        Some(OptimizationBound::Upper)
    } else {
        None
    }
}

fn likelihood_weighted_correlation(points: &[DecPairProfilePoint]) -> Option<f64> {
    let total_weight: f64 = points
        .iter()
        .map(|point| (-point.delta_log_likelihood).exp())
        .sum();
    let mean_first = points
        .iter()
        .map(|point| (-point.delta_log_likelihood).exp() * point.first)
        .sum::<f64>()
        / total_weight;
    let mean_second = points
        .iter()
        .map(|point| (-point.delta_log_likelihood).exp() * point.second)
        .sum::<f64>()
        / total_weight;

    let mut variance_first = 0.0;
    let mut variance_second = 0.0;
    let mut covariance = 0.0;
    for point in points {
        let weight = (-point.delta_log_likelihood).exp() / total_weight;
        let first_delta = point.first - mean_first;
        let second_delta = point.second - mean_second;
        variance_first += weight * first_delta * first_delta;
        variance_second += weight * second_delta * second_delta;
        covariance += weight * first_delta * second_delta;
    }

    let first_scale = mean_first.abs().max(1.0);
    let second_scale = mean_second.abs().max(1.0);
    if variance_first <= f64::EPSILON * first_scale * first_scale
        || variance_second <= f64::EPSILON * second_scale * second_scale
    {
        return None;
    }

    Some((covariance / (variance_first * variance_second).sqrt()).clamp(-1.0, 1.0))
}

fn validate_decj_optimization_config(
    config: DecJOptimizationConfig,
) -> Result<(), DecOptimizationError> {
    validate_positive_finite("initial_d", config.initial_d)?;
    validate_positive_finite("initial_e", config.initial_e)?;
    validate_positive_finite("min_rate", config.min_rate)?;
    validate_positive_finite("max_rate", config.max_rate)?;
    validate_positive_finite("initial_log_step", config.initial_log_step)?;
    validate_positive_finite("tolerance", config.tolerance)?;

    if config.min_rate >= config.max_rate {
        return Err(DecOptimizationError::InvalidRateBounds {
            min_rate: config.min_rate,
            max_rate: config.max_rate,
        });
    }
    if config.initial_d < config.min_rate || config.initial_d > config.max_rate {
        return Err(DecOptimizationError::InitialRateOutOfBounds {
            name: "initial_d",
            value: config.initial_d,
            min_rate: config.min_rate,
            max_rate: config.max_rate,
        });
    }
    if config.initial_e < config.min_rate || config.initial_e > config.max_rate {
        return Err(DecOptimizationError::InitialRateOutOfBounds {
            name: "initial_e",
            value: config.initial_e,
            min_rate: config.min_rate,
            max_rate: config.max_rate,
        });
    }
    if !config.min_j.is_finite()
        || !config.max_j.is_finite()
        || config.min_j < 0.0
        || config.max_j > 3.0
        || config.min_j >= config.max_j
    {
        return Err(DecOptimizationError::InvalidJBounds {
            min_j: config.min_j,
            max_j: config.max_j,
        });
    }
    if !config.initial_j.is_finite()
        || config.initial_j <= config.min_j
        || config.initial_j >= config.max_j
    {
        return Err(DecOptimizationError::InitialJOutOfBounds {
            value: config.initial_j,
            min_j: config.min_j,
            max_j: config.max_j,
        });
    }
    if config.max_iterations == 0 {
        return Err(DecOptimizationError::ZeroMaxIterations);
    }
    if config.multi_start_points_per_axis == 0 {
        return Err(DecOptimizationError::ZeroMultiStartPoints);
    }

    Ok(())
}

fn validate_preset_j_upper_bound(
    config: DecJOptimizationConfig,
    preset: &'static str,
    upper_exclusive: f64,
) -> Result<(), DecOptimizationError> {
    if config.max_j >= upper_exclusive {
        return Err(DecOptimizationError::InvalidPresetJUpperBound {
            preset,
            max_j: config.max_j,
            upper_exclusive,
        });
    }

    Ok(())
}

fn validate_positive_finite(name: &'static str, value: f64) -> Result<(), DecOptimizationError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(DecOptimizationError::InvalidPositiveValue { name, value });
    }

    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct OptimPoint<const N: usize> {
    coordinates: [f64; N],
    objective: f64,
}

#[derive(Clone, Copy, Debug)]
struct SingleOptimizationRun<const N: usize> {
    best: OptimPoint<N>,
    iterations: usize,
    evaluations: usize,
    converged: bool,
}

fn optimize_de_from_start<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    start: [f64; 2],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecOptimizationConfig,
    model_factory: &F,
) -> SingleOptimizationRun<2>
where
    F: Fn(f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    optimize_from_start(
        start,
        [config.initial_log_step; 2],
        config.tolerance,
        config.max_iterations,
        |log_rates| {
            evaluate_log_rates(
                engine,
                tip_likelihoods,
                log_rates,
                min_log_rate,
                max_log_rate,
                model_factory,
            )
        },
    )
}

fn optimize_de_exponent_from_start<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    start: [f64; 3],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecExponentOptimizationConfig,
    model_factory: &F,
) -> SingleOptimizationRun<3>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    optimize_from_start(
        start,
        [
            config.de.initial_log_step,
            config.de.initial_log_step,
            config.initial_exponent_step,
        ],
        config.de.tolerance,
        config.de.max_iterations,
        |coordinates| {
            evaluate_de_exponent_coordinates(
                engine,
                tip_likelihoods,
                coordinates,
                min_log_rate,
                max_log_rate,
                config,
                model_factory,
            )
        },
    )
}

fn optimize_de_xnu_from_start<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    start: [f64; 5],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecXnuOptimizationConfig,
    model_factory: &F,
) -> SingleOptimizationRun<5>
where
    F: Fn(f64, f64, f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    optimize_from_start(
        start,
        [
            config.de.initial_log_step,
            config.de.initial_log_step,
            config.initial_x_step,
            config.initial_n_step,
            config.initial_u_step,
        ],
        config.de.tolerance,
        config.de.max_iterations,
        |coordinates| {
            evaluate_de_xnu_coordinates(
                engine,
                tip_likelihoods,
                coordinates,
                min_log_rate,
                max_log_rate,
                config,
                model_factory,
            )
        },
    )
}

fn optimize_decj_dej_from_start(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    start: [f64; 3],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecJOptimizationConfig,
    model_factory: &impl Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
) -> SingleOptimizationRun<3> {
    optimize_from_start(
        start,
        [config.initial_log_step; 3],
        config.tolerance,
        config.max_iterations,
        |coordinates| {
            evaluate_decj_coordinates(
                engine,
                tip_likelihoods,
                coordinates,
                min_log_rate,
                max_log_rate,
                config,
                model_factory,
            )
        },
    )
}

fn optimize_from_start<const N: usize>(
    start: [f64; N],
    initial_steps: [f64; N],
    tolerance: f64,
    max_iterations: usize,
    mut evaluate: impl FnMut([f64; N]) -> OptimPoint<N>,
) -> SingleOptimizationRun<N> {
    let mut evaluations = 0;
    let mut evaluate_counted = |coordinates: [f64; N]| {
        evaluations += 1;
        evaluate(coordinates)
    };

    let mut simplex = Vec::with_capacity(N + 1);
    simplex.push(evaluate_counted(start));
    for axis in 0..N {
        let mut point = start;
        point[axis] += initial_steps[axis];
        simplex.push(evaluate_counted(point));
    }

    let mut iterations = 0;
    let mut converged = false;

    for iteration in 0..max_iterations {
        sort_simplex(&mut simplex);
        if simplex_has_converged(&simplex, tolerance) {
            iterations = iteration;
            converged = true;
            break;
        }

        let worst_index = simplex.len() - 1;
        let second_worst_index = worst_index - 1;
        let centroid = centroid_without_worst(&simplex);
        let reflected = evaluate_counted(add_scaled(
            centroid,
            subtract(centroid, simplex[worst_index].coordinates),
            1.0,
        ));

        if reflected.objective < simplex[0].objective {
            let expanded = evaluate_counted(add_scaled(
                centroid,
                subtract(reflected.coordinates, centroid),
                2.0,
            ));
            simplex[worst_index] = if expanded.objective < reflected.objective {
                expanded
            } else {
                reflected
            };
        } else if reflected.objective < simplex[second_worst_index].objective {
            simplex[worst_index] = reflected;
        } else {
            let contraction_direction = if reflected.objective < simplex[worst_index].objective {
                subtract(reflected.coordinates, centroid)
            } else {
                subtract(simplex[worst_index].coordinates, centroid)
            };
            let contracted = evaluate_counted(add_scaled(centroid, contraction_direction, 0.5));

            if contracted.objective < simplex[worst_index].objective {
                simplex[worst_index] = contracted;
            } else {
                let best_coordinates = simplex[0].coordinates;
                for point in simplex.iter_mut().skip(1) {
                    *point = evaluate_counted(shrink_toward(best_coordinates, point.coordinates));
                }
            }
        }

        iterations = iteration + 1;
    }

    sort_simplex(&mut simplex);

    SingleOptimizationRun {
        best: simplex[0],
        iterations,
        evaluations,
        converged,
    }
}

fn optimization_starts(
    config: DecOptimizationConfig,
    primary_start: [f64; 2],
    min_log_rate: f64,
    max_log_rate: f64,
) -> Vec<[f64; 2]> {
    let mut starts = Vec::new();
    push_unique_start(&mut starts, primary_start);

    if config.multi_start_points_per_axis == 1 {
        return starts;
    }

    let denominator = (config.multi_start_points_per_axis - 1) as f64;
    for d_index in 0..config.multi_start_points_per_axis {
        for e_index in 0..config.multi_start_points_per_axis {
            let d = interpolate_log_rate(min_log_rate, max_log_rate, d_index, denominator);
            let e = interpolate_log_rate(min_log_rate, max_log_rate, e_index, denominator);
            push_unique_start(&mut starts, [d, e]);
        }
    }

    starts
}

fn exponent_optimization_starts(
    config: DecExponentOptimizationConfig,
    primary_start: [f64; 3],
    min_log_rate: f64,
    max_log_rate: f64,
) -> Vec<[f64; 3]> {
    let mut starts = Vec::new();
    push_unique_start(&mut starts, primary_start);

    if config.de.multi_start_points_per_axis == 1 {
        return starts;
    }

    let rate_denominator = (config.de.multi_start_points_per_axis - 1) as f64;
    for d_index in 0..config.de.multi_start_points_per_axis {
        for e_index in 0..config.de.multi_start_points_per_axis {
            for exponent_index in 0..config.de.multi_start_points_per_axis {
                let d = interpolate_log_rate(min_log_rate, max_log_rate, d_index, rate_denominator);
                let e = interpolate_log_rate(min_log_rate, max_log_rate, e_index, rate_denominator);
                let exponent = interpolate_bounded_interior(
                    config.min_exponent,
                    config.max_exponent,
                    exponent_index,
                    config.de.multi_start_points_per_axis,
                );
                push_unique_start(&mut starts, [d, e, exponent]);
            }
        }
    }

    starts
}

fn xnu_optimization_starts(
    config: DecXnuOptimizationConfig,
    primary_start: [f64; 5],
    min_log_rate: f64,
    max_log_rate: f64,
) -> Vec<[f64; 5]> {
    let mut starts = Vec::new();
    push_unique_start(&mut starts, primary_start);

    let count = config.de.multi_start_points_per_axis;
    if count == 1 {
        return starts;
    }

    let rate_denominator = (count - 1) as f64;
    for d_index in 0..count {
        for e_index in 0..count {
            for x_index in 0..count {
                for n_index in 0..count {
                    for u_index in 0..count {
                        let d = interpolate_log_rate(
                            min_log_rate,
                            max_log_rate,
                            d_index,
                            rate_denominator,
                        );
                        let e = interpolate_log_rate(
                            min_log_rate,
                            max_log_rate,
                            e_index,
                            rate_denominator,
                        );
                        let x = interpolate_bounded_interior(
                            config.min_x,
                            config.max_x,
                            x_index,
                            count,
                        );
                        let n = interpolate_bounded_interior(
                            config.min_n,
                            config.max_n,
                            n_index,
                            count,
                        );
                        let u = interpolate_bounded_interior(
                            config.min_u,
                            config.max_u,
                            u_index,
                            count,
                        );
                        push_unique_start(&mut starts, [d, e, x, n, u]);
                    }
                }
            }
        }
    }

    starts
}

fn decj_optimization_starts(
    config: DecJOptimizationConfig,
    primary_start: [f64; 3],
    min_log_rate: f64,
    max_log_rate: f64,
) -> Vec<[f64; 3]> {
    let mut starts = Vec::new();
    push_unique_start(&mut starts, primary_start);

    if config.multi_start_points_per_axis == 1 {
        return starts;
    }

    let rate_denominator = (config.multi_start_points_per_axis - 1) as f64;
    for d_index in 0..config.multi_start_points_per_axis {
        for e_index in 0..config.multi_start_points_per_axis {
            for j_index in 0..config.multi_start_points_per_axis {
                let d = interpolate_log_rate(min_log_rate, max_log_rate, d_index, rate_denominator);
                let e = interpolate_log_rate(min_log_rate, max_log_rate, e_index, rate_denominator);
                let j = interpolate_bounded_interior(
                    config.min_j,
                    config.max_j,
                    j_index,
                    config.multi_start_points_per_axis,
                );
                push_unique_start(
                    &mut starts,
                    [d, e, bounded_to_unbounded(j, config.min_j, config.max_j)],
                );
            }
        }
    }

    starts
}

fn interpolate_log_rate(
    min_log_rate: f64,
    max_log_rate: f64,
    index: usize,
    denominator: f64,
) -> f64 {
    min_log_rate + (max_log_rate - min_log_rate) * index as f64 / denominator
}

fn interpolate_bounded_interior(min: f64, max: f64, index: usize, count: usize) -> f64 {
    min + (max - min) * (index as f64 + 1.0) / (count as f64 + 1.0)
}

fn push_unique_start<const N: usize>(starts: &mut Vec<[f64; N]>, candidate: [f64; N]) {
    let duplicate = starts
        .iter()
        .any(|existing| arrays_are_close(*existing, candidate, 1e-12));
    if !duplicate {
        starts.push(candidate);
    }
}

fn arrays_are_close<const N: usize>(left: [f64; N], right: [f64; N], tolerance: f64) -> bool {
    for axis in 0..N {
        if (left[axis] - right[axis]).abs() > tolerance {
            return false;
        }
    }

    true
}

fn evaluate_log_rates<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    log_rates: [f64; 2],
    min_log_rate: f64,
    max_log_rate: f64,
    model_factory: &F,
) -> OptimPoint<2>
where
    F: Fn(f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let clamped = [
        log_rates[0].clamp(min_log_rate, max_log_rate),
        log_rates[1].clamp(min_log_rate, max_log_rate),
    ];
    let d = clamped[0].exp();
    let e = clamped[1].exp();
    let objective = match model_factory(d, e) {
        Ok(model) => match engine.evaluate(&model, tip_likelihoods) {
            Ok(result) if result.log_likelihood.is_finite() => -result.log_likelihood,
            _ => f64::INFINITY,
        },
        Err(_) => f64::INFINITY,
    };

    OptimPoint {
        coordinates: clamped,
        objective,
    }
}

fn evaluate_de_exponent_coordinates<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    coordinates: [f64; 3],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecExponentOptimizationConfig,
    model_factory: &F,
) -> OptimPoint<3>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let clamped = [
        coordinates[0].clamp(min_log_rate, max_log_rate),
        coordinates[1].clamp(min_log_rate, max_log_rate),
        coordinates[2].clamp(config.min_exponent, config.max_exponent),
    ];
    let d = clamped[0].exp();
    let e = clamped[1].exp();
    let objective = match model_factory(d, e, clamped[2]) {
        Ok(model) => match engine.evaluate(&model, tip_likelihoods) {
            Ok(result) if result.log_likelihood.is_finite() => -result.log_likelihood,
            _ => f64::INFINITY,
        },
        Err(_) => f64::INFINITY,
    };

    OptimPoint {
        coordinates: clamped,
        objective,
    }
}

fn evaluate_de_xnu_coordinates<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    coordinates: [f64; 5],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecXnuOptimizationConfig,
    model_factory: &F,
) -> OptimPoint<5>
where
    F: Fn(f64, f64, f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let clamped = [
        coordinates[0].clamp(min_log_rate, max_log_rate),
        coordinates[1].clamp(min_log_rate, max_log_rate),
        coordinates[2].clamp(config.min_x, config.max_x),
        coordinates[3].clamp(config.min_n, config.max_n),
        coordinates[4].clamp(config.min_u, config.max_u),
    ];
    let d = clamped[0].exp();
    let e = clamped[1].exp();
    let objective = match model_factory(d, e, clamped[2], clamped[3], clamped[4]) {
        Ok(model) => match engine.evaluate(&model, tip_likelihoods) {
            Ok(result) if result.log_likelihood.is_finite() => -result.log_likelihood,
            _ => f64::INFINITY,
        },
        Err(_) => f64::INFINITY,
    };

    OptimPoint {
        coordinates: clamped,
        objective,
    }
}

fn evaluate_decj_coordinates<F>(
    engine: &LikelihoodEngine<'_>,
    tip_likelihoods: &[TipLikelihood],
    coordinates: [f64; 3],
    min_log_rate: f64,
    max_log_rate: f64,
    config: DecJOptimizationConfig,
    model_factory: &F,
) -> OptimPoint<3>
where
    F: Fn(f64, f64, f64) -> Result<ModelConfig, DecAnalysisError>,
{
    let clamped = [
        coordinates[0].clamp(min_log_rate, max_log_rate),
        coordinates[1].clamp(min_log_rate, max_log_rate),
        coordinates[2],
    ];
    let d = clamped[0].exp();
    let e = clamped[1].exp();
    let j = unbounded_to_bounded(clamped[2], config.min_j, config.max_j);
    let objective = match model_factory(d, e, j) {
        Ok(model) => match engine.evaluate(&model, tip_likelihoods) {
            Ok(result) if result.log_likelihood.is_finite() => -result.log_likelihood,
            _ => f64::INFINITY,
        },
        Err(_) => f64::INFINITY,
    };

    OptimPoint {
        coordinates: clamped,
        objective,
    }
}

fn bounded_to_unbounded(value: f64, min: f64, max: f64) -> f64 {
    let proportion = ((value - min) / (max - min)).clamp(f64::MIN_POSITIVE, 1.0 - f64::EPSILON);
    (proportion / (1.0 - proportion)).ln()
}

fn unbounded_to_bounded(value: f64, min: f64, max: f64) -> f64 {
    let proportion = if value >= 0.0 {
        1.0 / (1.0 + (-value).exp())
    } else {
        let exp_value = value.exp();
        exp_value / (1.0 + exp_value)
    };

    min + (max - min) * proportion
}

fn classify_optimization_bound(value: f64, min: f64, max: f64) -> Option<OptimizationBound> {
    let tolerance = (max - min).abs().max(1.0) * 1e-8;
    if (value - min).abs() <= tolerance {
        Some(OptimizationBound::Lower)
    } else if (value - max).abs() <= tolerance {
        Some(OptimizationBound::Upper)
    } else {
        None
    }
}

fn sort_simplex<const N: usize>(simplex: &mut [OptimPoint<N>]) {
    simplex.sort_by(|left, right| left.objective.total_cmp(&right.objective));
}

fn simplex_has_converged<const N: usize>(simplex: &[OptimPoint<N>], tolerance: f64) -> bool {
    let best = simplex[0];
    let worst = simplex[simplex.len() - 1];
    let value_spread = (worst.objective - best.objective).abs();
    let max_distance = simplex
        .iter()
        .skip(1)
        .map(|point| euclidean_distance(best.coordinates, point.coordinates))
        .fold(0.0, f64::max);

    value_spread <= tolerance && max_distance <= tolerance
}

fn centroid_without_worst<const N: usize>(simplex: &[OptimPoint<N>]) -> [f64; N] {
    let count = simplex.len() - 1;
    let mut centroid = [0.0; N];
    for point in &simplex[..count] {
        for (axis, value) in centroid.iter_mut().enumerate() {
            *value += point.coordinates[axis];
        }
    }
    for value in &mut centroid {
        *value /= count as f64;
    }

    centroid
}

fn subtract<const N: usize>(left: [f64; N], right: [f64; N]) -> [f64; N] {
    let mut result = [0.0; N];
    for axis in 0..N {
        result[axis] = left[axis] - right[axis];
    }

    result
}

fn add_scaled<const N: usize>(origin: [f64; N], direction: [f64; N], scale: f64) -> [f64; N] {
    let mut result = [0.0; N];
    for axis in 0..N {
        result[axis] = origin[axis] + direction[axis] * scale;
    }

    result
}

fn shrink_toward<const N: usize>(best: [f64; N], point: [f64; N]) -> [f64; N] {
    let mut result = [0.0; N];
    for axis in 0..N {
        result[axis] = best[axis] + 0.5 * (point[axis] - best[axis]);
    }

    result
}

fn euclidean_distance<const N: usize>(left: [f64; N], right: [f64; N]) -> f64 {
    let mut squared_sum = 0.0;
    for axis in 0..N {
        let delta = left[axis] - right[axis];
        squared_sum += delta * delta;
    }

    squared_sum.sqrt()
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecAnalysisError {
    TipRangeNotInStateSpace { node: usize, bits: u64 },
    Model(DecModelError),
    Anagenesis(AnagenesisError),
    Cladogenesis(CladogenesisError),
    Pruning(PruningError),
}

impl fmt::Display for DecAnalysisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TipRangeNotInStateSpace { node, bits } => write!(
                f,
                "tip range for node {node} with bits {bits:#b} is not in the state space"
            ),
            Self::Model(error) => write!(f, "DEC model setup failed: {error}"),
            Self::Anagenesis(error) => write!(f, "DEC anagenesis setup failed: {error}"),
            Self::Cladogenesis(error) => write!(f, "DEC cladogenesis setup failed: {error}"),
            Self::Pruning(error) => write!(f, "DEC pruning failed: {error}"),
        }
    }
}

impl Error for DecAnalysisError {}

impl From<DecModelError> for DecAnalysisError {
    fn from(value: DecModelError) -> Self {
        Self::Model(value)
    }
}

impl From<AnagenesisError> for DecAnalysisError {
    fn from(value: AnagenesisError) -> Self {
        Self::Anagenesis(value)
    }
}

impl From<CladogenesisError> for DecAnalysisError {
    fn from(value: CladogenesisError) -> Self {
        Self::Cladogenesis(value)
    }
}

impl From<PruningError> for DecAnalysisError {
    fn from(value: PruningError) -> Self {
        Self::Pruning(value)
    }
}

impl From<LikelihoodEngineError> for DecAnalysisError {
    fn from(value: LikelihoodEngineError) -> Self {
        match value {
            LikelihoodEngineError::Anagenesis(error) => Self::Anagenesis(error),
            LikelihoodEngineError::Cladogenesis(error) => Self::Cladogenesis(error),
            LikelihoodEngineError::Pruning(error) => Self::Pruning(error),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecOptimizationError {
    InvalidPositiveValue {
        name: &'static str,
        value: f64,
    },
    InvalidRateBounds {
        min_rate: f64,
        max_rate: f64,
    },
    InitialRateOutOfBounds {
        name: &'static str,
        value: f64,
        min_rate: f64,
        max_rate: f64,
    },
    InvalidExponentBounds {
        min_exponent: f64,
        max_exponent: f64,
    },
    InitialExponentOutOfBounds {
        value: f64,
        min_exponent: f64,
        max_exponent: f64,
    },
    InvalidNamedExponentBounds {
        name: &'static str,
        min: f64,
        max: f64,
    },
    InitialNamedExponentOutOfBounds {
        name: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },
    InvalidJBounds {
        min_j: f64,
        max_j: f64,
    },
    InvalidPresetJUpperBound {
        preset: &'static str,
        max_j: f64,
        upper_exclusive: f64,
    },
    InitialJOutOfBounds {
        value: f64,
        min_j: f64,
        max_j: f64,
    },
    ZeroMaxIterations,
    ZeroMultiStartPoints,
    EmptyProfileParameter,
    DuplicateProfileParameter(String),
    ProfileAxisTooShort {
        parameter: String,
        values: usize,
    },
    InvalidProfileAxisValue {
        parameter: String,
        index: usize,
        value: f64,
    },
    NonIncreasingProfileAxis {
        parameter: String,
        index: usize,
        previous: f64,
        value: f64,
    },
    InvalidProfileSupportDelta(f64),
    ProfilePointNoFiniteLikelihood {
        first: f64,
        second: f64,
    },
    NoFiniteLikelihood,
    Analysis(DecAnalysisError),
}

impl fmt::Display for DecOptimizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPositiveValue { name, value } => {
                write!(f, "{name} must be finite and positive, got {value}")
            }
            Self::InvalidRateBounds { min_rate, max_rate } => write!(
                f,
                "optimization min_rate must be less than max_rate, got min_rate={min_rate}, max_rate={max_rate}"
            ),
            Self::InitialRateOutOfBounds {
                name,
                value,
                min_rate,
                max_rate,
            } => write!(
                f,
                "{name}={value} is outside optimization bounds [{min_rate}, {max_rate}]"
            ),
            Self::InvalidExponentBounds {
                min_exponent,
                max_exponent,
            } => write!(
                f,
                "optimization exponent bounds must be finite with min_exponent < max_exponent, got min_exponent={min_exponent}, max_exponent={max_exponent}"
            ),
            Self::InitialExponentOutOfBounds {
                value,
                min_exponent,
                max_exponent,
            } => write!(
                f,
                "initial_exponent={value} must be finite and strictly inside optimization bounds ({min_exponent}, {max_exponent})"
            ),
            Self::InvalidNamedExponentBounds { name, min, max } => write!(
                f,
                "optimization {name} bounds must be finite with min < max, got min={min}, max={max}"
            ),
            Self::InitialNamedExponentOutOfBounds {
                name,
                value,
                min,
                max,
            } => write!(
                f,
                "initial_{name}={value} must be finite and strictly inside optimization bounds ({min}, {max})"
            ),
            Self::InvalidJBounds { min_j, max_j } => write!(
                f,
                "optimization j bounds must satisfy 0 <= min_j < max_j <= 3, got min_j={min_j}, max_j={max_j}"
            ),
            Self::InvalidPresetJUpperBound {
                preset,
                max_j,
                upper_exclusive,
            } => write!(
                f,
                "{preset} optimization requires max_j < {upper_exclusive}, got max_j={max_j}"
            ),
            Self::InitialJOutOfBounds {
                value,
                min_j,
                max_j,
            } => write!(
                f,
                "initial_j={value} must be finite and strictly inside optimization bounds ({min_j}, {max_j})"
            ),
            Self::ZeroMaxIterations => write!(f, "max_iterations must be greater than zero"),
            Self::ZeroMultiStartPoints => {
                write!(f, "multi_start_points_per_axis must be greater than zero")
            }
            Self::EmptyProfileParameter => {
                write!(f, "profile parameter names must not be empty")
            }
            Self::DuplicateProfileParameter(parameter) => write!(
                f,
                "profile axes must use different parameters, got {parameter:?} twice"
            ),
            Self::ProfileAxisTooShort { parameter, values } => write!(
                f,
                "profile axis {parameter:?} needs at least two values, got {values}"
            ),
            Self::InvalidProfileAxisValue {
                parameter,
                index,
                value,
            } => write!(
                f,
                "profile axis {parameter:?} value at index {index} must be finite, got {value}"
            ),
            Self::NonIncreasingProfileAxis {
                parameter,
                index,
                previous,
                value,
            } => write!(
                f,
                "profile axis {parameter:?} must be strictly increasing; index {index} has {value} after {previous}"
            ),
            Self::InvalidProfileSupportDelta(value) => write!(
                f,
                "profile support_delta must be finite and positive, got {value}"
            ),
            Self::ProfilePointNoFiniteLikelihood { first, second } => write!(
                f,
                "profile point ({first}, {second}) did not produce a finite DEC likelihood"
            ),
            Self::NoFiniteLikelihood => {
                write!(f, "optimization could not find any finite DEC likelihood")
            }
            Self::Analysis(error) => write!(f, "DEC optimization analysis failed: {error}"),
        }
    }
}

impl Error for DecOptimizationError {}

impl From<DecAnalysisError> for DecOptimizationError {
    fn from(value: DecAnalysisError) -> Self {
        Self::Analysis(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newick::parse_newick;
    use crate::ranges::parse_tip_ranges_table;
    use crate::tree::{Edge, Tree};

    fn assert_close(left: f64, right: f64, tolerance: f64) {
        assert!(
            (left - right).abs() < tolerance,
            "values differ: left={left}, right={right}"
        );
    }

    #[test]
    fn converts_tip_ranges_to_one_hot_likelihoods() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let likelihoods = tip_ranges_to_likelihoods(
            &states,
            &[
                TipRange {
                    node: 0,
                    range: AreaSet::from_bits(0b01),
                },
                TipRange {
                    node: 1,
                    range: AreaSet::from_bits(0b11),
                },
            ],
        )
        .unwrap();

        assert_eq!(likelihoods[0].likelihoods, vec![1.0, 0.0, 0.0]);
        assert_eq!(likelihoods[1].likelihoods, vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn fixed_dec_two_area_zero_branch_flat_prior_golden() {
        let tree = two_tip_tree(0.0, 0.0);
        let states = StateSpace::new(2, 2, false).unwrap();
        let result = run_fixed_dec(
            &tree,
            &states,
            &[
                TipRange {
                    node: 0,
                    range: AreaSet::from_bits(0b01),
                },
                TipRange {
                    node: 1,
                    range: AreaSet::from_bits(0b10),
                },
            ],
            0.1,
            0.2,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(result.log_likelihood, (1.0_f64 / 6.0).ln(), 1e-12);
        assert_eq!(result.root_likelihoods, vec![0.0, 0.0, 1.0]);
    }

    #[test]
    fn fixed_dec_two_area_zero_branch_equal_prior_golden() {
        let tree = two_tip_tree(0.0, 0.0);
        let states = StateSpace::new(2, 2, false).unwrap();
        let result = run_fixed_dec(
            &tree,
            &states,
            &[
                TipRange {
                    node: 0,
                    range: AreaSet::from_bits(0b01),
                },
                TipRange {
                    node: 1,
                    range: AreaSet::from_bits(0b10),
                },
            ],
            0.1,
            0.2,
            RootPrior::Equal,
        )
        .unwrap();

        assert_close(result.log_likelihood, (1.0_f64 / 18.0).ln(), 1e-12);
    }

    #[test]
    fn rejects_tip_range_not_in_state_space() {
        let states = StateSpace::new(2, 1, false).unwrap();
        let error = tip_ranges_to_likelihoods(
            &states,
            &[TipRange {
                node: 0,
                range: AreaSet::from_bits(0b11),
            }],
        )
        .unwrap_err();

        assert_eq!(
            error,
            DecAnalysisError::TipRangeNotInStateSpace {
                node: 0,
                bits: 0b11
            }
        );
    }

    #[test]
    fn fixed_dec_from_parsed_inputs_matches_hand_checked_golden() {
        let parsed_tree = parse_newick("(A:0,B:0);").unwrap();
        let parsed_ranges =
            parse_tip_ranges_table("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n", &parsed_tree).unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, false).unwrap();
        let result = run_fixed_dec(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            0.1,
            0.2,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(result.log_likelihood, (1.0_f64 / 6.0).ln(), 1e-12);
    }

    #[test]
    fn fixed_model_config_entry_matches_dec_wrapper() {
        let parsed_tree = parse_newick("((A:0.3,B:0.4):0.2,C:0.5);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 3, true).unwrap();
        let model = ModelConfig::preset_dec(0.04, 0.01).unwrap();

        let via_config = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &model,
            RootPrior::Flat,
        )
        .unwrap();
        let via_dec = run_fixed_dec(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            0.04,
            0.01,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(via_config.log_likelihood, via_dec.log_likelihood, 1e-12);
        assert_eq!(via_config.root_likelihoods, via_dec.root_likelihoods);
    }

    #[test]
    fn fixed_dec_j_uses_founder_event_model() {
        let parsed_tree = parse_newick("(A:0,B:0);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let dec = run_fixed_dec(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            0.1,
            0.2,
            RootPrior::Flat,
        )
        .unwrap();
        let dec_j = run_fixed_dec_j(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            0.1,
            0.2,
            1.0,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(dec.log_likelihood, (1.0_f64 / 6.0).ln(), 1e-12);
        assert_close(dec_j.log_likelihood, (34.0_f64 / 63.0).ln(), 1e-12);
        assert!(dec_j.log_likelihood > dec.log_likelihood);
    }

    #[test]
    fn optimizes_dec_de_and_improves_initial_likelihood() {
        let parsed_tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let initial = run_fixed_dec(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            0.01,
            0.01,
            RootPrior::Flat,
        )
        .unwrap();
        let optimized = optimize_dec_de(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecOptimizationConfig {
                max_iterations: 100,
                ..DecOptimizationConfig::default()
            },
        )
        .unwrap();

        assert!(optimized.log_likelihood >= initial.log_likelihood);
        assert!(optimized.d >= 1e-12 && optimized.d <= 10.0);
        assert!(optimized.e >= 1e-12 && optimized.e <= 10.0);
        assert!(optimized.evaluations >= 3);
    }

    #[test]
    fn optimized_dec_result_matches_fixed_likelihood_at_estimate() {
        let parsed_tree = parse_newick("(A:1,B:1);").unwrap();
        let parsed_ranges =
            parse_tip_ranges_table("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n", &parsed_tree).unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let optimized = optimize_dec_de(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecOptimizationConfig {
                max_iterations: 80,
                ..DecOptimizationConfig::default()
            },
        )
        .unwrap();
        let fixed = run_fixed_dec(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            optimized.d,
            optimized.e,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn optimizes_de_with_one_bounded_exponent_and_rebuilds_final_model() {
        let parsed_tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let distances =
            crate::dispersal::DispersalMultiplierMatrix::new(2, vec![0.0, 0.5, 2.0, 0.0]).unwrap();
        let mut config = DecExponentOptimizationConfig::for_x();
        config.de.max_iterations = 120;
        config.de.multi_start_points_per_axis = 1;

        let factory = |d, e, exponent| {
            let multipliers = distances
                .distance_power_checked(exponent)
                .map_err(AnagenesisError::from)?;
            Ok(ModelConfig::preset_dec(d, e)?
                .with_range_size_config(config.de.range_size)
                .with_dispersal_multipliers(multipliers))
        };
        let initial_model = factory(
            config.de.initial_d,
            config.de.initial_e,
            config.initial_exponent,
        )
        .unwrap();
        let initial = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &initial_model,
            RootPrior::Flat,
        )
        .unwrap();
        let optimized = optimize_de_exponent_with_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            config,
            factory,
        )
        .unwrap();
        let final_model = factory(optimized.d, optimized.e, optimized.exponent).unwrap();
        let fixed = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &final_model,
            RootPrior::Flat,
        )
        .unwrap();

        assert!(optimized.log_likelihood >= initial.log_likelihood);
        assert!(optimized.exponent >= config.min_exponent);
        assert!(optimized.exponent <= config.max_exponent);
        assert_eq!(optimized.starts, 1);
        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn optimizes_d_e_x_n_u_with_one_model_factory() {
        let parsed_tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let distances =
            crate::dispersal::DispersalMultiplierMatrix::new(2, vec![0.0, 0.5, 2.0, 0.0]).unwrap();
        let environment =
            crate::dispersal::DispersalMultiplierMatrix::new(2, vec![0.0, 1.5, 0.75, 0.0]).unwrap();
        let area_sizes = crate::dispersal::AreaSizeVector::new(vec![0.5, 2.0]).unwrap();
        let config = DecXnuOptimizationConfig {
            de: DecOptimizationConfig {
                max_iterations: 200,
                tolerance: 1e-7,
                ..DecOptimizationConfig::default()
            },
            ..DecXnuOptimizationConfig::default()
        };
        let factory = |d, e, x, n, u| {
            let geographic = distances
                .distance_power_checked(x)
                .map_err(AnagenesisError::from)?;
            let environmental = environment
                .distance_power_checked(n)
                .map_err(AnagenesisError::from)?;
            let dispersal = geographic
                .elementwise_product(&environmental)
                .map_err(AnagenesisError::from)?;
            let extirpation = area_sizes
                .extirpation_multipliers(u)
                .map_err(AnagenesisError::from)?;
            Ok(ModelConfig::preset_dec(d, e)?
                .with_dispersal_multipliers(dispersal)
                .with_extirpation_multipliers(extirpation))
        };
        let initial_model = factory(
            config.de.initial_d,
            config.de.initial_e,
            config.initial_x,
            config.initial_n,
            config.initial_u,
        )
        .unwrap();
        let initial = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &initial_model,
            RootPrior::Flat,
        )
        .unwrap();
        let optimized = optimize_de_xnu_with_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            config,
            factory,
        )
        .unwrap();
        let final_model = factory(
            optimized.d,
            optimized.e,
            optimized.x,
            optimized.n,
            optimized.u,
        )
        .unwrap();
        let fixed = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &final_model,
            RootPrior::Flat,
        )
        .unwrap();

        assert!(optimized.log_likelihood >= initial.log_likelihood);
        assert!((config.min_x..=config.max_x).contains(&optimized.x));
        assert!((config.min_n..=config.max_n).contains(&optimized.n));
        assert!((config.min_u..=config.max_u).contains(&optimized.u));
        assert_eq!(optimized.starts, 1);
        assert!(optimized.evaluations >= 6);
        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn xnu_multi_start_grid_has_five_dimensional_corners() {
        let config = DecXnuOptimizationConfig {
            de: DecOptimizationConfig {
                multi_start_points_per_axis: 2,
                ..DecOptimizationConfig::default()
            },
            ..DecXnuOptimizationConfig::default()
        };
        let starts = xnu_optimization_starts(
            config,
            [
                config.de.initial_d.ln(),
                config.de.initial_e.ln(),
                0.0,
                0.0,
                0.0,
            ],
            config.de.min_rate.ln(),
            config.de.max_rate.ln(),
        );

        assert_eq!(starts.len(), 33);
    }

    #[test]
    fn validates_exponent_bounds_and_classifies_boundary_estimates() {
        let mut config = DecExponentOptimizationConfig::for_x();
        config.min_exponent = 1.0;
        config.max_exponent = 1.0;
        assert_eq!(
            validate_exponent_optimization_config(config),
            Err(DecOptimizationError::InvalidExponentBounds {
                min_exponent: 1.0,
                max_exponent: 1.0,
            })
        );

        assert_eq!(
            classify_optimization_bound(-2.5, -2.5, 2.5),
            Some(OptimizationBound::Lower)
        );
        assert_eq!(
            classify_optimization_bound(2.5, -2.5, 2.5),
            Some(OptimizationBound::Upper)
        );
        assert_eq!(classify_optimization_bound(0.0, -2.5, 2.5), None);

        let invalid_xnu = DecXnuOptimizationConfig {
            min_x: 1.0,
            max_x: 1.0,
            ..DecXnuOptimizationConfig::default()
        };
        assert_eq!(
            validate_xnu_optimization_config(invalid_xnu),
            Err(DecOptimizationError::InvalidNamedExponentBounds {
                name: "x",
                min: 1.0,
                max: 1.0,
            })
        );
    }

    #[test]
    fn optimized_divalike_result_matches_fixed_likelihood_at_estimate() {
        let parsed_tree = parse_newick("(A:1,B:1);").unwrap();
        let parsed_ranges =
            parse_tip_ranges_table("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n", &parsed_tree).unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let config = DecOptimizationConfig {
            max_iterations: 80,
            ..DecOptimizationConfig::for_divalike()
        };
        let optimized = optimize_divalike_de(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            config,
        )
        .unwrap();
        let model = ModelConfig::preset_divalike(optimized.d, optimized.e)
            .unwrap()
            .with_range_size_config(config.range_size);
        let fixed = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &model,
            RootPrior::Flat,
        )
        .unwrap();

        assert_eq!(config.range_size.mx01v, 0.5);
        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn optimized_bayarealike_result_matches_fixed_likelihood_at_estimate() {
        let parsed_tree = parse_newick("(A:1,B:1);").unwrap();
        let parsed_ranges =
            parse_tip_ranges_table("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n", &parsed_tree).unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let config = DecOptimizationConfig {
            max_iterations: 80,
            ..DecOptimizationConfig::for_bayarealike()
        };
        let optimized = optimize_bayarealike_de(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            config,
        )
        .unwrap();
        let model = ModelConfig::preset_bayarealike(optimized.d, optimized.e)
            .unwrap()
            .with_range_size_config(config.range_size);
        let fixed = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &model,
            RootPrior::Flat,
        )
        .unwrap();

        assert_eq!(config.range_size.mx01y, 0.9999);
        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn optimizes_decj_dej_and_improves_initial_likelihood() {
        let parsed_tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let initial = run_fixed_dec_j(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            0.01,
            0.01,
            0.01,
            RootPrior::Flat,
        )
        .unwrap();
        let optimized = optimize_decj_dej(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecJOptimizationConfig {
                max_iterations: 120,
                ..DecJOptimizationConfig::default()
            },
        )
        .unwrap();

        assert!(optimized.log_likelihood >= initial.log_likelihood - 1e-10);
        assert!(optimized.d >= 1e-12 && optimized.d <= 10.0);
        assert!(optimized.e >= 1e-12 && optimized.e <= 10.0);
        assert!(optimized.j >= 1e-5 && optimized.j <= 2.99999);
        assert!(optimized.evaluations >= 4);
    }

    #[test]
    fn optimized_decj_result_matches_fixed_likelihood_at_estimate() {
        let parsed_tree = parse_newick("((A:0.3,B:0.4):0.2,C:0.6);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let optimized = optimize_decj_dej(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecJOptimizationConfig {
                max_iterations: 120,
                ..DecJOptimizationConfig::default()
            },
        )
        .unwrap();
        let fixed = run_fixed_dec_j(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            optimized.d,
            optimized.e,
            optimized.j,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn plus_j_optimization_configs_match_biogeobears_preset_bounds() {
        let divalike = DecJOptimizationConfig::for_divalike();
        let bayarealike = DecJOptimizationConfig::for_bayarealike();

        assert_close(divalike.max_j, 1.99999, 1e-12);
        assert_close(divalike.range_size.mx01v, 0.5, 1e-12);
        assert_close(bayarealike.max_j, 0.99999, 1e-12);
        assert_close(bayarealike.range_size.mx01y, 0.9999, 1e-12);

        let invalid_divalike = DecJOptimizationConfig {
            max_j: 2.0,
            ..divalike
        };
        assert_eq!(
            validate_preset_j_upper_bound(invalid_divalike, "DIVALIKE+J", 2.0),
            Err(DecOptimizationError::InvalidPresetJUpperBound {
                preset: "DIVALIKE+J",
                max_j: 2.0,
                upper_exclusive: 2.0,
            })
        );
    }

    #[test]
    fn optimized_plus_j_presets_match_fixed_likelihood_at_estimate() {
        let parsed_tree = parse_newick("((A:0.3,B:0.4):0.2,C:0.6);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();

        let divalike_config = DecJOptimizationConfig {
            max_iterations: 80,
            ..DecJOptimizationConfig::for_divalike()
        };
        let divalike = optimize_divalikej_dej(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            divalike_config,
        )
        .unwrap();
        let divalike_fixed = run_fixed_divalike_j(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            divalike.d,
            divalike.e,
            divalike.j,
            RootPrior::Flat,
        )
        .unwrap();

        let bayarealike_config = DecJOptimizationConfig {
            max_iterations: 80,
            ..DecJOptimizationConfig::for_bayarealike()
        };
        let bayarealike = optimize_bayarealikej_dej(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            bayarealike_config,
        )
        .unwrap();
        let bayarealike_fixed = run_fixed_bayarealike_j(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            bayarealike.d,
            bayarealike.e,
            bayarealike.j,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(
            divalike.log_likelihood,
            divalike_fixed.log_likelihood,
            1e-12,
        );
        assert_close(
            bayarealike.log_likelihood,
            bayarealike_fixed.log_likelihood,
            1e-12,
        );
        assert!(divalike.j < 2.0);
        assert!(bayarealike.j < 1.0);
    }

    #[test]
    fn decj_optimization_keeps_custom_range_size_config_fixed() {
        let parsed_tree = parse_newick("((A:0.3,B:0.4):0.2,C:0.6);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let range_size = CladogenesisRangeSizeConfig {
            mx01y: 0.8,
            mx01s: 0.3,
            mx01v: 0.5,
            mx01j: 0.6,
        };
        let optimized = optimize_decj_dej(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecJOptimizationConfig {
                max_iterations: 60,
                range_size,
                ..DecJOptimizationConfig::default()
            },
        )
        .unwrap();
        let model = ModelConfig::preset_dec_j(optimized.d, optimized.e, optimized.j)
            .unwrap()
            .with_range_size_config(range_size);
        let fixed = run_fixed_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            &model,
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn multi_start_optimization_keeps_best_result() {
        let parsed_tree = parse_newick("((A:0.3,B:0.7):0.4,C:0.9);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(parsed_ranges.area_names.len() as u8, 2, true).unwrap();
        let single = optimize_dec_de(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecOptimizationConfig::default(),
        )
        .unwrap();
        let multi = optimize_dec_de(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecOptimizationConfig {
                multi_start_points_per_axis: 3,
                ..DecOptimizationConfig::default()
            },
        )
        .unwrap();

        assert_eq!(multi.starts, 10);
        assert!(multi.evaluations > single.evaluations);
        assert!(multi.log_likelihood >= single.log_likelihood - 1e-10);
    }

    #[test]
    fn pair_profile_reports_flat_likelihood_support_across_both_axes() {
        let parsed_tree = parse_newick("((A:0.3,B:0.4):0.2,C:0.6);").unwrap();
        let parsed_ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t1\n",
            &parsed_tree,
        )
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let config = DecPairProfileConfig {
            de: DecOptimizationConfig {
                max_iterations: 80,
                ..DecOptimizationConfig::default()
            },
            first: DecProfileAxis::new("alpha", vec![-1.0, 1.0]),
            second: DecProfileAxis::new("beta", vec![0.0, 2.0]),
            support_delta: PROFILE_95_SUPPORT_DELTA,
        };
        let result = profile_de_pair_with_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            config,
            |d, e, _, _| Ok(ModelConfig::preset_dec(d, e)?),
        )
        .unwrap();

        assert_eq!(result.points.len(), 4);
        assert_eq!(result.support_points, 4);
        assert_eq!(result.first_support.grid_values, 2);
        assert_eq!(result.first_support.min, -1.0);
        assert_eq!(result.first_support.max, 1.0);
        assert_eq!(result.second_support.grid_values, 2);
        assert_eq!(result.second_support.min, 0.0);
        assert_eq!(result.second_support.max, 2.0);
        assert!(result.likelihood_weighted_correlation.unwrap().abs() < 1e-12);
        assert!(
            result
                .points
                .iter()
                .all(|point| point.delta_log_likelihood < 1e-12)
        );
        assert_eq!(
            result.total_evaluations,
            result
                .points
                .iter()
                .map(|point| point.evaluations)
                .sum::<usize>()
        );
    }

    #[test]
    fn pair_profile_rejects_non_increasing_axis() {
        let tree = two_tip_tree(1.0, 1.0);
        let states = StateSpace::new(2, 2, false).unwrap();
        let error = profile_de_pair_with_model(
            &tree,
            &states,
            &[],
            RootPrior::Flat,
            DecPairProfileConfig {
                de: DecOptimizationConfig::default(),
                first: DecProfileAxis::new("x", vec![0.0, 0.0]),
                second: DecProfileAxis::new("n", vec![-1.0, 1.0]),
                support_delta: PROFILE_95_SUPPORT_DELTA,
            },
            |d, e, _, _| Ok(ModelConfig::preset_dec(d, e)?),
        )
        .unwrap_err();

        assert_eq!(
            error,
            DecOptimizationError::NonIncreasingProfileAxis {
                parameter: "x".to_string(),
                index: 1,
                previous: 0.0,
                value: 0.0,
            }
        );
    }

    #[test]
    fn pair_profile_keeps_finite_points_when_other_grid_points_fail() {
        let parsed_tree = parse_newick("(A:0.5,B:0.5);").unwrap();
        let parsed_ranges =
            parse_tip_ranges_table("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\n", &parsed_tree).unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let result = profile_de_pair_with_model(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            DecPairProfileConfig {
                de: DecOptimizationConfig {
                    max_iterations: 30,
                    ..DecOptimizationConfig::default()
                },
                first: DecProfileAxis::new("x", vec![-1.0, 1.0]),
                second: DecProfileAxis::new("n", vec![0.0, 1.0]),
                support_delta: PROFILE_95_SUPPORT_DELTA,
            },
            |d, e, first, _| {
                if first > 0.0 {
                    Ok(ModelConfig::preset_dec(-d, e)?)
                } else {
                    Ok(ModelConfig::preset_dec(d, e)?)
                }
            },
        )
        .unwrap();

        assert_eq!(result.finite_points, 2);
        assert_eq!(result.failed_points, 2);
        assert_eq!(result.points.len(), 4);
        assert!(result.points.iter().filter(|point| point.finite).count() == 2);
        assert!(
            result
                .points
                .iter()
                .filter(|point| !point.finite)
                .all(|point| point.log_likelihood == f64::NEG_INFINITY)
        );
    }

    #[test]
    fn rejects_invalid_optimization_config() {
        let tree = two_tip_tree(1.0, 1.0);
        let states = StateSpace::new(2, 2, false).unwrap();
        let error = optimize_dec_de(
            &tree,
            &states,
            &[TipRange {
                node: 0,
                range: AreaSet::from_bits(0b01),
            }],
            RootPrior::Flat,
            DecOptimizationConfig {
                initial_d: 0.0,
                ..DecOptimizationConfig::default()
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            DecOptimizationError::InvalidPositiveValue {
                name: "initial_d",
                value: 0.0
            }
        );

        let error = optimize_dec_de(
            &tree,
            &states,
            &[TipRange {
                node: 0,
                range: AreaSet::from_bits(0b01),
            }],
            RootPrior::Flat,
            DecOptimizationConfig {
                multi_start_points_per_axis: 0,
                ..DecOptimizationConfig::default()
            },
        )
        .unwrap_err();

        assert_eq!(error, DecOptimizationError::ZeroMultiStartPoints);
    }

    #[test]
    fn rejects_invalid_decj_optimization_config() {
        let tree = two_tip_tree(1.0, 1.0);
        let states = StateSpace::new(2, 2, false).unwrap();
        let error = optimize_decj_dej(
            &tree,
            &states,
            &[TipRange {
                node: 0,
                range: AreaSet::from_bits(0b01),
            }],
            RootPrior::Flat,
            DecJOptimizationConfig {
                initial_j: 0.0,
                ..DecJOptimizationConfig::default()
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            DecOptimizationError::InitialJOutOfBounds {
                value: 0.0,
                min_j: 1e-5,
                max_j: 2.99999
            }
        );
    }

    fn two_tip_tree(left_length: f64, right_length: f64) -> Tree {
        Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: left_length,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: right_length,
                },
            ],
        )
        .unwrap()
    }
}
