use std::error::Error;
use std::fmt;

use crate::branch_process::{BranchSegment, OwnedBranchPropagator};
use crate::cladogenesis::{
    CladogenesisConfig, CladogenesisError, CladogenesisRangeSizeConfig, CladogeneticTable,
};
use crate::constraints::{AllowedRangeStates, BinaryAreaMatrix, StateConstraintError, StateMask};
use crate::dispersal::{
    AreaSizeError, DispersalMatrixError, DispersalMultiplierMatrix, DispersalScheduleError,
    ExtirpationMultiplierError, ExtirpationMultiplierVector, TimeStratifiedAnagenesis,
    TimeStratifiedDispersal,
};
use crate::parameters::{ParameterError, ResolvedParameters};
use crate::q::{RateTransition, SparseQ};
use crate::state::StateSpace;
use crate::tree::Tree;

pub const MODEL_IDENTITY_FORMAT_VERSION: &str = "biogeo-model-identity-v1";

#[derive(Clone, Debug, PartialEq)]
pub struct ModelConfig {
    pub anagenesis: DecAnageneticModel,
    pub cladogenesis: CladogenesisConfig,
}

impl ModelConfig {
    pub fn preset_dec(d: f64, e: f64) -> Result<Self, DecModelError> {
        Ok(Self {
            anagenesis: DecAnageneticModel::new(d, e)?,
            cladogenesis: CladogenesisConfig::preset_dec(),
        })
    }

    pub fn preset_dec_j(d: f64, e: f64, j: f64) -> Result<Self, DecModelError> {
        validate_founder_weight("DEC+J", j, 3.0)?;
        Ok(Self {
            anagenesis: DecAnageneticModel::new(d, e)?,
            cladogenesis: CladogenesisConfig::preset_dec_j(j),
        })
    }

    pub fn preset_divalike(d: f64, e: f64) -> Result<Self, DecModelError> {
        Ok(Self {
            anagenesis: DecAnageneticModel::new(d, e)?,
            cladogenesis: CladogenesisConfig::preset_divalike(),
        })
    }

    pub fn preset_divalike_j(d: f64, e: f64, j: f64) -> Result<Self, DecModelError> {
        validate_founder_weight("DIVALIKE+J", j, 2.0)?;
        Ok(Self {
            anagenesis: DecAnageneticModel::new(d, e)?,
            cladogenesis: CladogenesisConfig::preset_divalike_j(j),
        })
    }

    pub fn preset_bayarealike(d: f64, e: f64) -> Result<Self, DecModelError> {
        Ok(Self {
            anagenesis: DecAnageneticModel::new(d, e)?,
            cladogenesis: CladogenesisConfig::preset_bayarealike(),
        })
    }

    pub fn preset_bayarealike_j(d: f64, e: f64, j: f64) -> Result<Self, DecModelError> {
        validate_founder_weight("BAYAREALIKE+J", j, 1.0)?;
        Ok(Self {
            anagenesis: DecAnageneticModel::new(d, e)?,
            cladogenesis: CladogenesisConfig::preset_bayarealike_j(j),
        })
    }

    pub fn from_biogeobears_core_parameters(
        parameters: &ResolvedParameters,
    ) -> Result<Self, BioGeoBearsModelError> {
        let anagenesis =
            DecAnageneticModel::new(parameters.require("d")?, parameters.require("e")?)?
                .with_range_switching_rate(parameters.require("a")?)?
                .with_branch_length_exponent(parameters.require("b")?)?;
        let cladogenesis = CladogenesisConfig {
            event_weights: crate::cladogenesis::CladogeneticEventWeights {
                sympatry: parameters.require("y")?,
                subset_sympatry: parameters.require("s")?,
                vicariance: parameters.require("v")?,
                founder_event: parameters.require("j")?,
            },
            range_size: CladogenesisRangeSizeConfig {
                mx01y: parameters.require("mx01y")?,
                mx01s: parameters.require("mx01s")?,
                mx01v: parameters.require("mx01v")?,
                mx01j: parameters.require("mx01j")?,
            },
        };
        cladogenesis.validate()?;

        Ok(Self {
            anagenesis,
            cladogenesis,
        })
    }

    pub fn with_range_size_config(mut self, range_size: CladogenesisRangeSizeConfig) -> Self {
        self.cladogenesis.range_size = range_size;
        self
    }

    pub fn with_dispersal_multipliers(mut self, multipliers: DispersalMultiplierMatrix) -> Self {
        self.anagenesis = self.anagenesis.with_dispersal_multipliers(multipliers);
        self
    }

    pub fn with_time_stratified_dispersal(mut self, schedule: TimeStratifiedDispersal) -> Self {
        self.anagenesis = self.anagenesis.with_time_stratified_dispersal(schedule);
        self
    }

    pub fn with_time_stratified_anagenesis(mut self, schedule: TimeStratifiedAnagenesis) -> Self {
        self.anagenesis = self.anagenesis.with_time_stratified_anagenesis(schedule);
        self
    }

    pub fn with_extirpation_multipliers(
        mut self,
        multipliers: ExtirpationMultiplierVector,
    ) -> Self {
        self.anagenesis = self.anagenesis.with_extirpation_multipliers(multipliers);
        self
    }

    pub fn build_q(&self, states: &StateSpace) -> Result<SparseQ, AnagenesisError> {
        self.anagenesis.build_q(states)
    }

    pub(crate) fn build_branch_propagator(
        &self,
        tree: &Tree,
        states: &StateSpace,
    ) -> Result<OwnedBranchPropagator, AnagenesisError> {
        self.anagenesis.build_branch_propagator(tree, states)
    }

    pub fn build_cladogenetic_table(
        &self,
        states: &StateSpace,
    ) -> Result<CladogeneticTable, CladogenesisError> {
        self.cladogenesis
            .build_table_with_dispersal(states, self.anagenesis.dispersal_multipliers())
    }

