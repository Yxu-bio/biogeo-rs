use std::error::Error;
use std::fmt;

use crate::bsm::{AnageneticEventKind, BiogeographicStochasticMap, CladogeneticSplitSample};
use crate::constraints::StateMask;
use crate::state::StateSpace;

const TIME_TOLERANCE: f64 = 1e-10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CladogeneticEventKind {
    RangeCopying,
    SubsetSympatry,
    Vicariance,
    FounderEvent,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CladogeneticEventCounts {
    pub range_copying: usize,
    pub subset_sympatry: usize,
    pub vicariance: usize,
    pub founder_event: usize,
}

impl CladogeneticEventCounts {
    pub fn total(self) -> usize {
        self.range_copying + self.subset_sympatry + self.vicariance + self.founder_event
    }

    fn record(&mut self, kind: CladogeneticEventKind) {
        match kind {
            CladogeneticEventKind::RangeCopying => self.range_copying += 1,
            CladogeneticEventKind::SubsetSympatry => self.subset_sympatry += 1,
            CladogeneticEventKind::Vicariance => self.vicariance += 1,
            CladogeneticEventKind::FounderEvent => self.founder_event += 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BsmSampleDiagnostics {
    pub segment_count: usize,
    pub constrained_segment_count: usize,
    pub minimum_endpoint_probability: Option<f64>,
    pub maximum_virtual_jump_count: usize,
    pub maximum_anagenetic_events_per_segment: usize,
    pub forbidden_state_transitions: usize,
    pub forbidden_state_endpoints: usize,
    pub forbidden_state_time: f64,
}

impl Default for BsmSampleDiagnostics {
    fn default() -> Self {
        Self {
            segment_count: 0,
            constrained_segment_count: 0,
            minimum_endpoint_probability: None,
            maximum_virtual_jump_count: 0,
            maximum_anagenetic_events_per_segment: 0,
            forbidden_state_transitions: 0,
            forbidden_state_endpoints: 0,
            forbidden_state_time: 0.0,
        }
    }
}

impl BsmSampleDiagnostics {
    pub fn state_constraint_violation_count(&self) -> usize {
        self.forbidden_state_transitions + self.forbidden_state_endpoints
    }

    pub fn has_state_constraint_violations(&self) -> bool {
        self.state_constraint_violation_count() > 0 || self.forbidden_state_time > TIME_TOLERANCE
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BsmSampleSummary {
    pub anagenetic_event_count: usize,
    pub range_expansion_count: usize,
    pub local_extirpation_count: usize,
    pub range_switching_count: usize,
    pub cladogenetic_event_counts: CladogeneticEventCounts,
    pub event_counts_by_q: Vec<usize>,
    pub occupancy_time_by_state: Vec<f64>,
    pub occupancy_time_by_q_and_state: Vec<Vec<f64>>,
    pub total_branch_time: f64,
    pub diagnostics: BsmSampleDiagnostics,
}

impl BiogeographicStochasticMap {
    pub fn summarize(&self, states: &StateSpace) -> Result<BsmSampleSummary, BsmSummaryError> {
        summarize_stochastic_map(self, states)
    }

    pub fn summarize_with_state_masks(
        &self,
        states: &StateSpace,
        state_masks: Option<&[StateMask]>,
    ) -> Result<BsmSampleSummary, BsmSummaryError> {
        summarize_stochastic_map_with_state_masks(self, states, state_masks)
    }
}

pub fn summarize_stochastic_map(
    map: &BiogeographicStochasticMap,
    states: &StateSpace,
) -> Result<BsmSampleSummary, BsmSummaryError> {
    summarize_stochastic_map_with_state_masks(map, states, None)
}

pub fn summarize_stochastic_map_with_state_masks(
    map: &BiogeographicStochasticMap,
    states: &StateSpace,
    state_masks: Option<&[StateMask]>,
) -> Result<BsmSampleSummary, BsmSummaryError> {
    let q_count = map
        .branches
        .iter()
        .flat_map(|branch| &branch.segments)
        .map(|segment| segment.q_index + 1)
        .max()
        .unwrap_or(0);
    let mut summary = BsmSampleSummary {
        anagenetic_event_count: 0,
        range_expansion_count: 0,
        local_extirpation_count: 0,
        range_switching_count: 0,
        cladogenetic_event_counts: CladogeneticEventCounts::default(),
        event_counts_by_q: vec![0; q_count],
        occupancy_time_by_state: vec![0.0; states.len()],
        occupancy_time_by_q_and_state: vec![vec![0.0; states.len()]; q_count],
        total_branch_time: 0.0,
        diagnostics: BsmSampleDiagnostics::default(),
    };

    if let Some(masks) = state_masks {
        if masks.len() < q_count {
            return Err(BsmSummaryError::StateMaskCountMismatch {
                expected_at_least: q_count,
                actual: masks.len(),
            });
        }
        for (q_index, mask) in masks.iter().enumerate().take(q_count) {
            if mask.len() != states.len() {
                return Err(BsmSummaryError::StateMaskSizeMismatch {
                    q_index,
                    expected: states.len(),
                    actual: mask.len(),
                });
            }
        }
    }

    validate_state(states, map.skeleton.root_state, "skeleton root")?;
    for state in map.skeleton.node_states.iter().copied() {
        validate_state(states, state, "skeleton node")?;
    }
    for endpoint in &map.skeleton.branch_endpoints {
        validate_state(states, endpoint.start_state, "skeleton branch start")?;
        validate_state(states, endpoint.end_state, "skeleton branch end")?;
    }

    for split in &map.skeleton.splits {
        summary
            .cladogenetic_event_counts
            .record(classify_cladogenetic_event(states, *split)?);
    }

    for branch in &map.branches {
        validate_state(states, branch.start_state, "branch start")?;
        validate_state(states, branch.end_state, "branch end")?;
        let mut expected_state = branch.start_state;
        let mut expected_time = 0.0;

        for (expected_segment_index, segment) in branch.segments.iter().enumerate() {
            validate_state(states, segment.start_state, "segment start")?;
            validate_state(states, segment.end_state, "segment end")?;
            if segment.segment_index != expected_segment_index {
                return Err(BsmSummaryError::SegmentIndexMismatch {
                    edge_index: branch.edge_index,
                    expected: expected_segment_index,
                    actual: segment.segment_index,
                });
            }
            if segment.start_state != expected_state {
                return Err(BsmSummaryError::StateChainMismatch {
                    edge_index: branch.edge_index,
                    segment_index: segment.segment_index,
                    expected: expected_state,
                    actual: segment.start_state,
                });
            }
            if !segment.start_time_from_parent.is_finite()
                || !segment.end_time_from_parent.is_finite()
                || segment.start_time_from_parent < 0.0
                || segment.end_time_from_parent + TIME_TOLERANCE < segment.start_time_from_parent
                || (segment.start_time_from_parent - expected_time).abs() > TIME_TOLERANCE
            {
                return Err(BsmSummaryError::InvalidSegmentTimes {
                    edge_index: branch.edge_index,
                    segment_index: segment.segment_index,
                    start: segment.start_time_from_parent,
                    end: segment.end_time_from_parent,
                    expected_start: expected_time,
                });
            }
            if !segment.endpoint_probability.is_finite() || segment.endpoint_probability <= 0.0 {
                return Err(BsmSummaryError::InvalidEndpointProbability {
                    edge_index: branch.edge_index,
                    segment_index: segment.segment_index,
                    probability: segment.endpoint_probability,
                });
            }

            summary.diagnostics.segment_count += 1;
            summary.diagnostics.minimum_endpoint_probability = Some(
                summary
                    .diagnostics
                    .minimum_endpoint_probability
                    .map_or(segment.endpoint_probability, |current| {
                        current.min(segment.endpoint_probability)
                    }),
            );
            summary.diagnostics.maximum_virtual_jump_count = summary
                .diagnostics
                .maximum_virtual_jump_count
                .max(segment.virtual_jump_count);
            summary.diagnostics.maximum_anagenetic_events_per_segment = summary
                .diagnostics
                .maximum_anagenetic_events_per_segment
                .max(segment.events.len());
            let state_mask = state_masks.map(|masks| &masks[segment.q_index]);
            if let Some(mask) = state_mask {
                summary.diagnostics.constrained_segment_count += 1;
                summary.diagnostics.forbidden_state_endpoints +=
                    usize::from(!mask.is_allowed(segment.start_state))
                        + usize::from(!mask.is_allowed(segment.end_state));
            }

            let mut current_state = segment.start_state;
            let mut current_time = segment.start_time_from_parent;
            for event in &segment.events {
                validate_state(states, event.from_state, "event source")?;
                validate_state(states, event.to_state, "event destination")?;
                if event.edge_index != branch.edge_index
                    || event.segment_index != segment.segment_index
                    || event.q_index != segment.q_index
                {
                    return Err(BsmSummaryError::EventContextMismatch {
                        edge_index: branch.edge_index,
                        segment_index: segment.segment_index,
                    });
                }
                if !event.time_from_parent.is_finite()
                    || event.time_from_parent + TIME_TOLERANCE < current_time
                    || event.time_from_parent > segment.end_time_from_parent + TIME_TOLERANCE
                {
                    return Err(BsmSummaryError::InvalidEventTime {
                        edge_index: branch.edge_index,
                        segment_index: segment.segment_index,
                        time: event.time_from_parent,
                        previous_time: current_time,
                        segment_end: segment.end_time_from_parent,
                    });
                }
                if event.from_state != current_state {
                    return Err(BsmSummaryError::EventStateMismatch {
                        edge_index: branch.edge_index,
                        segment_index: segment.segment_index,
                        expected: current_state,
                        actual: event.from_state,
                    });
                }

                add_occupancy(
                    &mut summary,
                    segment.q_index,
                    current_state,
                    event.time_from_parent - current_time,
                    state_mask,
                );
                summary.anagenetic_event_count += 1;
                summary.event_counts_by_q[segment.q_index] += 1;
                match event.kind {
                    AnageneticEventKind::RangeExpansion { .. } => {
                        summary.range_expansion_count += 1
                    }
                    AnageneticEventKind::LocalExtirpation { .. } => {
                        summary.local_extirpation_count += 1
                    }
                    AnageneticEventKind::RangeSwitching { .. } => {
                        summary.range_switching_count += 1
                    }
                }
                current_state = event.to_state;
                if state_mask.is_some_and(|mask| !mask.is_allowed(current_state)) {
                    summary.diagnostics.forbidden_state_transitions += 1;
                }
                current_time = event.time_from_parent;
            }

            if current_state != segment.end_state {
                return Err(BsmSummaryError::SegmentEndStateMismatch {
                    edge_index: branch.edge_index,
                    segment_index: segment.segment_index,
                    expected: segment.end_state,
                    actual: current_state,
                });
            }
            add_occupancy(
                &mut summary,
                segment.q_index,
                current_state,
                segment.end_time_from_parent - current_time,
                state_mask,
            );
            summary.total_branch_time +=
                segment.end_time_from_parent - segment.start_time_from_parent;
            expected_state = segment.end_state;
            expected_time = segment.end_time_from_parent;
        }

        if expected_state != branch.end_state {
            return Err(BsmSummaryError::BranchEndStateMismatch {
                edge_index: branch.edge_index,
                expected: branch.end_state,
                actual: expected_state,
            });
        }
    }

    Ok(summary)
}

pub fn classify_cladogenetic_event(
    states: &StateSpace,
    split: CladogeneticSplitSample,
) -> Result<CladogeneticEventKind, BsmSummaryError> {
    let ancestor = states
        .get(split.ancestor)
        .ok_or(BsmSummaryError::StateOutOfBounds {
            context: "cladogenetic ancestor",
            state: split.ancestor,
            state_count: states.len(),
        })?;
    let left = states
        .get(split.left)
        .ok_or(BsmSummaryError::StateOutOfBounds {
            context: "left daughter",
            state: split.left,
            state_count: states.len(),
        })?;
    let right = states
        .get(split.right)
        .ok_or(BsmSummaryError::StateOutOfBounds {
            context: "right daughter",
            state: split.right,
            state_count: states.len(),
        })?;

    if left == ancestor && right == ancestor {
        return Ok(CladogeneticEventKind::RangeCopying);
    }
    if left == ancestor || right == ancestor {
        let other = if left == ancestor { right } else { left };
        if other.bits() != 0 && other.bits() & !ancestor.bits() == 0 {
            return Ok(CladogeneticEventKind::SubsetSympatry);
        }
        if other.bits() != 0 && other.bits() & ancestor.bits() == 0 {
            return Ok(CladogeneticEventKind::FounderEvent);
        }
    }
    if left.bits() & right.bits() == 0
        && left.bits() | right.bits() == ancestor.bits()
        && left.bits() != 0
        && right.bits() != 0
    {
        return Ok(CladogeneticEventKind::Vicariance);
    }

    Err(BsmSummaryError::UnsupportedCladogeneticSplit {
        node: split.node,
        ancestor: split.ancestor,
        left: split.left,
        right: split.right,
    })
}

fn validate_state(
    states: &StateSpace,
    state: usize,
    context: &'static str,
) -> Result<(), BsmSummaryError> {
    if state >= states.len() {
        return Err(BsmSummaryError::StateOutOfBounds {
            context,
            state,
            state_count: states.len(),
        });
    }
    Ok(())
}

fn add_occupancy(
    summary: &mut BsmSampleSummary,
    q_index: usize,
    state: usize,
    duration: f64,
    state_mask: Option<&StateMask>,
) {
    let duration = duration.max(0.0);
    summary.occupancy_time_by_state[state] += duration;
    summary.occupancy_time_by_q_and_state[q_index][state] += duration;
    if state_mask.is_some_and(|mask| !mask.is_allowed(state)) {
        summary.diagnostics.forbidden_state_time += duration;
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BsmSummaryError {
    StateOutOfBounds {
        context: &'static str,
        state: usize,
        state_count: usize,
    },
    SegmentIndexMismatch {
        edge_index: usize,
        expected: usize,
        actual: usize,
    },
    StateChainMismatch {
        edge_index: usize,
        segment_index: usize,
        expected: usize,
        actual: usize,
    },
    InvalidSegmentTimes {
        edge_index: usize,
        segment_index: usize,
        start: f64,
        end: f64,
        expected_start: f64,
    },
    InvalidEndpointProbability {
        edge_index: usize,
        segment_index: usize,
        probability: f64,
    },
    StateMaskCountMismatch {
        expected_at_least: usize,
        actual: usize,
    },
    StateMaskSizeMismatch {
        q_index: usize,
        expected: usize,
        actual: usize,
    },
    EventContextMismatch {
        edge_index: usize,
        segment_index: usize,
    },
    InvalidEventTime {
        edge_index: usize,
        segment_index: usize,
        time: f64,
        previous_time: f64,
        segment_end: f64,
    },
    EventStateMismatch {
        edge_index: usize,
        segment_index: usize,
        expected: usize,
        actual: usize,
    },
    SegmentEndStateMismatch {
        edge_index: usize,
        segment_index: usize,
        expected: usize,
        actual: usize,
    },
    BranchEndStateMismatch {
        edge_index: usize,
        expected: usize,
        actual: usize,
    },
    UnsupportedCladogeneticSplit {
        node: usize,
        ancestor: usize,
        left: usize,
        right: usize,
    },
}

impl fmt::Display for BsmSummaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateOutOfBounds {
                context,
                state,
                state_count,
            } => write!(
                f,
                "{context} state {state} is out of bounds for {state_count} states"
            ),
            Self::SegmentIndexMismatch {
                edge_index,
                expected,
                actual,
            } => write!(
                f,
                "edge {edge_index} expected segment index {expected}, got {actual}"
            ),
            Self::StateChainMismatch {
                edge_index,
                segment_index,
                expected,
                actual,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} starts in state {actual}, expected {expected}"
            ),
            Self::InvalidSegmentTimes {
                edge_index,
                segment_index,
                start,
                end,
                expected_start,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} has invalid time interval {start}..{end}; expected start {expected_start}"
            ),
            Self::InvalidEndpointProbability {
                edge_index,
                segment_index,
                probability,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} has invalid endpoint probability {probability}"
            ),
            Self::StateMaskCountMismatch {
                expected_at_least,
                actual,
            } => write!(
                f,
                "BSM summary requires at least {expected_at_least} state masks, got {actual}"
            ),
            Self::StateMaskSizeMismatch {
                q_index,
                expected,
                actual,
            } => write!(
                f,
                "BSM state mask {q_index} contains {actual} states, expected {expected}"
            ),
            Self::EventContextMismatch {
                edge_index,
                segment_index,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} contains an event tagged for another branch, segment, or Q"
            ),
            Self::InvalidEventTime {
                edge_index,
                segment_index,
                time,
                previous_time,
                segment_end,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} event time {time} is outside ordered interval {previous_time}..{segment_end}"
            ),
            Self::EventStateMismatch {
                edge_index,
                segment_index,
                expected,
                actual,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} event starts in state {actual}, expected {expected}"
            ),
            Self::SegmentEndStateMismatch {
                edge_index,
                segment_index,
                expected,
                actual,
            } => write!(
                f,
                "edge {edge_index} segment {segment_index} ends in state {actual}, expected {expected}"
            ),
            Self::BranchEndStateMismatch {
                edge_index,
                expected,
                actual,
            } => write!(
                f,
                "edge {edge_index} ends in state {actual}, expected {expected}"
            ),
            Self::UnsupportedCladogeneticSplit {
                node,
                ancestor,
                left,
                right,
            } => write!(
                f,
                "node {node} split {ancestor}->{left}+{right} is not y, s, v, or j"
            ),
        }
    }
}

