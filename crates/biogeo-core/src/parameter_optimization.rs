use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::sync::Arc;

use crate::dec::{DecAnalysisError, OptimizationBound, TipRange, tip_ranges_to_likelihoods};
use crate::engine::LikelihoodEngine;
use crate::execution::ExecutionCancellationToken;
use crate::model::ModelConfig;
use crate::parameters::{
    ParameterBounds, ParameterError, ParameterMode, ParameterSpec, ParameterTable,
    ParameterTransform, ResolvedParameters,
};
use crate::pruning::{PruningResult, RootPrior, TipLikelihood};
use crate::state::StateSpace;
use crate::tree::Tree;

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterOptimizationConfig {
    pub initial_step: f64,
    pub tolerance: f64,
    pub max_iterations: usize,
    /// Additional starts in model-space free-parameter order, not optimizer coordinates.
    pub additional_starts: Vec<Vec<f64>>,
}

impl Default for ParameterOptimizationConfig {
    fn default() -> Self {
        Self {
            initial_step: 0.5,
            tolerance: 1e-8,
            max_iterations: 200,
            additional_starts: Vec::new(),
        }
    }
}

impl ParameterOptimizationConfig {
    pub fn with_additional_start(mut self, free_values: Vec<f64>) -> Self {
        self.additional_starts.push(free_values);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterEstimate {
    pub name: String,
    pub value: f64,
    pub bounds: ParameterBounds,
    pub transform: ParameterTransform,
    pub bound: Option<OptimizationBound>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterOptimizationResult {
    pub free_parameters: Vec<ParameterEstimate>,
    pub resolved_parameters: ResolvedParameters,
    pub model: ModelConfig,
    pub log_likelihood: f64,
    pub iterations: usize,
    pub evaluations: usize,
    pub converged: bool,
    pub starts: usize,
    pub converged_starts: usize,
    pub pruning: PruningResult,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParameterOptimizationProgressPhase {
    StartInitialized,
    IterationCompleted,
    StartCompleted,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParameterOptimizationProgress {
    /// One-based index of the current optimization start.
    pub start: usize,
    pub starts: usize,
    pub phase: ParameterOptimizationProgressPhase,
    pub iteration: usize,
    pub max_iterations: usize,
    /// Cumulative likelihood evaluations across all starts.
    pub evaluations: usize,
    pub best_log_likelihood: Option<f64>,
}

pub struct ParameterOptimizationExecution<'a> {
    config: &'a ParameterOptimizationConfig,
    cancellation: &'a ExecutionCancellationToken,
    progress: &'a mut dyn FnMut(ParameterOptimizationProgress),
}

impl<'a> ParameterOptimizationExecution<'a> {
    pub fn new(
        config: &'a ParameterOptimizationConfig,
        cancellation: &'a ExecutionCancellationToken,
        progress: &'a mut dyn FnMut(ParameterOptimizationProgress),
    ) -> Self {
        Self {
            config,
            cancellation,
            progress,
        }
    }
}

pub fn optimize_parameter_table<E, F>(
    tree: &Tree,
    states: &StateSpace,
    tip_ranges: &[TipRange],
    root_prior: RootPrior<'_>,
    table: &ParameterTable,
    config: &ParameterOptimizationConfig,
    model_factory: F,
) -> Result<ParameterOptimizationResult, ParameterOptimizationError>
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<ModelConfig, E>,
{
    let tip_likelihoods = tip_ranges_to_likelihoods(states, tip_ranges)?;
    optimize_parameter_table_likelihoods(
        tree,
        states,
        &tip_likelihoods,
        root_prior,
        table,
        config,
        model_factory,
    )
}

pub fn optimize_parameter_table_likelihoods<E, F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    table: &ParameterTable,
    config: &ParameterOptimizationConfig,
    model_factory: F,
) -> Result<ParameterOptimizationResult, ParameterOptimizationError>
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<ModelConfig, E>,
{
    let cancellation = ExecutionCancellationToken::new();
    let mut ignore_progress = |_: ParameterOptimizationProgress| {};
    optimize_parameter_table_likelihoods_with_control(
        tree,
        states,
        tip_likelihoods,
        root_prior,
        table,
        ParameterOptimizationExecution::new(config, &cancellation, &mut ignore_progress),
        model_factory,
    )
}

pub fn optimize_parameter_table_likelihoods_with_control<E, F>(
    tree: &Tree,
    states: &StateSpace,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
    table: &ParameterTable,
    execution: ParameterOptimizationExecution<'_>,
    model_factory: F,
) -> Result<ParameterOptimizationResult, ParameterOptimizationError>
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<ModelConfig, E>,
{
    let tip_likelihoods = Arc::<[TipLikelihood]>::from(tip_likelihoods.to_vec());
    optimize_parameter_table_evaluations(
        tree,
        states,
        root_prior,
        table,
        execution,
        move |parameters| {
            model_factory(parameters).map(|model| ParameterLikelihoodEvaluation {
                model,
                tip_likelihoods: Arc::clone(&tip_likelihoods),
            })
        },
    )
}

pub fn optimize_parameter_table_dynamic_likelihoods<E, F>(
    tree: &Tree,
    states: &StateSpace,
    root_prior: RootPrior<'_>,
    table: &ParameterTable,
    config: &ParameterOptimizationConfig,
    evaluation_factory: F,
) -> Result<ParameterOptimizationResult, ParameterOptimizationError>
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<(ModelConfig, Vec<TipLikelihood>), E>,
{
    let cancellation = ExecutionCancellationToken::new();
    let mut ignore_progress = |_: ParameterOptimizationProgress| {};
    optimize_parameter_table_dynamic_likelihoods_with_control(
        tree,
        states,
        root_prior,
        table,
        ParameterOptimizationExecution::new(config, &cancellation, &mut ignore_progress),
        evaluation_factory,
    )
}

pub fn optimize_parameter_table_dynamic_likelihoods_with_control<E, F>(
    tree: &Tree,
    states: &StateSpace,
    root_prior: RootPrior<'_>,
    table: &ParameterTable,
    execution: ParameterOptimizationExecution<'_>,
    evaluation_factory: F,
) -> Result<ParameterOptimizationResult, ParameterOptimizationError>
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<(ModelConfig, Vec<TipLikelihood>), E>,
{
    optimize_parameter_table_evaluations(
        tree,
        states,
        root_prior,
        table,
        execution,
        move |parameters| {
            evaluation_factory(parameters).map(|(model, tip_likelihoods)| {
                ParameterLikelihoodEvaluation {
                    model,
                    tip_likelihoods: Arc::from(tip_likelihoods),
                }
            })
        },
    )
}

#[derive(Clone, Debug)]
struct ParameterLikelihoodEvaluation {
    model: ModelConfig,
    tip_likelihoods: Arc<[TipLikelihood]>,
}

fn optimize_parameter_table_evaluations<E, F>(
    tree: &Tree,
    states: &StateSpace,
    root_prior: RootPrior<'_>,
    table: &ParameterTable,
    execution: ParameterOptimizationExecution<'_>,
    evaluation_factory: F,
) -> Result<ParameterOptimizationResult, ParameterOptimizationError>
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<ParameterLikelihoodEvaluation, E>,
{
    let ParameterOptimizationExecution {
        config,
        cancellation,
        progress,
    } = execution;
    validate_config(config)?;
    check_cancelled(cancellation)?;
    let axes = free_parameter_axes(table)?;
    if axes.is_empty() {
        return Err(ParameterOptimizationError::NoFreeParameters);
    }

    let starts = optimization_starts(table, &axes, config)?;
    let engine = LikelihoodEngine::new(tree, states, root_prior);
    let mut best_run = None;
    let mut total_iterations = 0;
    let mut total_evaluations = 0;
    let mut converged_starts = 0;

    for (start_index, start) in starts.iter().enumerate() {
        check_cancelled(cancellation)?;
        let initial_steps = axes
            .iter()
            .zip(start)
            .map(|(axis, coordinate)| axis.initial_step(*coordinate, config.initial_step))
            .collect::<Result<Vec<_>, _>>()?;
        let prior_evaluations = total_evaluations;
        let run = optimize_from_start(
            start,
            &initial_steps,
            config.tolerance,
            config.max_iterations,
            cancellation,
            |phase, iteration, evaluations, best_objective| {
                progress(ParameterOptimizationProgress {
                    start: start_index + 1,
                    starts: starts.len(),
                    phase,
                    iteration,
                    max_iterations: config.max_iterations,
                    evaluations: prior_evaluations + evaluations,
                    best_log_likelihood: best_objective.is_finite().then_some(-best_objective),
                });
            },
            |coordinates| {
                evaluate_coordinates(&axes, table, &engine, coordinates, &evaluation_factory)
            },
        )?;
        total_iterations += run.iterations;
        total_evaluations += run.evaluations;
        if run.converged && run.best.objective.is_finite() {
            converged_starts += 1;
        }
        if run.best.objective.is_finite()
            && best_run
                .as_ref()
                .is_none_or(|best: &SingleOptimizationRun| run.best.objective < best.best.objective)
        {
            best_run = Some(run);
        }
    }

    check_cancelled(cancellation)?;
    let best_run = best_run.ok_or(ParameterOptimizationError::NoFiniteLikelihood)?;
    let free_values = decode_coordinates(&axes, &best_run.best.coordinates);
    let resolved_parameters = table.resolve_free_values(&free_values)?;
    let evaluation = evaluation_factory(&resolved_parameters).map_err(|error| {
        ParameterOptimizationError::ModelBuild {
            message: error.to_string(),
        }
    })?;
    check_cancelled(cancellation)?;
    let pruning = engine
        .evaluate(&evaluation.model, &evaluation.tip_likelihoods)
        .map_err(DecAnalysisError::from)?;
    if !pruning.log_likelihood.is_finite() {
        return Err(ParameterOptimizationError::NoFiniteLikelihood);
    }

    let free_parameters = axes
        .iter()
        .zip(free_values)
        .map(|(axis, value)| ParameterEstimate {
            name: axis.name.clone(),
            value,
            bounds: axis.bounds,
            transform: axis.transform,
            bound: classify_optimization_bound(value, axis.bounds),
        })
        .collect();

    Ok(ParameterOptimizationResult {
        free_parameters,
        resolved_parameters,
        model: evaluation.model,
        log_likelihood: pruning.log_likelihood,
        iterations: total_iterations,
        evaluations: total_evaluations,
        converged: best_run.converged,
        starts: starts.len(),
        converged_starts,
        pruning,
    })
}

fn validate_config(config: &ParameterOptimizationConfig) -> Result<(), ParameterOptimizationError> {
    for (field, value) in [
        ("initial_step", config.initial_step),
        ("tolerance", config.tolerance),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(ParameterOptimizationError::InvalidPositiveConfigValue { field, value });
        }
    }
    if config.max_iterations == 0 {
        return Err(ParameterOptimizationError::ZeroMaxIterations);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct FreeParameterAxis {
    name: String,
    bounds: ParameterBounds,
    transform: ParameterTransform,
    initial_coordinate: f64,
}

impl FreeParameterAxis {
    fn from_spec(spec: &ParameterSpec) -> Result<Self, ParameterOptimizationError> {
        let ParameterMode::Free { initial } = spec.mode() else {
            unreachable!("only free parameter specs become optimization axes");
        };
        let mut axis = Self {
            name: spec.name().to_owned(),
            bounds: spec.bounds(),
            transform: spec.transform(),
            initial_coordinate: 0.0,
        };
        axis.initial_coordinate = axis.encode(*initial, 0)?;
        Ok(axis)
    }

    fn encode(&self, value: f64, start_index: usize) -> Result<f64, ParameterOptimizationError> {
        let coordinate = match self.transform {
            ParameterTransform::Linear => value,
            ParameterTransform::Log if value > 0.0 => value.ln(),
            ParameterTransform::Log => {
                return Err(self.invalid_start_value(start_index, value));
            }
            ParameterTransform::Logit if value > self.bounds.min && value < self.bounds.max => {
                let proportion = (value - self.bounds.min) / (self.bounds.max - self.bounds.min);
                (proportion / (1.0 - proportion)).ln()
            }
            ParameterTransform::Logit => {
                return Err(self.invalid_start_value(start_index, value));
            }
        };
        if coordinate.is_finite() {
            Ok(coordinate)
        } else {
            Err(self.invalid_start_value(start_index, value))
        }
    }

    fn invalid_start_value(&self, start_index: usize, value: f64) -> ParameterOptimizationError {
        ParameterOptimizationError::StartValueNotTransformable {
            start_index,
            parameter: self.name.clone(),
            value,
            bounds: self.bounds,
            transform: self.transform,
        }
    }

    fn normalize_coordinate(&self, coordinate: f64) -> f64 {
        if coordinate.is_nan() {
            return self.initial_coordinate;
        }
        match self.transform {
            ParameterTransform::Linear => coordinate.clamp(self.bounds.min, self.bounds.max),
            ParameterTransform::Log => coordinate.clamp(self.bounds.min.ln(), self.bounds.max.ln()),
            ParameterTransform::Logit if coordinate == f64::INFINITY => 700.0,
            ParameterTransform::Logit if coordinate == f64::NEG_INFINITY => -700.0,
            ParameterTransform::Logit => coordinate,
        }
    }

    fn decode(&self, coordinate: f64) -> f64 {
        let coordinate = self.normalize_coordinate(coordinate);
        match self.transform {
            ParameterTransform::Linear => coordinate,
            ParameterTransform::Log => coordinate.exp(),
            ParameterTransform::Logit => {
                let proportion = if coordinate >= 0.0 {
                    1.0 / (1.0 + (-coordinate).exp())
                } else {
                    let exp_coordinate = coordinate.exp();
                    exp_coordinate / (1.0 + exp_coordinate)
                };
                self.bounds.min + (self.bounds.max - self.bounds.min) * proportion
            }
        }
    }

    fn initial_step(&self, coordinate: f64, step: f64) -> Result<f64, ParameterOptimizationError> {
        let positive = self.normalize_coordinate(coordinate + step);
        if positive != coordinate {
            return Ok(positive - coordinate);
        }
        let negative = self.normalize_coordinate(coordinate - step);
        if negative != coordinate {
            return Ok(negative - coordinate);
        }
        Err(ParameterOptimizationError::NoUsableInitialStep {
            parameter: self.name.clone(),
        })
    }
}

fn free_parameter_axes(
    table: &ParameterTable,
) -> Result<Vec<FreeParameterAxis>, ParameterOptimizationError> {
    table
        .free_parameter_specs()
        .into_iter()
        .map(FreeParameterAxis::from_spec)
        .collect()
}

fn optimization_starts(
    table: &ParameterTable,
    axes: &[FreeParameterAxis],
    config: &ParameterOptimizationConfig,
) -> Result<Vec<Vec<f64>>, ParameterOptimizationError> {
    let mut starts = Vec::with_capacity(config.additional_starts.len() + 1);
    push_unique_start(
        &mut starts,
        encode_start(axes, &table.initial_free_values(), 0)?,
    );

    for (index, free_values) in config.additional_starts.iter().enumerate() {
        let start_index = index + 1;
        if free_values.len() != axes.len() {
            return Err(ParameterOptimizationError::StartDimensionMismatch {
                start_index,
                expected: axes.len(),
                actual: free_values.len(),
            });
        }
        table.resolve_free_values(free_values).map_err(|source| {
            ParameterOptimizationError::InvalidAdditionalStart {
                start_index,
                source,
            }
        })?;
        push_unique_start(&mut starts, encode_start(axes, free_values, start_index)?);
    }
    Ok(starts)
}

fn encode_start(
    axes: &[FreeParameterAxis],
    free_values: &[f64],
    start_index: usize,
) -> Result<Vec<f64>, ParameterOptimizationError> {
    axes.iter()
        .zip(free_values)
        .map(|(axis, value)| axis.encode(*value, start_index))
        .collect()
}

fn decode_coordinates(axes: &[FreeParameterAxis], coordinates: &[f64]) -> Vec<f64> {
    axes.iter()
        .zip(coordinates)
        .map(|(axis, coordinate)| axis.decode(*coordinate))
        .collect()
}

fn push_unique_start(starts: &mut Vec<Vec<f64>>, candidate: Vec<f64>) {
    if !starts
        .iter()
        .any(|existing| vectors_are_close(existing, &candidate, 1e-12))
    {
        starts.push(candidate);
    }
}

fn vectors_are_close(left: &[f64], right: &[f64], tolerance: f64) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| (left - right).abs() <= tolerance)
}

#[derive(Clone, Debug)]
struct OptimPoint {
    coordinates: Vec<f64>,
    objective: f64,
}

#[derive(Clone, Debug)]
struct SingleOptimizationRun {
    best: OptimPoint,
    iterations: usize,
    evaluations: usize,
    converged: bool,
}

fn evaluate_coordinates<E, F>(
    axes: &[FreeParameterAxis],
    table: &ParameterTable,
    engine: &LikelihoodEngine<'_>,
    coordinates: Vec<f64>,
    evaluation_factory: &F,
) -> OptimPoint
where
    E: fmt::Display,
    F: Fn(&ResolvedParameters) -> Result<ParameterLikelihoodEvaluation, E>,
{
    let coordinates = axes
        .iter()
        .zip(coordinates)
        .map(|(axis, coordinate)| axis.normalize_coordinate(coordinate))
        .collect::<Vec<_>>();
    let free_values = decode_coordinates(axes, &coordinates);
    let objective = table
        .resolve_free_values(&free_values)
        .ok()
        .and_then(|parameters| evaluation_factory(&parameters).ok())
        .and_then(|evaluation| {
            engine
                .evaluate(&evaluation.model, &evaluation.tip_likelihoods)
                .ok()
        })
        .map_or(f64::INFINITY, |result| {
            if result.log_likelihood.is_finite() {
                -result.log_likelihood
            } else {
                f64::INFINITY
            }
        });

    OptimPoint {
        coordinates,
        objective,
    }
}

fn optimize_from_start(
    start: &[f64],
    initial_steps: &[f64],
    tolerance: f64,
    max_iterations: usize,
    cancellation: &ExecutionCancellationToken,
    mut progress: impl FnMut(ParameterOptimizationProgressPhase, usize, usize, f64),
    mut evaluate: impl FnMut(Vec<f64>) -> OptimPoint,
) -> Result<SingleOptimizationRun, ParameterOptimizationError> {
    let dimensions = start.len();
    debug_assert!(dimensions > 0);
    debug_assert_eq!(initial_steps.len(), dimensions);
    let evaluations = Cell::new(0);
    let mut evaluate_counted =
        |coordinates: Vec<f64>| -> Result<OptimPoint, ParameterOptimizationError> {
            check_cancelled(cancellation)?;
            evaluations.set(evaluations.get() + 1);
            let point = evaluate(coordinates);
            check_cancelled(cancellation)?;
            Ok(point)
        };

    let mut simplex = Vec::with_capacity(dimensions + 1);
    simplex.push(evaluate_counted(start.to_vec())?);
    for axis in 0..dimensions {
        let mut point = start.to_vec();
        point[axis] += initial_steps[axis];
        simplex.push(evaluate_counted(point)?);
    }
    sort_simplex(&mut simplex);
    progress(
        ParameterOptimizationProgressPhase::StartInitialized,
        0,
        evaluations.get(),
        simplex[0].objective,
    );

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
            &centroid,
            &subtract(&centroid, &simplex[worst_index].coordinates),
            1.0,
        ))?;

