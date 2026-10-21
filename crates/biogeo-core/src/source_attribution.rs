//! Conditional source allocation, separate from the stochastic-history RNG.
use crate::{AreaSet, DispersalMultiplierMatrix};
use rand::{RngExt, SeedableRng};
use rand_chacha::ChaCha12Rng;

pub const SOURCE_ALGORITHM: &str = "effective-dmat-conditional-v1";
pub const SOURCE_RNG_PROTOCOL: &str = "chacha12-source-event-v1";
pub const ROUTE_ALGORITHM: &str = "effective-dmat-joint-conditional-v1";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteCandidate {
    pub source_area: u8,
    pub target_area: u8,
    pub probability: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceCandidate {
    pub area: u8,
    pub probability: f64,
}

/// A logical identity, never a physical file offset. Ordinals start at zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceEventId {
    Anagenetic {
        sample: u64,
        edge: u64,
        ordinal: u64,
    },
    Cladogenetic {
        sample: u64,
        node: u64,
    },
}

/// Empty output means zero total effective weight, not a uniform distribution.
/// The matrix must already include the fitted geographic modifiers.
pub fn source_probabilities(
    source: AreaSet,
    target: u8,
    num_areas: u8,
    matrix: Option<&DispersalMultiplierMatrix>,
) -> Result<Vec<SourceCandidate>, &'static str> {
    if target >= 64 {
        return Err("invalid target area");
    }
    Ok(route_probabilities(
        source,
        AreaSet::from_bits(1u64 << target),
        num_areas,
        matrix,
    )?
    .into_iter()
    .map(|route| SourceCandidate {
        area: route.source_area,
        probability: route.probability,
    })
    .collect())
}

/// Conditional contributions for a fixed event, not additional dispersal events.
/// For a founder split, j, the range-size weight, the pair-count divisor and
/// the ancestor-row normalizer are common to all pairs and cancel.
/// Empty output means zero total effective weight; absent matrix means unit weights.
pub fn route_probabilities(
    source: AreaSet,
    target: AreaSet,
    num_areas: u8,
    matrix: Option<&DispersalMultiplierMatrix>,
) -> Result<Vec<RouteCandidate>, &'static str> {
    if num_areas == 0
        || num_areas > 64
        || (num_areas < 64 && (source.bits() | target.bits()) >> num_areas != 0)
        || source.is_empty()
        || target.is_empty()
        || source.bits() & target.bits() != 0
    {
        return Err("invalid source range or target area");
    }
    if matrix.is_some_and(|matrix| matrix.num_areas() != usize::from(num_areas)) {
        return Err("source matrix dimension mismatch");
    }
    let mut candidates = Vec::new();
    for area in 0..num_areas {
        if source.contains(area) {
            for destination in 0..num_areas {
                if !target.contains(destination) {
                    continue;
                }
                let weight = matrix.map_or(1.0, |matrix| {
                    matrix.get(usize::from(area), usize::from(destination))
                });
                if !weight.is_finite() || weight < 0.0 {
                    return Err("source weights must be finite and nonnegative");
                }
                if weight > 0.0 {
                    candidates.push(RouteCandidate {
                        source_area: area,
                        target_area: destination,
                        probability: weight,
                    });
                }
            }
        }
    }
    // Scaling avoids overflowing when several individually finite weights are huge.
    let maximum = candidates.iter().map(|c| c.probability).fold(0.0, f64::max);
    let total: f64 = candidates.iter().map(|c| c.probability / maximum).sum();
    for candidate in &mut candidates {
        candidate.probability = (candidate.probability / maximum) / total;
    }
    candidates.retain(|candidate| candidate.probability > 0.0);
    Ok(candidates)
}