    /// Returns a versioned, architecture-independent byte identity for every
    /// field that affects likelihood or stochastic-history sampling.
    pub fn stable_identity_v1(&self) -> Vec<u8> {
        let mut identity = Vec::new();
        identity.extend_from_slice(MODEL_IDENTITY_FORMAT_VERSION.as_bytes());
        identity.push(0);
        push_identity_f64(&mut identity, self.anagenesis.d);
        push_identity_f64(&mut identity, self.anagenesis.e);
        push_identity_f64(&mut identity, self.anagenesis.a);
        push_identity_f64(&mut identity, self.anagenesis.branch_length_exponent);
        push_identity_optional_matrix(&mut identity, self.anagenesis.dispersal_multipliers());
        match self.anagenesis.time_stratified_anagenesis() {
            Some(schedule) => {
                identity.push(1);
                push_identity_usize(&mut identity, schedule.strata().len());
                for stratum in schedule.strata() {
                    push_identity_f64(&mut identity, stratum.oldest_age);
                    push_identity_optional_matrix(
                        &mut identity,
                        stratum.dispersal_multipliers.as_ref(),
                    );
                    push_identity_optional_extirpation(
                        &mut identity,
                        stratum.extirpation_multipliers.as_ref(),
                    );
                    match &stratum.state_constraint {
                        Some(constraint) => {
                            identity.push(1);
                            push_identity_optional_binary_matrix(
                                &mut identity,
                                constraint.areas_allowed(),
                            );
                            push_identity_optional_binary_matrix(
                                &mut identity,
                                constraint.areas_adjacency(),
                            );
                        }
                        None => identity.push(0),
                    }
                }
            }
            None => identity.push(0),
        }
        push_identity_optional_extirpation(
            &mut identity,
            self.anagenesis.extirpation_multipliers(),
        );

        let weights = self.cladogenesis.event_weights;
        push_identity_f64(&mut identity, weights.sympatry);
        push_identity_f64(&mut identity, weights.subset_sympatry);
        push_identity_f64(&mut identity, weights.vicariance);
        push_identity_f64(&mut identity, weights.founder_event);
        let range_size = self.cladogenesis.range_size;
        push_identity_f64(&mut identity, range_size.mx01y);
        push_identity_f64(&mut identity, range_size.mx01s);
        push_identity_f64(&mut identity, range_size.mx01v);
        push_identity_f64(&mut identity, range_size.mx01j);
        if let Some(schedule) = self.anagenesis.time_stratified_anagenesis()
            && schedule.strata().iter().any(|stratum| {
                stratum
                    .state_constraint
                    .as_ref()
                    .and_then(|constraint| constraint.allowed_ranges())
                    .is_some()
            })
        {
            // This trailing extension preserves every pre-extension v1 identity byte-for-byte.
            identity.extend_from_slice(b"\0allowed-ranges-v1\0");
            push_identity_usize(&mut identity, schedule.strata().len());
            for stratum in schedule.strata() {
                push_identity_optional_allowed_ranges(
                    &mut identity,
                    stratum
                        .state_constraint
                        .as_ref()
                        .and_then(|constraint| constraint.allowed_ranges()),
                );
            }
        }
        identity
    }
}

fn push_identity_usize(identity: &mut Vec<u8>, value: usize) {
    identity.extend_from_slice(
        &u64::try_from(value)
            .expect("usize must fit the u64 identity format")
            .to_le_bytes(),
    );
}

fn push_identity_f64(identity: &mut Vec<u8>, value: f64) {
    identity.extend_from_slice(&value.to_bits().to_le_bytes());
}

fn push_identity_matrix(identity: &mut Vec<u8>, matrix: &DispersalMultiplierMatrix) {
    push_identity_usize(identity, matrix.num_areas());
    for value in matrix.values() {
        push_identity_f64(identity, *value);
    }
}

fn push_identity_optional_matrix(
    identity: &mut Vec<u8>,
    matrix: Option<&DispersalMultiplierMatrix>,
) {
    match matrix {
        Some(matrix) => {
            identity.push(1);
            push_identity_matrix(identity, matrix);
        }
        None => identity.push(0),
    }
}

fn push_identity_optional_extirpation(
    identity: &mut Vec<u8>,
    multipliers: Option<&ExtirpationMultiplierVector>,
) {
    match multipliers {
        Some(multipliers) => {
            identity.push(1);
            push_identity_usize(identity, multipliers.num_areas());
            for value in multipliers.values() {
                push_identity_f64(identity, *value);
            }
        }
        None => identity.push(0),
    }
}

fn push_identity_optional_binary_matrix(identity: &mut Vec<u8>, matrix: Option<&BinaryAreaMatrix>) {
    match matrix {
        Some(matrix) => {
            identity.push(1);
            push_identity_usize(identity, matrix.num_areas());
            for from in 0..matrix.num_areas() {
                for to in 0..matrix.num_areas() {
                    identity.push(u8::from(matrix.get(from, to)));
                }
            }
        }
        None => identity.push(0),
    }
}

fn push_identity_optional_allowed_ranges(
    identity: &mut Vec<u8>,
    ranges: Option<&AllowedRangeStates>,
) {
    match ranges {
        Some(ranges) => {
            identity.push(1);
            push_identity_usize(identity, ranges.num_areas());
            push_identity_usize(identity, ranges.states().len());
            for state in ranges.states() {
                identity.extend_from_slice(&state.bits().to_le_bytes());
            }
        }
        None => identity.push(0),
    }
}

pub type BioGeoModelConfig = ModelConfig;

#[derive(Clone, Debug, PartialEq)]
pub struct DecAnageneticModel {
    pub d: f64,
    pub e: f64,
    pub a: f64,
    pub branch_length_exponent: f64,
    dispersal_multipliers: Option<DispersalMultiplierMatrix>,
    time_stratified_anagenesis: Option<TimeStratifiedAnagenesis>,
    extirpation_multipliers: Option<ExtirpationMultiplierVector>,
}