impl Error for BsmSummaryError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsm::{AnageneticEventSample, BranchHistory, BranchSegmentHistory, HistorySkeleton};
    use crate::state::AreaSet;

    #[test]
    fn classifies_all_cladogenetic_event_families() {
        let states = StateSpace::new(3, 3, true).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b001)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b010)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b011)).unwrap();

        for (left, right, expected) in [
            (ab, ab, CladogeneticEventKind::RangeCopying),
            (ab, a, CladogeneticEventKind::SubsetSympatry),
            (a, b, CladogeneticEventKind::Vicariance),
            (a, b, CladogeneticEventKind::FounderEvent),
        ] {
            let ancestor = if expected == CladogeneticEventKind::FounderEvent {
                a
            } else {
                ab
            };
            assert_eq!(
                classify_cladogenetic_event(
                    &states,
                    CladogeneticSplitSample {
                        node: 0,
                        ancestor,
                        left,
                        right,
                        weight: 1.0,
                    }
                )
                .unwrap(),
                expected
            );
        }
    }

    #[test]
    fn summarizes_event_counts_periods_and_state_occupancy() {
        let states = StateSpace::new(1, 1, true).unwrap();
        let null = states.index_of(AreaSet::from_bits(0)).unwrap();
        let a = states.index_of(AreaSet::from_bits(1)).unwrap();
        let event = AnageneticEventSample {
            edge_index: 0,
            segment_index: 0,
            q_index: 1,
            time_from_parent: 0.4,
            from_state: a,
            to_state: null,
            kind: AnageneticEventKind::LocalExtirpation { area: 0 },
        };
        let map = BiogeographicStochasticMap {
            skeleton: HistorySkeleton {
                root_state: a,
                node_states: vec![null, a],
                branch_endpoints: Vec::new(),
                splits: Vec::new(),
            },
            branches: vec![BranchHistory {
                edge_index: 0,
                parent: 1,
                child: 0,
                start_state: a,
                end_state: null,
                segments: vec![BranchSegmentHistory {
                    segment_index: 0,
                    q_index: 1,
                    start_time_from_parent: 0.0,
                    end_time_from_parent: 1.0,
                    start_state: a,
                    end_state: null,
                    endpoint_probability: 1.0,
                    virtual_jump_count: 1,
                    events: vec![event],
                }],
            }],
        };

        let summary = map.summarize(&states).unwrap();
        assert_eq!(summary.anagenetic_event_count, 1);
        assert_eq!(summary.range_expansion_count, 0);
        assert_eq!(summary.local_extirpation_count, 1);
        assert_eq!(summary.range_switching_count, 0);
        assert_eq!(summary.event_counts_by_q, vec![0, 1]);
        assert!((summary.occupancy_time_by_state[a] - 0.4).abs() < 1e-12);
        assert!((summary.occupancy_time_by_state[null] - 0.6).abs() < 1e-12);
        assert!((summary.occupancy_time_by_q_and_state[1][a] - 0.4).abs() < 1e-12);
        assert!((summary.total_branch_time - 1.0).abs() < 1e-12);
        assert_eq!(summary.diagnostics.segment_count, 1);
        assert_eq!(summary.diagnostics.constrained_segment_count, 0);
        assert_eq!(summary.diagnostics.minimum_endpoint_probability, Some(1.0));
        assert_eq!(summary.diagnostics.maximum_virtual_jump_count, 1);
        assert_eq!(summary.diagnostics.maximum_anagenetic_events_per_segment, 1);
        assert!(!summary.diagnostics.has_state_constraint_violations());

        let masks = vec![
            StateMask::all(states.len()).unwrap(),
            StateMask::new(vec![false, true]).unwrap(),
        ];
        let constrained = map
            .summarize_with_state_masks(&states, Some(&masks))
            .unwrap();
        assert_eq!(constrained.diagnostics.constrained_segment_count, 1);
        assert_eq!(constrained.diagnostics.forbidden_state_transitions, 1);
        assert_eq!(constrained.diagnostics.forbidden_state_endpoints, 1);
        assert!((constrained.diagnostics.forbidden_state_time - 0.6).abs() < 1e-12);
        assert_eq!(
            constrained.diagnostics.state_constraint_violation_count(),
            2
        );
        assert!(constrained.diagnostics.has_state_constraint_violations());
    }

    #[test]
    fn summarizes_range_switching_as_one_atomic_anagenetic_event() {
        let states = StateSpace::new(2, 1, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let event = AnageneticEventSample {
            edge_index: 0,
            segment_index: 0,
            q_index: 0,
            time_from_parent: 0.25,
            from_state: a,
            to_state: b,
            kind: AnageneticEventKind::RangeSwitching {
                from_area: 0,
                to_area: 1,
            },
        };
        let map = BiogeographicStochasticMap {
            skeleton: HistorySkeleton {
                root_state: a,
                node_states: vec![b, a],
                branch_endpoints: Vec::new(),
                splits: Vec::new(),
            },
            branches: vec![BranchHistory {
                edge_index: 0,
                parent: 1,
                child: 0,
                start_state: a,
                end_state: b,
                segments: vec![BranchSegmentHistory {
                    segment_index: 0,
                    q_index: 0,
                    start_time_from_parent: 0.0,
                    end_time_from_parent: 1.0,
                    start_state: a,
                    end_state: b,
                    endpoint_probability: 1.0,
                    virtual_jump_count: 1,
                    events: vec![event],
                }],
            }],
        };

        let summary = map.summarize(&states).unwrap();
        assert_eq!(summary.anagenetic_event_count, 1);
        assert_eq!(summary.range_expansion_count, 0);
        assert_eq!(summary.local_extirpation_count, 0);
        assert_eq!(summary.range_switching_count, 1);
        assert_eq!(summary.event_counts_by_q, vec![1]);
        assert!((summary.occupancy_time_by_state[a] - 0.25).abs() < 1e-12);
        assert!((summary.occupancy_time_by_state[b] - 0.75).abs() < 1e-12);
    }

    #[test]
    fn rejects_invalid_diagnostic_inputs() {
        let states = StateSpace::new(1, 1, true).unwrap();
        let a = states.index_of(AreaSet::from_bits(1)).unwrap();
        let map = BiogeographicStochasticMap {
            skeleton: HistorySkeleton {
                root_state: a,
                node_states: vec![a, a],
                branch_endpoints: Vec::new(),
                splits: Vec::new(),
            },
            branches: vec![BranchHistory {
                edge_index: 0,
                parent: 1,
                child: 0,
                start_state: a,
                end_state: a,
                segments: vec![BranchSegmentHistory {
                    segment_index: 0,
                    q_index: 0,
                    start_time_from_parent: 0.0,
                    end_time_from_parent: 1.0,
                    start_state: a,
                    end_state: a,
                    endpoint_probability: 0.0,
                    virtual_jump_count: 0,
                    events: Vec::new(),
                }],
            }],
        };
        assert!(matches!(
            map.summarize(&states),
            Err(BsmSummaryError::InvalidEndpointProbability { .. })
        ));

        let mut valid = map;
        valid.branches[0].segments[0].endpoint_probability = 1.0;
        assert!(matches!(
            valid.summarize_with_state_masks(&states, Some(&[])),
            Err(BsmSummaryError::StateMaskCountMismatch { .. })
        ));
        let wrong_size = [StateMask::all(states.len() + 1).unwrap()];
        assert!(matches!(
            valid.summarize_with_state_masks(&states, Some(&wrong_size)),
            Err(BsmSummaryError::StateMaskSizeMismatch { .. })
        ));

        valid.skeleton.node_states[0] = states.len();
        assert!(matches!(
            valid.summarize(&states),
            Err(BsmSummaryError::StateOutOfBounds {
                context: "skeleton node",
                ..
            })
        ));
    }
}
