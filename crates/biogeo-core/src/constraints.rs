use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use crate::dispersal::{DispersalMatrixParseError, parse_dispersal_multipliers_table};
use crate::state::{AreaSet, StateSpace};

#[derive(Clone, Debug, PartialEq)]
pub struct BinaryAreaMatrix {
    num_areas: usize,
    values: Vec<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowedRangeStates {
    num_areas: usize,
    states: BTreeSet<AreaSet>,
}

impl AllowedRangeStates {
    pub fn new(
        num_areas: usize,
        states: impl IntoIterator<Item = AreaSet>,
    ) -> Result<Self, StateConstraintError> {
        if num_areas == 0 {
            return Err(StateConstraintError::ZeroAreas);
        }
        if num_areas > 64 {
            return Err(StateConstraintError::TooManyAreas { num_areas });
        }
        let states = states.into_iter().collect::<BTreeSet<_>>();
        if states.is_empty() {
            return Err(StateConstraintError::EmptyAllowedRanges);
        }
        let valid_bits = if num_areas == 64 {
            u64::MAX
        } else {
            (1_u64 << num_areas) - 1
        };
        if let Some(state) = states.iter().find(|state| state.bits() & !valid_bits != 0) {
            return Err(StateConstraintError::AllowedRangeOutsideAreas {
                num_areas,
                bits: state.bits(),
            });
        }
        Ok(Self { num_areas, states })
    }

    pub fn num_areas(&self) -> usize {
        self.num_areas
    }

    pub fn contains(&self, state: AreaSet) -> bool {
        self.states.contains(&state)
    }

    pub fn states(&self) -> impl ExactSizeIterator<Item = AreaSet> + '_ {
        self.states.iter().copied()
    }
}

impl BinaryAreaMatrix {
    pub fn new(num_areas: usize, values: Vec<bool>) -> Result<Self, StateConstraintError> {
        if num_areas == 0 {
            return Err(StateConstraintError::ZeroAreas);
        }
        let expected = num_areas
            .checked_mul(num_areas)
            .ok_or(StateConstraintError::DimensionOverflow { num_areas })?;
        if values.len() != expected {
            return Err(StateConstraintError::ValueCountMismatch {
                num_areas,
                expected,
                actual: values.len(),
            });
        }
        Ok(Self { num_areas, values })
    }

    pub fn num_areas(&self) -> usize {
        self.num_areas
    }