impl DecAnageneticModel {
    pub fn new(d: f64, e: f64) -> Result<Self, DecModelError> {
        validate_rate("d", d)?;
        validate_rate("e", e)?;

        Ok(Self {
            d,
            e,
            a: 0.0,
            branch_length_exponent: 1.0,
            dispersal_multipliers: None,
            time_stratified_anagenesis: None,
            extirpation_multipliers: None,
        })
    }

    pub fn with_range_switching_rate(mut self, a: f64) -> Result<Self, DecModelError> {
        validate_rate("a", a)?;
        self.a = a;
        Ok(self)
    }

    pub fn with_branch_length_exponent(mut self, b: f64) -> Result<Self, DecModelError> {
        if !b.is_finite() || b < 0.0 {
            return Err(DecModelError::InvalidBranchLengthExponent { value: b });
        }
        self.branch_length_exponent = b;
        Ok(self)
    }

    pub fn with_dispersal_multipliers(mut self, multipliers: DispersalMultiplierMatrix) -> Self {
        self.dispersal_multipliers = Some(multipliers);
        self.time_stratified_anagenesis = None;
        self
    }

    pub fn with_time_stratified_dispersal(mut self, schedule: TimeStratifiedDispersal) -> Self {
        self.time_stratified_anagenesis = Some(schedule.into());
        self.dispersal_multipliers = None;
        self
    }

    pub fn with_time_stratified_anagenesis(mut self, schedule: TimeStratifiedAnagenesis) -> Self {
        self.time_stratified_anagenesis = Some(schedule);
        self.dispersal_multipliers = None;
        self
    }

    pub fn with_extirpation_multipliers(
        mut self,
        multipliers: ExtirpationMultiplierVector,
    ) -> Self {
        self.extirpation_multipliers = Some(multipliers);
        self
    }

    pub fn dispersal_multipliers(&self) -> Option<&DispersalMultiplierMatrix> {
        self.dispersal_multipliers.as_ref()
    }

    pub fn time_stratified_anagenesis(&self) -> Option<&TimeStratifiedAnagenesis> {
        self.time_stratified_anagenesis.as_ref()
    }

    pub fn extirpation_multipliers(&self) -> Option<&ExtirpationMultiplierVector> {
        self.extirpation_multipliers.as_ref()
    }

    pub fn build_q(&self, states: &StateSpace) -> Result<SparseQ, AnagenesisError> {
        if self.time_stratified_anagenesis.is_some() {
            return Err(DispersalScheduleError::RequiresBranchContext.into());
        }
        self.build_q_with_multipliers(
            states,
            self.dispersal_multipliers.as_ref(),
            self.extirpation_multipliers.as_ref(),
            None,
        )
    }

    fn build_q_with_multipliers(
        &self,
        states: &StateSpace,
        dispersal_multipliers: Option<&DispersalMultiplierMatrix>,
        extirpation_multipliers: Option<&ExtirpationMultiplierVector>,
        state_mask: Option<&StateMask>,
    ) -> Result<SparseQ, AnagenesisError> {
        if let Some(multipliers) = dispersal_multipliers
            && multipliers.num_areas() != usize::from(states.num_areas())
        {
            return Err(DispersalMatrixError::AreaCountMismatch {
                matrix_areas: multipliers.num_areas(),
                state_space_areas: usize::from(states.num_areas()),
            }
            .into());
        }
        if let Some(multipliers) = extirpation_multipliers
            && multipliers.num_areas() != usize::from(states.num_areas())
        {
            return Err(ExtirpationMultiplierError::AreaCountMismatch {
                multiplier_areas: multipliers.num_areas(),
                state_space_areas: usize::from(states.num_areas()),
            }
            .into());
        }

        let mut transitions = Vec::new();

        for (from_index, source) in states.states().iter().copied().enumerate() {
            if source.is_empty() || state_mask.is_some_and(|mask| !mask.is_allowed(from_index)) {
                continue;
            }

            for area in 0..states.num_areas() {
                if source.contains(area) {
                    let multiplier = extirpation_multipliers
                        .map_or(1.0, |multipliers| multipliers.get(usize::from(area)));
                    let rate = self.e * multiplier;
                    if !rate.is_finite() {
                        return Err(AnagenesisError::NonFiniteTransitionRate {
                            process: "extirpation",
                            from_state: from_index,
                            area: usize::from(area),
                            rate,
                        });
                    }
                    if rate <= 0.0 {
                        continue;
                    }
                    if let Some(target) = source.without_area(area)
                        && let Some(to_index) = states.index_of(target)
                        && state_mask.is_none_or(|mask| mask.is_allowed(to_index))
                    {
                        transitions.push(RateTransition {
                            from: from_index,
                            to: to_index,
                            rate,
                        });
                    }
                } else {
                    if self.d > 0.0 {
                        let multiplier_sum = match dispersal_multipliers {
                            Some(multipliers) => (0..states.num_areas())
                                .filter(|source_area| source.contains(*source_area))
                                .map(|source_area| {
                                    multipliers.get(usize::from(source_area), usize::from(area))
                                })
                                .sum(),
                            None => f64::from(source.size()),
                        };
                        let rate = self.d * multiplier_sum;
                        if !rate.is_finite() {
                            return Err(AnagenesisError::NonFiniteTransitionRate {
                                process: "dispersal",
                                from_state: from_index,
                                area: usize::from(area),
                                rate,
                            });
                        }
                        if rate > 0.0
                            && let Some(target) = source.with_area(area)
                            && target.size() <= states.max_range_size()
                            && let Some(to_index) = states.index_of(target)
                            && state_mask.is_none_or(|mask| mask.is_allowed(to_index))
                        {
                            transitions.push(RateTransition {
                                from: from_index,
                                to: to_index,
                                rate,
                            });
                        }
                    }

                    if self.a > 0.0 && source.size() == 1 {
                        let source_area = (0..states.num_areas())
                            .find(|source_area| source.contains(*source_area))
                            .expect("a non-empty singleton range contains one area");
                        let multiplier = dispersal_multipliers.map_or(1.0, |multipliers| {
                            multipliers.get(usize::from(source_area), usize::from(area))
                        });
                        let rate = self.a * multiplier;
                        if !rate.is_finite() {
                            return Err(AnagenesisError::NonFiniteTransitionRate {
                                process: "range switching",
                                from_state: from_index,
                                area: usize::from(area),
                                rate,
                            });
                        }
                        let target = crate::state::AreaSet::singleton(area)
                            .expect("a valid area can form a singleton range");
                        if rate > 0.0
                            && let Some(to_index) = states.index_of(target)
                            && state_mask.is_none_or(|mask| mask.is_allowed(to_index))
                        {
                            transitions.push(RateTransition {
                                from: from_index,
                                to: to_index,
                                rate,
                            });
                        }
                    }
                }
            }
        }

        Ok(SparseQ::new(states.len(), transitions))
    }