        if reflected.objective < simplex[0].objective {
            let expanded = evaluate_counted(add_scaled(
                &centroid,
                &subtract(&reflected.coordinates, &centroid),
                2.0,
            ))?;
            simplex[worst_index] = if expanded.objective < reflected.objective {
                expanded
            } else {
                reflected
            };
        } else if reflected.objective < simplex[second_worst_index].objective {
            simplex[worst_index] = reflected;
        } else {
            let contraction_direction = if reflected.objective < simplex[worst_index].objective {
                subtract(&reflected.coordinates, &centroid)
            } else {
                subtract(&simplex[worst_index].coordinates, &centroid)
            };
            let contracted = evaluate_counted(add_scaled(&centroid, &contraction_direction, 0.5))?;

            if contracted.objective < simplex[worst_index].objective {
                simplex[worst_index] = contracted;
            } else {
                let best_coordinates = simplex[0].coordinates.clone();
                for point in simplex.iter_mut().skip(1) {
                    *point =
                        evaluate_counted(shrink_toward(&best_coordinates, &point.coordinates))?;
                }
            }
        }
        iterations = iteration + 1;
        sort_simplex(&mut simplex);
        progress(
            ParameterOptimizationProgressPhase::IterationCompleted,
            iterations,
            evaluations.get(),
            simplex[0].objective,
        );
    }

    sort_simplex(&mut simplex);
    progress(
        ParameterOptimizationProgressPhase::StartCompleted,
        iterations,
        evaluations.get(),
        simplex[0].objective,
    );
    Ok(SingleOptimizationRun {
        best: simplex.remove(0),
        iterations,
        evaluations: evaluations.get(),
        converged,
    })
}