    pub fn get(&self, from: usize, to: usize) -> bool {
        self.values[from * self.num_areas + to]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RangeStateConstraint {
    areas_allowed: Option<BinaryAreaMatrix>,
    areas_adjacency: Option<BinaryAreaMatrix>,
    allowed_ranges: Option<AllowedRangeStates>,
}

impl RangeStateConstraint {
    pub fn new(
        areas_allowed: Option<BinaryAreaMatrix>,
        areas_adjacency: Option<BinaryAreaMatrix>,
    ) -> Result<Self, StateConstraintError> {
        if let (Some(allowed), Some(adjacency)) = (&areas_allowed, &areas_adjacency)
            && allowed.num_areas() != adjacency.num_areas()
        {
            return Err(StateConstraintError::AreaCountMismatch {
                expected: allowed.num_areas(),
                actual: adjacency.num_areas(),
            });
        }
        Ok(Self {
            areas_allowed,
            areas_adjacency,
            allowed_ranges: None,
        })
    }

    pub fn with_allowed_ranges(
        mut self,
        allowed_ranges: AllowedRangeStates,
    ) -> Result<Self, StateConstraintError> {
        if let Some(expected) = self.num_areas()
            && expected != allowed_ranges.num_areas()
        {
            return Err(StateConstraintError::AreaCountMismatch {
                expected,
                actual: allowed_ranges.num_areas(),
            });
        }
        self.allowed_ranges = Some(allowed_ranges);
        Ok(self)
    }

    pub fn num_areas(&self) -> Option<usize> {
        self.areas_allowed
            .as_ref()
            .map(BinaryAreaMatrix::num_areas)
            .or_else(|| {
                self.areas_adjacency
                    .as_ref()
                    .map(BinaryAreaMatrix::num_areas)
            })
            .or_else(|| {
                self.allowed_ranges
                    .as_ref()
                    .map(AllowedRangeStates::num_areas)
            })
    }

    pub fn areas_allowed(&self) -> Option<&BinaryAreaMatrix> {
        self.areas_allowed.as_ref()
    }

    pub fn areas_adjacency(&self) -> Option<&BinaryAreaMatrix> {
        self.areas_adjacency.as_ref()
    }

    pub fn allowed_ranges(&self) -> Option<&AllowedRangeStates> {
        self.allowed_ranges.as_ref()
    }

    pub fn allows(&self, state: AreaSet) -> bool {
        if let Some(allowed_ranges) = &self.allowed_ranges
            && !allowed_ranges.contains(state)
        {
            return false;
        }

        if state.is_empty() {
            return true;
        }

        if let Some(matrix) = &self.areas_allowed {
            let first_area = (0..matrix.num_areas())
                .find(|area| state.contains(*area as u8))
                .expect("non-empty range must contain an area");
            if !(0..matrix.num_areas())
                .filter(|area| state.contains(*area as u8))
                .all(|area| matrix.get(first_area, area))
            {
                return false;
            }
        }

        if let Some(matrix) = &self.areas_adjacency {
            let areas: Vec<usize> = (0..matrix.num_areas())
                .filter(|area| state.contains(*area as u8))
                .collect();
            if !areas
                .iter()
                .all(|from| areas.iter().all(|to| matrix.get(*from, *to)))
            {
                return false;
            }
        }

        true
    }

    pub fn state_mask(&self, states: &StateSpace) -> Result<StateMask, StateConstraintError> {
        if let Some(num_areas) = self.num_areas()
            && num_areas != usize::from(states.num_areas())
        {
            return Err(StateConstraintError::StateSpaceAreaCountMismatch {
                constraint_areas: num_areas,
                state_space_areas: usize::from(states.num_areas()),
            });
        }
        StateMask::new(
            states
                .states()
                .iter()
                .map(|state| self.allows(*state))
                .collect(),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateMask {
    allowed: Vec<bool>,
}

impl StateMask {
    pub fn new(allowed: Vec<bool>) -> Result<Self, StateConstraintError> {
        if allowed.is_empty() {
            return Err(StateConstraintError::EmptyStateMask);
        }
        if !allowed.iter().any(|value| *value) {
            return Err(StateConstraintError::NoAllowedStates);
        }
        Ok(Self { allowed })
    }

    pub fn all(state_count: usize) -> Result<Self, StateConstraintError> {
        Self::new(vec![true; state_count])
    }

    pub fn len(&self) -> usize {
        self.allowed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.allowed.is_empty()
    }

    pub fn is_allowed(&self, state: usize) -> bool {
        self.allowed[state]
    }

    pub fn values(&self) -> &[bool] {
        &self.allowed
    }

    pub fn allowed_count(&self) -> usize {
        self.allowed.iter().filter(|value| **value).count()
    }

    pub fn project(&self, values: &mut [f64]) -> Result<(), StateConstraintError> {
        if values.len() != self.len() {
            return Err(StateConstraintError::VectorLengthMismatch {
                expected: self.len(),
                actual: values.len(),
            });
        }
        for (value, allowed) in values.iter_mut().zip(&self.allowed) {
            if !allowed {
                *value = 0.0;
            }
        }
        Ok(())
    }
}

pub fn parse_binary_area_matrix(
    input: &str,
    area_names: &[String],
) -> Result<BinaryAreaMatrix, StateConstraintParseError> {
    let parsed = parse_dispersal_multipliers_table(input, area_names)
        .map_err(StateConstraintParseError::MatrixTable)?;
    let mut values = Vec::with_capacity(parsed.values().len());
    for (index, value) in parsed.values().iter().copied().enumerate() {
        if value != 0.0 && value != 1.0 {
            return Err(StateConstraintParseError::NonBinaryValue {
                from: index / parsed.num_areas(),
                to: index % parsed.num_areas(),
                value,
            });
        }
        values.push(value == 1.0);
    }
    BinaryAreaMatrix::new(parsed.num_areas(), values).map_err(StateConstraintParseError::Constraint)
}

pub fn parse_allowed_range_states(
    input: &str,
    area_names: &[String],
) -> Result<AllowedRangeStates, StateConstraintParseError> {
    if area_names.is_empty() {
        return Err(StateConstraintParseError::Constraint(
            StateConstraintError::ZeroAreas,
        ));
    }
    if area_names.len() > 64 {
        return Err(StateConstraintParseError::Constraint(
            StateConstraintError::TooManyAreas {
                num_areas: area_names.len(),
            },
        ));
    }
    let mut lines = input
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                None
            } else {
                Some((index + 1, trimmed))
            }
        });
    let (header_line, header) = lines
        .next()
        .ok_or(StateConstraintParseError::EmptyAllowedRangesInput)?;
    let header_fields = header.split_whitespace().collect::<Vec<_>>();
    let expected_columns = area_names.len() + 1;
    if header_fields.len() != expected_columns
        || header_fields.first().copied() != Some("range")
        || header_fields[1..]
            .iter()
            .zip(area_names)
            .any(|(actual, expected)| *actual != expected)
    {
        return Err(StateConstraintParseError::InvalidAllowedRangesHeader {
            line: header_line,
            expected: format!("range {}", area_names.join(" ")),
            actual: header_fields.join(" "),
        });
    }

    let mut states = Vec::new();
    let mut seen = BTreeSet::new();
    for (line_number, line) in lines {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != expected_columns {
            return Err(StateConstraintParseError::AllowedRangesColumnCount {
                line: line_number,
                expected: expected_columns,
                actual: fields.len(),
            });
        }
        let mut bits = 0_u64;
        let mut selected = Vec::new();
        for (area_index, value) in fields[1..].iter().enumerate() {
            match *value {
                "0" => {}
                "1" => {
                    bits |= 1_u64 << area_index;
                    selected.push(area_names[area_index].as_str());
                }
                _ => {
                    return Err(StateConstraintParseError::InvalidAllowedRangeValue {
                        line: line_number,
                        column: area_index + 2,
                        value: (*value).to_string(),
                    });
                }
            }
        }
        let expected_label = if selected.is_empty() {
            "_".to_string()
        } else {
            selected.join("+")
        };
        if fields[0] != expected_label {
            return Err(StateConstraintParseError::AllowedRangeLabelMismatch {
                line: line_number,
                expected: expected_label,
                actual: fields[0].to_string(),
            });
        }
        if !seen.insert(bits) {
            return Err(StateConstraintParseError::DuplicateAllowedRange {
                line: line_number,
                label: fields[0].to_string(),
            });
        }
        states.push(AreaSet::from_bits(bits));
    }
    AllowedRangeStates::new(area_names.len(), states).map_err(StateConstraintParseError::Constraint)
}

#[derive(Clone, Debug, PartialEq)]
pub enum StateConstraintError {
    ZeroAreas,
    TooManyAreas {
        num_areas: usize,
    },
    DimensionOverflow {
        num_areas: usize,
    },
    ValueCountMismatch {
        num_areas: usize,
        expected: usize,
        actual: usize,
    },
    AreaCountMismatch {
        expected: usize,
        actual: usize,
    },
    EmptyAllowedRanges,
    AllowedRangeOutsideAreas {
        num_areas: usize,
        bits: u64,
    },
    StateSpaceAreaCountMismatch {
        constraint_areas: usize,
        state_space_areas: usize,
    },
    EmptyStateMask,
    NoAllowedStates,
    VectorLengthMismatch {
        expected: usize,
        actual: usize,
    },
}

impl fmt::Display for StateConstraintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAreas => write!(f, "state constraint matrices require at least one area"),
            Self::TooManyAreas { num_areas } => write!(
                f,
                "state constraints support at most 64 areas, got {num_areas}"
            ),
            Self::DimensionOverflow { num_areas } => {
                write!(
                    f,
                    "state constraint dimensions overflow for {num_areas} areas"
                )
            }
            Self::ValueCountMismatch {
                num_areas,
                expected,
                actual,
            } => write!(
                f,
                "a {num_areas}x{num_areas} state constraint matrix requires {expected} values, got {actual}"
            ),
            Self::AreaCountMismatch { expected, actual } => write!(
                f,
                "state constraint area count mismatch: expected {expected}, got {actual}"
            ),
            Self::EmptyAllowedRanges => {
                write!(f, "an explicit allowed-range list cannot be empty")
            }
            Self::AllowedRangeOutsideAreas { num_areas, bits } => write!(
                f,
                "allowed range bitset {bits:#x} uses an area outside the configured {num_areas} areas"
            ),
            Self::StateSpaceAreaCountMismatch {
                constraint_areas,
                state_space_areas,
            } => write!(
                f,
                "state constraint has {constraint_areas} areas but state space has {state_space_areas}"
            ),
            Self::EmptyStateMask => write!(f, "state masks cannot be empty"),
            Self::NoAllowedStates => write!(f, "state constraints disallow every state"),
            Self::VectorLengthMismatch { expected, actual } => write!(
                f,
                "state mask expected a vector of length {expected}, got {actual}"
            ),
        }
    }
}