    pub(crate) fn build_branch_propagator(
        &self,
        tree: &Tree,
        states: &StateSpace,
    ) -> Result<OwnedBranchPropagator, AnagenesisError> {
        let Some(schedule) = &self.time_stratified_anagenesis else {
            return Ok(
                OwnedBranchPropagator::homogeneous_with_branch_length_exponent(
                    self.build_q(states)?,
                    self.branch_length_exponent,
                ),
            );
        };
        if (self.branch_length_exponent - 1.0).abs() > 1e-12 {
            return Err(AnagenesisError::BranchLengthExponentWithStratification {
                exponent: self.branch_length_exponent,
            });
        }
        if let Some(schedule_areas) = schedule.num_areas()
            && schedule_areas != usize::from(states.num_areas())
        {
            return Err(DispersalScheduleError::StateSpaceAreaCountMismatch {
                schedule_areas,
                state_space_areas: usize::from(states.num_areas()),
            }
            .into());
        }

        let node_ages = tree.node_ages_from_present();
        let root_age = node_ages[tree.root()];
        if schedule.oldest_age() + 1e-12 < root_age {
            return Err(DispersalScheduleError::DoesNotCoverRoot {
                oldest_age: schedule.oldest_age(),
                root_age,
            }
            .into());
        }

        let state_masks = self.stratified_state_masks(states)?;
        let q_matrices = schedule
            .strata()
            .iter()
            .enumerate()
            .map(|(index, stratum)| {
                self.build_q_with_multipliers(
                    states,
                    stratum.dispersal_multipliers.as_ref(),
                    stratum
                        .extirpation_multipliers
                        .as_ref()
                        .or(self.extirpation_multipliers.as_ref()),
                    state_masks.as_ref().map(|masks| &masks[index]),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut segments_by_edge = Vec::with_capacity(tree.edges().len());
        for edge in tree.edges() {
            let child_age = node_ages[edge.child];
            let parent_age = node_ages[edge.parent];
            let mut younger_boundary: f64 = 0.0;
            let mut segments = Vec::new();
            for (q_index, stratum) in schedule.strata().iter().enumerate() {
                let segment_young = child_age.max(younger_boundary);
                let segment_old = parent_age.min(stratum.oldest_age);
                let duration = segment_old - segment_young;
                if duration > 1e-12 {
                    segments.push(BranchSegment { q_index, duration });
                }
                younger_boundary = stratum.oldest_age;
                if younger_boundary >= parent_age {
                    break;
                }
            }
            segments_by_edge.push(segments);
        }

        Ok(match state_masks {
            Some(masks) => OwnedBranchPropagator::piecewise_with_state_masks(
                q_matrices,
                segments_by_edge,
                masks,
            ),
            None => OwnedBranchPropagator::piecewise(q_matrices, segments_by_edge),
        })
    }

    pub fn stratified_state_masks(
        &self,
        states: &StateSpace,
    ) -> Result<Option<Vec<StateMask>>, AnagenesisError> {
        let Some(schedule) = &self.time_stratified_anagenesis else {
            return Ok(None);
        };
        if !schedule
            .strata()
            .iter()
            .any(|stratum| stratum.state_constraint.is_some())
        {
            return Ok(None);
        }

        schedule
            .strata()
            .iter()
            .map(|stratum| match &stratum.state_constraint {
                Some(constraint) => constraint.state_mask(states).map_err(AnagenesisError::from),
                None => StateMask::all(states.len()).map_err(AnagenesisError::from),
            })
            .collect::<Result<Vec<_>, _>>()
            .map(Some)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AnagenesisError {
    AreaSizes(AreaSizeError),
    DispersalMatrix(DispersalMatrixError),
    DispersalSchedule(DispersalScheduleError),
    ExtirpationMultipliers(ExtirpationMultiplierError),
    StateConstraints(StateConstraintError),
    BranchLengthExponentWithStratification {
        exponent: f64,
    },
    NonFiniteTransitionRate {
        process: &'static str,
        from_state: usize,
        area: usize,
        rate: f64,
    },
}

impl fmt::Display for AnagenesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AreaSizes(error) => write!(f, "{error}"),
            Self::DispersalMatrix(error) => write!(f, "{error}"),
            Self::DispersalSchedule(error) => write!(f, "{error}"),
            Self::ExtirpationMultipliers(error) => write!(f, "{error}"),
            Self::StateConstraints(error) => write!(f, "{error}"),
            Self::BranchLengthExponentWithStratification { exponent } => write!(
                f,
                "branch-length exponent b={exponent} is not supported with time-stratified anagenesis, matching BioGeoBEARS' non-stratified-only b semantics"
            ),
            Self::NonFiniteTransitionRate {
                process,
                from_state,
                area,
                rate,
            } => write!(
                f,
                "{process} transition rate from state {from_state} involving area {area} is not finite: {rate}"
            ),
        }
    }
}

impl Error for AnagenesisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::AreaSizes(error) => Some(error),
            Self::DispersalMatrix(error) => Some(error),
            Self::DispersalSchedule(error) => Some(error),
            Self::ExtirpationMultipliers(error) => Some(error),
            Self::StateConstraints(error) => Some(error),
            Self::BranchLengthExponentWithStratification { .. } => None,
            Self::NonFiniteTransitionRate { .. } => None,
        }
    }
}

