use std::collections::HashMap;
use std::error::Error;
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct AreaSet {
    bits: u64,
}

impl AreaSet {
    pub const EMPTY: Self = Self { bits: 0 };

    pub fn from_bits(bits: u64) -> Self {
        Self { bits }
    }

    pub fn singleton(area_index: u8) -> Result<Self, StateSpaceError> {
        if area_index >= 64 {
            return Err(StateSpaceError::TooManyAreas {
                num_areas: area_index + 1,
            });
        }

        Ok(Self {
            bits: 1_u64 << area_index,
        })
    }

    pub fn bits(self) -> u64 {
        self.bits
    }

    pub fn is_empty(self) -> bool {
        self.bits == 0
    }

    pub fn size(self) -> u8 {
        self.bits.count_ones() as u8
    }

    pub fn contains(self, area_index: u8) -> bool {
        area_index < 64 && (self.bits & (1_u64 << area_index)) != 0
    }

    pub fn with_area(self, area_index: u8) -> Option<Self> {
        if area_index >= 64 {
            return None;
        }

        Some(Self {
            bits: self.bits | (1_u64 << area_index),
        })
    }

    pub fn without_area(self, area_index: u8) -> Option<Self> {
        if area_index >= 64 {
            return None;
        }

        Some(Self {
            bits: self.bits & !(1_u64 << area_index),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateSpace {
    num_areas: u8,
    max_range_size: u8,
    include_null_range: bool,
    states: Vec<AreaSet>,
    index_by_bits: HashMap<u64, usize>,
}

impl StateSpace {
    pub fn estimated_state_count(
        num_areas: u8,
        max_range_size: u8,
        include_null_range: bool,
    ) -> Result<usize, StateSpaceError> {
        validate_dimensions(num_areas, max_range_size)?;
        let mut total = u128::from(include_null_range);
        for range_size in 1..=max_range_size {
            total = total
                .checked_add(binomial_coefficient(num_areas, range_size))
                .ok_or(StateSpaceError::StateCountOverflow {
                    num_areas,
                    max_range_size,
                    include_null_range,
                })?;
        }
        usize::try_from(total).map_err(|_| StateSpaceError::StateCountOverflow {
            num_areas,
            max_range_size,
            include_null_range,
        })
    }

    pub fn new(
        num_areas: u8,
        max_range_size: u8,
        include_null_range: bool,
    ) -> Result<Self, StateSpaceError> {
        let state_count =
            Self::estimated_state_count(num_areas, max_range_size, include_null_range)?;

        let mut states = Vec::new();
        states
            .try_reserve_exact(state_count)
            .map_err(|_| StateSpaceError::AllocationFailed { state_count })?;
        if include_null_range {
            states.push(AreaSet::EMPTY);
        }

        for range_size in 1..=max_range_size {
            generate_combinations(num_areas, range_size, &mut states);
        }

        let mut index_by_bits = HashMap::new();
        index_by_bits
            .try_reserve(state_count)
            .map_err(|_| StateSpaceError::AllocationFailed { state_count })?;
        index_by_bits.extend(
            states
                .iter()
                .enumerate()
                .map(|(index, state)| (state.bits(), index)),
        );

        Ok(Self {
            num_areas,
            max_range_size,
            include_null_range,
            states,
            index_by_bits,
        })
    }

    pub fn num_areas(&self) -> u8 {
        self.num_areas
    }

    pub fn max_range_size(&self) -> u8 {
        self.max_range_size
    }

    pub fn include_null_range(&self) -> bool {
        self.include_null_range
    }

    pub fn len(&self) -> usize {
        self.states.len()
    }

    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    pub fn states(&self) -> &[AreaSet] {
        &self.states
    }

    pub fn get(&self, index: usize) -> Option<AreaSet> {
        self.states.get(index).copied()
    }

    pub fn index_of(&self, state: AreaSet) -> Option<usize> {
        self.index_by_bits.get(&state.bits()).copied()
    }
}

fn validate_dimensions(num_areas: u8, max_range_size: u8) -> Result<(), StateSpaceError> {
    if num_areas == 0 {
        return Err(StateSpaceError::ZeroAreas);
    }
    if num_areas > 64 {
        return Err(StateSpaceError::TooManyAreas { num_areas });
    }
    if max_range_size == 0 {
        return Err(StateSpaceError::ZeroMaxRangeSize);
    }
    if max_range_size > num_areas {
        return Err(StateSpaceError::MaxRangeSizeExceedsAreas {
            max_range_size,
            num_areas,
        });
    }
    Ok(())
}

fn binomial_coefficient(n: u8, k: u8) -> u128 {
    let k = k.min(n - k);
    (0..k).fold(1_u128, |value, index| {
        value * u128::from(n - index) / u128::from(index + 1)
    })
}

fn generate_combinations(num_areas: u8, range_size: u8, states: &mut Vec<AreaSet>) {
    fn visit(start: u8, remaining: u8, num_areas: u8, bits: u64, states: &mut Vec<AreaSet>) {
        if remaining == 0 {
            states.push(AreaSet::from_bits(bits));
            return;
        }

        let max_start = num_areas - remaining;
        for area in start..=max_start {
            visit(
                area + 1,
                remaining - 1,
                num_areas,
                bits | (1_u64 << area),
                states,
            );
        }
    }

    visit(0, range_size, num_areas, 0, states);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StateSpaceError {
    ZeroAreas,
    TooManyAreas {
        num_areas: u8,
    },
    ZeroMaxRangeSize,
    MaxRangeSizeExceedsAreas {
        max_range_size: u8,
        num_areas: u8,
    },
    StateCountOverflow {
        num_areas: u8,
        max_range_size: u8,
        include_null_range: bool,
    },
    AllocationFailed {
        state_count: usize,
    },
}

impl fmt::Display for StateSpaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAreas => write!(f, "state spaces require at least one area"),
            Self::TooManyAreas { num_areas } => {
                write!(
                    f,
                    "bitset state spaces support at most 64 areas, got {num_areas}"
                )
            }
            Self::ZeroMaxRangeSize => write!(f, "max_range_size must be greater than zero"),
            Self::MaxRangeSizeExceedsAreas {
                max_range_size,
                num_areas,
            } => write!(
                f,
                "max_range_size ({max_range_size}) cannot exceed num_areas ({num_areas})"
            ),
            Self::StateCountOverflow {
                num_areas,
                max_range_size,
                include_null_range,
            } => write!(
                f,
                "state count does not fit this platform for num_areas={num_areas}, max_range_size={max_range_size}, include_null_range={include_null_range}"
            ),
            Self::AllocationFailed { state_count } => {
                write!(f, "could not reserve memory for {state_count} range states")
            }
        }
    }
}

impl Error for StateSpaceError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_two_area_states_without_null_by_size() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let bits: Vec<u64> = states.states().iter().map(|state| state.bits()).collect();

        assert_eq!(bits, vec![0b01, 0b10, 0b11]);
        assert_eq!(states.index_of(AreaSet::from_bits(0b01)), Some(0));
        assert_eq!(states.index_of(AreaSet::from_bits(0b10)), Some(1));
        assert_eq!(states.index_of(AreaSet::from_bits(0b11)), Some(2));
    }