fn check_cancelled(
    cancellation: &ExecutionCancellationToken,
) -> Result<(), ParameterOptimizationError> {
    if cancellation.is_cancelled() {
        Err(ParameterOptimizationError::Cancelled)
    } else {
        Ok(())
    }
}

fn sort_simplex(simplex: &mut [OptimPoint]) {
    simplex.sort_by(|left, right| left.objective.total_cmp(&right.objective));
}

fn simplex_has_converged(simplex: &[OptimPoint], tolerance: f64) -> bool {
    let best = &simplex[0];
    let worst = &simplex[simplex.len() - 1];
    let value_spread = (worst.objective - best.objective).abs();
    let max_distance = simplex
        .iter()
        .skip(1)
        .map(|point| euclidean_distance(&best.coordinates, &point.coordinates))
        .fold(0.0, f64::max);
    value_spread.is_finite()
        && max_distance.is_finite()
        && value_spread <= tolerance
        && max_distance <= tolerance
}

fn centroid_without_worst(simplex: &[OptimPoint]) -> Vec<f64> {
    let count = simplex.len() - 1;
    let mut centroid = vec![0.0; simplex[0].coordinates.len()];
    for point in &simplex[..count] {
        for (centroid_value, point_value) in centroid.iter_mut().zip(&point.coordinates) {
            *centroid_value += point_value;
        }
    }
    for value in &mut centroid {
        *value /= count as f64;
    }
    centroid
}