impl From<DispersalMatrixError> for AnagenesisError {
    fn from(value: DispersalMatrixError) -> Self {
        Self::DispersalMatrix(value)
    }
}

impl From<AreaSizeError> for AnagenesisError {
    fn from(value: AreaSizeError) -> Self {
        Self::AreaSizes(value)
    }
}

impl From<DispersalScheduleError> for AnagenesisError {
    fn from(value: DispersalScheduleError) -> Self {
        Self::DispersalSchedule(value)
    }
}

impl From<ExtirpationMultiplierError> for AnagenesisError {
    fn from(value: ExtirpationMultiplierError) -> Self {
        Self::ExtirpationMultipliers(value)
    }
}

impl From<StateConstraintError> for AnagenesisError {
    fn from(value: StateConstraintError) -> Self {
        Self::StateConstraints(value)
    }
}

fn validate_rate(name: &'static str, value: f64) -> Result<(), DecModelError> {
    if !value.is_finite() {
        return Err(DecModelError::NonFiniteRate { name, value });
    }
    if value < 0.0 {
        return Err(DecModelError::NegativeRate { name, value });
    }

    Ok(())
}

fn validate_founder_weight(
    preset: &'static str,
    value: f64,
    upper_exclusive: f64,
) -> Result<(), DecModelError> {
    if !value.is_finite() || value < 0.0 || value >= upper_exclusive {
        return Err(DecModelError::InvalidFounderWeight {
            preset,
            value,
            upper_exclusive,
        });
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub enum DecModelError {
    NonFiniteRate {
        name: &'static str,
        value: f64,
    },
    NegativeRate {
        name: &'static str,
        value: f64,
    },
    InvalidFounderWeight {
        preset: &'static str,
        value: f64,
        upper_exclusive: f64,
    },
    InvalidBranchLengthExponent {
        value: f64,
    },
}

impl fmt::Display for DecModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteRate { name, value } => {
                write!(f, "DEC rate {name} must be finite, got {value}")
            }
            Self::NegativeRate { name, value } => {
                write!(f, "DEC rate {name} must be non-negative, got {value}")
            }
            Self::InvalidFounderWeight {
                preset,
                value,
                upper_exclusive,
            } => write!(
                f,
                "{preset} founder-event weight j must be finite and satisfy 0 <= j < {upper_exclusive}, got {value}"
            ),
            Self::InvalidBranchLengthExponent { value } => write!(
                f,
                "BioGeoBEARS branch-length exponent b must be finite and non-negative, got {value}"
            ),
        }
    }
}

impl Error for DecModelError {}

#[derive(Clone, Debug, PartialEq)]
pub enum BioGeoBearsModelError {
    Parameters(ParameterError),
    Anagenesis(DecModelError),
    Cladogenesis(CladogenesisError),
}

impl fmt::Display for BioGeoBearsModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parameters(error) => write!(f, "BioGeoBEARS parameter error: {error}"),
            Self::Anagenesis(error) => write!(f, "BioGeoBEARS anagenesis error: {error}"),
            Self::Cladogenesis(error) => write!(f, "BioGeoBEARS cladogenesis error: {error}"),
        }
    }
}

impl Error for BioGeoBearsModelError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Parameters(error) => Some(error),
            Self::Anagenesis(error) => Some(error),
            Self::Cladogenesis(error) => Some(error),
        }
    }
}

impl From<ParameterError> for BioGeoBearsModelError {
    fn from(value: ParameterError) -> Self {
        Self::Parameters(value)
    }
}

impl From<DecModelError> for BioGeoBearsModelError {
    fn from(value: DecModelError) -> Self {
        Self::Anagenesis(value)
    }
}

impl From<CladogenesisError> for BioGeoBearsModelError {
    fn from(value: CladogenesisError) -> Self {
        Self::Cladogenesis(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraints::{AllowedRangeStates, BinaryAreaMatrix, RangeStateConstraint};
    use crate::dispersal::AnageneticTimeStratum;
    use crate::parameters::BioGeoBearsPreset;
    use crate::state::{AreaSet, StateSpace};

    fn assert_close_slice(left: &[f64], right: &[f64]) {
        assert_eq!(left.len(), right.len());
        for (index, (left_value, right_value)) in left.iter().zip(right).enumerate() {
            assert!(
                (left_value - right_value).abs() < 1e-12,
                "values differ at index {index}: left={left_value}, right={right_value}"
            );
        }
    }

    #[test]
    fn model_identity_v1_has_a_locked_full_configuration_golden() {
        let allowed = BinaryAreaMatrix::new(2, vec![true, true, true, true]).unwrap();
        let adjacency = BinaryAreaMatrix::new(2, vec![true, false, false, true]).unwrap();
        let young = AnageneticTimeStratum::new(
            0.5,
            Some(DispersalMultiplierMatrix::new(2, vec![1.0, 0.5, 2.0, 1.0]).unwrap()),
            Some(ExtirpationMultiplierVector::new(vec![0.75, 1.25]).unwrap()),
        )
        .unwrap()
        .with_state_constraint(RangeStateConstraint::new(Some(allowed), None).unwrap())
        .unwrap();
        let old = AnageneticTimeStratum::new(
            1.5,
            Some(DispersalMultiplierMatrix::new(2, vec![1.0, 3.0, 0.25, 1.0]).unwrap()),
            None,
        )
        .unwrap()
        .with_state_constraint(RangeStateConstraint::new(None, Some(adjacency)).unwrap())
        .unwrap();
        let anagenesis = DecAnageneticModel::new(0.12, 0.03)
            .unwrap()
            .with_range_switching_rate(0.02)
            .unwrap()
            .with_branch_length_exponent(1.0)
            .unwrap()
            .with_time_stratified_anagenesis(
                TimeStratifiedAnagenesis::new(vec![young, old]).unwrap(),
            )
            .with_extirpation_multipliers(
                ExtirpationMultiplierVector::new(vec![0.8, 1.2]).unwrap(),
            );
        let model = ModelConfig {
            anagenesis,
            cladogenesis: CladogenesisConfig {
                event_weights: crate::cladogenesis::CladogeneticEventWeights {
                    sympatry: 0.7,
                    subset_sympatry: 0.4,
                    vicariance: 0.9,
                    founder_event: 0.2,
                },
                range_size: CladogenesisRangeSizeConfig {
                    mx01y: 0.1,
                    mx01s: 0.2,
                    mx01v: 0.3,
                    mx01j: 0.4,
                },
            },
        };

        let identity = model.stable_identity_v1();
        assert!(identity.starts_with(b"biogeo-model-identity-v1\0"));
        assert_eq!(identity, model.clone().stable_identity_v1());
        let mut changed = model.clone();
        changed.cladogenesis.range_size.mx01v = 0.31;
        assert_ne!(identity, changed.stable_identity_v1());
        let hash = identity
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
            });
        assert_eq!(hash, 0x3f1a_a187_2e4b_737a);
    }