impl Error for StateConstraintError {}

#[derive(Clone, Debug, PartialEq)]
pub enum StateConstraintParseError {
    MatrixTable(DispersalMatrixParseError),
    NonBinaryValue {
        from: usize,
        to: usize,
        value: f64,
    },
    EmptyAllowedRangesInput,
    InvalidAllowedRangesHeader {
        line: usize,
        expected: String,
        actual: String,
    },
    AllowedRangesColumnCount {
        line: usize,
        expected: usize,
        actual: usize,
    },
    InvalidAllowedRangeValue {
        line: usize,
        column: usize,
        value: String,
    },
    AllowedRangeLabelMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    DuplicateAllowedRange {
        line: usize,
        label: String,
    },
    Constraint(StateConstraintError),
}

impl fmt::Display for StateConstraintParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MatrixTable(error) => write!(f, "invalid state constraint matrix: {error}"),
            Self::NonBinaryValue { from, to, value } => write!(
                f,
                "state constraint matrix value at row {from}, column {to} must be 0 or 1, got {value}"
            ),
            Self::EmptyAllowedRangesInput => write!(f, "allowed-range table is empty"),
            Self::InvalidAllowedRangesHeader {
                line,
                expected,
                actual,
            } => write!(
                f,
                "invalid allowed-range header on line {line}: expected {expected:?}, got {actual:?}"
            ),
            Self::AllowedRangesColumnCount {
                line,
                expected,
                actual,
            } => write!(
                f,
                "allowed-range row on line {line} has {actual} columns, expected {expected}"
            ),
            Self::InvalidAllowedRangeValue {
                line,
                column,
                value,
            } => write!(
                f,
                "allowed-range value on line {line}, column {column} must be 0 or 1, got {value:?}"
            ),
            Self::AllowedRangeLabelMismatch {
                line,
                expected,
                actual,
            } => write!(
                f,
                "allowed-range label on line {line} does not match its bit vector: expected {expected:?}, got {actual:?}"
            ),
            Self::DuplicateAllowedRange { line, label } => {
                write!(f, "allowed range {label:?} is duplicated on line {line}")
            }
            Self::Constraint(error) => write!(f, "invalid state constraint: {error}"),
        }
    }
}

