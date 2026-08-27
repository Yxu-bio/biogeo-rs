use std::error::Error;
use std::fmt;

use crate::q::SparseQ;

const DEFAULT_TOLERANCE: f64 = 1e-13;
const DEFAULT_MAX_TERMS: usize = 10_000;
const DEFAULT_MAX_POISSON_MEAN_PER_SUBDIVISION: f64 = 64.0;
const DEFAULT_MAX_SUBDIVISIONS: usize = 65_536;

#[derive(Clone, Copy)]
struct AdaptivePropagationOptions {
    tolerance: f64,
    max_terms: usize,
    max_poisson_mean_per_subdivision: f64,
    max_subdivisions: usize,
}

const DEFAULT_ADAPTIVE_OPTIONS: AdaptivePropagationOptions = AdaptivePropagationOptions {
    tolerance: DEFAULT_TOLERANCE,
    max_terms: DEFAULT_MAX_TERMS,
    max_poisson_mean_per_subdivision: DEFAULT_MAX_POISSON_MEAN_PER_SUBDIVISION,
    max_subdivisions: DEFAULT_MAX_SUBDIVISIONS,
};

pub fn propagate_uniformized(
    q: &SparseQ,
    branch_length: f64,
    vector: &[f64],
) -> Result<Vec<f64>, PropagationError> {
    propagate_uniformized_adaptive_direction(
        q,
        branch_length,
        vector,
        DEFAULT_ADAPTIVE_OPTIONS,
        PropagationDirection::Forward,
    )
}

pub fn propagate_uniformized_transpose(
    q: &SparseQ,
    branch_length: f64,
    vector: &[f64],
) -> Result<Vec<f64>, PropagationError> {
    propagate_uniformized_adaptive_direction(
        q,
        branch_length,
        vector,
        DEFAULT_ADAPTIVE_OPTIONS,
        PropagationDirection::Transpose,
    )
}

pub fn propagate_uniformized_with_options(
    q: &SparseQ,
    branch_length: f64,
    vector: &[f64],
    tolerance: f64,
    max_terms: usize,
) -> Result<Vec<f64>, PropagationError> {
    propagate_uniformized_direction(
        q,
        branch_length,
        vector,
        tolerance,
        max_terms,
        PropagationDirection::Forward,
    )
}

fn propagate_uniformized_adaptive_direction(
    q: &SparseQ,
    branch_length: f64,
    vector: &[f64],
    options: AdaptivePropagationOptions,
    direction: PropagationDirection,
) -> Result<Vec<f64>, PropagationError> {
    validate_inputs(
        q,
        branch_length,
        vector,
        options.tolerance,
        options.max_terms,
    )?;

    if branch_length == 0.0 {
        return Ok(vector.to_vec());
    }

    let lambda = q.max_exit_rate();
    if lambda == 0.0 {
        return Ok(vector.to_vec());
    }

    let poisson_mean = lambda * branch_length;
    if !poisson_mean.is_finite() {
        return Err(PropagationError::NonFinitePoissonMean {
            lambda,
            branch_length,
        });
    }

    let required_subdivisions = (poisson_mean / options.max_poisson_mean_per_subdivision)
        .ceil()
        .max(1.0);
    if required_subdivisions > options.max_subdivisions as f64 {
        return Err(PropagationError::SubdivisionLimitExceeded {
            poisson_mean,
            max_poisson_mean_per_subdivision: options.max_poisson_mean_per_subdivision,
            max_subdivisions: options.max_subdivisions,
        });
    }
    let subdivision_count = required_subdivisions as usize;
    if subdivision_count == 1 {
        return propagate_uniformized_direction(
            q,
            branch_length,
            vector,
            options.tolerance,
            options.max_terms,
            direction,
        );
    }

    let subdivision_length = branch_length / subdivision_count as f64;
    let subdivision_tolerance =
        (options.tolerance / subdivision_count as f64).max(f64::MIN_POSITIVE);
    let mut propagated = vector.to_vec();
    for subdivision_index in 0..subdivision_count {
        let current_length = if subdivision_index + 1 == subdivision_count {
            branch_length - subdivision_length * (subdivision_count - 1) as f64
        } else {
            subdivision_length
        };
        propagated = propagate_uniformized_direction(
            q,
            current_length,
            &propagated,
            subdivision_tolerance,
            options.max_terms,
            direction,
        )?;
    }
    Ok(propagated)
}