    #[test]
    fn model_identity_records_explicit_allowed_ranges_without_changing_legacy_identities() {
        let build = |allowed: [AreaSet; 2]| {
            let constraint = RangeStateConstraint::new(None, None)
                .unwrap()
                .with_allowed_ranges(AllowedRangeStates::new(2, allowed).unwrap())
                .unwrap();
            let stratum = AnageneticTimeStratum::new(1.0, None, None)
                .unwrap()
                .with_state_constraint(constraint)
                .unwrap();
            ModelConfig::preset_dec(0.1, 0.2)
                .unwrap()
                .with_time_stratified_anagenesis(
                    TimeStratifiedAnagenesis::new(vec![stratum]).unwrap(),
                )
        };
        let first = build([AreaSet::EMPTY, AreaSet::from_bits(0b01)]);
        let second = build([AreaSet::EMPTY, AreaSet::from_bits(0b10)]);

        assert_ne!(first.stable_identity_v1(), second.stable_identity_v1());
        assert!(
            first
                .stable_identity_v1()
                .windows(b"allowed-ranges-v1".len())
                .any(|window| window == b"allowed-ranges-v1")
        );
    }

    #[test]
    fn preset_dec_builds_q_and_cladogenesis_from_one_config() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let model = ModelConfig::preset_dec(0.1, 0.2).unwrap();

        let q = model.build_q(&states).unwrap();
        let cladogenesis = model.build_cladogenetic_table(&states).unwrap();