fn subtract(left: &[f64], right: &[f64]) -> Vec<f64> {
    left.iter()
        .zip(right)
        .map(|(left, right)| left - right)
        .collect()
}

fn add_scaled(origin: &[f64], direction: &[f64], scale: f64) -> Vec<f64> {
    origin
        .iter()
        .zip(direction)
        .map(|(origin, direction)| origin + direction * scale)
        .collect()
}

fn shrink_toward(best: &[f64], point: &[f64]) -> Vec<f64> {
    best.iter()
        .zip(point)
        .map(|(best, point)| best + 0.5 * (point - best))
        .collect()
}

fn euclidean_distance(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| {
            let delta = left - right;
            delta * delta
        })
        .sum::<f64>()
        .sqrt()
}

fn classify_optimization_bound(value: f64, bounds: ParameterBounds) -> Option<OptimizationBound> {
    let tolerance = (bounds.max - bounds.min).abs().max(1.0) * 1e-8;
    if (value - bounds.min).abs() <= tolerance {
        Some(OptimizationBound::Lower)
    } else if (value - bounds.max).abs() <= tolerance {
        Some(OptimizationBound::Upper)
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterOptimizationError {
    Cancelled,
    InvalidPositiveConfigValue {
        field: &'static str,
        value: f64,
    },
    ZeroMaxIterations,
    NoFreeParameters,
    StartDimensionMismatch {
        start_index: usize,
        expected: usize,
        actual: usize,
    },
    InvalidAdditionalStart {
        start_index: usize,
        source: ParameterError,
    },
    StartValueNotTransformable {
        start_index: usize,
        parameter: String,
        value: f64,
        bounds: ParameterBounds,
        transform: ParameterTransform,
    },
    NoUsableInitialStep {
        parameter: String,
    },
    NoFiniteLikelihood,
    ModelBuild {
        message: String,
    },
    Parameter(ParameterError),
    Analysis(DecAnalysisError),
}

impl fmt::Display for ParameterOptimizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "optimization cancelled"),
            Self::InvalidPositiveConfigValue { field, value } => {
                write!(f, "{field} must be finite and positive, got {value}")
            }
            Self::ZeroMaxIterations => write!(f, "max_iterations must be greater than zero"),
            Self::NoFreeParameters => write!(f, "parameter table has no free parameters"),
            Self::StartDimensionMismatch {
                start_index,
                expected,
                actual,
            } => write!(
                f,
                "optimization start {start_index} has {actual} values, expected {expected}"
            ),
            Self::InvalidAdditionalStart {
                start_index,
                source,
            } => write!(f, "optimization start {start_index} is invalid: {source}"),
            Self::StartValueNotTransformable {
                start_index,
                parameter,
                value,
                bounds,
                transform,
            } => write!(
                f,
                "optimization start {start_index} value {parameter}={value} cannot use {transform:?} coordinates with bounds [{}, {}]",
                bounds.min, bounds.max
            ),
            Self::NoUsableInitialStep { parameter } => write!(
                f,
                "parameter {parameter} has no distinct point for the configured initial step"
            ),
            Self::NoFiniteLikelihood => {
                write!(f, "optimization could not find any finite likelihood")
            }
            Self::ModelBuild { message } => {
                write!(f, "final optimized model could not be built: {message}")
            }
            Self::Parameter(error) => write!(f, "parameter optimization failed: {error}"),
            Self::Analysis(error) => write!(f, "likelihood optimization failed: {error}"),
        }
    }
}