impl Error for StateConstraintParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::MatrixTable(error) => Some(error),
            Self::Constraint(error) => Some(error),
            Self::NonBinaryValue { .. }
            | Self::EmptyAllowedRangesInput
            | Self::InvalidAllowedRangesHeader { .. }
            | Self::AllowedRangesColumnCount { .. }
            | Self::InvalidAllowedRangeValue { .. }
            | Self::AllowedRangeLabelMismatch { .. }
            | Self::DuplicateAllowedRange { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn areas_allowed_matches_biogeobears_first_area_row_semantics() {
        let matrix = BinaryAreaMatrix::new(
            3,
            vec![true, true, false, true, true, false, false, false, false],
        )
        .unwrap();
        let constraint = RangeStateConstraint::new(Some(matrix), None).unwrap();

        assert!(constraint.allows(AreaSet::from_bits(0b001)));
        assert!(constraint.allows(AreaSet::from_bits(0b011)));
        assert!(!constraint.allows(AreaSet::from_bits(0b100)));
        assert!(!constraint.allows(AreaSet::from_bits(0b101)));
    }

    #[test]
    fn adjacency_requires_every_pair_within_a_range() {
        let matrix = BinaryAreaMatrix::new(
            3,
            vec![true, true, false, true, true, true, false, true, true],
        )
        .unwrap();
        let constraint = RangeStateConstraint::new(None, Some(matrix)).unwrap();

        assert!(constraint.allows(AreaSet::from_bits(0b011)));
        assert!(constraint.allows(AreaSet::from_bits(0b110)));
        assert!(!constraint.allows(AreaSet::from_bits(0b101)));
        assert!(!constraint.allows(AreaSet::from_bits(0b111)));
    }

    #[test]
    fn parses_named_binary_matrix_and_rejects_weights() {
        let areas = vec!["A".to_string(), "B".to_string()];
        let parsed = parse_binary_area_matrix("from A B\nA 1 0\nB 0 1\n", &areas).unwrap();
        assert!(parsed.get(0, 0));
        assert!(!parsed.get(0, 1));

        let error = parse_binary_area_matrix("from A B\nA 1 0.5\nB 0 1\n", &areas).unwrap_err();
        assert_eq!(
            error,
            StateConstraintParseError::NonBinaryValue {
                from: 0,
                to: 1,
                value: 0.5,
            }
        );
    }

    #[test]
    fn explicit_allowed_ranges_are_distinct_from_pairwise_adjacency() {
        let allowed = AllowedRangeStates::new(
            3,
            [
                AreaSet::EMPTY,
                AreaSet::from_bits(0b001),
                AreaSet::from_bits(0b010),
                AreaSet::from_bits(0b100),
                AreaSet::from_bits(0b011),
                AreaSet::from_bits(0b110),
                AreaSet::from_bits(0b111),
            ],
        )
        .unwrap();
        let constraint = RangeStateConstraint::new(None, None)
            .unwrap()
            .with_allowed_ranges(allowed)
            .unwrap();

        assert!(constraint.allows(AreaSet::from_bits(0b111)));
        assert!(!constraint.allows(AreaSet::from_bits(0b101)));
    }

    #[test]
    fn parses_allowed_ranges_with_auditable_labels() {
        let areas = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let parsed = parse_allowed_range_states(
            "\u{feff}# format biogeo-allowed-ranges-v1\nrange A B C\n_ 0 0 0\nA 1 0 0\nA+B+C 1 1 1\n",
            &areas,
        )
        .unwrap();
        assert!(parsed.contains(AreaSet::EMPTY));
        assert!(parsed.contains(AreaSet::from_bits(0b111)));
        assert!(!parsed.contains(AreaSet::from_bits(0b101)));

        let error = parse_allowed_range_states("range A B C\nA+C 1 1 0\n", &areas).unwrap_err();
        assert!(matches!(
            error,
            StateConstraintParseError::AllowedRangeLabelMismatch { .. }
        ));
    }
}