        assert_eq!(q.size(), states.len());
        assert_eq!(q.off_diagonal_count(), 6);
        assert_eq!(cladogenesis.state_count(), states.len());
        assert_eq!(cladogenesis.scenario_count(), 8);
    }

    #[test]
    fn parameter_tables_resolve_to_all_six_existing_presets() {
        let cases = [
            (
                BioGeoBearsPreset::Dec,
                vec![0.1, 0.2],
                ModelConfig::preset_dec(0.1, 0.2).unwrap(),
            ),
            (
                BioGeoBearsPreset::DecJ,
                vec![0.1, 0.2, 0.6],
                ModelConfig::preset_dec_j(0.1, 0.2, 0.6).unwrap(),
            ),
            (
                BioGeoBearsPreset::DivaLike,
                vec![0.1, 0.2],
                ModelConfig::preset_divalike(0.1, 0.2).unwrap(),
            ),
            (
                BioGeoBearsPreset::DivaLikeJ,
                vec![0.1, 0.2, 0.6],
                ModelConfig::preset_divalike_j(0.1, 0.2, 0.6).unwrap(),
            ),
            (
                BioGeoBearsPreset::BayAreaLike,
                vec![0.1, 0.2],
                ModelConfig::preset_bayarealike(0.1, 0.2).unwrap(),
            ),
            (
                BioGeoBearsPreset::BayAreaLikeJ,
                vec![0.1, 0.2, 0.4],
                ModelConfig::preset_bayarealike_j(0.1, 0.2, 0.4).unwrap(),
            ),
        ];

        for (preset, free_values, expected) in cases {
            let resolved = preset
                .parameter_table()
                .unwrap()
                .resolve_free_values(&free_values)
                .unwrap();
            let actual = ModelConfig::from_biogeobears_core_parameters(&resolved).unwrap();

            assert_eq!(actual, expected, "parameter-table mismatch for {preset:?}");
        }
    }

    #[test]
    fn preset_dec_j_adds_founder_event_scenarios() {
        let states = StateSpace::new(3, 2, true).unwrap();
        let model = ModelConfig::preset_dec_j(0.1, 0.2, 1.0).unwrap();

        let q = model.build_q(&states).unwrap();
        let cladogenesis = model.build_cladogenetic_table(&states).unwrap();

        assert_eq!(q.size(), states.len());
        assert_eq!(cladogenesis.state_count(), states.len());
        assert_eq!(cladogenesis.scenario_count(), 39);
    }

    #[test]
    fn preset_divalike_reuses_dec_q_with_divalike_cladogenesis() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let dec = ModelConfig::preset_dec(0.1, 0.2).unwrap();
        let divalike = ModelConfig::preset_divalike(0.1, 0.2).unwrap();

        assert_eq!(dec.build_q(&states), divalike.build_q(&states));
        assert_eq!(
            divalike.cladogenesis.event_weights,
            crate::cladogenesis::CladogeneticEventWeights::preset_divalike()
        );
        assert_eq!(divalike.cladogenesis.range_size.mx01v, 0.5);
        assert_ne!(
            dec.build_cladogenetic_table(&states).unwrap(),
            divalike.build_cladogenetic_table(&states).unwrap()
        );
    }

    #[test]
    fn preset_bayarealike_reuses_dec_q_with_range_copying_cladogenesis() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let dec = ModelConfig::preset_dec(0.1, 0.2).unwrap();
        let bayarealike = ModelConfig::preset_bayarealike(0.1, 0.2).unwrap();

        assert_eq!(dec.build_q(&states), bayarealike.build_q(&states));
        assert_eq!(
            bayarealike.cladogenesis.event_weights,
            crate::cladogenesis::CladogeneticEventWeights::preset_bayarealike()
        );
        assert_eq!(bayarealike.cladogenesis.range_size.mx01y, 0.9999);
        assert_eq!(
            bayarealike
                .build_cladogenetic_table(&states)
                .unwrap()
                .scenario_count(),
            states.len() - 1
        );
    }

    #[test]
    fn plus_j_presets_reuse_their_nested_models_q_and_range_size_semantics() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let divalike = ModelConfig::preset_divalike(0.1, 0.2).unwrap();
        let divalike_j = ModelConfig::preset_divalike_j(0.1, 0.2, 0.4).unwrap();
        let bayarealike = ModelConfig::preset_bayarealike(0.1, 0.2).unwrap();
        let bayarealike_j = ModelConfig::preset_bayarealike_j(0.1, 0.2, 0.4).unwrap();

        assert_eq!(divalike.build_q(&states), divalike_j.build_q(&states));
        assert_eq!(
            divalike.cladogenesis.range_size,
            divalike_j.cladogenesis.range_size
        );
        assert_eq!(bayarealike.build_q(&states), bayarealike_j.build_q(&states));
        assert_eq!(
            bayarealike.cladogenesis.range_size,
            bayarealike_j.cladogenesis.range_size
        );
        assert!(
            divalike_j
                .build_cladogenetic_table(&states)
                .unwrap()
                .scenario_count()
                > divalike
                    .build_cladogenetic_table(&states)
                    .unwrap()
                    .scenario_count()
        );
        assert!(
            bayarealike_j
                .build_cladogenetic_table(&states)
                .unwrap()
                .scenario_count()
                > bayarealike
                    .build_cladogenetic_table(&states)
                    .unwrap()
                    .scenario_count()
        );
    }

    #[test]
    fn plus_j_presets_reject_weights_that_make_linked_weights_negative() {
        assert!(matches!(
            ModelConfig::preset_dec_j(0.1, 0.2, 3.0),
            Err(DecModelError::InvalidFounderWeight {
                preset: "DEC+J",
                ..
            })
        ));
        assert!(matches!(
            ModelConfig::preset_divalike_j(0.1, 0.2, 2.0),
            Err(DecModelError::InvalidFounderWeight {
                preset: "DIVALIKE+J",
                ..
            })
        ));
        assert!(matches!(
            ModelConfig::preset_bayarealike_j(0.1, 0.2, 1.0),
            Err(DecModelError::InvalidFounderWeight {
                preset: "BAYAREALIKE+J",
                ..
            })
        ));
        assert!(ModelConfig::preset_bayarealike_j(0.1, 0.2, -0.1).is_err());
        assert!(ModelConfig::preset_divalike_j(0.1, 0.2, f64::NAN).is_err());
    }

    #[test]
    fn model_config_accepts_event_specific_range_size_constraints() {
        let states = StateSpace::new(4, 4, false).unwrap();
        let range_size = CladogenesisRangeSizeConfig {
            mx01y: 0.8,
            mx01s: 0.25,
            mx01v: 0.5,
            mx01j: 0.75,
        };
        let model = ModelConfig::preset_dec(0.1, 0.2)
            .unwrap()
            .with_range_size_config(range_size);
        let table = model.build_cladogenetic_table(&states).unwrap();

        assert_eq!(model.cladogenesis.range_size, range_size);
        assert!(table.scenario_count() > 0);
    }

    #[test]
    fn builds_two_area_dec_q_without_null_range() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .build_q(&states)
            .unwrap();

        assert_eq!(q.size(), 3);
        assert_close_slice(q.diagonal(), &[-0.1, -0.1, -0.4]);
        assert_close_slice(
            &q.to_dense_row_major(),
            &[
                -0.1, 0.0, 0.1, //
                0.0, -0.1, 0.1, //
                0.2, 0.2, -0.4,
            ],
        );
        for row in 0..q.size() {
            assert!(q.row_sum(row).abs() < 1e-12);
        }
    }

    #[test]
    fn builds_two_area_dec_q_with_absorbing_null_range() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .build_q(&states)
            .unwrap();

        assert_eq!(q.size(), 4);
        assert_close_slice(q.diagonal(), &[-0.0, -0.3, -0.3, -0.4]);
        assert_close_slice(
            &q.to_dense_row_major(),
            &[
                -0.0, 0.0, 0.0, 0.0, //
                0.2, -0.3, 0.0, 0.1, //
                0.2, 0.0, -0.3, 0.1, //
                0.0, 0.2, 0.2, -0.4,
            ],
        );
        for row in 0..q.size() {
            assert!(q.row_sum(row).abs() < 1e-12);
        }
    }

    #[test]
    fn range_switching_only_connects_singleton_ranges() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .with_range_switching_rate(0.3)
            .unwrap()
            .build_q(&states)
            .unwrap();

        assert_close_slice(q.diagonal(), &[-0.0, -0.6, -0.6, -0.4]);
        assert_close_slice(
            &q.to_dense_row_major(),
            &[
                -0.0, 0.0, 0.0, 0.0, //
                0.2, -0.6, 0.3, 0.1, //
                0.2, 0.3, -0.6, 0.1, //
                0.0, 0.2, 0.2, -0.4,
            ],
        );
    }

    #[test]
    fn range_switching_uses_directional_dispersal_modifiers() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let multipliers = DispersalMultiplierMatrix::new(2, vec![1.0, 0.25, 0.75, 1.0]).unwrap();
        let q = DecAnageneticModel::new(0.0, 0.0)
            .unwrap()
            .with_range_switching_rate(0.3)
            .unwrap()
            .with_dispersal_multipliers(multipliers)
            .build_q(&states)
            .unwrap();
        let dense = q.to_dense_row_major();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b11)).unwrap();

        assert_close_slice(
            &[
                dense[a * q.size() + b],
                dense[b * q.size() + a],
                dense[ab * q.size() + a],
                dense[ab * q.size() + b],
            ],
            &[0.075, 0.225, 0.0, 0.0],
        );
    }

    #[test]
    fn state_constraint_removes_forbidden_q_rows_and_transitions() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let adjacency = BinaryAreaMatrix::new(2, vec![true, false, false, true]).unwrap();
        let constraint = RangeStateConstraint::new(None, Some(adjacency)).unwrap();
        let mask = constraint.state_mask(&states).unwrap();
        let model = DecAnageneticModel::new(0.1, 0.2).unwrap();
        let q = model
            .build_q_with_multipliers(&states, None, None, Some(&mask))
            .unwrap();
        let dense = q.to_dense_row_major();
        let null = states.index_of(AreaSet::from_bits(0)).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b11)).unwrap();

        assert_eq!(q.off_diagonal_count(), 2);
        assert_close_slice(&[dense[a * q.size() + null]], &[0.2]);
        assert_close_slice(&[dense[b * q.size() + null]], &[0.2]);
        assert_eq!(dense[a * q.size() + ab], 0.0);
        assert_eq!(dense[b * q.size() + ab], 0.0);
        assert!(
            dense[ab * q.size()..(ab + 1) * q.size()]
                .iter()
                .all(|value| *value == 0.0)
        );
    }

    #[test]
    fn respects_max_range_size_when_adding_areas() {
        let states = StateSpace::new(3, 1, false).unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .build_q(&states)
            .unwrap();

        assert_eq!(q.off_diagonal_count(), 0);
        assert_eq!(q.diagonal(), &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn dispersal_rate_scales_with_source_range_size() {
        let states = StateSpace::new(3, 3, false).unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .build_q(&states)
            .unwrap();
        let dense = q.to_dense_row_major();
        let ab = states
            .index_of(crate::state::AreaSet::from_bits(0b011))
            .unwrap();
        let abc = states
            .index_of(crate::state::AreaSet::from_bits(0b111))
            .unwrap();

        assert_close_slice(&[dense[ab * q.size() + abc]], &[0.2]);
    }

    #[test]
    fn directional_dispersal_rate_sums_sources_into_the_new_area() {
        let states = StateSpace::new(3, 3, false).unwrap();
        let multipliers = DispersalMultiplierMatrix::new(
            3,
            vec![
                1.0, 0.25, 0.5, // A -> A/B/C
                0.75, 1.0, 2.0, // B -> A/B/C
                0.0, 3.0, 1.0, // C -> A/B/C
            ],
        )
        .unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .with_dispersal_multipliers(multipliers)
            .build_q(&states)
            .unwrap();
        let dense = q.to_dense_row_major();
        let ab = states
            .index_of(crate::state::AreaSet::from_bits(0b011))
            .unwrap();
        let abc = states
            .index_of(crate::state::AreaSet::from_bits(0b111))
            .unwrap();

        assert_close_slice(&[dense[ab * q.size() + abc]], &[0.25]);
    }

    #[test]
    fn area_specific_extirpation_multiplier_scales_removed_area() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let multipliers = ExtirpationMultiplierVector::new(vec![0.5, 2.0]).unwrap();
        let q = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .with_extirpation_multipliers(multipliers)
            .build_q(&states)
            .unwrap();
        let dense = q.to_dense_row_major();
        let a = states
            .index_of(crate::state::AreaSet::from_bits(0b01))
            .unwrap();
        let b = states
            .index_of(crate::state::AreaSet::from_bits(0b10))
            .unwrap();
        let ab = states
            .index_of(crate::state::AreaSet::from_bits(0b11))
            .unwrap();
        let null = states
            .index_of(crate::state::AreaSet::from_bits(0))
            .unwrap();

        assert_close_slice(
            &[
                dense[a * q.size() + null],
                dense[b * q.size() + null],
                dense[ab * q.size() + b],
                dense[ab * q.size() + a],
            ],
            &[0.1, 0.4, 0.1, 0.4],
        );
    }

    #[test]
    fn rejects_dispersal_matrix_for_different_state_space() {
        let states = StateSpace::new(3, 3, false).unwrap();
        let multipliers = DispersalMultiplierMatrix::new(2, vec![1.0, 1.0, 1.0, 1.0]).unwrap();
        let error = DecAnageneticModel::new(0.1, 0.2)
            .unwrap()
            .with_dispersal_multipliers(multipliers)
            .build_q(&states)
            .unwrap_err();

        assert_eq!(
            error,
            AnagenesisError::DispersalMatrix(DispersalMatrixError::AreaCountMismatch {
                matrix_areas: 2,
                state_space_areas: 3,
            })
        );
    }

    #[test]
    fn rejects_invalid_rates() {
        assert_eq!(
            DecAnageneticModel::new(-0.1, 0.2),
            Err(DecModelError::NegativeRate {
                name: "d",
                value: -0.1
            })
        );
        assert!(matches!(
            DecAnageneticModel::new(f64::NAN, 0.2),
            Err(DecModelError::NonFiniteRate { name: "d", .. })
        ));
        assert!(matches!(
            DecAnageneticModel::new(0.1, 0.2)
                .unwrap()
                .with_range_switching_rate(-0.3),
            Err(DecModelError::NegativeRate {
                name: "a",
                value: -0.3
            })
        ));
        assert!(matches!(
            DecAnageneticModel::new(0.1, 0.2)
                .unwrap()
                .with_branch_length_exponent(f64::NAN),
            Err(DecModelError::InvalidBranchLengthExponent { .. })
        ));
    }
}