impl Error for ParameterOptimizationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidAdditionalStart { source, .. } | Self::Parameter(source) => Some(source),
            Self::Analysis(source) => Some(source),
            _ => None,
        }
    }
}

impl From<ParameterError> for ParameterOptimizationError {
    fn from(value: ParameterError) -> Self {
        Self::Parameter(value)
    }
}

impl From<DecAnalysisError> for ParameterOptimizationError {
    fn from(value: DecAnalysisError) -> Self {
        Self::Analysis(value)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;
    use crate::dec::{
        DecJOptimizationConfig, DecOptimizationConfig, optimize_dec_de, optimize_decj_dej,
    };
    use crate::newick::parse_newick;
    use crate::parameters::BioGeoBearsPreset;
    use crate::ranges::parse_tip_ranges_table;
    use crate::{DetectionModel, parse_detection_data};

    fn assert_close(left: f64, right: f64, tolerance: f64) {
        assert!(
            (left - right).abs() <= tolerance,
            "values differ: left={left}, right={right}, tolerance={tolerance}"
        );
    }

    #[test]
    fn dynamic_nelder_mead_optimizes_four_dimensions() {
        let target = [1.5, -2.0, 0.25, 4.0];
        let cancellation = ExecutionCancellationToken::new();
        let mut events = Vec::new();
        let run = optimize_from_start(
            &[0.0; 4],
            &[0.5; 4],
            1e-10,
            500,
            &cancellation,
            |phase, iteration, evaluations, objective| {
                events.push((phase, iteration, evaluations, objective));
            },
            |coordinates| OptimPoint {
                objective: coordinates
                    .iter()
                    .zip(target)
                    .map(|(value, target)| (value - target).powi(2))
                    .sum(),
                coordinates,
            },
        )
        .unwrap();

        assert!(run.converged);
        assert!(run.best.objective < 1e-12);
        for (actual, expected) in run.best.coordinates.iter().zip(target) {
            assert_close(*actual, expected, 2e-6);
        }
        assert_eq!(
            events.first().unwrap().0,
            ParameterOptimizationProgressPhase::StartInitialized
        );
        assert_eq!(
            events.last().unwrap().0,
            ParameterOptimizationProgressPhase::StartCompleted
        );
        assert!(events.windows(2).all(|pair| pair[0].2 <= pair[1].2));
    }

    #[test]
    fn controlled_optimizer_stops_before_a_cancelled_evaluation() {
        let cancellation = ExecutionCancellationToken::new();
        cancellation.cancel();
        let evaluations = Cell::new(0);
        let error = optimize_from_start(
            &[0.0],
            &[0.5],
            1e-8,
            10,
            &cancellation,
            |_, _, _, _| {},
            |coordinates| {
                evaluations.set(evaluations.get() + 1);
                OptimPoint {
                    coordinates,
                    objective: 0.0,
                }
            },
        )
        .unwrap_err();

        assert_eq!(error, ParameterOptimizationError::Cancelled);
        assert_eq!(evaluations.get(), 0);
    }

    #[test]
    fn generic_dec_optimizer_matches_specialized_de_path() {
        let tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let ranges =
            parse_tip_ranges_table("tip\tAreaA\tAreaB\nA\t1\t0\nB\t0\t1\nC\t1\t0\n", &tree)
                .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let table = BioGeoBearsPreset::Dec.parameter_table().unwrap();
        let rate_bounds = table.spec("d").unwrap().bounds();
        let config = ParameterOptimizationConfig {
            max_iterations: 100,
            ..ParameterOptimizationConfig::default()
        };

        let generic = optimize_parameter_table(
            &tree.tree,
            &states,
            &ranges.tip_ranges,
            RootPrior::Flat,
            &table,
            &config,
            ModelConfig::from_biogeobears_core_parameters,
        )
        .unwrap();
        let specialized = optimize_dec_de(
            &tree.tree,
            &states,
            &ranges.tip_ranges,
            RootPrior::Flat,
            DecOptimizationConfig {
                max_rate: rate_bounds.max,
                max_iterations: 100,
                ..DecOptimizationConfig::default()
            },
        )
        .unwrap();

        assert_eq!(generic.free_parameters.len(), 2);
        assert_eq!(generic.free_parameters[0].name, "d");
        assert_eq!(generic.free_parameters[1].name, "e");
        assert_close(generic.log_likelihood, specialized.log_likelihood, 1e-10);
        assert_close(generic.free_parameters[0].value, specialized.d, 1e-8);
        assert_close(generic.free_parameters[1].value, specialized.e, 1e-8);
    }

    #[test]
    fn generic_decj_optimizer_matches_specialized_dej_path() {
        let tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t0\t0\t1\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(3, 2, true).unwrap();
        let table = BioGeoBearsPreset::DecJ.parameter_table().unwrap();
        let j_bounds = table.spec("j").unwrap().bounds();
        let table = table.with_free("j", 0.01, j_bounds).unwrap();
        let rate_bounds = table.spec("d").unwrap().bounds();
        let j_initial = match table.spec("j").unwrap().mode() {
            ParameterMode::Free { initial } => *initial,
            _ => unreachable!(),
        };
        let config = ParameterOptimizationConfig {
            max_iterations: 250,
            ..ParameterOptimizationConfig::default()
        };

        let generic = optimize_parameter_table(
            &tree.tree,
            &states,
            &ranges.tip_ranges,
            RootPrior::Flat,
            &table,
            &config,
            ModelConfig::from_biogeobears_core_parameters,
        )
        .unwrap();
        let specialized = optimize_decj_dej(
            &tree.tree,
            &states,
            &ranges.tip_ranges,
            RootPrior::Flat,
            DecJOptimizationConfig {
                initial_j: j_initial,
                max_rate: rate_bounds.max,
                max_iterations: 250,
                ..DecJOptimizationConfig::default()
            },
        )
        .unwrap();

        assert_eq!(
            generic
                .free_parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "e", "j"]
        );
        assert_close(generic.log_likelihood, specialized.log_likelihood, 1e-9);
        assert_close(generic.free_parameters[0].value, specialized.d, 1e-7);
        assert_close(generic.free_parameters[1].value, specialized.e, 1e-7);
        assert_close(generic.free_parameters[2].value, specialized.j, 1e-7);
    }

