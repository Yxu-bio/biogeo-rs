use std::error::Error;
use std::fmt;

use rand::{Rng, RngExt};

use crate::propagation::{
    PropagationError, propagate_uniformized, propagate_uniformized_transpose,
};
use crate::q::SparseQ;

const DEFAULT_TOLERANCE: f64 = 1e-11;
const DEFAULT_MAX_VIRTUAL_JUMPS: usize = 10_000;
const DEFAULT_MAX_POISSON_MEAN_PER_SUBBRIDGE: f64 = 64.0;
const DEFAULT_MAX_SUBBRIDGES: usize = 65_536;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CtmcBridgeOptions {
    pub tolerance: f64,
    pub max_virtual_jumps: usize,
}

impl Default for CtmcBridgeOptions {
    fn default() -> Self {
        Self {
            tolerance: DEFAULT_TOLERANCE,
            max_virtual_jumps: DEFAULT_MAX_VIRTUAL_JUMPS,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveCtmcBridgeOptions {
    pub direct_options: CtmcBridgeOptions,
    pub max_poisson_mean_per_subbridge: f64,
    pub max_subbridges: usize,
    pub max_real_events: Option<usize>,
}

impl Default for AdaptiveCtmcBridgeOptions {
    fn default() -> Self {
        Self {
            direct_options: CtmcBridgeOptions::default(),
            max_poisson_mean_per_subbridge: DEFAULT_MAX_POISSON_MEAN_PER_SUBBRIDGE,
            max_subbridges: DEFAULT_MAX_SUBBRIDGES,
            max_real_events: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CtmcBridgeEvent {
    pub time: f64,
    pub from_state: usize,
    pub to_state: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CtmcBridge {
    pub duration: f64,
    pub start_state: usize,
    pub end_state: usize,
    pub endpoint_probability: f64,
    pub virtual_jump_count: usize,
    pub events: Vec<CtmcBridgeEvent>,
}

pub fn sample_uniformized_bridge<R: Rng + ?Sized>(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
    rng: &mut R,
) -> Result<CtmcBridge, CtmcBridgeError> {
    sample_uniformized_bridge_adaptive_with_options(
        q,
        duration,
        start_state,
        end_state,
        AdaptiveCtmcBridgeOptions::default(),
        rng,
    )
}

pub fn sample_uniformized_bridge_adaptive_with_options<R: Rng + ?Sized>(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
    options: AdaptiveCtmcBridgeOptions,
    rng: &mut R,
) -> Result<CtmcBridge, CtmcBridgeError> {
    validate_adaptive_inputs(q, duration, start_state, end_state, options)?;

    let lambda = q.max_exit_rate();
    let poisson_mean = lambda * duration;
    if !poisson_mean.is_finite() {
        return Err(CtmcBridgeError::NonFinitePoissonMean { lambda, duration });
    }
    if poisson_mean <= options.max_poisson_mean_per_subbridge {
        let bridge = sample_uniformized_bridge_with_options(
            q,
            duration,
            start_state,
            end_state,
            options.direct_options,
            rng,
        )?;
        enforce_real_event_limit(0, bridge.events.len(), options.max_real_events)?;
        return Ok(bridge);
    }

    let required_subbridges = (poisson_mean / options.max_poisson_mean_per_subbridge)
        .ceil()
        .max(1.0);
    if required_subbridges > options.max_subbridges as f64 {
        return Err(CtmcBridgeError::SubdivisionLimitExceeded {
            poisson_mean,
            max_poisson_mean_per_subbridge: options.max_poisson_mean_per_subbridge,
            max_subbridges: options.max_subbridges,
        });
    }
    let subbridge_count = required_subbridges as usize;

    let endpoint_probability = transition_probability(q, duration, start_state, end_state)?;
    if endpoint_probability <= 0.0 {
        return Err(CtmcBridgeError::ImpossibleEndpoint {
            start_state,
            end_state,
            duration,
        });
    }

    let mut accumulator = AdaptiveBridgeAccumulator::new(options.max_real_events);
    sample_adaptive_subbridges(
        AdaptiveSubbridgeContext {
            q,
            duration,
            time_offset: 0.0,
            start_state,
            end_state,
            subbridge_count,
            direct_options: options.direct_options,
        },
        rng,
        &mut accumulator,
    )?;

    Ok(CtmcBridge {
        duration,
        start_state,
        end_state,
        endpoint_probability,
        virtual_jump_count: accumulator.virtual_jump_count,
        events: accumulator.events,
    })
}

pub fn sample_uniformized_bridge_with_options<R: Rng + ?Sized>(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
    options: CtmcBridgeOptions,
    rng: &mut R,
) -> Result<CtmcBridge, CtmcBridgeError> {
    validate_inputs(q, duration, start_state, end_state, options)?;

    if duration == 0.0 {
        if start_state != end_state {
            return Err(CtmcBridgeError::ZeroDurationEndpointMismatch {
                start_state,
                end_state,
            });
        }
        return Ok(no_event_bridge(duration, start_state, end_state));
    }

    let lambda = q.max_exit_rate();
    if lambda == 0.0 {
        if start_state != end_state {
            return Err(CtmcBridgeError::ZeroRateEndpointMismatch {
                start_state,
                end_state,
            });
        }
        return Ok(no_event_bridge(duration, start_state, end_state));
    }

    let poisson_mean = lambda * duration;
    if !poisson_mean.is_finite() {
        return Err(CtmcBridgeError::NonFinitePoissonMean { lambda, duration });
    }
    let mut poisson_weight = (-poisson_mean).exp();
    if poisson_weight == 0.0 {
        return Err(CtmcBridgeError::PoissonWeightUnderflow { poisson_mean });
    }

    let mut endpoint_masses = Vec::new();
    let mut endpoint_probability = 0.0;
    let mut accumulated_poisson = 0.0;
    let mut current = vec![0.0; q.size()];
    current[end_state] = 1.0;

    for virtual_jumps in 0..=options.max_virtual_jumps {
        let endpoint_mass = poisson_weight * current[start_state];
        if !endpoint_mass.is_finite() || endpoint_mass < 0.0 {
            return Err(CtmcBridgeError::InvalidEndpointMass {
                virtual_jumps,
                value: endpoint_mass,
            });
        }
        endpoint_masses.push(endpoint_mass);
        endpoint_probability += endpoint_mass;
        accumulated_poisson += poisson_weight;

        let rounded_poisson_tail = (1.0 - accumulated_poisson).max(0.0);
        // Near one, subtraction stops resolving the remaining tail; the recurrence bound does not.
        let poisson_tail =
            if virtual_jumps as f64 > poisson_mean && rounded_poisson_tail <= 8.0 * f64::EPSILON {
                remaining_poisson_tail_upper_bound(poisson_weight, poisson_mean, virtual_jumps)
            } else {
                rounded_poisson_tail
            };
        if virtual_jumps as f64 > poisson_mean
            && endpoint_probability > 0.0
            && poisson_tail <= options.tolerance * endpoint_probability
        {
            let sampled_virtual_jumps =
                sample_weighted_index("conditional virtual jump count", &endpoint_masses, rng)?;
            return sample_path_given_virtual_jumps(
                UniformizedBridgeContext {
                    q,
                    duration,
                    start_state,
                    end_state,
                    lambda,
                    endpoint_probability,
                },
                sampled_virtual_jumps,
                rng,
            );
        }

        if virtual_jumps == options.max_virtual_jumps {
            break;
        }
        current = apply_uniformized_backward_step(q, lambda, &current);
        poisson_weight *= poisson_mean / (virtual_jumps + 1) as f64;
    }

    if endpoint_probability <= 0.0 {
        return Err(CtmcBridgeError::ImpossibleEndpoint {
            start_state,
            end_state,
            duration,
        });
    }
    Err(CtmcBridgeError::DidNotConverge {
        poisson_mean,
        max_virtual_jumps: options.max_virtual_jumps,
        endpoint_probability,
        accumulated_poisson,
    })
}

fn remaining_poisson_tail_upper_bound(
    current_weight: f64,
    poisson_mean: f64,
    current_index: usize,
) -> f64 {
    let ratio = poisson_mean / (current_index + 1) as f64;
    debug_assert!(ratio < 1.0);
    let next_weight = current_weight * ratio;
    next_weight / (1.0 - ratio)
}

struct AdaptiveBridgeAccumulator {
    virtual_jump_count: usize,
    events: Vec<CtmcBridgeEvent>,
    max_real_events: Option<usize>,
}

impl AdaptiveBridgeAccumulator {
    fn new(max_real_events: Option<usize>) -> Self {
        Self {
            virtual_jump_count: 0,
            events: Vec::new(),
            max_real_events,
        }
    }
}

#[derive(Clone, Copy)]
struct AdaptiveSubbridgeContext<'a> {
    q: &'a SparseQ,
    duration: f64,
    time_offset: f64,
    start_state: usize,
    end_state: usize,
    subbridge_count: usize,
    direct_options: CtmcBridgeOptions,
}

fn sample_adaptive_subbridges<R: Rng + ?Sized>(
    context: AdaptiveSubbridgeContext<'_>,
    rng: &mut R,
    accumulator: &mut AdaptiveBridgeAccumulator,
) -> Result<(), CtmcBridgeError> {
    if context.subbridge_count == 1 {
        let bridge = sample_uniformized_bridge_with_options(
            context.q,
            context.duration,
            context.start_state,
            context.end_state,
            context.direct_options,
            rng,
        )?;
        enforce_real_event_limit(
            accumulator.events.len(),
            bridge.events.len(),
            accumulator.max_real_events,
        )?;
        accumulator.virtual_jump_count = accumulator
            .virtual_jump_count
            .checked_add(bridge.virtual_jump_count)
            .ok_or(CtmcBridgeError::VirtualJumpCountOverflow)?;
        accumulator
            .events
            .extend(bridge.events.into_iter().map(|mut event| {
                event.time += context.time_offset;
                event
            }));
        return Ok(());
    }

    let left_count = context.subbridge_count / 2;
    let right_count = context.subbridge_count - left_count;
    let left_duration = context.duration * left_count as f64 / context.subbridge_count as f64;
    let right_duration = context.duration - left_duration;

    let forward_probabilities = transition_row(context.q, left_duration, context.start_state)?;
    let mut endpoint_indicator = vec![0.0; context.q.size()];
    endpoint_indicator[context.end_state] = 1.0;
    let backward_probabilities =
        propagate_uniformized(context.q, right_duration, &endpoint_indicator)?;
    let midpoint_masses: Vec<f64> = forward_probabilities
        .iter()
        .zip(backward_probabilities)
        .map(|(forward, backward)| forward * backward)
        .collect();
    let midpoint_state = sample_weighted_index(
        "conditional numerical subdivision boundary state",
        &midpoint_masses,
        rng,
    )?;

    sample_adaptive_subbridges(
        AdaptiveSubbridgeContext {
            q: context.q,
            duration: left_duration,
            time_offset: context.time_offset,
            start_state: context.start_state,
            end_state: midpoint_state,
            subbridge_count: left_count,
            direct_options: context.direct_options,
        },
        rng,
        accumulator,
    )?;
    sample_adaptive_subbridges(
        AdaptiveSubbridgeContext {
            q: context.q,
            duration: right_duration,
            time_offset: context.time_offset + left_duration,
            start_state: midpoint_state,
            end_state: context.end_state,
            subbridge_count: right_count,
            direct_options: context.direct_options,
        },
        rng,
        accumulator,
    )
}

fn enforce_real_event_limit(
    accumulated: usize,
    additional: usize,
    limit: Option<usize>,
) -> Result<(), CtmcBridgeError> {
    let attempted = accumulated
        .checked_add(additional)
        .ok_or(CtmcBridgeError::RealEventCountOverflow)?;
    if let Some(limit) = limit
        && attempted > limit
    {
        return Err(CtmcBridgeError::RealEventLimitExceeded { limit, attempted });
    }
    Ok(())
}

fn transition_probability(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
) -> Result<f64, CtmcBridgeError> {
    Ok(transition_row(q, duration, start_state)?[end_state])
}

fn transition_row(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
) -> Result<Vec<f64>, CtmcBridgeError> {
    let mut start_indicator = vec![0.0; q.size()];
    start_indicator[start_state] = 1.0;
    Ok(propagate_uniformized_transpose(
        q,
        duration,
        &start_indicator,
    )?)
}

#[derive(Clone, Copy)]
struct UniformizedBridgeContext<'a> {
    q: &'a SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
    lambda: f64,
    endpoint_probability: f64,
}

fn sample_path_given_virtual_jumps<R: Rng + ?Sized>(
    context: UniformizedBridgeContext<'_>,
    virtual_jump_count: usize,
    rng: &mut R,
) -> Result<CtmcBridge, CtmcBridgeError> {
    let UniformizedBridgeContext {
        q,
        duration,
        start_state,
        end_state,
        lambda,
        endpoint_probability,
    } = context;
    let mut backward_powers = Vec::with_capacity(virtual_jump_count + 1);
    let mut current_power = vec![0.0; q.size()];
    current_power[end_state] = 1.0;
    backward_powers.push(current_power.clone());
    for _ in 0..virtual_jump_count {
        current_power = apply_uniformized_backward_step(q, lambda, &current_power);
        backward_powers.push(current_power.clone());
    }

    let mut virtual_times: Vec<f64> = (0..virtual_jump_count)
        .map(|_| rng.random::<f64>() * duration)
        .collect();
    virtual_times.sort_by(f64::total_cmp);

    let mut state = start_state;
    let mut events = Vec::new();
    for (step, time) in virtual_times.into_iter().enumerate() {
        let remaining = virtual_jump_count - step - 1;
        let mut masses = vec![0.0; q.size()];
        let self_probability = 1.0 + q.diagonal()[state] / lambda;
        masses[state] += self_probability * backward_powers[remaining][state];
        for transition in q
            .transitions()
            .iter()
            .filter(|transition| transition.from == state)
        {
            masses[transition.to] +=
                transition.rate / lambda * backward_powers[remaining][transition.to];
        }
        let next_state = sample_weighted_index("conditional virtual transition", &masses, rng)?;
        if next_state != state {
            events.push(CtmcBridgeEvent {
                time,
                from_state: state,
                to_state: next_state,
            });
        }
        state = next_state;
    }

    if state != end_state {
        return Err(CtmcBridgeError::PathEndpointMismatch {
            expected: end_state,
            actual: state,
        });
    }

    Ok(CtmcBridge {
        duration,
        start_state,
        end_state,
        endpoint_probability,
        virtual_jump_count,
        events,
    })
}

fn no_event_bridge(duration: f64, start_state: usize, end_state: usize) -> CtmcBridge {
    CtmcBridge {
        duration,
        start_state,
        end_state,
        endpoint_probability: 1.0,
        virtual_jump_count: 0,
        events: Vec::new(),
    }
}

fn apply_uniformized_backward_step(q: &SparseQ, lambda: f64, input: &[f64]) -> Vec<f64> {
    let mut output = vec![0.0; q.size()];
    for state in 0..q.size() {
        output[state] += input[state] * (1.0 + q.diagonal()[state] / lambda);
    }
    for transition in q.transitions() {
        output[transition.from] += transition.rate / lambda * input[transition.to];
    }
    output
}

fn sample_weighted_index<R: Rng + ?Sized>(
    stage: &'static str,
    masses: &[f64],
    rng: &mut R,
) -> Result<usize, CtmcBridgeError> {
    let mut total = 0.0;
    for (item, mass) in masses.iter().copied().enumerate() {
        if !mass.is_finite() || mass < 0.0 {
            return Err(CtmcBridgeError::InvalidSamplingMass {
                stage,
                item,
                value: mass,
            });
        }
        total += mass;
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(CtmcBridgeError::NonPositiveSamplingMass {
            stage,
            value: total,
        });
    }

    let threshold = rng.random::<f64>() * total;
    let mut cumulative = 0.0;
    let mut last_positive = None;
    for (index, mass) in masses.iter().copied().enumerate() {
        if mass <= 0.0 {
            continue;
        }
        last_positive = Some(index);
        cumulative += mass;
        if threshold < cumulative {
            return Ok(index);
        }
    }
    Ok(last_positive.expect("positive total mass must contain a positive entry"))
}

fn validate_inputs(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
    options: CtmcBridgeOptions,
) -> Result<(), CtmcBridgeError> {
    for (role, state) in [("start", start_state), ("end", end_state)] {
        if state >= q.size() {
            return Err(CtmcBridgeError::StateOutOfBounds {
                role,
                state,
                state_count: q.size(),
            });
        }
    }
    if !duration.is_finite() {
        return Err(CtmcBridgeError::NonFiniteDuration { duration });
    }
    if duration < 0.0 {
        return Err(CtmcBridgeError::NegativeDuration { duration });
    }
    if !options.tolerance.is_finite() || options.tolerance <= 0.0 {
        return Err(CtmcBridgeError::InvalidTolerance {
            tolerance: options.tolerance,
        });
    }
    if options.max_virtual_jumps == 0 {
        return Err(CtmcBridgeError::ZeroMaxVirtualJumps);
    }
    Ok(())
}

fn validate_adaptive_inputs(
    q: &SparseQ,
    duration: f64,
    start_state: usize,
    end_state: usize,
    options: AdaptiveCtmcBridgeOptions,
) -> Result<(), CtmcBridgeError> {
    validate_inputs(q, duration, start_state, end_state, options.direct_options)?;
    if !options.max_poisson_mean_per_subbridge.is_finite()
        || options.max_poisson_mean_per_subbridge <= 0.0
    {
        return Err(CtmcBridgeError::InvalidMaxPoissonMeanPerSubbridge {
            value: options.max_poisson_mean_per_subbridge,
        });
    }
    if options.max_subbridges == 0 {
        return Err(CtmcBridgeError::ZeroMaxSubbridges);
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub enum CtmcBridgeError {
    StateOutOfBounds {
        role: &'static str,
        state: usize,
        state_count: usize,
    },
    NonFiniteDuration {
        duration: f64,
    },
    NegativeDuration {
        duration: f64,
    },
    InvalidTolerance {
        tolerance: f64,
    },
    ZeroMaxVirtualJumps,
    InvalidMaxPoissonMeanPerSubbridge {
        value: f64,
    },
    ZeroMaxSubbridges,
    ZeroDurationEndpointMismatch {
        start_state: usize,
        end_state: usize,
    },
    ZeroRateEndpointMismatch {
        start_state: usize,
        end_state: usize,
    },
    NonFinitePoissonMean {
        lambda: f64,
        duration: f64,
    },
    PoissonWeightUnderflow {
        poisson_mean: f64,
    },
    SubdivisionLimitExceeded {
        poisson_mean: f64,
        max_poisson_mean_per_subbridge: f64,
        max_subbridges: usize,
    },
    InvalidEndpointMass {
        virtual_jumps: usize,
        value: f64,
    },
    ImpossibleEndpoint {
        start_state: usize,
        end_state: usize,
        duration: f64,
    },
    DidNotConverge {
        poisson_mean: f64,
        max_virtual_jumps: usize,
        endpoint_probability: f64,
        accumulated_poisson: f64,
    },
    InvalidSamplingMass {
        stage: &'static str,
        item: usize,
        value: f64,
    },
    NonPositiveSamplingMass {
        stage: &'static str,
        value: f64,
    },
    PathEndpointMismatch {
        expected: usize,
        actual: usize,
    },
    RealEventLimitExceeded {
        limit: usize,
        attempted: usize,
    },
    RealEventCountOverflow,
    VirtualJumpCountOverflow,
    Propagation(PropagationError),
}

impl fmt::Display for CtmcBridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateOutOfBounds {
                role,
                state,
                state_count,
            } => write!(
                f,
                "CTMC bridge {role} state {state} is out of bounds for {state_count} states"
            ),
            Self::NonFiniteDuration { duration } => {
                write!(f, "CTMC bridge duration must be finite, got {duration}")
            }
            Self::NegativeDuration { duration } => {
                write!(
                    f,
                    "CTMC bridge duration must be non-negative, got {duration}"
                )
            }
            Self::InvalidTolerance { tolerance } => write!(
                f,
                "CTMC bridge tolerance must be finite and positive, got {tolerance}"
            ),
            Self::ZeroMaxVirtualJumps => {
                write!(f, "CTMC bridge max_virtual_jumps must be greater than zero")
            }
            Self::InvalidMaxPoissonMeanPerSubbridge { value } => write!(
                f,
                "CTMC bridge max_poisson_mean_per_subbridge must be finite and positive, got {value}"
            ),
            Self::ZeroMaxSubbridges => {
                write!(f, "CTMC bridge max_subbridges must be greater than zero")
            }
            Self::ZeroDurationEndpointMismatch {
                start_state,
                end_state,
            } => write!(
                f,
                "zero-duration CTMC bridge cannot connect state {start_state} to {end_state}"
            ),
            Self::ZeroRateEndpointMismatch {
                start_state,
                end_state,
            } => write!(
                f,
                "zero-rate CTMC bridge cannot connect state {start_state} to {end_state}"
            ),
            Self::NonFinitePoissonMean { lambda, duration } => write!(
                f,
                "CTMC bridge Poisson mean is not finite for lambda={lambda}, duration={duration}"
            ),
            Self::PoissonWeightUnderflow { poisson_mean } => write!(
                f,
                "CTMC bridge initial Poisson weight underflowed for mean {poisson_mean}"
            ),
            Self::SubdivisionLimitExceeded {
                poisson_mean,
                max_poisson_mean_per_subbridge,
                max_subbridges,
            } => write!(
                f,
                "CTMC bridge mean {poisson_mean} requires more than {max_subbridges} subbridges at a maximum mean of {max_poisson_mean_per_subbridge} per subbridge"
            ),
            Self::InvalidEndpointMass {
                virtual_jumps,
                value,
            } => write!(
                f,
                "CTMC bridge endpoint mass for {virtual_jumps} virtual jumps is invalid: {value}"
            ),
            Self::ImpossibleEndpoint {
                start_state,
                end_state,
                duration,
            } => write!(
                f,
                "CTMC bridge endpoint {start_state}->{end_state} has zero probability over duration {duration}"
            ),
            Self::DidNotConverge {
                poisson_mean,
                max_virtual_jumps,
                endpoint_probability,
                accumulated_poisson,
            } => write!(
                f,
                "CTMC bridge did not converge for mean {poisson_mean} within {max_virtual_jumps} virtual jumps; endpoint probability={endpoint_probability}, accumulated Poisson mass={accumulated_poisson}"
            ),
            Self::InvalidSamplingMass { stage, item, value } => write!(
                f,
                "{stage} mass at item {item} must be finite and non-negative, got {value}"
            ),
            Self::NonPositiveSamplingMass { stage, value } => {
                write!(
                    f,
                    "{stage} masses must have a positive finite sum, got {value}"
                )
            }
            Self::PathEndpointMismatch { expected, actual } => write!(
                f,
                "sampled CTMC bridge ended in state {actual}, expected {expected}"
            ),
            Self::RealEventLimitExceeded { limit, attempted } => write!(
                f,
                "CTMC bridge sampled at least {attempted} real events, exceeding the configured limit of {limit}"
            ),
            Self::RealEventCountOverflow => {
                write!(f, "CTMC bridge real event count overflowed usize")
            }
            Self::VirtualJumpCountOverflow => {
                write!(f, "CTMC bridge virtual jump count overflowed usize")
            }
            Self::Propagation(error) => write!(f, "CTMC bridge propagation failed: {error}"),
        }
    }
}

impl Error for CtmcBridgeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Propagation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PropagationError> for CtmcBridgeError {
    fn from(value: PropagationError) -> Self {
        Self::Propagation(value)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use super::*;
    use crate::q::RateTransition;

    #[test]
    fn zero_duration_and_zero_rate_require_matching_endpoints() {
        let q = SparseQ::new(2, Vec::new());
        let mut rng = StdRng::seed_from_u64(1);
        let bridge = sample_uniformized_bridge(&q, 0.0, 0, 0, &mut rng).unwrap();
        assert_eq!(bridge.events, Vec::new());
        assert!(matches!(
            sample_uniformized_bridge(&q, 0.0, 0, 1, &mut rng),
            Err(CtmcBridgeError::ZeroDurationEndpointMismatch { .. })
        ));
        assert!(matches!(
            sample_uniformized_bridge(&q, 1.0, 0, 1, &mut rng),
            Err(CtmcBridgeError::ZeroRateEndpointMismatch { .. })
        ));
    }

    #[test]
    fn one_way_bridge_event_time_matches_truncated_exponential_mean() {
        let rate = 0.8;
        let duration = 1.7;
        let q = SparseQ::new(
            2,
            vec![RateTransition {
                from: 0,
                to: 1,
                rate,
            }],
        );
        let expected_endpoint_probability = 1.0 - (-rate * duration).exp();
        let expected_time = 1.0 / rate - duration / ((rate * duration).exp() - 1.0);
        let sample_count = 20_000;
        let mut rng = StdRng::seed_from_u64(20260716);
        let mut time_sum = 0.0;

        for _ in 0..sample_count {
            let bridge = sample_uniformized_bridge(&q, duration, 0, 1, &mut rng).unwrap();
            assert_eq!(bridge.events.len(), 1);
            assert_eq!(bridge.events[0].from_state, 0);
            assert_eq!(bridge.events[0].to_state, 1);
            assert!(bridge.events[0].time >= 0.0 && bridge.events[0].time < duration);
            assert!((bridge.endpoint_probability - expected_endpoint_probability).abs() < 1e-11);
            time_sum += bridge.events[0].time;
        }

        let empirical_time = time_sum / sample_count as f64;
        assert!(
            (empirical_time - expected_time).abs() < 0.015,
            "empirical event time {empirical_time}, expected {expected_time}"
        );
    }

    #[test]
    fn symmetric_bridge_jump_count_matches_odd_poisson_mean() {
        let rate: f64 = 0.7;
        let duration: f64 = 1.3;
        let poisson_mean = rate * duration;
        let q = SparseQ::new(
            2,
            vec![
                RateTransition {
                    from: 0,
                    to: 1,
                    rate,
                },
                RateTransition {
                    from: 1,
                    to: 0,
                    rate,
                },
            ],
        );
        let expected_endpoint_probability = (1.0 - (-2.0 * poisson_mean).exp()) / 2.0;
        let expected_jumps = poisson_mean / poisson_mean.tanh();
        let sample_count = 25_000;
        let mut rng = StdRng::seed_from_u64(99);
        let mut jump_sum = 0.0;

        for _ in 0..sample_count {
            let bridge = sample_uniformized_bridge(&q, duration, 0, 1, &mut rng).unwrap();
            assert_eq!(bridge.virtual_jump_count, bridge.events.len());
            assert_eq!(bridge.events.len() % 2, 1);
            assert_eq!(bridge.events.last().unwrap().to_state, 1);
            assert!((bridge.endpoint_probability - expected_endpoint_probability).abs() < 1e-11);
            jump_sum += bridge.events.len() as f64;
        }

        let empirical_jumps = jump_sum / sample_count as f64;
        assert!(
            (empirical_jumps - expected_jumps).abs() < 0.025,
            "empirical jumps {empirical_jumps}, expected {expected_jumps}"
        );
    }

    #[test]
    fn rare_endpoint_converges_after_accumulated_poisson_rounds_to_one() {
        let rate = 1.0;
        let duration = 3.1047283117220568;
        let state_count = 18;
        let q = SparseQ::new(
            state_count,
            (0..state_count - 1)
                .map(|from| RateTransition {
                    from,
                    to: from + 1,
                    rate,
                })
                .collect(),
        );
        let mut rng = StdRng::seed_from_u64(20260718);

        let bridge = sample_uniformized_bridge_with_options(
            &q,
            duration,
            0,
            state_count - 1,
            CtmcBridgeOptions::default(),
            &mut rng,
        )
        .unwrap();

        assert!(bridge.endpoint_probability > 1e-8);
        assert!(bridge.endpoint_probability < 1e-7);
        assert!(bridge.virtual_jump_count >= state_count - 1);
        assert_eq!(bridge.events.len(), state_count - 1);
        assert_eq!(bridge.events.last().unwrap().to_state, state_count - 1);
    }

    #[test]
    fn adaptive_low_mean_path_is_identical_to_direct_path() {
        let q = symmetric_two_state_q(0.7);
        let mut adaptive_rng = StdRng::seed_from_u64(42);
        let mut direct_rng = StdRng::seed_from_u64(42);

        let adaptive = sample_uniformized_bridge(&q, 1.3, 0, 1, &mut adaptive_rng).unwrap();
        let direct = sample_uniformized_bridge_with_options(
            &q,
            1.3,
            0,
            1,
            CtmcBridgeOptions::default(),
            &mut direct_rng,
        )
        .unwrap();

        assert_eq!(adaptive, direct);
    }

    #[test]
    fn adaptive_bridge_handles_poisson_weight_underflow() {
        let q = symmetric_two_state_q(1_000.0);
        let mut rng = StdRng::seed_from_u64(20260716);

        let bridge = sample_uniformized_bridge(&q, 1.0, 0, 1, &mut rng).unwrap();

        assert!((bridge.endpoint_probability - 0.5).abs() < 1e-10);
        assert_eq!(bridge.virtual_jump_count, bridge.events.len());
        assert_eq!(bridge.events.len() % 2, 1);
        assert_eq!(bridge.events.last().unwrap().to_state, 1);
        assert!(
            bridge
                .events
                .windows(2)
                .all(|pair| pair[0].time <= pair[1].time)
        );
        assert!(
            bridge
                .events
                .iter()
                .all(|event| event.time >= 0.0 && event.time < 1.0)
        );
        assert!(matches!(
            sample_uniformized_bridge_with_options(
                &q,
                1.0,
                0,
                1,
                CtmcBridgeOptions::default(),
                &mut rng,
            ),
            Err(CtmcBridgeError::PoissonWeightUnderflow { .. })
        ));
    }

    #[test]
    fn forced_subdivision_preserves_conditional_jump_distribution() {
        let rate: f64 = 0.7;
        let duration: f64 = 1.3;
        let poisson_mean = rate * duration;
        let expected_jumps = poisson_mean / poisson_mean.tanh();
        let q = symmetric_two_state_q(rate);
        let options = AdaptiveCtmcBridgeOptions {
            max_poisson_mean_per_subbridge: 0.25,
            ..AdaptiveCtmcBridgeOptions::default()
        };
        let sample_count = 10_000;
        let mut rng = StdRng::seed_from_u64(7);
        let mut jump_sum = 0.0;

        for _ in 0..sample_count {
            let bridge = sample_uniformized_bridge_adaptive_with_options(
                &q, duration, 0, 1, options, &mut rng,
            )
            .unwrap();
            assert_eq!(bridge.events.len() % 2, 1);
            assert_eq!(bridge.events.last().unwrap().to_state, 1);
            jump_sum += bridge.events.len() as f64;
        }

        let empirical_jumps = jump_sum / sample_count as f64;
        assert!(
            (empirical_jumps - expected_jumps).abs() < 0.04,
            "empirical jumps {empirical_jumps}, expected {expected_jumps}"
        );
    }

    #[test]
    fn adaptive_bridge_rejects_excessive_subdivision_count() {
        let q = symmetric_two_state_q(3.0);
        let options = AdaptiveCtmcBridgeOptions {
            max_poisson_mean_per_subbridge: 1.0,
            max_subbridges: 2,
            ..AdaptiveCtmcBridgeOptions::default()
        };
        let mut rng = StdRng::seed_from_u64(1);

        assert!(matches!(
            sample_uniformized_bridge_adaptive_with_options(&q, 1.0, 0, 1, options, &mut rng),
            Err(CtmcBridgeError::SubdivisionLimitExceeded { .. })
        ));
    }

    #[test]
    fn adaptive_bridge_enforces_real_event_limit_in_direct_and_subdivided_paths() {
        let low_rate_q = symmetric_two_state_q(0.7);
        let high_rate_q = symmetric_two_state_q(1_000.0);
        let options = AdaptiveCtmcBridgeOptions {
            max_real_events: Some(0),
            ..AdaptiveCtmcBridgeOptions::default()
        };

        let mut direct_rng = StdRng::seed_from_u64(3);
        assert!(matches!(
            sample_uniformized_bridge_adaptive_with_options(
                &low_rate_q,
                1.3,
                0,
                1,
                options,
                &mut direct_rng,
            ),
            Err(CtmcBridgeError::RealEventLimitExceeded {
                limit: 0,
                attempted: 1
            })
        ));

        let mut subdivided_rng = StdRng::seed_from_u64(3);
        assert!(matches!(
            sample_uniformized_bridge_adaptive_with_options(
                &high_rate_q,
                1.0,
                0,
                1,
                options,
                &mut subdivided_rng,
            ),
            Err(CtmcBridgeError::RealEventLimitExceeded {
                limit: 0,
                attempted
            }) if attempted > 0
        ));
    }

    fn symmetric_two_state_q(rate: f64) -> SparseQ {
        SparseQ::new(
            2,
            vec![
                RateTransition {
                    from: 0,
                    to: 1,
                    rate,
                },
                RateTransition {
                    from: 1,
                    to: 0,
                    rate,
                },
            ],
        )
    }
}