    #[test]
    fn includes_null_range_first_when_requested() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let bits: Vec<u64> = states.states().iter().map(|state| state.bits()).collect();

        assert_eq!(bits, vec![0b00, 0b01, 0b10, 0b11]);
        assert_eq!(states.index_of(AreaSet::EMPTY), Some(0));
    }

    #[test]
    fn max_range_size_filters_large_ranges() {
        let states = StateSpace::new(3, 2, false).unwrap();
        let bits: Vec<u64> = states.states().iter().map(|state| state.bits()).collect();

        assert_eq!(bits, vec![0b001, 0b010, 0b100, 0b011, 0b101, 0b110]);
        assert_eq!(states.index_of(AreaSet::from_bits(0b111)), None);
    }

    #[test]
    fn rejects_invalid_dimensions() {
        assert_eq!(
            StateSpace::new(0, 1, false),
            Err(StateSpaceError::ZeroAreas)
        );
        assert_eq!(
            StateSpace::new(2, 3, false),
            Err(StateSpaceError::MaxRangeSizeExceedsAreas {
                max_range_size: 3,
                num_areas: 2
            })
        );
    }

    #[test]
    fn estimates_combinatorial_state_counts_without_building_states() {
        assert_eq!(StateSpace::estimated_state_count(5, 5, true), Ok(32));
        assert_eq!(StateSpace::estimated_state_count(10, 5, true), Ok(638));
        assert_eq!(StateSpace::estimated_state_count(20, 5, true), Ok(21_700));
        assert_eq!(StateSpace::estimated_state_count(30, 5, true), Ok(174_437));
        assert_eq!(
            StateSpace::new(10, 5, true).unwrap().len(),
            StateSpace::estimated_state_count(10, 5, true).unwrap()
        );
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn reports_state_counts_that_do_not_fit_usize() {
        assert_eq!(
            StateSpace::estimated_state_count(64, 64, true),
            Err(StateSpaceError::StateCountOverflow {
                num_areas: 64,
                max_range_size: 64,
                include_null_range: true,
            })
        );
    }
}
