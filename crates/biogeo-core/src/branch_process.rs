use crate::constraints::StateMask;
use crate::propagation::{
    PropagationError, propagate_uniformized, propagate_uniformized_transpose,
};
use crate::q::SparseQ;

pub(crate) trait BranchPropagator {
    fn state_count(&self) -> usize;

    fn propagate(
        &self,
        edge_index: usize,
        branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError>;

    fn propagate_transpose(
        &self,
        edge_index: usize,
        branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError>;
}

pub(crate) struct HomogeneousBranchPropagator<'a> {
    q: &'a SparseQ,
}

impl<'a> HomogeneousBranchPropagator<'a> {
    pub(crate) fn new(q: &'a SparseQ) -> Self {
        Self { q }
    }
}

impl BranchPropagator for HomogeneousBranchPropagator<'_> {
    fn state_count(&self) -> usize {
        self.q.size()
    }

    fn propagate(
        &self,
        _edge_index: usize,
        branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        propagate_uniformized(self.q, branch_length, vector)
    }

    fn propagate_transpose(
        &self,
        _edge_index: usize,
        branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        propagate_uniformized_transpose(self.q, branch_length, vector)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BranchSegment {
    pub(crate) q_index: usize,
    pub(crate) duration: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BranchProcessSegment<'a> {
    pub(crate) q_index: usize,
    pub(crate) duration: f64,
    pub(crate) q: &'a SparseQ,
    pub(crate) state_mask: Option<&'a StateMask>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PiecewiseBranchPropagator {
    q_matrices: Vec<SparseQ>,
    segments_by_edge: Vec<Vec<BranchSegment>>,
    state_masks: Option<Vec<StateMask>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OwnedBranchPropagator {
    Homogeneous {
        q: SparseQ,
        branch_length_exponent: f64,
    },
    Piecewise(PiecewiseBranchPropagator),
}

impl OwnedBranchPropagator {
    pub(crate) fn homogeneous_with_branch_length_exponent(
        q: SparseQ,
        branch_length_exponent: f64,
    ) -> Self {
        debug_assert!(branch_length_exponent.is_finite());
        debug_assert!(branch_length_exponent >= 0.0);
        Self::Homogeneous {
            q,
            branch_length_exponent,
        }
    }

    pub(crate) fn piecewise(
        q_matrices: Vec<SparseQ>,
        segments_by_edge: Vec<Vec<BranchSegment>>,
    ) -> Self {
        Self::Piecewise(PiecewiseBranchPropagator::new(q_matrices, segments_by_edge))
    }

    pub(crate) fn piecewise_with_state_masks(
        q_matrices: Vec<SparseQ>,
        segments_by_edge: Vec<Vec<BranchSegment>>,
        state_masks: Vec<StateMask>,
    ) -> Self {
        Self::Piecewise(PiecewiseBranchPropagator::with_state_masks(
            q_matrices,
            segments_by_edge,
            state_masks,
        ))
    }

    pub(crate) fn history_segments(
        &self,
        edge_index: usize,
        branch_length: f64,
    ) -> Vec<BranchProcessSegment<'_>> {
        match self {
            Self::Homogeneous {
                q,
                branch_length_exponent,
            } => vec![BranchProcessSegment {
                q_index: 0,
                duration: branch_length.powf(*branch_length_exponent),
                q,
                state_mask: None,
            }],
            Self::Piecewise(process) => process.history_segments(edge_index),
        }
    }

    pub(crate) fn effective_branch_length(&self, _edge_index: usize, branch_length: f64) -> f64 {
        match self {
            Self::Homogeneous {
                branch_length_exponent,
                ..
            } => branch_length.powf(*branch_length_exponent),
            Self::Piecewise(_) => branch_length,
        }
    }

    pub(crate) fn history_segment_count(&self, edge_index: usize) -> usize {
        match self {
            Self::Homogeneous { .. } => 1,
            Self::Piecewise(process) => process.segments_by_edge[edge_index].len(),
        }
    }
}

impl BranchPropagator for OwnedBranchPropagator {
    fn state_count(&self) -> usize {
        match self {
            Self::Homogeneous { q, .. } => q.size(),
            Self::Piecewise(process) => process.state_count(),
        }
    }

    fn propagate(
        &self,
        edge_index: usize,
        branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        match self {
            Self::Homogeneous {
                q,
                branch_length_exponent,
            } => propagate_uniformized(q, branch_length.powf(*branch_length_exponent), vector),
            Self::Piecewise(process) => process.propagate(edge_index, branch_length, vector),
        }
    }

    fn propagate_transpose(
        &self,
        edge_index: usize,
        branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        match self {
            Self::Homogeneous {
                q,
                branch_length_exponent,
            } => propagate_uniformized_transpose(
                q,
                branch_length.powf(*branch_length_exponent),
                vector,
            ),
            Self::Piecewise(process) => {
                process.propagate_transpose(edge_index, branch_length, vector)
            }
        }
    }
}

impl PiecewiseBranchPropagator {
    pub(crate) fn new(q_matrices: Vec<SparseQ>, segments_by_edge: Vec<Vec<BranchSegment>>) -> Self {
        Self {
            q_matrices,
            segments_by_edge,
            state_masks: None,
        }
    }

    pub(crate) fn with_state_masks(
        q_matrices: Vec<SparseQ>,
        segments_by_edge: Vec<Vec<BranchSegment>>,
        state_masks: Vec<StateMask>,
    ) -> Self {
        debug_assert_eq!(q_matrices.len(), state_masks.len());
        debug_assert!(
            q_matrices
                .iter()
                .zip(&state_masks)
                .all(|(q, mask)| q.size() == mask.len())
        );
        Self {
            q_matrices,
            segments_by_edge,
            state_masks: Some(state_masks),
        }
    }

    fn history_segments(&self, edge_index: usize) -> Vec<BranchProcessSegment<'_>> {
        self.segments_by_edge[edge_index]
            .iter()
            .rev()
            .map(|segment| BranchProcessSegment {
                q_index: segment.q_index,
                duration: segment.duration,
                q: &self.q_matrices[segment.q_index],
                state_mask: self
                    .state_masks
                    .as_ref()
                    .map(|masks| &masks[segment.q_index]),
            })
            .collect()
    }
}

impl BranchPropagator for PiecewiseBranchPropagator {
    fn state_count(&self) -> usize {
        self.q_matrices.first().map_or(0, SparseQ::size)
    }