    #[test]
    fn optimizer_consumes_custom_free_and_linked_cladogenesis_parameters() {
        let tree = parse_newick("((A:0.3,B:0.4):0.2,C:0.6);").unwrap();
        let ranges = parse_tip_ranges_table(
            "tip\tAreaA\tAreaB\tAreaC\nA\t1\t0\t0\nB\t0\t1\t0\nC\t1\t1\t0\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(3, 2, true).unwrap();
        let weight_bounds = ParameterBounds::new(1e-4, 0.9999).unwrap();
        let table = BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.04)
            .unwrap()
            .with_fixed("e", 0.02)
            .unwrap()
            .with_free("y", 0.4, weight_bounds)
            .unwrap()
            .with_derived_from_str("s", "y/2")
            .unwrap()
            .with_free("v", 0.5, weight_bounds)
            .unwrap();
        let config = ParameterOptimizationConfig {
            max_iterations: 160,
            ..ParameterOptimizationConfig::default()
        }
        .with_additional_start(vec![0.8, 0.2]);

        let optimized = optimize_parameter_table(
            &tree.tree,
            &states,
            &ranges.tip_ranges,
            RootPrior::Flat,
            &table,
            &config,
            ModelConfig::from_biogeobears_core_parameters,
        )
        .unwrap();
        let tips = tip_ranges_to_likelihoods(&states, &ranges.tip_ranges).unwrap();
        let fixed = LikelihoodEngine::new(&tree.tree, &states, RootPrior::Flat)
            .evaluate(&optimized.model, &tips)
            .unwrap();

        assert_eq!(optimized.starts, 2);
        assert_eq!(
            optimized
                .free_parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>(),
            vec!["y", "v"]
        );
        assert_close(
            optimized.resolved_parameters.get("s").unwrap(),
            optimized.resolved_parameters.get("y").unwrap() / 2.0,
            1e-12,
        );
        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn dynamic_likelihood_optimizer_rebuilds_detection_tips_for_each_candidate() {
        let tree = parse_newick("((A:0.5,B:0.5):0.25,C:0.75);").unwrap();
        let data = parse_detection_data(
            "X\tY\nA\t2\t0\nB\t0\t2\nC\t1\t0\n",
            "X\tY\nA\t10\t10\nB\t10\t10\nC\t10\t10\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let table = BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", 0.05)
            .unwrap()
            .with_fixed("e", 0.02)
            .unwrap()
            .with_free("mf", 0.1, ParameterBounds::new(0.005, 0.995).unwrap())
            .unwrap()
            .with_fixed("dp", 0.8)
            .unwrap();
        let evaluations = Cell::new(0_usize);
        let optimized = optimize_parameter_table_dynamic_likelihoods(
            &tree.tree,
            &states,
            RootPrior::Flat,
            &table,
            &ParameterOptimizationConfig {
                max_iterations: 80,
                ..ParameterOptimizationConfig::default()
            },
            |parameters| {
                evaluations.set(evaluations.get() + 1);
                let model = ModelConfig::from_biogeobears_core_parameters(parameters)
                    .map_err(|error| error.to_string())?;
                let detection = DetectionModel::new(
                    parameters
                        .require("mf")
                        .map_err(|error| error.to_string())?,
                    parameters
                        .require("dp")
                        .map_err(|error| error.to_string())?,
                    parameters
                        .require("fdp")
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?;
                let tips = detection
                    .tip_likelihoods(&data, &states)
                    .map_err(|error| error.to_string())?;
                Ok::<_, String>((model, tips))
            },
        )
        .unwrap();

        assert!(evaluations.get() > 3);
        assert_eq!(optimized.free_parameters[0].name, "mf");
        let detection = DetectionModel::new(
            optimized.resolved_parameters.require("mf").unwrap(),
            optimized.resolved_parameters.require("dp").unwrap(),
            optimized.resolved_parameters.require("fdp").unwrap(),
        )
        .unwrap();
        let tips = detection.tip_likelihoods(&data, &states).unwrap();
        let fixed = LikelihoodEngine::new(&tree.tree, &states, RootPrior::Flat)
            .evaluate(&optimized.model, &tips)
            .unwrap();
        assert_close(optimized.log_likelihood, fixed.log_likelihood, 1e-12);
    }

    #[test]
    fn rejects_logit_start_on_exact_boundary() {
        let table = ParameterTable::new(vec![
            ParameterSpec::free("weight", 0.5, ParameterBounds::new(0.0, 1.0).unwrap())
                .unwrap()
                .with_transform(ParameterTransform::Logit)
                .unwrap(),
        ])
        .unwrap();
        let axes = free_parameter_axes(&table).unwrap();
        let config = ParameterOptimizationConfig::default().with_additional_start(vec![0.0]);

        assert!(matches!(
            optimization_starts(&table, &axes, &config),
            Err(ParameterOptimizationError::StartValueNotTransformable {
                start_index: 1,
                parameter,
                ..
            }) if parameter == "weight"
        ));
    }
}