fn propagate_uniformized_direction(
    q: &SparseQ,
    branch_length: f64,
    vector: &[f64],
    tolerance: f64,
    max_terms: usize,
    direction: PropagationDirection,
) -> Result<Vec<f64>, PropagationError> {
    validate_inputs(q, branch_length, vector, tolerance, max_terms)?;

    if branch_length == 0.0 {
        return Ok(vector.to_vec());
    }

    let lambda = q.max_exit_rate();
    if lambda == 0.0 {
        return Ok(vector.to_vec());
    }

    let poisson_mean = lambda * branch_length;
    if !poisson_mean.is_finite() {
        return Err(PropagationError::NonFinitePoissonMean {
            lambda,
            branch_length,
        });
    }

    let mut weight = (-poisson_mean).exp();
    if weight == 0.0 {
        return Err(PropagationError::PoissonWeightUnderflow { poisson_mean });
    }

    let mut total_weight = 0.0;
    let mut current = vector.to_vec();
    let mut next = vec![0.0; q.size()];
    let mut result = vec![0.0; q.size()];

    for term in 0..=max_terms {
        for (result_value, current_value) in result.iter_mut().zip(&current) {
            *result_value += weight * current_value;
        }
        total_weight += weight;

        if term as f64 > poisson_mean && (weight < tolerance || 1.0 - total_weight <= tolerance) {
            return Ok(result);
        }

        let next_term = term + 1;
        if next_term > max_terms {
            break;
        }

        apply_uniformized_step(q, lambda, &current, &mut next, direction);
        std::mem::swap(&mut current, &mut next);
        weight *= poisson_mean / next_term as f64;
    }

    Err(PropagationError::DidNotConverge {
        poisson_mean,
        max_terms,
        accumulated_weight: total_weight,
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PropagationDirection {
    Forward,
    Transpose,
}

fn validate_inputs(
    q: &SparseQ,
    branch_length: f64,
    vector: &[f64],
    tolerance: f64,
    max_terms: usize,
) -> Result<(), PropagationError> {
    if vector.len() != q.size() {
        return Err(PropagationError::VectorLengthMismatch {
            expected: q.size(),
            actual: vector.len(),
        });
    }
    if !branch_length.is_finite() {
        return Err(PropagationError::NonFiniteBranchLength { branch_length });
    }
    if branch_length < 0.0 {
        return Err(PropagationError::NegativeBranchLength { branch_length });
    }
    if !tolerance.is_finite() || tolerance <= 0.0 {
        return Err(PropagationError::InvalidTolerance { tolerance });
    }
    if max_terms == 0 {
        return Err(PropagationError::ZeroMaxTerms);
    }

    for (index, value) in vector.iter().enumerate() {
        if !value.is_finite() {
            return Err(PropagationError::NonFiniteVectorEntry {
                index,
                value: *value,
            });
        }
    }

    Ok(())
}

fn apply_uniformized_step(
    q: &SparseQ,
    lambda: f64,
    input: &[f64],
    output: &mut [f64],
    direction: PropagationDirection,
) {
    output.fill(0.0);

    for row in 0..q.size() {
        output[row] += input[row] * (1.0 + q.diagonal()[row] / lambda);
    }

    match direction {
        PropagationDirection::Forward => {
            for transition in q.transitions() {
                output[transition.from] += transition.rate / lambda * input[transition.to];
            }
        }
        PropagationDirection::Transpose => {
            for transition in q.transitions() {
                output[transition.to] += transition.rate / lambda * input[transition.from];
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum PropagationError {
    VectorLengthMismatch {
        expected: usize,
        actual: usize,
    },
    NonFiniteBranchLength {
        branch_length: f64,
    },
    NegativeBranchLength {
        branch_length: f64,
    },
    InvalidTolerance {
        tolerance: f64,
    },
    ZeroMaxTerms,
    NonFiniteVectorEntry {
        index: usize,
        value: f64,
    },
    NonFinitePoissonMean {
        lambda: f64,
        branch_length: f64,
    },
    PoissonWeightUnderflow {
        poisson_mean: f64,
    },
    SubdivisionLimitExceeded {
        poisson_mean: f64,
        max_poisson_mean_per_subdivision: f64,
        max_subdivisions: usize,
    },
    DidNotConverge {
        poisson_mean: f64,
        max_terms: usize,
        accumulated_weight: f64,
    },
}

impl fmt::Display for PropagationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::VectorLengthMismatch { expected, actual } => write!(
                f,
                "propagation vector length mismatch: expected {expected}, got {actual}"
            ),
            Self::NonFiniteBranchLength { branch_length } => {
                write!(f, "branch length must be finite, got {branch_length}")
            }
            Self::NegativeBranchLength { branch_length } => {
                write!(f, "branch length must be non-negative, got {branch_length}")
            }
            Self::InvalidTolerance { tolerance } => {
                write!(
                    f,
                    "uniformization tolerance must be finite and positive, got {tolerance}"
                )
            }
            Self::ZeroMaxTerms => write!(f, "uniformization max_terms must be greater than zero"),
            Self::NonFiniteVectorEntry { index, value } => write!(
                f,
                "propagation vector entry {index} must be finite, got {value}"
            ),
            Self::NonFinitePoissonMean {
                lambda,
                branch_length,
            } => write!(
                f,
                "uniformization Poisson mean is not finite for lambda={lambda}, branch_length={branch_length}"
            ),
            Self::PoissonWeightUnderflow { poisson_mean } => write!(
                f,
                "initial Poisson weight underflowed for mean {poisson_mean}"
            ),
            Self::SubdivisionLimitExceeded {
                poisson_mean,
                max_poisson_mean_per_subdivision,
                max_subdivisions,
            } => write!(
                f,
                "uniformization mean {poisson_mean} requires more than {max_subdivisions} subdivisions at a maximum mean of {max_poisson_mean_per_subdivision} per subdivision"
            ),
            Self::DidNotConverge {
                poisson_mean,
                max_terms,
                accumulated_weight,
            } => write!(
                f,
                "uniformization did not converge for mean {poisson_mean} within {max_terms} terms; accumulated Poisson weight={accumulated_weight}"
            ),
        }
    }
}

impl Error for PropagationError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::q::{RateTransition, SparseQ};

    fn assert_close_slice(left: &[f64], right: &[f64], tolerance: f64) {
        assert_eq!(left.len(), right.len());
        for (index, (left_value, right_value)) in left.iter().zip(right).enumerate() {
            assert!(
                (left_value - right_value).abs() < tolerance,
                "values differ at index {index}: left={left_value}, right={right_value}"
            );
        }
    }

    #[test]
    fn zero_branch_length_returns_input_vector() {
        let q = symmetric_two_state_q(0.25);
        let propagated = propagate_uniformized(&q, 0.0, &[0.25, 0.75]).unwrap();

        assert_eq!(propagated, vec![0.25, 0.75]);
    }

    #[test]
    fn zero_rate_q_returns_input_vector() {
        let q = SparseQ::new(2, Vec::new());
        let propagated = propagate_uniformized(&q, 5.0, &[0.25, 0.75]).unwrap();

        assert_eq!(propagated, vec![0.25, 0.75]);
    }

    #[test]
    fn symmetric_two_state_model_matches_closed_form() {
        let rate = 0.25;
        let branch_length = 2.0;
        let q = symmetric_two_state_q(rate);
        let propagated = propagate_uniformized(&q, branch_length, &[1.0, 0.0]).unwrap();
        let decay = (-2.0_f64 * rate * branch_length).exp();
        let expected = [0.5 + 0.5 * decay, 0.5 - 0.5 * decay];

        assert_close_slice(&propagated, &expected, 1e-12);
    }

    #[test]
    fn preserves_constant_likelihood_vector() {
        let q = symmetric_two_state_q(0.25);
        let propagated = propagate_uniformized(&q, 3.0, &[1.0, 1.0]).unwrap();

        assert_close_slice(&propagated, &[1.0, 1.0], 1e-12);
    }

    #[test]
    fn transpose_propagation_matches_closed_form_for_asymmetric_two_state_model() {
        let forward_rate = 0.25;
        let reverse_rate = 0.75;
        let branch_length = 1.2;
        let q = SparseQ::new(
            2,
            vec![
                RateTransition {
                    from: 0,
                    to: 1,
                    rate: forward_rate,
                },
                RateTransition {
                    from: 1,
                    to: 0,
                    rate: reverse_rate,
                },
            ],
        );
        let vector = [0.2, 0.8];
        let propagated = propagate_uniformized_transpose(&q, branch_length, &vector).unwrap();

        let total_rate = forward_rate + reverse_rate;
        let decay = (-total_rate * branch_length).exp();
        let p00 = reverse_rate / total_rate + forward_rate / total_rate * decay;
        let p01 = forward_rate / total_rate * (1.0 - decay);
        let p10 = reverse_rate / total_rate * (1.0 - decay);
        let p11 = forward_rate / total_rate + reverse_rate / total_rate * decay;
        let expected = [
            p00 * vector[0] + p10 * vector[1],
            p01 * vector[0] + p11 * vector[1],
        ];

        assert_close_slice(&propagated, &expected, 1e-12);
    }

    #[test]
    fn rejects_invalid_inputs() {
        let q = symmetric_two_state_q(0.25);

        assert_eq!(
            propagate_uniformized(&q, 1.0, &[1.0]),
            Err(PropagationError::VectorLengthMismatch {
                expected: 2,
                actual: 1
            })
        );
        assert_eq!(
            propagate_uniformized(&q, -1.0, &[1.0, 0.0]),
            Err(PropagationError::NegativeBranchLength {
                branch_length: -1.0
            })
        );
        assert!(matches!(
            propagate_uniformized(&q, 1.0, &[f64::NAN, 0.0]),
            Err(PropagationError::NonFiniteVectorEntry { index: 0, .. })
        ));
    }

    #[test]
    fn reports_non_convergence_when_term_budget_is_too_small() {
        let q = symmetric_two_state_q(1.0);
        let error =
            propagate_uniformized_with_options(&q, 10.0, &[1.0, 0.0], 1e-13, 1).unwrap_err();

        assert!(matches!(error, PropagationError::DidNotConverge { .. }));
    }

    #[test]
    fn adaptive_propagation_handles_poisson_weight_underflow() {
        let forward_rate = 1_000.0;
        let reverse_rate = 250.0;
        let q = SparseQ::new(
            2,
            vec![
                RateTransition {
                    from: 0,
                    to: 1,
                    rate: forward_rate,
                },
                RateTransition {
                    from: 1,
                    to: 0,
                    rate: reverse_rate,
                },
            ],
        );

        let backward = propagate_uniformized(&q, 1.0, &[1.0, 0.0]).unwrap();
        let forward = propagate_uniformized_transpose(&q, 1.0, &[1.0, 0.0]).unwrap();

        assert_close_slice(&backward, &[0.2, 0.2], 1e-10);
        assert_close_slice(&forward, &[0.2, 0.8], 1e-10);
        assert!(matches!(
            propagate_uniformized_with_options(&q, 1.0, &[1.0, 0.0], 1e-13, 10_000),
            Err(PropagationError::PoissonWeightUnderflow { .. })
        ));
    }

    #[test]
    fn adaptive_low_mean_path_is_bitwise_identical_to_direct_path() {
        let q = symmetric_two_state_q(0.25);
        let vector = [0.3, 0.7];

        let adaptive = propagate_uniformized(&q, 2.0, &vector).unwrap();
        let direct = propagate_uniformized_with_options(
            &q,
            2.0,
            &vector,
            DEFAULT_TOLERANCE,
            DEFAULT_MAX_TERMS,
        )
        .unwrap();

        assert_eq!(adaptive, direct);
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