    fn propagate(
        &self,
        edge_index: usize,
        _branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        let mut current = vector.to_vec();
        for segment in &self.segments_by_edge[edge_index] {
            if let Some(masks) = &self.state_masks {
                project(&mut current, &masks[segment.q_index]);
            }
            current = propagate_uniformized(
                &self.q_matrices[segment.q_index],
                segment.duration,
                &current,
            )?;
            if let Some(masks) = &self.state_masks {
                project(&mut current, &masks[segment.q_index]);
            }
        }
        Ok(current)
    }

    fn propagate_transpose(
        &self,
        edge_index: usize,
        _branch_length: f64,
        vector: &[f64],
    ) -> Result<Vec<f64>, PropagationError> {
        let mut current = vector.to_vec();
        for segment in self.segments_by_edge[edge_index].iter().rev() {
            if let Some(masks) = &self.state_masks {
                project(&mut current, &masks[segment.q_index]);
            }
            current = propagate_uniformized_transpose(
                &self.q_matrices[segment.q_index],
                segment.duration,
                &current,
            )?;
            if let Some(masks) = &self.state_masks {
                project(&mut current, &masks[segment.q_index]);
            }
        }
        Ok(current)
    }
}

fn project(values: &mut [f64], mask: &StateMask) {
    debug_assert_eq!(values.len(), mask.len());
    for (value, allowed) in values.iter_mut().zip(mask.values()) {
        if !allowed {
            *value = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::q::RateTransition;

    fn asymmetric_q(rate_01: f64, rate_10: f64) -> SparseQ {
        SparseQ::new(
            2,
            vec![
                RateTransition {
                    from: 0,
                    to: 1,
                    rate: rate_01,
                },
                RateTransition {
                    from: 1,
                    to: 0,
                    rate: rate_10,
                },
            ],
        )
    }

    #[test]
    fn piecewise_forward_and_transpose_use_opposite_segment_orders() {
        let q_young = asymmetric_q(0.1, 0.8);
        let q_old = asymmetric_q(0.7, 0.2);
        let process = PiecewiseBranchPropagator::new(
            vec![q_young.clone(), q_old.clone()],
            vec![vec![
                BranchSegment {
                    q_index: 0,
                    duration: 0.4,
                },
                BranchSegment {
                    q_index: 1,
                    duration: 0.6,
                },
            ]],
        );
        let vector = [0.3, 0.7];

        let expected_forward = propagate_uniformized(
            &q_old,
            0.6,
            &propagate_uniformized(&q_young, 0.4, &vector).unwrap(),
        )
        .unwrap();
        let expected_transpose = propagate_uniformized_transpose(
            &q_young,
            0.4,
            &propagate_uniformized_transpose(&q_old, 0.6, &vector).unwrap(),
        )
        .unwrap();

        assert_eq!(
            process.propagate(0, 1.0, &vector).unwrap(),
            expected_forward
        );
        assert_eq!(
            process.propagate_transpose(0, 1.0, &vector).unwrap(),
            expected_transpose
        );
    }

    #[test]
    fn homogeneous_branch_length_exponent_is_shared_by_propagation_and_history() {
        let q = asymmetric_q(0.2, 0.4);
        let powered =
            OwnedBranchPropagator::homogeneous_with_branch_length_exponent(q.clone(), 0.0);
        let ordinary = HomogeneousBranchPropagator::new(&q);
        let vector = [1.0, 0.0];

        let powered_result = powered.propagate(0, 3.5, &vector).unwrap();
        let unit_result = ordinary.propagate(0, 1.0, &vector).unwrap();
        assert_eq!(powered_result, unit_result);
        assert_eq!(powered.effective_branch_length(0, 3.5), 1.0);
        assert_eq!(powered.history_segments(0, 3.5)[0].duration, 1.0);
    }

    #[test]
    fn piecewise_state_masks_project_at_epoch_boundaries() {
        let process = PiecewiseBranchPropagator::with_state_masks(
            vec![SparseQ::new(2, Vec::new()), SparseQ::new(2, Vec::new())],
            vec![vec![
                BranchSegment {
                    q_index: 0,
                    duration: 0.4,
                },
                BranchSegment {
                    q_index: 1,
                    duration: 0.6,
                },
            ]],
            vec![
                StateMask::new(vec![true, true]).unwrap(),
                StateMask::new(vec![true, false]).unwrap(),
            ],
        );

        assert_eq!(
            process.propagate(0, 1.0, &[0.25, 0.75]).unwrap(),
            vec![0.25, 0.0]
        );
        assert_eq!(
            process.propagate_transpose(0, 1.0, &[0.25, 0.75]).unwrap(),
            vec![0.25, 0.0]
        );
    }
}
