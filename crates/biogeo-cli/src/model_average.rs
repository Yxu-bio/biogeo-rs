use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;

use biogeo_core::{NodeStatePosterior, SplitScenarioPosterior};

pub const MODEL_AVERAGED_ANCESTRAL_RANGES_FORMAT: &str =
    "biogeo-model-averaged-ancestral-ranges-v2";

const PROBABILITY_TOLERANCE: f64 = 1e-10;
const WEIGHT_TOLERANCE: f64 = 1e-12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InformationCriterion {
    Aic,
    Aicc,
}

impl InformationCriterion {
    fn as_str(self) -> &'static str {
        match self {
            Self::Aic => "AIC",
            Self::Aicc => "AICc",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CriterionWeight {
    pub value: f64,
    pub delta: f64,
    pub weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WeightedModel {
    pub model_id: String,
    pub analysis_result: String,
    pub log_likelihood: f64,
    pub aic: Option<CriterionWeight>,
    pub aicc: Option<CriterionWeight>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeMetadata {
    pub node: usize,
    pub label: String,
    pub kind: &'static str,
    pub clade: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitNodeMetadata {
    pub node: usize,
    pub left_node: usize,
    pub right_node: usize,
    pub left_clade: String,
    pub right_clade: String,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum SplitEvent {
    RangeCopying,
    SubsetSympatry,
    Vicariance,
    FounderEvent,
}

impl SplitEvent {
    fn as_str(self) -> &'static str {
        match self {
            Self::RangeCopying => "range_copying",
            Self::SubsetSympatry => "subset_sympatry",
            Self::Vicariance => "vicariance",
            Self::FounderEvent => "founder_event",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct SplitKey {
    node: usize,
    ancestor: usize,
    left: usize,
    right: usize,
    event: SplitEvent,
}

#[derive(Clone, Copy, Debug, Default)]
struct CompensatedSum {
    value: f64,
    correction: f64,
}

impl CompensatedSum {
    fn add(&mut self, value: f64) {
        let adjusted = value - self.correction;
        let next = self.value + adjusted;
        self.correction = (next - self.value) - adjusted;
        self.value = next;
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateMetadata {
    pub state_index: usize,
    pub range_bits: u64,
    pub range: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AreaMetadata {
    pub area_index: usize,
    pub area_bit: u64,
    pub area: String,
}

#[derive(Debug)]
struct CriterionAccumulator {
    criterion: InformationCriterion,
    sums: Vec<f64>,
    corrections: Vec<f64>,
    split_sums: BTreeMap<SplitKey, CompensatedSum>,
    models: Vec<(WeightedModel, CriterionWeight)>,
    weight_sum: f64,
}

impl CriterionAccumulator {
    fn new(criterion: InformationCriterion, cells: usize) -> Self {
        Self {
            criterion,
            sums: vec![0.0; cells],
            corrections: vec![0.0; cells],
            split_sums: BTreeMap::new(),
            models: Vec::new(),
            weight_sum: 0.0,
        }
    }

    fn add(
        &mut self,
        model: &WeightedModel,
        criterion_weight: CriterionWeight,
        posteriors: &[NodeStatePosterior],
        splits: &[SplitScenarioPosterior],
        states: &[StateMetadata],
        state_count: usize,
    ) -> Result<(), ModelAverageError> {
        validate_criterion_weight(self.criterion, model, criterion_weight)?;
        for posterior in posteriors {
            for (state_index, probability) in posterior.probabilities.iter().enumerate() {
                let index = posterior.node * state_count + state_index;
                let contribution = criterion_weight.weight * probability;
                let adjusted = contribution - self.corrections[index];
                let next = self.sums[index] + adjusted;
                self.corrections[index] = (next - self.sums[index]) - adjusted;
                self.sums[index] = next;
            }
        }
        for split in splits {
            let key = split_key(split, states)?;
            self.split_sums
                .entry(key)
                .or_default()
                .add(criterion_weight.weight * split.probability);
        }
        self.weight_sum += criterion_weight.weight;
        self.models.push((model.clone(), criterion_weight));
        Ok(())
    }

    fn validate_and_normalize(&mut self) -> Result<(), ModelAverageError> {
        if self.models.is_empty() {
            return Ok(());
        }
        if !self.weight_sum.is_finite()
            || self.weight_sum <= 0.0
            || (self.weight_sum - 1.0).abs() > WEIGHT_TOLERANCE
        {
            return Err(ModelAverageError::CriterionWeightSum {
                criterion: self.criterion,
                value: self.weight_sum,
            });
        }
        for value in &mut self.sums {
            *value /= self.weight_sum;
        }
        for sum in self.split_sums.values_mut() {
            sum.value /= self.weight_sum;
            sum.correction /= self.weight_sum;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct ModelAverageAccumulator {
    total_models: usize,
    tree_nodes: usize,
    areas: Vec<AreaMetadata>,
    states: Vec<StateMetadata>,
    internal_nodes: Vec<NodeMetadata>,
    split_nodes: Vec<SplitNodeMetadata>,
    aic: CriterionAccumulator,
    aicc: CriterionAccumulator,
}

impl ModelAverageAccumulator {
    pub fn new(
        total_models: usize,
        tree_nodes: usize,
        internal_nodes: Vec<NodeMetadata>,
        split_nodes: Vec<SplitNodeMetadata>,
        areas: Vec<AreaMetadata>,
        states: Vec<StateMetadata>,
    ) -> Result<Self, ModelAverageError> {
        if tree_nodes == 0 {
            return Err(ModelAverageError::EmptyTree);
        }
        if states.is_empty() {
            return Err(ModelAverageError::EmptyStateSpace);
        }
        if areas.is_empty() {
            return Err(ModelAverageError::EmptyAreaSet);
        }
        for (expected, area) in areas.iter().enumerate() {
            let expected_bit = 1_u64
                .checked_shl(expected as u32)
                .ok_or(ModelAverageError::AreaIndexOverflow(expected))?;
            if area.area_index != expected || area.area_bit != expected_bit {
                return Err(ModelAverageError::AreaOrder {
                    expected_index: expected,
                    actual_index: area.area_index,
                    expected_bit,
                    actual_bit: area.area_bit,
                });
            }
        }
        for (expected, state) in states.iter().enumerate() {
            if state.state_index != expected {
                return Err(ModelAverageError::StateOrder {
                    expected,
                    actual: state.state_index,
                });
            }
        }
        for node in &internal_nodes {
            if node.node >= tree_nodes {
                return Err(ModelAverageError::UnknownInternalNode {
                    node: node.node,
                    tree_nodes,
                });
            }
        }
        validate_split_node_metadata(tree_nodes, &internal_nodes, &split_nodes)?;
        let cells = tree_nodes
            .checked_mul(states.len())
            .ok_or(ModelAverageError::DimensionOverflow)?;
        Ok(Self {
            total_models,
            tree_nodes,
            areas,
            states,
            internal_nodes,
            split_nodes,
            aic: CriterionAccumulator::new(InformationCriterion::Aic, cells),
            aicc: CriterionAccumulator::new(InformationCriterion::Aicc, cells),
        })
    }

    pub fn add_model(
        &mut self,
        model: WeightedModel,
        posteriors: &[NodeStatePosterior],
        splits: &[SplitScenarioPosterior],
    ) -> Result<(), ModelAverageError> {
        validate_posteriors(
            posteriors,
            self.tree_nodes,
            self.states.len(),
            &model.model_id,
        )?;
        validate_split_posteriors(splits, &self.split_nodes, &self.states, &model.model_id)?;
        if let Some(weight) = model.aic {
            self.aic.add(
                &model,
                weight,
                posteriors,
                splits,
                &self.states,
                self.states.len(),
            )?;
        }
        if let Some(weight) = model.aicc {
            self.aicc.add(
                &model,
                weight,
                posteriors,
                splits,
                &self.states,
                self.states.len(),
            )?;
        }
        Ok(())
    }

    pub fn finish(mut self) -> Result<String, ModelAverageError> {
        self.aic.validate_and_normalize()?;
        self.aicc.validate_and_normalize()?;
        validate_averaged_probabilities(&self.aic, &self.internal_nodes, self.states.len())?;
        validate_averaged_probabilities(&self.aicc, &self.internal_nodes, self.states.len())?;
        validate_averaged_split_probabilities(&self.aic, &self.split_nodes)?;
        validate_averaged_split_probabilities(&self.aicc, &self.split_nodes)?;
        Ok(format_result(&self))
    }
}

fn validate_posteriors(
    posteriors: &[NodeStatePosterior],
    tree_nodes: usize,
    state_count: usize,
    model_id: &str,
) -> Result<(), ModelAverageError> {
    if posteriors.len() != tree_nodes {
        return Err(ModelAverageError::PosteriorNodeCount {
            model_id: model_id.to_string(),
            expected: tree_nodes,
            actual: posteriors.len(),
        });
    }
    for (expected_node, posterior) in posteriors.iter().enumerate() {
        if posterior.node != expected_node {
            return Err(ModelAverageError::PosteriorNodeOrder {
                model_id: model_id.to_string(),
                expected: expected_node,
                actual: posterior.node,
            });
        }
        if posterior.probabilities.len() != state_count {
            return Err(ModelAverageError::PosteriorStateCount {
                model_id: model_id.to_string(),
                node: posterior.node,
                expected: state_count,
                actual: posterior.probabilities.len(),
            });
        }
        let mut sum = 0.0;
        for (state, probability) in posterior.probabilities.iter().copied().enumerate() {
            if !probability.is_finite() || probability < 0.0 {
                return Err(ModelAverageError::InvalidPosteriorProbability {
                    model_id: model_id.to_string(),
                    node: posterior.node,
                    state,
                    value: probability,
                });
            }
            sum += probability;
        }
        if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
            return Err(ModelAverageError::PosteriorProbabilitySum {
                model_id: model_id.to_string(),
                node: posterior.node,
                value: sum,
            });
        }
    }
    Ok(())
}

fn validate_split_node_metadata(
    tree_nodes: usize,
    internal_nodes: &[NodeMetadata],
    split_nodes: &[SplitNodeMetadata],
) -> Result<(), ModelAverageError> {
    let internal = internal_nodes
        .iter()
        .map(|metadata| metadata.node)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for metadata in split_nodes {
        if metadata.node >= tree_nodes
            || metadata.left_node >= tree_nodes
            || metadata.right_node >= tree_nodes
            || !internal.contains(&metadata.node)
            || metadata.left_node == metadata.right_node
            || !seen.insert(metadata.node)
        {
            return Err(ModelAverageError::InvalidSplitNodeMetadata {
                node: metadata.node,
                left: metadata.left_node,
                right: metadata.right_node,
                tree_nodes,
            });
        }
    }
    Ok(())
}

fn validate_split_posteriors(
    splits: &[SplitScenarioPosterior],
    split_nodes: &[SplitNodeMetadata],
    states: &[StateMetadata],
    model_id: &str,
) -> Result<(), ModelAverageError> {
    let expected_nodes = split_nodes
        .iter()
        .map(|metadata| metadata.node)
        .collect::<BTreeSet<_>>();
    let mut sums = BTreeMap::<usize, f64>::new();
    let mut seen = BTreeSet::new();
    for split in splits {
        if !expected_nodes.contains(&split.node) {
            return Err(ModelAverageError::UnexpectedSplitNode {
                model_id: model_id.to_string(),
                node: split.node,
            });
        }
        if split.ancestor >= states.len()
            || split.left >= states.len()
            || split.right >= states.len()
        {
            return Err(ModelAverageError::SplitStateOutOfBounds {
                model_id: model_id.to_string(),
                node: split.node,
                ancestor: split.ancestor,
                left: split.left,
                right: split.right,
                states: states.len(),
            });
        }
        if !split.probability.is_finite()
            || split.probability < 0.0
            || !split.weight.is_finite()
            || split.weight < 0.0
        {
            return Err(ModelAverageError::InvalidSplitProbability {
                model_id: model_id.to_string(),
                node: split.node,
                ancestor: split.ancestor,
                left: split.left,
                right: split.right,
                weight: split.weight,
                probability: split.probability,
            });
        }
        let key = split_key(split, states)?;
        if !seen.insert(key) {
            return Err(ModelAverageError::DuplicateSplitScenario {
                model_id: model_id.to_string(),
                node: split.node,
                ancestor: split.ancestor,
                left: split.left,
                right: split.right,
            });
        }
        *sums.entry(split.node).or_default() += split.probability;
    }
    for node in expected_nodes {
        let sum = sums.get(&node).copied().unwrap_or(0.0);
        if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
            return Err(ModelAverageError::SplitProbabilitySum {
                model_id: model_id.to_string(),
                node,
                value: sum,
            });
        }
    }
    Ok(())
}

fn split_key(
    split: &SplitScenarioPosterior,
    states: &[StateMetadata],
) -> Result<SplitKey, ModelAverageError> {
    let ancestor = states
        .get(split.ancestor)
        .ok_or(ModelAverageError::InternalSplitStateOutOfBounds(
            split.ancestor,
        ))?
        .range_bits;
    let left = states
        .get(split.left)
        .ok_or(ModelAverageError::InternalSplitStateOutOfBounds(split.left))?
        .range_bits;
    let right = states
        .get(split.right)
        .ok_or(ModelAverageError::InternalSplitStateOutOfBounds(
            split.right,
        ))?
        .range_bits;
    let event = classify_split_event(split.node, ancestor, left, right)?;
    Ok(SplitKey {
        node: split.node,
        ancestor: split.ancestor,
        left: split.left,
        right: split.right,
        event,
    })
}

fn classify_split_event(
    node: usize,
    ancestor: u64,
    left: u64,
    right: u64,
) -> Result<SplitEvent, ModelAverageError> {
    if left == ancestor && right == ancestor {
        return Ok(SplitEvent::RangeCopying);
    }
    if left == ancestor || right == ancestor {
        let other = if left == ancestor { right } else { left };
        if other != 0 && other & !ancestor == 0 {
            return Ok(SplitEvent::SubsetSympatry);
        }
        if other != 0 && other & ancestor == 0 {
            return Ok(SplitEvent::FounderEvent);
        }
    }
    if left & right == 0 && left | right == ancestor && left != 0 && right != 0 {
        return Ok(SplitEvent::Vicariance);
    }
    Err(ModelAverageError::UnsupportedSplitScenario {
        node,
        ancestor,
        left,
        right,
    })
}

fn validate_criterion_weight(
    criterion: InformationCriterion,
    model: &WeightedModel,
    value: CriterionWeight,
) -> Result<(), ModelAverageError> {
    if !value.value.is_finite()
        || !value.delta.is_finite()
        || !value.weight.is_finite()
        || value.delta < 0.0
        || value.weight < 0.0
        || value.weight > 1.0
    {
        return Err(ModelAverageError::InvalidCriterionWeight {
            criterion,
            model_id: model.model_id.clone(),
            value: value.value,
            delta: value.delta,
            weight: value.weight,
        });
    }
    Ok(())
}

fn validate_averaged_probabilities(
    accumulator: &CriterionAccumulator,
    internal_nodes: &[NodeMetadata],
    state_count: usize,
) -> Result<(), ModelAverageError> {
    if accumulator.models.is_empty() {
        return Ok(());
    }
    for node in internal_nodes {
        let start = node.node * state_count;
        let sum = accumulator.sums[start..start + state_count]
            .iter()
            .sum::<f64>();
        if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
            return Err(ModelAverageError::AveragedProbabilitySum {
                criterion: accumulator.criterion,
                node: node.node,
                value: sum,
            });
        }
    }
    Ok(())
}

fn validate_averaged_split_probabilities(
    accumulator: &CriterionAccumulator,
    split_nodes: &[SplitNodeMetadata],
) -> Result<(), ModelAverageError> {
    if accumulator.models.is_empty() {
        return Ok(());
    }
    for node in split_nodes {
        let sum = accumulator
            .split_sums
            .iter()
            .filter(|(key, _)| key.node == node.node)
            .map(|(_, value)| value.value)
            .sum::<f64>();
        if (sum - 1.0).abs() > PROBABILITY_TOLERANCE {
            return Err(ModelAverageError::AveragedSplitProbabilitySum {
                criterion: accumulator.criterion,
                node: node.node,
                value: sum,
            });
        }
    }
    Ok(())
}

fn format_result(result: &ModelAverageAccumulator) -> String {
    let criteria =
        usize::from(!result.aic.models.is_empty()) + usize::from(!result.aicc.models.is_empty());
    let status = if criteria == 0 {
        "unavailable"
    } else {
        "available"
    };
    let split_keys = result
        .aic
        .split_sums
        .keys()
        .chain(result.aicc.split_sums.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut output = format!(
        "format\t{}\nstatus\t{}\nsource_model_comparison_format\tbiogeo-model-comparison-v3\nmodels\t{}\ncriteria\t{}\naic_models\t{}\naicc_models\t{}\ntree_nodes\t{}\ninternal_nodes\t{}\nsplit_nodes\t{}\nareas\t{}\nstates\t{}\nsplit_scenarios\t{}\nposterior_location\tbranch_top_at_node\nstate_probability_rows\tinternal_nodes_only\nsplit_probability_rows\tcladogenesis_nodes_only\naveraging_formula\tsum_model(weight_model * posterior_probability_given_model)\nmissing_split_scenario_semantics\tzero_probability_before_weighted_sum\nordered_daughters\tinput_tree_child_order\nfield_encoding\tpercent_for_percent_tab_cr_lf\n\nmodel_weights\ncriterion\tmodel_id\tanalysis_result\tlnL\tinformation_criterion\tdelta\tweight\n",
        MODEL_AVERAGED_ANCESTRAL_RANGES_FORMAT,
        status,
        result.total_models,
        criteria,
        result.aic.models.len(),
        result.aicc.models.len(),
        result.tree_nodes,
        result.internal_nodes.len(),
        result.split_nodes.len(),
        result.areas.len(),
        result.states.len(),
        split_keys.len(),
    );
    append_model_weights(&mut output, &result.aic);
    append_model_weights(&mut output, &result.aicc);
    output.push_str("\nnodes\nnode\tlabel\tkind\tclade\n");
    for node in &result.internal_nodes {
        writeln!(
            output,
            "{}\t{}\t{}\t{}",
            node.node,
            encode_field(&node.label),
            node.kind,
            encode_field(&node.clade),
        )
        .unwrap();
    }
    output.push_str("\nsplit_nodes\nnode\tleft_node\tright_node\tleft_clade\tright_clade\n");
    for node in &result.split_nodes {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}",
            node.node,
            node.left_node,
            node.right_node,
            encode_field(&node.left_clade),
            encode_field(&node.right_clade),
        )
        .unwrap();
    }
    output.push_str("\nareas\narea_index\tarea_bit\tarea\n");
    for area in &result.areas {
        writeln!(
            output,
            "{}\t{}\t{}",
            area.area_index,
            area.area_bit,
            encode_field(&area.area),
        )
        .unwrap();
    }
    output.push_str("\nstates\nstate_index\trange_bits\trange\n");
    for state in &result.states {
        writeln!(
            output,
            "{}\t{}\t{}",
            state.state_index,
            state.range_bits,
            encode_field(&state.range),
        )
        .unwrap();
    }
    output.push_str("\nancestral_state_probabilities\ncriterion\tnode\tstate_index\tprobability\n");
    append_probabilities(
        &mut output,
        &result.aic,
        &result.internal_nodes,
        &result.states,
    );
    append_probabilities(
        &mut output,
        &result.aicc,
        &result.internal_nodes,
        &result.states,
    );
    output.push_str("\nsplit_scenarios\nscenario_index\tnode\tancestor_state_index\tleft_state_index\tright_state_index\tevent\n");
    for (scenario_index, key) in split_keys.iter().enumerate() {
        writeln!(
            output,
            "{}\t{}\t{}\t{}\t{}\t{}",
            scenario_index,
            key.node,
            key.ancestor,
            key.left,
            key.right,
            key.event.as_str(),
        )
        .unwrap();
    }
    output.push_str("\ncladogenetic_split_probabilities\ncriterion\tscenario_index\tprobability\n");
    append_split_probabilities(&mut output, &result.aic, &split_keys);
    append_split_probabilities(&mut output, &result.aicc, &split_keys);
    output
}

fn append_model_weights(output: &mut String, accumulator: &CriterionAccumulator) {
    for (model, criterion) in &accumulator.models {
        writeln!(
            output,
            "{}\t{}\t{}\t{:.17}\t{:.17}\t{:.17}\t{:.17}",
            accumulator.criterion.as_str(),
            encode_field(&model.model_id),
            encode_field(&model.analysis_result),
            model.log_likelihood,
            criterion.value,
            criterion.delta,
            criterion.weight,
        )
        .unwrap();
    }
}

fn append_probabilities(
    output: &mut String,
    accumulator: &CriterionAccumulator,
    internal_nodes: &[NodeMetadata],
    states: &[StateMetadata],
) {
    if accumulator.models.is_empty() {
        return;
    }
    for node in internal_nodes {
        let start = node.node * states.len();
        for state in states {
            writeln!(
                output,
                "{}\t{}\t{}\t{:.17}",
                accumulator.criterion.as_str(),
                node.node,
                state.state_index,
                accumulator.sums[start + state.state_index],
            )
            .unwrap();
        }
    }
}

fn append_split_probabilities(
    output: &mut String,
    accumulator: &CriterionAccumulator,
    keys: &BTreeSet<SplitKey>,
) {
    if accumulator.models.is_empty() {
        return;
    }
    for (scenario_index, key) in keys.iter().enumerate() {
        let probability = accumulator.split_sums.get(key).map_or(0.0, |sum| sum.value);
        writeln!(
            output,
            "{}\t{}\t{:.17}",
            accumulator.criterion.as_str(),
            scenario_index,
            probability,
        )
        .unwrap();
    }
}

fn encode_field(value: &str) -> String {
    let mut encoded = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'%' | b'\t' | b'\r' | b'\n' => {
                encoded.push(b'%');
                encoded.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
            _ => encoded.push(byte),
        }
    }
    String::from_utf8(encoded).expect("encoding a UTF-8 model-average field preserves UTF-8")
}

#[derive(Debug, PartialEq)]
pub enum ModelAverageError {
    EmptyTree,
    EmptyAreaSet,
    EmptyStateSpace,
    DimensionOverflow,
    AreaIndexOverflow(usize),
    AreaOrder {
        expected_index: usize,
        actual_index: usize,
        expected_bit: u64,
        actual_bit: u64,
    },
    StateOrder {
        expected: usize,
        actual: usize,
    },
    UnknownInternalNode {
        node: usize,
        tree_nodes: usize,
    },
    InvalidSplitNodeMetadata {
        node: usize,
        left: usize,
        right: usize,
        tree_nodes: usize,
    },
    PosteriorNodeCount {
        model_id: String,
        expected: usize,
        actual: usize,
    },
    PosteriorNodeOrder {
        model_id: String,
        expected: usize,
        actual: usize,
    },
    PosteriorStateCount {
        model_id: String,
        node: usize,
        expected: usize,
        actual: usize,
    },
    InvalidPosteriorProbability {
        model_id: String,
        node: usize,
        state: usize,
        value: f64,
    },
    PosteriorProbabilitySum {
        model_id: String,
        node: usize,
        value: f64,
    },
    UnexpectedSplitNode {
        model_id: String,
        node: usize,
    },
    SplitStateOutOfBounds {
        model_id: String,
        node: usize,
        ancestor: usize,
        left: usize,
        right: usize,
        states: usize,
    },
    InternalSplitStateOutOfBounds(usize),
    InvalidSplitProbability {
        model_id: String,
        node: usize,
        ancestor: usize,
        left: usize,
        right: usize,
        weight: f64,
        probability: f64,
    },
    DuplicateSplitScenario {
        model_id: String,
        node: usize,
        ancestor: usize,
        left: usize,
        right: usize,
    },
    SplitProbabilitySum {
        model_id: String,
        node: usize,
        value: f64,
    },
    UnsupportedSplitScenario {
        node: usize,
        ancestor: u64,
        left: u64,
        right: u64,
    },
    InvalidCriterionWeight {
        criterion: InformationCriterion,
        model_id: String,
        value: f64,
        delta: f64,
        weight: f64,
    },
    CriterionWeightSum {
        criterion: InformationCriterion,
        value: f64,
    },
    AveragedProbabilitySum {
        criterion: InformationCriterion,
        node: usize,
        value: f64,
    },
    AveragedSplitProbabilitySum {
        criterion: InformationCriterion,
        node: usize,
        value: f64,
    },
}

impl fmt::Display for ModelAverageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyTree => write!(f, "model averaging requires at least one tree node"),
            Self::EmptyAreaSet => write!(f, "model averaging requires at least one area"),
            Self::EmptyStateSpace => write!(f, "model averaging requires at least one state"),
            Self::DimensionOverflow => write!(f, "model-average matrix dimensions overflow usize"),
            Self::AreaIndexOverflow(index) => {
                write!(f, "model-average area index {index} exceeds the u64 bitset")
            }
            Self::AreaOrder {
                expected_index,
                actual_index,
                expected_bit,
                actual_bit,
            } => write!(
                f,
                "model-average area metadata is out of order: expected index {expected_index}/bit {expected_bit}, found index {actual_index}/bit {actual_bit}"
            ),
            Self::StateOrder { expected, actual } => write!(
                f,
                "model-average state metadata is out of order: expected {expected}, found {actual}"
            ),
            Self::UnknownInternalNode { node, tree_nodes } => write!(
                f,
                "model-average internal node {node} is outside a {tree_nodes}-node tree"
            ),
            Self::InvalidSplitNodeMetadata {
                node,
                left,
                right,
                tree_nodes,
            } => write!(
                f,
                "invalid split-node metadata for node {node}: daughters {left}/{right}, tree nodes {tree_nodes}"
            ),
            Self::PosteriorNodeCount {
                model_id,
                expected,
                actual,
            } => write!(
                f,
                "model {model_id:?} posterior has {actual} nodes; expected {expected}"
            ),
            Self::PosteriorNodeOrder {
                model_id,
                expected,
                actual,
            } => write!(
                f,
                "model {model_id:?} posterior node order differs: expected {expected}, found {actual}"
            ),
            Self::PosteriorStateCount {
                model_id,
                node,
                expected,
                actual,
            } => write!(
                f,
                "model {model_id:?} posterior node {node} has {actual} states; expected {expected}"
            ),
            Self::InvalidPosteriorProbability {
                model_id,
                node,
                state,
                value,
            } => write!(
                f,
                "model {model_id:?} posterior at node {node}, state {state} is invalid: {value}"
            ),
            Self::PosteriorProbabilitySum {
                model_id,
                node,
                value,
            } => write!(
                f,
                "model {model_id:?} posterior probabilities at node {node} sum to {value}, not 1"
            ),
            Self::UnexpectedSplitNode { model_id, node } => write!(
                f,
                "model {model_id:?} has a split posterior for non-cladogenesis node {node}"
            ),
            Self::SplitStateOutOfBounds {
                model_id,
                node,
                ancestor,
                left,
                right,
                states,
            } => write!(
                f,
                "model {model_id:?} split at node {node} uses state indices {ancestor}/{left}/{right} outside {states} states"
            ),
            Self::InternalSplitStateOutOfBounds(state) => write!(
                f,
                "internal model-average split state {state} is out of bounds"
            ),
            Self::InvalidSplitProbability {
                model_id,
                node,
                ancestor,
                left,
                right,
                weight,
                probability,
            } => write!(
                f,
                "model {model_id:?} split {ancestor}->{left}/{right} at node {node} has invalid scenario weight/probability {weight}/{probability}"
            ),
            Self::DuplicateSplitScenario {
                model_id,
                node,
                ancestor,
                left,
                right,
            } => write!(
                f,
                "model {model_id:?} repeats split {ancestor}->{left}/{right} at node {node}"
            ),
            Self::SplitProbabilitySum {
                model_id,
                node,
                value,
            } => write!(
                f,
                "model {model_id:?} split probabilities at node {node} sum to {value}, not 1"
            ),
            Self::UnsupportedSplitScenario {
                node,
                ancestor,
                left,
                right,
            } => write!(
                f,
                "split at node {node} with range bits {ancestor}->{left}/{right} has no supported BioGeoBEARS event classification"
            ),
            Self::InvalidCriterionWeight {
                criterion,
                model_id,
                value,
                delta,
                weight,
            } => write!(
                f,
                "model {model_id:?} has invalid {} values: criterion={value}, delta={delta}, weight={weight}",
                criterion.as_str()
            ),
            Self::CriterionWeightSum { criterion, value } => write!(
                f,
                "{} model weights sum to {value}, not 1",
                criterion.as_str()
            ),
            Self::AveragedProbabilitySum {
                criterion,
                node,
                value,
            } => write!(
                f,
                "{} model-averaged probabilities at node {node} sum to {value}, not 1",
                criterion.as_str()
            ),
            Self::AveragedSplitProbabilitySum {
                criterion,
                node,
                value,
            } => write!(
                f,
                "{} model-averaged split probabilities at node {node} sum to {value}, not 1",
                criterion.as_str()
            ),
        }
    }
}

impl Error for ModelAverageError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn averages_posteriors_by_aic_and_aicc_weights() {
        let mut accumulator = ModelAverageAccumulator::new(
            2,
            3,
            vec![NodeMetadata {
                node: 2,
                label: "root".to_string(),
                kind: "root",
                clade: "A+B".to_string(),
            }],
            vec![SplitNodeMetadata {
                node: 2,
                left_node: 0,
                right_node: 1,
                left_clade: "A".to_string(),
                right_clade: "B".to_string(),
            }],
            vec![
                AreaMetadata {
                    area_index: 0,
                    area_bit: 1,
                    area: "AreaA".to_string(),
                },
                AreaMetadata {
                    area_index: 1,
                    area_bit: 2,
                    area: "AreaB".to_string(),
                },
            ],
            vec![
                StateMetadata {
                    state_index: 0,
                    range_bits: 1,
                    range: "AreaA".to_string(),
                },
                StateMetadata {
                    state_index: 1,
                    range_bits: 2,
                    range: "AreaB".to_string(),
                },
            ],
        )
        .unwrap();
        accumulator
            .add_model(
                model("DEC", 0.25, 0.4),
                &posteriors(&[[1.0, 0.0], [0.0, 1.0], [0.8, 0.2]]),
                &[split(2, 0, 0, 0, 1.0)],
            )
            .unwrap();
        accumulator
            .add_model(
                model("DEC+J", 0.75, 0.6),
                &posteriors(&[[1.0, 0.0], [0.0, 1.0], [0.2, 0.8]]),
                &[split(2, 1, 1, 1, 1.0)],
            )
            .unwrap();

        let output = accumulator.finish().unwrap();
        assert_probability(&output, "AIC", 0, 0.35);
        assert_probability(&output, "AICc", 0, 0.44);
        assert!(output.contains("aic_models\t2\n"));
        assert!(output.contains("aicc_models\t2\n"));
        assert!(
            output.contains(
                "missing_split_scenario_semantics\tzero_probability_before_weighted_sum\n"
            )
        );
        assert_split_probability(&output, "AIC", 0, 0.25);
        assert_split_probability(&output, "AIC", 1, 0.75);
        assert_split_probability(&output, "AICc", 0, 0.4);
        assert_split_probability(&output, "AICc", 1, 0.6);
    }

    #[test]
    fn rejects_non_normalized_input_posteriors() {
        let mut accumulator = ModelAverageAccumulator::new(
            1,
            1,
            vec![NodeMetadata {
                node: 0,
                label: "root".to_string(),
                kind: "root",
                clade: "A".to_string(),
            }],
            vec![],
            vec![AreaMetadata {
                area_index: 0,
                area_bit: 1,
                area: "AreaA".to_string(),
            }],
            vec![StateMetadata {
                state_index: 0,
                range_bits: 1,
                range: "AreaA".to_string(),
            }],
        )
        .unwrap();
        let error = accumulator
            .add_model(
                model("bad", 1.0, 1.0),
                &[NodeStatePosterior {
                    node: 0,
                    probabilities: vec![0.5],
                }],
                &[],
            )
            .unwrap_err();
        assert!(matches!(
            error,
            ModelAverageError::PosteriorProbabilitySum { .. }
        ));
    }

    fn model(model_id: &str, aic_weight: f64, aicc_weight: f64) -> WeightedModel {
        WeightedModel {
            model_id: model_id.to_string(),
            analysis_result: format!("models/{model_id}"),
            log_likelihood: -10.0,
            aic: Some(CriterionWeight {
                value: 24.0,
                delta: 0.0,
                weight: aic_weight,
            }),
            aicc: Some(CriterionWeight {
                value: 25.0,
                delta: 0.0,
                weight: aicc_weight,
            }),
        }
    }

    fn posteriors(values: &[[f64; 2]]) -> Vec<NodeStatePosterior> {
        values
            .iter()
            .enumerate()
            .map(|(node, probabilities)| NodeStatePosterior {
                node,
                probabilities: probabilities.to_vec(),
            })
            .collect()
    }

    fn split(
        node: usize,
        ancestor: usize,
        left: usize,
        right: usize,
        probability: f64,
    ) -> SplitScenarioPosterior {
        SplitScenarioPosterior {
            node,
            ancestor,
            left,
            right,
            weight: 1.0,
            probability,
        }
    }

    fn assert_probability(output: &str, criterion: &str, state: usize, expected: f64) {
        let prefix = format!("{criterion}\t2\t{state}\t");
        let row = output
            .lines()
            .find(|line| line.starts_with(&prefix))
            .expect("expected model-average probability row");
        let actual = row.split('\t').next_back().unwrap().parse::<f64>().unwrap();
        assert!((actual - expected).abs() < 1e-15, "{actual} != {expected}");
    }

    fn assert_split_probability(output: &str, criterion: &str, scenario: usize, expected: f64) {
        let prefix = format!("{criterion}\t{scenario}\t");
        let row = output
            .lines()
            .rev()
            .find(|line| line.starts_with(&prefix))
            .expect("expected model-average split probability row");
        let actual = row.split('\t').next_back().unwrap().parse::<f64>().unwrap();
        assert!((actual - expected).abs() < 1e-15, "{actual} != {expected}");
    }
}
