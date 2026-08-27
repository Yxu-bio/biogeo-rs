use std::error::Error;
use std::fmt;

use crate::constraints::StateMask;
use crate::dispersal::DispersalMultiplierMatrix;
use crate::state::{AreaSet, StateSpace};

pub const DEFAULT_MAXENT_CONSTRAINT: f64 = 0.0001;
pub const MIN_MAXENT_CONSTRAINT: f64 = 0.00001;
pub const MAX_MAXENT_CONSTRAINT: f64 = 0.99999;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CladogeneticScenario {
    pub ancestor: usize,
    pub left: usize,
    pub right: usize,
    pub weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CladogeneticTable {
    state_count: usize,
    rows: Vec<Vec<CladogeneticScenario>>,
}

impl CladogeneticTable {
    pub fn new(
        state_count: usize,
        scenarios: Vec<CladogeneticScenario>,
    ) -> Result<Self, CladogenesisError> {
        let mut rows = vec![Vec::new(); state_count];

        for scenario in scenarios {
            validate_scenario(state_count, scenario)?;
            rows[scenario.ancestor].push(scenario);
        }

        Ok(Self { state_count, rows })
    }

    pub fn state_count(&self) -> usize {
        self.state_count
    }

    pub fn row(&self, ancestor: usize) -> Option<&[CladogeneticScenario]> {
        self.rows.get(ancestor).map(Vec::as_slice)
    }

    pub fn rows(&self) -> &[Vec<CladogeneticScenario>] {
        &self.rows
    }

    pub fn scenario_count(&self) -> usize {
        self.rows.iter().map(Vec::len).sum()
    }

    pub fn combine(
        &self,
        left_likelihoods: &[f64],
        right_likelihoods: &[f64],
    ) -> Result<Vec<f64>, CladogenesisError> {
        if left_likelihoods.len() != self.state_count {
            return Err(CladogenesisError::LikelihoodLengthMismatch {
                side: "left",
                expected: self.state_count,
                actual: left_likelihoods.len(),
            });
        }
        if right_likelihoods.len() != self.state_count {
            return Err(CladogenesisError::LikelihoodLengthMismatch {
                side: "right",
                expected: self.state_count,
                actual: right_likelihoods.len(),
            });
        }

        let mut result = vec![0.0; self.state_count];
        for (ancestor, row) in self.rows.iter().enumerate() {
            result[ancestor] = row
                .iter()
                .map(|scenario| {
                    scenario.weight
                        * left_likelihoods[scenario.left]
                        * right_likelihoods[scenario.right]
                })
                .sum();
        }

        Ok(result)
    }

    pub fn constrained(&self, mask: &StateMask) -> Result<Self, CladogenesisError> {
        if mask.len() != self.state_count {
            return Err(CladogenesisError::StateMaskLengthMismatch {
                expected: self.state_count,
                actual: mask.len(),
            });
        }

        let mut scenarios = Vec::new();
        for (ancestor, row) in self.rows.iter().enumerate() {
            if !mask.is_allowed(ancestor) {
                continue;
            }
            let kept: Vec<&CladogeneticScenario> = row
                .iter()
                .filter(|scenario| {
                    mask.is_allowed(scenario.left) && mask.is_allowed(scenario.right)
                })
                .collect();
            if kept.is_empty() {
                if row.is_empty() {
                    continue;
                }
                return Err(CladogenesisError::NoAllowedScenariosForState { ancestor });
            }
            let total: f64 = kept.iter().map(|scenario| scenario.weight).sum();
            if !total.is_finite() || total <= 0.0 {
                return Err(CladogenesisError::NoAllowedScenariosForState { ancestor });
            }
            scenarios.extend(kept.into_iter().map(|scenario| CladogeneticScenario {
                ancestor,
                left: scenario.left,
                right: scenario.right,
                weight: scenario.weight / total,
            }));
        }

        Self::new(self.state_count, scenarios)
    }
}

pub(crate) trait CladogeneticProcess {
    fn state_count(&self) -> usize;
    fn table_for_node(&self, node: usize) -> &CladogeneticTable;
    fn state_mask_for_node(&self, node: usize) -> Option<&StateMask>;
}

pub(crate) struct HomogeneousCladogeneticProcess<'a> {
    table: &'a CladogeneticTable,
}

impl<'a> HomogeneousCladogeneticProcess<'a> {
    pub(crate) fn new(table: &'a CladogeneticTable) -> Self {
        Self { table }
    }
}