/// No RNG is needed for an empty or unique conditional source.
pub fn sample_source(
    candidates: &[SourceCandidate],
    seed: u64,
    id: SourceEventId,
) -> Option<SourceCandidate> {
    if candidates.len() < 2 {
        return candidates.first().copied();
    }
    let (sample, location, ordinal, domain) = match id {
        SourceEventId::Anagenetic {
            sample,
            edge,
            ordinal,
        } => (sample, edge, ordinal, 0x616e6167656e6574),
        SourceEventId::Cladogenetic { sample, node } => (sample, node, 0, 0x636c61646f67656e),
    };
    let mut bytes = [0u8; 32];
    for (lane, value) in [seed ^ domain, sample, location, ordinal]
        .into_iter()
        .enumerate()
    {
        bytes[lane * 8..lane * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    let draw = ChaCha12Rng::from_seed(bytes).random::<f64>();
    let mut cumulative = 0.0;
    for candidate in candidates {
        cumulative += candidate.probability;
        if draw < cumulative {
            return Some(*candidate);
        }
    }
    candidates.last().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joint_founder_probabilities_use_all_directional_pairs() {
        let matrix = DispersalMultiplierMatrix::new(
            4,
            vec![
                0., 0., 1., 2., 0., 0., 6., 1., 9., 9., 0., 0., 9., 9., 0., 0.,
            ],
        )
        .unwrap();
        let routes = route_probabilities(
            AreaSet::from_bits(3),
            AreaSet::from_bits(12),
            4,
            Some(&matrix),
        )
        .unwrap();
        assert_eq!(
            routes
                .iter()
                .map(|r| (r.source_area, r.target_area))
                .collect::<Vec<_>>(),
            vec![(0, 2), (0, 3), (1, 2), (1, 3)]
        );
        for (route, expected) in routes.iter().zip([0.1, 0.2, 0.6, 0.1]) {
            assert!((route.probability - expected).abs() < 1e-15);
        }
        let uniform =
            route_probabilities(AreaSet::from_bits(3), AreaSet::from_bits(12), 4, None).unwrap();
        assert!(uniform.iter().all(|r| r.probability == 0.25));
        let huge = DispersalMultiplierMatrix::new(4, vec![f64::MAX; 16]).unwrap();
        assert_eq!(
            route_probabilities(
                AreaSet::from_bits(3),
                AreaSet::from_bits(12),
                4,
                Some(&huge)
            )
            .unwrap(),
            uniform
        );
        let zero = DispersalMultiplierMatrix::new(4, vec![0.; 16]).unwrap();
        assert!(
            route_probabilities(
                AreaSet::from_bits(3),
                AreaSet::from_bits(12),
                4,
                Some(&zero)
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            route_probabilities(AreaSet::from_bits(3), AreaSet::from_bits(6), 4, None).is_err()
        );
        assert!(route_probabilities(AreaSet::from_bits(3), AreaSet::EMPTY, 4, None).is_err());
        assert!(
            route_probabilities(AreaSet::from_bits(3), AreaSet::from_bits(16), 4, None).is_err()
        );
    }

    #[test]
    fn directional_probabilities_and_distribution() {
        let matrix =
            DispersalMultiplierMatrix::new(3, vec![0., 1., 9., 1., 0., 1., 3., 7., 0.]).unwrap();
        let probabilities =
            source_probabilities(AreaSet::from_bits(3), 2, 3, Some(&matrix)).unwrap();
        assert_eq!(
            probabilities.iter().map(|p| p.area).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!((probabilities[0].probability - 0.9).abs() < 1e-15);
        assert!((probabilities[1].probability - 0.1).abs() < 1e-15);
        let count = (0..20_000)
            .filter(|sample| {
                sample_source(
                    &probabilities,
                    19,
                    SourceEventId::Anagenetic {
                        sample: *sample,
                        edge: 3,
                        ordinal: 0,
                    },
                )
                .unwrap()
                .area
                    == 0
            })
            .count();
        assert!((count as f64 - 18_000.).abs() < 6. * (1800.0_f64).sqrt());
        let reversed = source_probabilities(AreaSet::from_bits(6), 0, 3, Some(&matrix)).unwrap();
        assert_eq!(reversed[0].probability, 0.25);
    }

    #[test]
    fn zero_unique_uniform_and_large_weights() {
        let zero = DispersalMultiplierMatrix::new(3, vec![0.; 9]).unwrap();
        assert!(DispersalMultiplierMatrix::new(3, vec![-1.; 9]).is_err());
        assert!(DispersalMultiplierMatrix::new(3, vec![f64::NAN; 9]).is_err());
        assert!(DispersalMultiplierMatrix::new(3, vec![f64::INFINITY; 9]).is_err());
        assert_eq!(
            sample_source(&[], 17, SourceEventId::Cladogenetic { sample: 0, node: 0 }),
            None
        );
        assert!(
            source_probabilities(AreaSet::from_bits(3), 2, 3, Some(&zero))
                .unwrap()
                .is_empty()
        );
        let uniform = source_probabilities(AreaSet::from_bits(3), 2, 3, None).unwrap();
        assert_eq!(uniform[0].probability, 0.5);
        let unique = source_probabilities(AreaSet::from_bits(1), 2, 3, None).unwrap();
        assert_eq!(unique[0].probability, 1.);
        let huge = DispersalMultiplierMatrix::new(3, vec![f64::MAX; 9]).unwrap();
        assert_eq!(
            source_probabilities(AreaSet::from_bits(3), 2, 3, Some(&huge)).unwrap(),
            uniform
        );
        assert!(source_probabilities(AreaSet::EMPTY, 2, 3, None).is_err());
        assert!(source_probabilities(AreaSet::from_bits(3), 0, 3, None).is_err());
        assert!(source_probabilities(AreaSet::from_bits(8), 0, 3, None).is_err());
    }

    #[test]
    fn logical_identity_is_deterministic_and_domain_separated() {
        let p = source_probabilities(AreaSet::from_bits(3), 2, 3, None).unwrap();
        let ana: Vec<_> = (0..100)
            .map(|sample| {
                sample_source(
                    &p,
                    77,
                    SourceEventId::Anagenetic {
                        sample,
                        edge: 0,
                        ordinal: 0,
                    },
                )
            })
            .collect();
        let again: Vec<_> = (0..100)
            .rev()
            .map(|sample| {
                sample_source(
                    &p,
                    77,
                    SourceEventId::Anagenetic {
                        sample,
                        edge: 0,
                        ordinal: 0,
                    },
                )
            })
            .rev()
            .collect();
        let clado: Vec<_> = (0..100)
            .map(|sample| sample_source(&p, 77, SourceEventId::Cladogenetic { sample, node: 0 }))
            .collect();
        assert_eq!(ana, again);
        assert_ne!(ana, clado);
    }
}