impl CladogeneticProcess for HomogeneousCladogeneticProcess<'_> {
    fn state_count(&self) -> usize {
        self.table.state_count()
    }

    fn table_for_node(&self, _node: usize) -> &CladogeneticTable {
        self.table
    }

    fn state_mask_for_node(&self, _node: usize) -> Option<&StateMask> {
        None
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum OwnedCladogeneticProcess {
    Homogeneous(CladogeneticTable),
    Stratified {
        tables: Vec<CladogeneticTable>,
        table_index_by_node: Vec<usize>,
        state_masks: Option<Vec<StateMask>>,
    },
}

impl OwnedCladogeneticProcess {
    pub(crate) fn homogeneous(table: CladogeneticTable) -> Self {
        Self::Homogeneous(table)
    }

    pub(crate) fn stratified(
        tables: Vec<CladogeneticTable>,
        table_index_by_node: Vec<usize>,
        state_masks: Option<Vec<StateMask>>,
    ) -> Self {
        debug_assert!(
            state_masks
                .as_ref()
                .is_none_or(|state_masks| tables.len() == state_masks.len())
        );
        Self::Stratified {
            tables,
            table_index_by_node,
            state_masks,
        }
    }
}

impl CladogeneticProcess for OwnedCladogeneticProcess {
    fn state_count(&self) -> usize {
        match self {
            Self::Homogeneous(table) => table.state_count(),
            Self::Stratified { tables, .. } => {
                tables.first().map_or(0, CladogeneticTable::state_count)
            }
        }
    }

    fn table_for_node(&self, node: usize) -> &CladogeneticTable {
        match self {
            Self::Homogeneous(table) => table,
            Self::Stratified {
                tables,
                table_index_by_node,
                ..
            } => &tables[table_index_by_node[node]],
        }
    }

    fn state_mask_for_node(&self, node: usize) -> Option<&StateMask> {
        match self {
            Self::Homogeneous(_) => None,
            Self::Stratified {
                table_index_by_node,
                state_masks,
                ..
            } => state_masks
                .as_ref()
                .map(|state_masks| &state_masks[table_index_by_node[node]]),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CladogenesisConfig {
    pub event_weights: CladogeneticEventWeights,
    pub range_size: CladogenesisRangeSizeConfig,
}

impl CladogenesisConfig {
    pub fn preset_dec() -> Self {
        Self {
            event_weights: CladogeneticEventWeights::preset_dec(),
            range_size: CladogenesisRangeSizeConfig::linked(DEFAULT_MAXENT_CONSTRAINT),
        }
    }

    pub fn preset_dec_j(j: f64) -> Self {
        let mut config = Self::preset_dec();
        let non_founder_weight = (3.0 - j) / 3.0;
        config.event_weights.sympatry = non_founder_weight;
        config.event_weights.subset_sympatry = non_founder_weight;
        config.event_weights.vicariance = non_founder_weight;
        config.event_weights.founder_event = j;
        config
    }

    pub fn preset_divalike() -> Self {
        let mut range_size = CladogenesisRangeSizeConfig::linked(DEFAULT_MAXENT_CONSTRAINT);
        range_size.mx01v = 0.5;

        Self {
            event_weights: CladogeneticEventWeights::preset_divalike(),
            range_size,
        }
    }

    pub fn preset_divalike_j(j: f64) -> Self {
        let mut config = Self::preset_divalike();
        let non_founder_weight = (2.0 - j) / 2.0;
        config.event_weights.sympatry = non_founder_weight;
        config.event_weights.vicariance = non_founder_weight;
        config.event_weights.founder_event = j;
        config
    }

    pub fn preset_bayarealike() -> Self {
        let mut range_size = CladogenesisRangeSizeConfig::linked(DEFAULT_MAXENT_CONSTRAINT);
        range_size.mx01y = 0.9999;

        Self {
            event_weights: CladogeneticEventWeights::preset_bayarealike(),
            range_size,
        }
    }

    pub fn preset_bayarealike_j(j: f64) -> Self {
        let mut config = Self::preset_bayarealike();
        config.event_weights.sympatry = 1.0 - j;
        config.event_weights.founder_event = j;
        config
    }

    pub fn validate(&self) -> Result<(), CladogenesisError> {
        self.event_weights.validate()?;
        self.range_size.validate()?;
        Ok(())
    }

    pub fn build_table(&self, states: &StateSpace) -> Result<CladogeneticTable, CladogenesisError> {
        self.build_table_with_dispersal(states, None)
    }

    pub fn build_table_with_dispersal(
        &self,
        states: &StateSpace,
        dispersal_multipliers: Option<&DispersalMultiplierMatrix>,
    ) -> Result<CladogeneticTable, CladogenesisError> {
        self.validate()?;
        if let Some(multipliers) = dispersal_multipliers
            && multipliers.num_areas() != usize::from(states.num_areas())
        {
            return Err(CladogenesisError::DispersalMatrixAreaCountMismatch {
                matrix_areas: multipliers.num_areas(),
                state_space_areas: usize::from(states.num_areas()),
            });
        }
        let size_weights = CladogenesisRangeSizeWeights::new(states, self.range_size);

        let mut scenarios = Vec::new();

        for (ancestor_index, ancestor_state) in states.states().iter().copied().enumerate() {
            if ancestor_state.is_empty() {
                continue;
            }

            let mut row = Vec::new();
            add_range_copying(
                ancestor_index,
                self.event_weights.sympatry
                    * size_weights
                        .sympatry
                        .weight(ancestor_state.size(), ancestor_state.size()),
                &mut row,
            );

            if ancestor_state.size() > 1 {
                add_subset_sympatry(
                    states,
                    ancestor_state,
                    ancestor_index,
                    self.event_weights.subset_sympatry,
                    &size_weights.subset_sympatry,
                    &mut row,
                );
                add_ordered_vicariance(
                    states,
                    ancestor_state,
                    self.event_weights.vicariance,
                    &size_weights.vicariance,
                    &mut row,
                );
            }

            add_founder_event(
                states,
                ancestor_state,
                ancestor_index,
                self.event_weights.founder_event,
                &size_weights.founder_event,
                dispersal_multipliers,
                &mut row,
            );

            normalize_row(ancestor_index, ancestor_state, &row, &mut scenarios)?;
        }

        CladogeneticTable::new(states.len(), scenarios)
    }
}

impl Default for CladogenesisConfig {
    fn default() -> Self {
        Self::preset_dec()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CladogeneticEventWeights {
    pub sympatry: f64,
    pub subset_sympatry: f64,
    pub vicariance: f64,
    pub founder_event: f64,
}

impl CladogeneticEventWeights {
    pub fn preset_dec() -> Self {
        Self {
            sympatry: 1.0,
            subset_sympatry: 1.0,
            vicariance: 1.0,
            founder_event: 0.0,
        }
    }

    pub fn preset_divalike() -> Self {
        Self {
            sympatry: 1.0,
            subset_sympatry: 0.0,
            vicariance: 1.0,
            founder_event: 0.0,
        }
    }

    pub fn preset_bayarealike() -> Self {
        Self {
            sympatry: 1.0,
            subset_sympatry: 0.0,
            vicariance: 0.0,
            founder_event: 0.0,
        }
    }

    fn validate(self) -> Result<(), CladogenesisError> {
        validate_event_weight("y", self.sympatry)?;
        validate_event_weight("s", self.subset_sympatry)?;
        validate_event_weight("v", self.vicariance)?;
        validate_event_weight("j", self.founder_event)?;

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CladogenesisRangeSizeConfig {
    pub mx01y: f64,
    pub mx01s: f64,
    pub mx01v: f64,
    pub mx01j: f64,
}

impl CladogenesisRangeSizeConfig {
    pub fn linked(mx01: f64) -> Self {
        Self {
            mx01y: mx01,
            mx01s: mx01,
            mx01v: mx01,
            mx01j: mx01,
        }
    }

    pub fn validate(self) -> Result<(), CladogenesisError> {
        validate_maxent_constraint("mx01y", self.mx01y)?;
        validate_maxent_constraint("mx01s", self.mx01s)?;
        validate_maxent_constraint("mx01v", self.mx01v)?;
        validate_maxent_constraint("mx01j", self.mx01j)?;

        Ok(())
    }
}

impl Default for CladogenesisRangeSizeConfig {
    fn default() -> Self {
        Self::linked(DEFAULT_MAXENT_CONSTRAINT)
    }
}

#[derive(Clone, Debug)]
struct CladogenesisRangeSizeWeights {
    sympatry: MaxentRangeSizeTable,
    subset_sympatry: MaxentRangeSizeTable,
    vicariance: MaxentRangeSizeTable,
    founder_event: MaxentRangeSizeTable,
}

impl CladogenesisRangeSizeWeights {
    fn new(states: &StateSpace, config: CladogenesisRangeSizeConfig) -> Self {
        let max_ancestor_size = states.max_range_size();
        let sympatry = MaxentRangeSizeTable::for_subsets(max_ancestor_size, config.mx01y);
        let subset_sympatry = if config.mx01s.to_bits() == config.mx01y.to_bits() {
            sympatry.clone()
        } else {
            MaxentRangeSizeTable::for_subsets(max_ancestor_size, config.mx01s)
        };
        let founder_event = if config.mx01j.to_bits() == config.mx01y.to_bits() {
            sympatry.clone()
        } else if config.mx01j.to_bits() == config.mx01s.to_bits() {
            subset_sympatry.clone()
        } else {
            MaxentRangeSizeTable::for_subsets(max_ancestor_size, config.mx01j)
        };

        Self {
            sympatry,
            subset_sympatry,
            vicariance: MaxentRangeSizeTable::for_vicariance(max_ancestor_size, config.mx01v),
            founder_event,
        }
    }
}

#[derive(Clone, Debug)]
struct MaxentRangeSizeTable {
    rows: Vec<Vec<f64>>,
}

impl MaxentRangeSizeTable {
    fn for_subsets(max_ancestor_size: u8, constraint: f64) -> Self {
        let rows = (1..=max_ancestor_size)
            .map(|ancestor_size| maxent_probabilities(ancestor_size, constraint))
            .collect();

        Self { rows }
    }

    fn for_vicariance(max_ancestor_size: u8, constraint: f64) -> Self {
        let rows = (1..=max_ancestor_size)
            .map(|ancestor_size| {
                let max_smaller_size = ancestor_size / 2;
                if max_smaller_size == 0 {
                    Vec::new()
                } else {
                    maxent_probabilities(max_smaller_size, constraint)
                }
            })
            .collect();

        Self { rows }
    }

    fn weight(&self, ancestor_size: u8, daughter_size: u8) -> f64 {
        if ancestor_size == 0 || daughter_size == 0 {
            return 0.0;
        }

        self.rows
            .get(usize::from(ancestor_size - 1))
            .and_then(|row| row.get(usize::from(daughter_size - 1)))
            .copied()
            .unwrap_or(0.0)
    }
}

fn maxent_probabilities(state_count: u8, constraint: f64) -> Vec<f64> {
    debug_assert!(state_count > 0);

    let state_count_usize = usize::from(state_count);
    if state_count == 1 {
        return vec![1.0];
    }

    // BioGeoBEARS delegates this calculation to rexpokit::maxent(), whose
    // itscale5 routine stops once every probability changes by at most 1e-7.
    // Reproducing that stopping rule matters at the rounded boundary regimes.
    let target_mean = (f64::from(state_count) + 1.0) * constraint;
    let state_sum = f64::from(state_count) * (f64::from(state_count) + 1.0) * 0.5;
    let mut probabilities = vec![1.0 / f64::from(state_count); state_count_usize];
    let mut updated = vec![0.0; state_count_usize];
    let mut max_change = f64::INFINITY;
    while max_change > 1e-7 {
        let mut current_mean = 0.0;
        for (index, probability) in probabilities.iter().enumerate() {
            current_mean += (index as f64 + 1.0) * probability;
        }
        let gamma = (target_mean / current_mean).ln() / state_sum;
        let mut total = 0.0;
        let step = gamma.exp();
        let mut multiplier = step;
        for (next, previous) in updated.iter_mut().zip(&probabilities) {
            *next = previous * multiplier;
            total += *next;
            multiplier *= step;
        }
        max_change = 0.0;
        for (next, previous) in updated.iter_mut().zip(&probabilities) {
            *next /= total;
            max_change = max_change.max((*next - previous).abs());
        }
        std::mem::swap(&mut probabilities, &mut updated);
    }

    probabilities
        .into_iter()
        .map(round_biogeobears_maxent_probability)
        .collect()
}

fn round_biogeobears_maxent_probability(probability: f64) -> f64 {
    (probability * 1_000.0).round_ties_even() / 1_000.0
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct WeightedSplit {
    left: usize,
    right: usize,
    weight: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecCladogeneticModel {
    range_size: CladogenesisRangeSizeConfig,
}

impl DecCladogeneticModel {
    pub fn new() -> Self {
        Self {
            range_size: CladogenesisRangeSizeConfig::default(),
        }
    }

    pub fn with_widespread_sympatry(mut self, allow: bool) -> Self {
        self.range_size.mx01y = if allow {
            0.5
        } else {
            DEFAULT_MAXENT_CONSTRAINT
        };
        self
    }

    pub fn with_range_size_config(mut self, range_size: CladogenesisRangeSizeConfig) -> Self {
        self.range_size = range_size;
        self
    }

    pub fn build_table(self, states: &StateSpace) -> Result<CladogeneticTable, CladogenesisError> {
        let mut config = CladogenesisConfig::preset_dec();
        config.range_size = self.range_size;

        config.build_table(states)
    }
}

impl Default for DecCladogeneticModel {
    fn default() -> Self {
        Self::new()
    }
}

fn add_range_copying(ancestor_index: usize, weight: f64, row: &mut Vec<WeightedSplit>) {
    if weight <= 0.0 {
        return;
    }

    row.push(WeightedSplit {
        left: ancestor_index,
        right: ancestor_index,
        weight,
    });
}

fn add_subset_sympatry(
    states: &StateSpace,
    ancestor_state: AreaSet,
    ancestor_index: usize,
    event_weight: f64,
    size_weights: &MaxentRangeSizeTable,
    row: &mut Vec<WeightedSplit>,
) {
    if event_weight <= 0.0 {
        return;
    }

    let ancestor_bits = ancestor_state.bits();
    let mut subset_bits = (ancestor_bits - 1) & ancestor_bits;

    while subset_bits != 0 {
        let subset = AreaSet::from_bits(subset_bits);
        let weight = event_weight * size_weights.weight(ancestor_state.size(), subset.size());
        if weight > 0.0
            && let Some(subset_index) = states.index_of(subset)
        {
            push_ordered_copy_split(ancestor_index, subset_index, weight, row);
        }

        subset_bits = (subset_bits - 1) & ancestor_bits;
    }
}

fn add_ordered_vicariance(
    states: &StateSpace,
    ancestor_state: AreaSet,
    event_weight: f64,
    size_weights: &MaxentRangeSizeTable,
    row: &mut Vec<WeightedSplit>,
) {
    if event_weight <= 0.0 {
        return;
    }

    let ancestor_bits = ancestor_state.bits();
    let mut subset_bits = (ancestor_bits - 1) & ancestor_bits;

    while subset_bits != 0 {
        let complement_bits = ancestor_bits & !subset_bits;
        let left = AreaSet::from_bits(subset_bits);
        let right = AreaSet::from_bits(complement_bits);
        let smaller_size = left.size().min(right.size());
        let weight = event_weight * size_weights.weight(ancestor_state.size(), smaller_size);

        if weight > 0.0
            && let (Some(left_index), Some(right_index)) =
                (states.index_of(left), states.index_of(right))
        {
            row.push(WeightedSplit {
                left: left_index,
                right: right_index,
                weight,
            });
        }

        subset_bits = (subset_bits - 1) & ancestor_bits;
    }
}

fn add_founder_event(
    states: &StateSpace,
    ancestor_state: AreaSet,
    ancestor_index: usize,
    event_weight: f64,
    size_weights: &MaxentRangeSizeTable,
    dispersal_multipliers: Option<&DispersalMultiplierMatrix>,
    row: &mut Vec<WeightedSplit>,
) {
    if event_weight <= 0.0 {
        return;
    }

    for (daughter_index, daughter_state) in states.states().iter().copied().enumerate() {
        if daughter_state.is_empty() || (ancestor_state.bits() & daughter_state.bits()) != 0 {
            continue;
        }

        let pairwise_weight = dispersal_multipliers.map_or(1.0, |multipliers| {
            founder_event_pairwise_mean(ancestor_state, daughter_state, multipliers)
        });
        let weight = event_weight
            * size_weights.weight(ancestor_state.size(), daughter_state.size())
            * pairwise_weight;
        if weight > 0.0 {
            push_ordered_copy_split(ancestor_index, daughter_index, weight, row);
        }
    }
}

fn founder_event_pairwise_mean(
    ancestor_state: AreaSet,
    daughter_state: AreaSet,
    multipliers: &DispersalMultiplierMatrix,
) -> f64 {
    let mut total = 0.0;
    let mut pair_count = 0usize;
    for source_area in 0..multipliers.num_areas() {
        if !ancestor_state.contains(source_area as u8) {
            continue;
        }
        for target_area in 0..multipliers.num_areas() {
            if daughter_state.contains(target_area as u8) {
                total += multipliers.get(source_area, target_area);
                pair_count += 1;
            }
        }
    }

    debug_assert_eq!(
        pair_count,
        usize::from(ancestor_state.size()) * usize::from(daughter_state.size())
    );
    total / pair_count as f64
}

fn push_ordered_copy_split(
    copied_range: usize,
    daughter_range: usize,
    weight: f64,
    row: &mut Vec<WeightedSplit>,
) {
    row.push(WeightedSplit {
        left: copied_range,
        right: daughter_range,
        weight,
    });
    row.push(WeightedSplit {
        left: daughter_range,
        right: copied_range,
        weight,
    });
}

fn normalize_row(
    ancestor_index: usize,
    ancestor_state: AreaSet,
    row: &[WeightedSplit],
    scenarios: &mut Vec<CladogeneticScenario>,
) -> Result<(), CladogenesisError> {
    let total_weight: f64 = row.iter().map(|split| split.weight).sum();
    if total_weight <= 0.0 {
        return Err(CladogenesisError::NoScenariosForState {
            ancestor: ancestor_index,
            bits: ancestor_state.bits(),
        });
    }

    for split in row {
        scenarios.push(CladogeneticScenario {
            ancestor: ancestor_index,
            left: split.left,
            right: split.right,
            weight: split.weight / total_weight,
        });
    }

    Ok(())
}

fn validate_event_weight(name: &'static str, value: f64) -> Result<(), CladogenesisError> {
    if !value.is_finite() {
        return Err(CladogenesisError::NonFiniteEventWeight {
            name,
            weight: value,
        });
    }
    if value < 0.0 {
        return Err(CladogenesisError::NegativeEventWeight {
            name,
            weight: value,
        });
    }

    Ok(())
}

fn validate_maxent_constraint(name: &'static str, value: f64) -> Result<(), CladogenesisError> {
    if !value.is_finite() {
        return Err(CladogenesisError::NonFiniteMaxentConstraint { name, value });
    }
    if !(MIN_MAXENT_CONSTRAINT..=MAX_MAXENT_CONSTRAINT).contains(&value) {
        return Err(CladogenesisError::MaxentConstraintOutOfRange {
            name,
            value,
            min: MIN_MAXENT_CONSTRAINT,
            max: MAX_MAXENT_CONSTRAINT,
        });
    }

    Ok(())
}

fn validate_scenario(
    state_count: usize,
    scenario: CladogeneticScenario,
) -> Result<(), CladogenesisError> {
    if scenario.ancestor >= state_count {
        return Err(CladogenesisError::ScenarioStateOutOfBounds {
            field: "ancestor",
            state: scenario.ancestor,
            state_count,
        });
    }
    if scenario.left >= state_count {
        return Err(CladogenesisError::ScenarioStateOutOfBounds {
            field: "left",
            state: scenario.left,
            state_count,
        });
    }
    if scenario.right >= state_count {
        return Err(CladogenesisError::ScenarioStateOutOfBounds {
            field: "right",
            state: scenario.right,
            state_count,
        });
    }
    if !scenario.weight.is_finite() {
        return Err(CladogenesisError::NonFiniteScenarioWeight {
            weight: scenario.weight,
        });
    }
    if scenario.weight < 0.0 {
        return Err(CladogenesisError::NegativeScenarioWeight {
            weight: scenario.weight,
        });
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub enum CladogenesisError {
    NonFiniteEventWeight {
        name: &'static str,
        weight: f64,
    },
    NegativeEventWeight {
        name: &'static str,
        weight: f64,
    },
    NonFiniteMaxentConstraint {
        name: &'static str,
        value: f64,
    },
    MaxentConstraintOutOfRange {
        name: &'static str,
        value: f64,
        min: f64,
        max: f64,
    },
    NoScenariosForState {
        ancestor: usize,
        bits: u64,
    },
    ScenarioStateOutOfBounds {
        field: &'static str,
        state: usize,
        state_count: usize,
    },
    NonFiniteScenarioWeight {
        weight: f64,
    },
    NegativeScenarioWeight {
        weight: f64,
    },
    LikelihoodLengthMismatch {
        side: &'static str,
        expected: usize,
        actual: usize,
    },
    StateMaskLengthMismatch {
        expected: usize,
        actual: usize,
    },
    NoAllowedScenariosForState {
        ancestor: usize,
    },
    DispersalMatrixAreaCountMismatch {
        matrix_areas: usize,
        state_space_areas: usize,
    },
}

impl fmt::Display for CladogenesisError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteEventWeight { name, weight } => write!(
                f,
                "cladogenesis event weight {name} must be finite, got {weight}"
            ),
            Self::NegativeEventWeight { name, weight } => write!(
                f,
                "cladogenesis event weight {name} must be non-negative, got {weight}"
            ),
            Self::NonFiniteMaxentConstraint { name, value } => write!(
                f,
                "cladogenesis maxent constraint {name} must be finite, got {value}"
            ),
            Self::MaxentConstraintOutOfRange {
                name,
                value,
                min,
                max,
            } => write!(
                f,
                "cladogenesis maxent constraint {name} must be in [{min}, {max}], got {value}"
            ),
            Self::NoScenariosForState { ancestor, bits } => write!(
                f,
                "cladogenesis config generated no scenarios for ancestor state {ancestor} with bits {bits:#b}"
            ),
            Self::ScenarioStateOutOfBounds {
                field,
                state,
                state_count,
            } => write!(
                f,
                "cladogenesis scenario {field} state {state} is out of bounds for {state_count} states"
            ),
            Self::NonFiniteScenarioWeight { weight } => {
                write!(
                    f,
                    "cladogenesis scenario weight must be finite, got {weight}"
                )
            }
            Self::NegativeScenarioWeight { weight } => write!(
                f,
                "cladogenesis scenario weight must be non-negative, got {weight}"
            ),
            Self::LikelihoodLengthMismatch {
                side,
                expected,
                actual,
            } => write!(
                f,
                "cladogenesis {side} likelihood length mismatch: expected {expected}, got {actual}"
            ),
            Self::StateMaskLengthMismatch { expected, actual } => write!(
                f,
                "cladogenesis state mask length mismatch: expected {expected}, got {actual}"
            ),
            Self::NoAllowedScenariosForState { ancestor } => write!(
                f,
                "state constraints leave no cladogenetic scenarios for ancestor state {ancestor}"
            ),
            Self::DispersalMatrixAreaCountMismatch {
                matrix_areas,
                state_space_areas,
            } => write!(
                f,
                "founder-event dispersal matrix has {matrix_areas} areas but state space has {state_space_areas}"
            ),
        }
    }
}

impl Error for CladogenesisError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(left: f64, right: f64, tolerance: f64) {
        assert!(
            (left - right).abs() < tolerance,
            "values differ: left={left}, right={right}"
        );
    }

    #[test]
    fn builds_two_area_dec_scenarios_without_null_range() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let table = DecCladogeneticModel::new().build_table(&states).unwrap();

        assert_eq!(table.state_count(), 3);
        assert_eq!(table.row(0).unwrap().len(), 1);
        assert_eq!(table.row(1).unwrap().len(), 1);
        assert_eq!(table.row(2).unwrap().len(), 6);
        assert_eq!(table.scenario_count(), 8);

        for row in table.rows() {
            if row.is_empty() {
                continue;
            }
            let row_sum: f64 = row.iter().map(|scenario| scenario.weight).sum();
            assert_close(row_sum, 1.0, 1e-12);
        }
    }

    #[test]
    fn leaves_null_range_without_split_scenarios() {
        let states = StateSpace::new(2, 2, true).unwrap();
        let table = DecCladogeneticModel::new().build_table(&states).unwrap();

        assert!(table.row(0).unwrap().is_empty());
        assert_eq!(table.row(1).unwrap().len(), 1);
        assert_eq!(table.row(2).unwrap().len(), 1);
        assert_eq!(table.row(3).unwrap().len(), 6);
    }

    #[test]
    fn combines_split_likelihoods_by_ancestor_state() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let table = DecCladogeneticModel::new().build_table(&states).unwrap();
        let combined = table.combine(&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]).unwrap();

        assert_close(combined[0], 0.0, 1e-12);
        assert_close(combined[1], 0.0, 1e-12);
        assert_close(combined[2], 1.0 / 6.0, 1e-12);
    }

    #[test]
    fn dec_cladogenesis_wrapper_matches_preset_config() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let via_wrapper = DecCladogeneticModel::new().build_table(&states).unwrap();
        let via_config = CladogenesisConfig::preset_dec()
            .build_table(&states)
            .unwrap();

        assert_eq!(via_wrapper, via_config);
    }

    #[test]
    fn divalike_preset_disables_subset_sympatry_and_allows_balanced_vicariance() {
        let states = StateSpace::new(4, 4, false).unwrap();
        let config = CladogenesisConfig::preset_divalike();
        let table = config.build_table(&states).unwrap();
        let ancestor = states.index_of(AreaSet::from_bits(0b1111)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b0011)).unwrap();
        let cd = states.index_of(AreaSet::from_bits(0b1100)).unwrap();
        let row = table.row(ancestor).unwrap();

        assert_eq!(
            config.event_weights,
            CladogeneticEventWeights::preset_divalike()
        );
        assert_eq!(config.range_size.mx01v, 0.5);
        assert_eq!(row.len(), 14);
        assert!(
            row.iter()
                .any(|scenario| scenario.left == ab && scenario.right == cd)
        );
        assert!(row.iter().all(|scenario| {
            let left = states.states()[scenario.left];
            let right = states.states()[scenario.right];
            left.bits() & right.bits() == 0 && left.bits() | right.bits() == 0b1111
        }));
    }

    #[test]
    fn bayarealike_preset_only_copies_the_ancestor_range() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let config = CladogenesisConfig::preset_bayarealike();
        let table = config.build_table(&states).unwrap();

        assert_eq!(
            config.event_weights,
            CladogeneticEventWeights::preset_bayarealike()
        );
        assert_eq!(config.range_size.mx01y, 0.9999);
        for (ancestor, state) in states.states().iter().enumerate() {
            let row = table.row(ancestor).unwrap();
            if state.is_empty() {
                assert!(row.is_empty());
                continue;
            }

            assert_eq!(row.len(), 1);
            assert_eq!(row[0].ancestor, ancestor);
            assert_eq!(row[0].left, ancestor);
            assert_eq!(row[0].right, ancestor);
            assert_close(row[0].weight, 1.0, 1e-12);
        }
    }

    #[test]
    fn optional_widespread_sympatry_adds_one_multi_area_scenario() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let table = DecCladogeneticModel::new()
            .with_widespread_sympatry(true)
            .build_table(&states)
            .unwrap();

        assert_eq!(table.row(2).unwrap().len(), 7);
    }

    #[test]
    fn cladogenesis_event_weights_are_normalized_within_ancestor_state() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.event_weights.vicariance = 3.0;
        let table = config.build_table(&states).unwrap();
        let ancestor = states.index_of(AreaSet::from_bits(0b11)).unwrap();
        let row = table.row(ancestor).unwrap();

        assert_eq!(row.len(), 6);
        for scenario in row {
            if scenario.left == ancestor || scenario.right == ancestor {
                assert_close(scenario.weight, 1.0 / 10.0, 1e-12);
            } else {
                assert_close(scenario.weight, 3.0 / 10.0, 1e-12);
            }
        }
        assert_close(row.iter().map(|scenario| scenario.weight).sum(), 1.0, 1e-12);
    }

    #[test]
    fn four_area_dec_vicariance_uses_singleton_splits() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let table = DecCladogeneticModel::new().build_table(&states).unwrap();
        let ancestor = states.index_of(AreaSet::from_bits(0b1111)).unwrap();
        let row = table.row(ancestor).unwrap();

        assert_eq!(row.len(), 16);
        for scenario in row {
            let left = states.get(scenario.left).unwrap();
            let right = states.get(scenario.right).unwrap();
            assert_close(scenario.weight, 1.0 / 16.0, 1e-12);
            assert!(
                left.size() == 1
                    || right.size() == 1
                    || left.bits() == 0b1111
                    || right.bits() == 0b1111
            );
            assert!(
                !(left.size() == 2 && right.size() == 2),
                "DEC vicariance should not include balanced 2+2 split"
            );
        }
    }

    #[test]
    fn any_vicariance_split_size_rule_allows_balanced_splits() {
        let states = StateSpace::new(4, 4, true).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.range_size.mx01v = 0.5;
        let table = config.build_table(&states).unwrap();
        let ancestor = states.index_of(AreaSet::from_bits(0b1111)).unwrap();
        let row = table.row(ancestor).unwrap();

        assert_eq!(row.len(), 22);
        assert!(row.iter().any(|scenario| {
            let left = states.get(scenario.left).unwrap();
            let right = states.get(scenario.right).unwrap();

            left.size() == 2 && right.size() == 2
        }));
        assert_close(row.iter().map(|scenario| scenario.weight).sum(), 1.0, 1e-12);
    }

    #[test]
    fn maxent_probabilities_match_biogeobears_rounded_values() {
        assert_eq!(maxent_probabilities(4, 0.5), vec![0.25; 4]);
        assert_eq!(
            maxent_probabilities(4, 0.25),
            vec![0.797, 0.163, 0.033, 0.007]
        );
        assert_eq!(
            maxent_probabilities(5, 0.8),
            vec![0.001, 0.004, 0.023, 0.139, 0.833]
        );
        assert_eq!(maxent_probabilities(4, 0.2), vec![0.999, 0.001, 0.0, 0.0]);
        assert_eq!(maxent_probabilities(4, 0.8), vec![0.0, 0.0, 0.002, 0.998]);
        assert_eq!(
            maxent_probabilities(6, DEFAULT_MAXENT_CONSTRAINT),
            vec![1.0, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn linked_half_constraint_enables_all_split_sizes() {
        let states = StateSpace::new(4, 4, false).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.range_size = CladogenesisRangeSizeConfig::linked(0.5);
        let table = config.build_table(&states).unwrap();
        let ancestor = states.index_of(AreaSet::from_bits(0b1111)).unwrap();
        let row = table.row(ancestor).unwrap();

        assert_eq!(row.len(), 43);
        assert!(
            row.iter()
                .any(|scenario| { scenario.left == ancestor && scenario.right == ancestor })
        );
        assert!(row.iter().any(|scenario| {
            let left = states.get(scenario.left).unwrap();
            let right = states.get(scenario.right).unwrap();
            left.size() == 2 && right.size() == 2 && (left.bits() & right.bits()) == 0
        }));
        assert_close(row.iter().map(|scenario| scenario.weight).sum(), 1.0, 1e-12);
    }

    #[test]
    fn founder_event_constraint_enables_multi_area_new_range() {
        let states = StateSpace::new(4, 2, false).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.event_weights.founder_event = 1.0;
        config.range_size.mx01j = 0.5;
        let table = config.build_table(&states).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b0011)).unwrap();
        let cd = states.index_of(AreaSet::from_bits(0b1100)).unwrap();
        let row = table.row(ab).unwrap();

        assert!(row.iter().any(|scenario| {
            (scenario.left == ab && scenario.right == cd)
                || (scenario.left == cd && scenario.right == ab)
        }));
    }

    #[test]
    fn custom_founder_event_weights_match_biogeobears_small_coo_table() {
        let states = StateSpace::new(3, 2, true).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.event_weights.founder_event = 1.0;
        let table = config.build_table(&states).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b001)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b010)).unwrap();
        let c = states.index_of(AreaSet::from_bits(0b100)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b011)).unwrap();
        let ac = states.index_of(AreaSet::from_bits(0b101)).unwrap();
        let bc = states.index_of(AreaSet::from_bits(0b110)).unwrap();

        let a_row = table.row(a).unwrap();
        assert_eq!(a_row.len(), 5);
        assert_contains_split(a_row, a, a, 1.0 / 5.0);
        assert_contains_split(a_row, a, b, 1.0 / 5.0);
        assert_contains_split(a_row, b, a, 1.0 / 5.0);
        assert_contains_split(a_row, a, c, 1.0 / 5.0);
        assert_contains_split(a_row, c, a, 1.0 / 5.0);

        let ab_row = table.row(ab).unwrap();
        assert_eq!(ab_row.len(), 8);
        assert_contains_split(ab_row, ab, c, 1.0 / 8.0);
        assert_contains_split(ab_row, c, ab, 1.0 / 8.0);
        assert!(
            !ab_row
                .iter()
                .any(|scenario| scenario.left == ab && scenario.right == ab)
        );

        assert_eq!(table.row(ac).unwrap().len(), 8);
        assert_eq!(table.row(bc).unwrap().len(), 8);
    }

    #[test]
    fn founder_event_uses_directed_pairwise_mean_before_row_normalization() {
        let states = StateSpace::new(4, 2, false).unwrap();
        let config = CladogenesisConfig {
            event_weights: CladogeneticEventWeights {
                sympatry: 0.0,
                subset_sympatry: 0.0,
                vicariance: 0.0,
                founder_event: 1.0,
            },
            range_size: CladogenesisRangeSizeConfig::default(),
        };
        let multipliers = DispersalMultiplierMatrix::new(
            4,
            vec![
                1.0, 1.0, 2.0, 1.0, // A -> A/B/C/D
                1.0, 1.0, 4.0, 3.0, // B -> A/B/C/D
                5.0, 1.0, 1.0, 1.0, // C -> A/B/C/D
                7.0, 1.0, 1.0, 1.0, // D -> A/B/C/D
            ],
        )
        .unwrap();
        let table = config
            .build_table_with_dispersal(&states, Some(&multipliers))
            .unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b0011)).unwrap();
        let c = states.index_of(AreaSet::from_bits(0b0100)).unwrap();
        let d = states.index_of(AreaSet::from_bits(0b1000)).unwrap();
        let row = table.row(ab).unwrap();

        // mean(A->C, B->C)=3 and mean(A->D, B->D)=2.
        assert_eq!(row.len(), 4);
        assert_contains_split(row, ab, c, 0.3);
        assert_contains_split(row, c, ab, 0.3);
        assert_contains_split(row, ab, d, 0.2);
        assert_contains_split(row, d, ab, 0.2);
    }

    #[test]
    fn all_one_founder_event_matrix_matches_unmodified_table() {
        let states = StateSpace::new(3, 2, true).unwrap();
        let config = CladogenesisConfig::preset_dec_j(0.5);
        let unmodified = config.build_table(&states).unwrap();
        let ones = DispersalMultiplierMatrix::new(3, vec![1.0; 9]).unwrap();
        let modified = config
            .build_table_with_dispersal(&states, Some(&ones))
            .unwrap();

        assert_eq!(unmodified, modified);
    }

    #[test]
    fn founder_event_rejects_dispersal_matrix_for_other_state_space() {
        let states = StateSpace::new(3, 2, false).unwrap();
        let matrix = DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap();
        let error = CladogenesisConfig::preset_dec_j(0.5)
            .build_table_with_dispersal(&states, Some(&matrix))
            .unwrap_err();

        assert_eq!(
            error,
            CladogenesisError::DispersalMatrixAreaCountMismatch {
                matrix_areas: 2,
                state_space_areas: 3,
            }
        );
    }

    #[test]
    fn dec_j_preset_uses_biogeobears_linked_event_weights() {
        let config = CladogenesisConfig::preset_dec_j(0.6);

        assert_close(config.event_weights.founder_event, 0.6, 1e-12);
        assert_close(config.event_weights.sympatry, 0.8, 1e-12);
        assert_close(config.event_weights.subset_sympatry, 0.8, 1e-12);
        assert_close(config.event_weights.vicariance, 0.8, 1e-12);
    }

    #[test]
    fn divalike_j_preset_uses_biogeobears_linked_event_weights() {
        let config = CladogenesisConfig::preset_divalike_j(0.6);

        assert_close(config.event_weights.founder_event, 0.6, 1e-12);
        assert_close(config.event_weights.sympatry, 0.7, 1e-12);
        assert_close(config.event_weights.subset_sympatry, 0.0, 1e-12);
        assert_close(config.event_weights.vicariance, 0.7, 1e-12);
        assert_close(config.range_size.mx01v, 0.5, 1e-12);
    }

    #[test]
    fn bayarealike_j_preset_uses_biogeobears_linked_event_weights() {
        let config = CladogenesisConfig::preset_bayarealike_j(0.4);

        assert_close(config.event_weights.founder_event, 0.4, 1e-12);
        assert_close(config.event_weights.sympatry, 0.6, 1e-12);
        assert_close(config.event_weights.subset_sympatry, 0.0, 1e-12);
        assert_close(config.event_weights.vicariance, 0.0, 1e-12);
        assert_close(config.range_size.mx01y, 0.9999, 1e-12);
    }

    #[test]
    fn constrained_table_removes_forbidden_daughters_and_renormalizes_rows() {
        let table = CladogeneticTable::new(
            3,
            vec![
                CladogeneticScenario {
                    ancestor: 2,
                    left: 0,
                    right: 0,
                    weight: 0.2,
                },
                CladogeneticScenario {
                    ancestor: 2,
                    left: 0,
                    right: 1,
                    weight: 0.3,
                },
                CladogeneticScenario {
                    ancestor: 2,
                    left: 2,
                    right: 0,
                    weight: 0.5,
                },
            ],
        )
        .unwrap();
        let mask = StateMask::new(vec![true, false, true]).unwrap();

        let constrained = table.constrained(&mask).unwrap();
        let row = constrained.row(2).unwrap();

        assert_eq!(row.len(), 2);
        assert_contains_split(row, 0, 0, 2.0 / 7.0);
        assert_contains_split(row, 2, 0, 5.0 / 7.0);
        assert!(
            row.iter()
                .all(|scenario| scenario.left != 1 && scenario.right != 1)
        );
        assert_close(row.iter().map(|scenario| scenario.weight).sum(), 1.0, 1e-12);
    }

    #[test]
    fn rejects_invalid_cladogenesis_event_weights() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.event_weights.founder_event = -0.1;

        assert_eq!(
            config.build_table(&states),
            Err(CladogenesisError::NegativeEventWeight {
                name: "j",
                weight: -0.1
            })
        );

        config.event_weights.founder_event = f64::NAN;
        assert!(matches!(
            config.build_table(&states),
            Err(CladogenesisError::NonFiniteEventWeight { name: "j", .. })
        ));
    }

    #[test]
    fn rejects_invalid_maxent_constraints() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let mut config = CladogenesisConfig::preset_dec();
        config.range_size.mx01v = 0.0;

        assert_eq!(
            config.build_table(&states),
            Err(CladogenesisError::MaxentConstraintOutOfRange {
                name: "mx01v",
                value: 0.0,
                min: MIN_MAXENT_CONSTRAINT,
                max: MAX_MAXENT_CONSTRAINT,
            })
        );

        config.range_size.mx01v = f64::NAN;
        assert!(matches!(
            config.build_table(&states),
            Err(CladogenesisError::NonFiniteMaxentConstraint { name: "mx01v", .. })
        ));
    }

    fn assert_contains_split(row: &[CladogeneticScenario], left: usize, right: usize, weight: f64) {
        assert!(
            row.iter().any(|scenario| {
                scenario.left == left
                    && scenario.right == right
                    && (scenario.weight - weight).abs() < 1e-12
            }),
            "missing split ({left}, {right}) with weight {weight}"
        );
    }
}
