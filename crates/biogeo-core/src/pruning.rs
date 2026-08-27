use std::error::Error;
use std::fmt;

use crate::branch_process::{BranchPropagator, HomogeneousBranchPropagator};
use crate::cladogenesis::{
    CladogenesisError, CladogeneticProcess, CladogeneticTable, HomogeneousCladogeneticProcess,
};
use crate::constraints::StateMask;
use crate::propagation::{PropagationError, propagate_uniformized};
use crate::q::SparseQ;
use crate::tree::Tree;

#[derive(Clone, Debug, PartialEq)]
pub struct TipLikelihood {
    pub node: usize,
    pub likelihoods: Vec<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RootPrior<'a> {
    Equal,
    Flat,
    Given(&'a [f64]),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PruningResult {
    pub log_likelihood: f64,
    pub root_likelihoods: Vec<f64>,
    pub scaled_likelihoods: Vec<Vec<f64>>,
    pub scale_factors: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NodeStatePosterior {
    pub node: usize,
    pub probabilities: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SplitScenarioPosterior {
    pub node: usize,
    pub ancestor: usize,
    pub left: usize,
    pub right: usize,
    pub weight: f64,
    pub probability: f64,
}

pub fn prune_fixed_q(
    tree: &Tree,
    q: &SparseQ,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, PruningError> {
    let state_count = q.size();
    if state_count == 0 {
        return Err(PruningError::ZeroStates);
    }

    let mut scaled_likelihoods = vec![vec![0.0; state_count]; tree.node_count()];
    let mut has_likelihood = vec![false; tree.node_count()];
    let mut scale_factors = vec![1.0; tree.node_count()];

    load_tip_likelihoods(
        tree,
        state_count,
        tip_likelihoods,
        &mut scaled_likelihoods,
        &mut has_likelihood,
    )?;

    for node in tree.postorder_internal_nodes() {
        let children = tree
            .children(*node)
            .expect("postorder internal node should be in tree");
        let mut node_likelihood = vec![1.0; state_count];

        for child in children {
            if !has_likelihood[child.node] {
                return Err(PruningError::MissingChildLikelihood {
                    node: *node,
                    child: child.node,
                });
            }

            let propagated =
                propagate_uniformized(q, child.length, &scaled_likelihoods[child.node])?;
            for (node_value, propagated_value) in node_likelihood.iter_mut().zip(propagated) {
                *node_value *= propagated_value;
            }
        }

        let scale = checked_sum(*node, &node_likelihood)?;
        for value in &mut node_likelihood {
            *value /= scale;
        }

        scale_factors[*node] = scale;
        scaled_likelihoods[*node] = node_likelihood;
        has_likelihood[*node] = true;
    }

    let root = tree.root();
    if !has_likelihood[root] {
        return Err(PruningError::MissingRootLikelihood { root });
    }

    let prior = resolve_root_prior(root_prior, state_count, None)?;
    let root_likelihood = dot(&scaled_likelihoods[root], &prior);
    if !root_likelihood.is_finite() || root_likelihood <= 0.0 {
        return Err(PruningError::NonPositiveRootLikelihood {
            value: root_likelihood,
        });
    }

    let log_scale_sum: f64 = scale_factors
        .iter()
        .filter(|scale| **scale != 1.0)
        .map(|scale| scale.ln())
        .sum();

    Ok(PruningResult {
        log_likelihood: log_scale_sum + root_likelihood.ln(),
        root_likelihoods: scaled_likelihoods[root].clone(),
        scaled_likelihoods,
        scale_factors,
    })
}

pub fn prune_with_cladogenesis(
    tree: &Tree,
    q: &SparseQ,
    cladogenesis: &CladogeneticTable,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, PruningError> {
    let propagator = HomogeneousBranchPropagator::new(q);
    let process = HomogeneousCladogeneticProcess::new(cladogenesis);
    prune_with_cladogenesis_by_branch(tree, &propagator, &process, tip_likelihoods, root_prior)
}

pub(crate) fn prune_with_cladogenesis_by_branch(
    tree: &Tree,
    propagator: &dyn BranchPropagator,
    cladogenesis: &dyn CladogeneticProcess,
    tip_likelihoods: &[TipLikelihood],
    root_prior: RootPrior<'_>,
) -> Result<PruningResult, PruningError> {
    let state_count = propagator.state_count();
    if state_count == 0 {
        return Err(PruningError::ZeroStates);
    }
    if cladogenesis.state_count() != state_count {
        return Err(PruningError::CladogenesisStateCountMismatch {
            q_states: state_count,
            cladogenesis_states: cladogenesis.state_count(),
        });
    }

    let mut scaled_likelihoods = vec![vec![0.0; state_count]; tree.node_count()];
    let mut has_likelihood = vec![false; tree.node_count()];
    let mut scale_factors = vec![1.0; tree.node_count()];

    load_tip_likelihoods(
        tree,
        state_count,
        tip_likelihoods,
        &mut scaled_likelihoods,
        &mut has_likelihood,
    )?;

    for node in tree.postorder_internal_nodes() {
        let children = tree
            .children(*node)
            .expect("postorder internal node should be in tree");

        if children.len() != 2 {
            return Err(PruningError::NonBinaryCladogenesisNode {
                node: *node,
                child_count: children.len(),
            });
        }

        let left_child = children[0];
        let right_child = children[1];
        if !has_likelihood[left_child.node] {
            return Err(PruningError::MissingChildLikelihood {
                node: *node,
                child: left_child.node,
            });
        }
        if !has_likelihood[right_child.node] {
            return Err(PruningError::MissingChildLikelihood {
                node: *node,
                child: right_child.node,
            });
        }

        let left_likelihood = propagator.propagate(
            left_child.edge_index,
            left_child.length,
            &scaled_likelihoods[left_child.node],
        )?;
        let right_likelihood = propagator.propagate(
            right_child.edge_index,
            right_child.length,
            &scaled_likelihoods[right_child.node],
        )?;
        let mut node_likelihood = if tree.is_direct_ancestor_node(*node) {
            left_likelihood
                .iter()
                .zip(&right_likelihood)
                .map(|(left, right)| left * right)
                .collect()
        } else {
            cladogenesis
                .table_for_node(*node)
                .combine(&left_likelihood, &right_likelihood)?
        };

        let scale = checked_sum(*node, &node_likelihood)?;
        for value in &mut node_likelihood {
            *value /= scale;
        }

        scale_factors[*node] = scale;
        scaled_likelihoods[*node] = node_likelihood;
        has_likelihood[*node] = true;
    }

    finish_pruning(
        tree,
        root_prior,
        &scaled_likelihoods,
        &scale_factors,
        &has_likelihood,
        cladogenesis.state_mask_for_node(tree.root()),
    )
}

pub fn cladogenetic_node_state_posteriors(
    tree: &Tree,
    q: &SparseQ,
    cladogenesis: &CladogeneticTable,
    pruning: &PruningResult,
    root_prior: RootPrior<'_>,
) -> Result<Vec<NodeStatePosterior>, PruningError> {
    let propagator = HomogeneousBranchPropagator::new(q);
    let process = HomogeneousCladogeneticProcess::new(cladogenesis);
    cladogenetic_node_state_posteriors_by_branch(tree, &propagator, &process, pruning, root_prior)
}

pub(crate) fn cladogenetic_node_state_posteriors_by_branch(
    tree: &Tree,
    propagator: &dyn BranchPropagator,
    cladogenesis: &dyn CladogeneticProcess,
    pruning: &PruningResult,
    root_prior: RootPrior<'_>,
) -> Result<Vec<NodeStatePosterior>, PruningError> {
    let downpass = cladogenetic_downpass(tree, propagator, cladogenesis, pruning, root_prior)?;

    let mut posteriors = Vec::with_capacity(tree.node_count());
    for (node, outside) in downpass.outside_likelihoods.iter().enumerate() {
        let mut probabilities: Vec<f64> = outside
            .iter()
            .zip(&pruning.scaled_likelihoods[node])
            .map(|(outside_value, subtree_value)| outside_value * subtree_value)
            .collect();
        normalize_positive_vector(node, &mut probabilities)?;
        posteriors.push(NodeStatePosterior {
            node,
            probabilities,
        });
    }

    Ok(posteriors)
}

pub fn cladogenetic_split_scenario_posteriors(
    tree: &Tree,
    q: &SparseQ,
    cladogenesis: &CladogeneticTable,
    pruning: &PruningResult,
    root_prior: RootPrior<'_>,
) -> Result<Vec<SplitScenarioPosterior>, PruningError> {
    let propagator = HomogeneousBranchPropagator::new(q);
    let process = HomogeneousCladogeneticProcess::new(cladogenesis);
    cladogenetic_split_scenario_posteriors_by_branch(
        tree,
        &propagator,
        &process,
        pruning,
        root_prior,
    )
}

pub(crate) fn cladogenetic_split_scenario_posteriors_by_branch(
    tree: &Tree,
    propagator: &dyn BranchPropagator,
    cladogenesis: &dyn CladogeneticProcess,
    pruning: &PruningResult,
    root_prior: RootPrior<'_>,
) -> Result<Vec<SplitScenarioPosterior>, PruningError> {
    let downpass = cladogenetic_downpass(tree, propagator, cladogenesis, pruning, root_prior)?;
    let mut posteriors = Vec::new();

    for node in tree.postorder_internal_nodes() {
        if tree.is_direct_ancestor_node(*node) {
            continue;
        }
        let children = tree
            .children(*node)
            .expect("postorder internal node should be in tree");
        if children.len() != 2 {
            return Err(PruningError::NonBinaryCladogenesisNode {
                node: *node,
                child_count: children.len(),
            });
        }

        let left_child = children[0];
        let right_child = children[1];
        let mut node_rows = Vec::new();

        for scenarios in cladogenesis.table_for_node(*node).rows() {
            for scenario in scenarios {
                let probability = downpass.outside_likelihoods[*node][scenario.ancestor]
                    * scenario.weight
                    * downpass.branch_likelihoods[left_child.edge_index][scenario.left]
                    * downpass.branch_likelihoods[right_child.edge_index][scenario.right];
                if !probability.is_finite() {
                    return Err(PruningError::NonFiniteLikelihood {
                        node: *node,
                        state: scenario.ancestor,
                        value: probability,
                    });
                }
                if probability < 0.0 {
                    return Err(PruningError::NegativePosteriorMass {
                        node: *node,
                        state: scenario.ancestor,
                        value: probability,
                    });
                }

                node_rows.push(SplitScenarioPosterior {
                    node: *node,
                    ancestor: scenario.ancestor,
                    left: scenario.left,
                    right: scenario.right,
                    weight: scenario.weight,
                    probability,
                });
            }
        }

        let sum: f64 = node_rows.iter().map(|row| row.probability).sum();
        if !sum.is_finite() || sum <= 0.0 {
            return Err(PruningError::NonPositivePosteriorMass {
                node: *node,
                value: sum,
            });
        }

        for row in &mut node_rows {
            row.probability /= sum;
        }
        posteriors.extend(node_rows);
    }

    Ok(posteriors)
}

#[derive(Clone, Debug, PartialEq)]
struct CladogeneticDownpass {
    outside_likelihoods: Vec<Vec<f64>>,
    branch_likelihoods: Vec<Vec<f64>>,
}

fn cladogenetic_downpass(
    tree: &Tree,
    propagator: &dyn BranchPropagator,
    cladogenesis: &dyn CladogeneticProcess,
    pruning: &PruningResult,
    root_prior: RootPrior<'_>,
) -> Result<CladogeneticDownpass, PruningError> {
    let state_count = propagator.state_count();
    if state_count == 0 {
        return Err(PruningError::ZeroStates);
    }
    if cladogenesis.state_count() != state_count {
        return Err(PruningError::CladogenesisStateCountMismatch {
            q_states: state_count,
            cladogenesis_states: cladogenesis.state_count(),
        });
    }
    validate_pruning_result_dimensions(tree, state_count, pruning)?;

    let branch_likelihoods = propagated_branch_likelihoods(tree, propagator, pruning)?;
    let mut outside_likelihoods = vec![vec![0.0; state_count]; tree.node_count()];
    outside_likelihoods[tree.root()] = resolve_root_prior(
        root_prior,
        state_count,
        cladogenesis.state_mask_for_node(tree.root()),
    )?;
    normalize_positive_vector(tree.root(), &mut outside_likelihoods[tree.root()])?;

    let mut stack = vec![tree.root()];
    while let Some(parent) = stack.pop() {
        let children = tree
            .children(parent)
            .expect("tree traversal should only produce valid nodes");
        if children.is_empty() {
            continue;
        }
        if children.len() != 2 {
            return Err(PruningError::NonBinaryCladogenesisNode {
                node: parent,
                child_count: children.len(),
            });
        }

        let left_child = children[0];
        let right_child = children[1];
        let mut left_start_outside = vec![0.0; state_count];
        let mut right_start_outside = vec![0.0; state_count];

        if tree.is_direct_ancestor_node(parent) {
            for state in 0..state_count {
                left_start_outside[state] = outside_likelihoods[parent][state]
                    * branch_likelihoods[right_child.edge_index][state];
                right_start_outside[state] = outside_likelihoods[parent][state]
                    * branch_likelihoods[left_child.edge_index][state];
            }
        } else {
            for (ancestor, scenarios) in cladogenesis
                .table_for_node(parent)
                .rows()
                .iter()
                .enumerate()
            {
                let ancestor_outside = outside_likelihoods[parent][ancestor];
                if ancestor_outside == 0.0 {
                    continue;
                }

                for scenario in scenarios {
                    let weighted = ancestor_outside * scenario.weight;
                    left_start_outside[scenario.left] +=
                        weighted * branch_likelihoods[right_child.edge_index][scenario.right];
                    right_start_outside[scenario.right] +=
                        weighted * branch_likelihoods[left_child.edge_index][scenario.left];
                }
            }
        }

        outside_likelihoods[left_child.node] = propagator.propagate_transpose(
            left_child.edge_index,
            left_child.length,
            &left_start_outside,
        )?;
        outside_likelihoods[right_child.node] = propagator.propagate_transpose(
            right_child.edge_index,
            right_child.length,
            &right_start_outside,
        )?;
        normalize_positive_vector(left_child.node, &mut outside_likelihoods[left_child.node])?;
        normalize_positive_vector(right_child.node, &mut outside_likelihoods[right_child.node])?;

        stack.push(right_child.node);
        stack.push(left_child.node);
    }

    Ok(CladogeneticDownpass {
        outside_likelihoods,
        branch_likelihoods,
    })
}

fn load_tip_likelihoods(
    tree: &Tree,
    state_count: usize,
    tip_likelihoods: &[TipLikelihood],
    scaled_likelihoods: &mut [Vec<f64>],
    has_likelihood: &mut [bool],
) -> Result<(), PruningError> {
    for tip in tip_likelihoods {
        if tip.node >= tree.node_count() {
            return Err(PruningError::NodeOutOfBounds {
                node: tip.node,
                node_count: tree.node_count(),
            });
        }
        if !tree.is_tip(tip.node) {
            return Err(PruningError::LikelihoodForInternalNode { node: tip.node });
        }
        if has_likelihood[tip.node] {
            return Err(PruningError::DuplicateTipLikelihood { node: tip.node });
        }
        validate_vector(tip.node, state_count, &tip.likelihoods)?;

        scaled_likelihoods[tip.node] = tip.likelihoods.clone();
        has_likelihood[tip.node] = true;
    }

    for tip_node in tree.tip_nodes() {
        if !has_likelihood[*tip_node] {
            return Err(PruningError::MissingTipLikelihood { node: *tip_node });
        }
    }

    Ok(())
}

fn validate_vector(node: usize, state_count: usize, values: &[f64]) -> Result<(), PruningError> {
    if values.len() != state_count {
        return Err(PruningError::StateCountMismatch {
            expected: state_count,
            actual: values.len(),
        });
    }
    for (state, value) in values.iter().enumerate() {
        if !value.is_finite() {
            return Err(PruningError::NonFiniteLikelihood {
                node,
                state,
                value: *value,
            });
        }
        if *value < 0.0 {
            return Err(PruningError::NegativeLikelihood {
                node,
                state,
                value: *value,
            });
        }
    }

    Ok(())
}

pub(crate) fn validate_pruning_result_dimensions(
    tree: &Tree,
    state_count: usize,
    pruning: &PruningResult,
) -> Result<(), PruningError> {
    if pruning.scaled_likelihoods.len() != tree.node_count() {
        return Err(PruningError::PruningResultNodeCountMismatch {
            expected: tree.node_count(),
            actual: pruning.scaled_likelihoods.len(),
        });
    }
    if pruning.scale_factors.len() != tree.node_count() {
        return Err(PruningError::PruningResultNodeCountMismatch {
            expected: tree.node_count(),
            actual: pruning.scale_factors.len(),
        });
    }

    for (node, likelihoods) in pruning.scaled_likelihoods.iter().enumerate() {
        validate_vector(node, state_count, likelihoods)?;
    }

    Ok(())
}

pub(crate) fn propagated_branch_likelihoods(
    tree: &Tree,
    propagator: &dyn BranchPropagator,
    pruning: &PruningResult,
) -> Result<Vec<Vec<f64>>, PruningError> {
    tree.edges()
        .iter()
        .enumerate()
        .map(|(edge_index, edge)| {
            propagator.propagate(
                edge_index,
                edge.length,
                &pruning.scaled_likelihoods[edge.child],
            )
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(PruningError::from)
}

fn normalize_positive_vector(node: usize, values: &mut [f64]) -> Result<(), PruningError> {
    let mut sum = 0.0;
    for (state, value) in values.iter_mut().enumerate() {
        if !value.is_finite() {
            return Err(PruningError::NonFiniteLikelihood {
                node,
                state,
                value: *value,
            });
        }
        if *value < 0.0 {
            return Err(PruningError::NegativePosteriorMass {
                node,
                state,
                value: *value,
            });
        }
        sum += *value;
    }

    if !sum.is_finite() || sum <= 0.0 {
        return Err(PruningError::NonPositivePosteriorMass { node, value: sum });
    }

    for value in values {
        *value /= sum;
    }

    Ok(())
}

fn checked_sum(node: usize, values: &[f64]) -> Result<f64, PruningError> {
    let sum: f64 = values.iter().sum();
    if !sum.is_finite() || sum <= 0.0 {
        return Err(PruningError::NonPositiveNodeLikelihood { node, value: sum });
    }

    Ok(sum)
}

pub(crate) fn resolve_root_prior(
    root_prior: RootPrior<'_>,
    state_count: usize,
    state_mask: Option<&StateMask>,
) -> Result<Vec<f64>, PruningError> {
    let mut prior = match root_prior {
        RootPrior::Equal => {
            let denominator = state_mask.map_or(state_count, StateMask::allowed_count);
            vec![1.0 / denominator as f64; state_count]
        }
        RootPrior::Flat => vec![1.0; state_count],
        RootPrior::Given(values) => {
            if values.len() != state_count {
                return Err(PruningError::RootPriorLengthMismatch {
                    expected: state_count,
                    actual: values.len(),
                });
            }

            let mut sum = 0.0;
            for (state, value) in values.iter().enumerate() {
                if !value.is_finite() {
                    return Err(PruningError::NonFiniteRootPrior {
                        state,
                        value: *value,
                    });
                }
                if *value < 0.0 {
                    return Err(PruningError::NegativeRootPrior {
                        state,
                        value: *value,
                    });
                }
                sum += value;
            }
            if sum <= 0.0 {
                return Err(PruningError::ZeroRootPriorMass);
            }

            values.to_vec()
        }
    };

    if let Some(mask) = state_mask {
        for (value, allowed) in prior.iter_mut().zip(mask.values()) {
            if !allowed {
                *value = 0.0;
            }
        }
        if prior.iter().sum::<f64>() <= 0.0 {
            return Err(PruningError::ZeroRootPriorMass);
        }
    }

    Ok(prior)
}

fn dot(left: &[f64], right: &[f64]) -> f64 {
    left.iter()
        .zip(right)
        .map(|(left, right)| left * right)
        .sum()
}

fn finish_pruning(
    tree: &Tree,
    root_prior: RootPrior<'_>,
    scaled_likelihoods: &[Vec<f64>],
    scale_factors: &[f64],
    has_likelihood: &[bool],
    root_state_mask: Option<&StateMask>,
) -> Result<PruningResult, PruningError> {
    let root = tree.root();
    if !has_likelihood[root] {
        return Err(PruningError::MissingRootLikelihood { root });
    }

    let state_count = scaled_likelihoods[root].len();
    let prior = resolve_root_prior(root_prior, state_count, root_state_mask)?;
    let root_likelihood = dot(&scaled_likelihoods[root], &prior);
    if !root_likelihood.is_finite() || root_likelihood <= 0.0 {
        return Err(PruningError::NonPositiveRootLikelihood {
            value: root_likelihood,
        });
    }

    let log_scale_sum: f64 = scale_factors
        .iter()
        .filter(|scale| **scale != 1.0)
        .map(|scale| scale.ln())
        .sum();

    Ok(PruningResult {
        log_likelihood: log_scale_sum + root_likelihood.ln(),
        root_likelihoods: scaled_likelihoods[root].clone(),
        scaled_likelihoods: scaled_likelihoods.to_vec(),
        scale_factors: scale_factors.to_vec(),
    })
}

#[derive(Clone, Debug, PartialEq)]
pub enum PruningError {
    ZeroStates,
    NodeOutOfBounds {
        node: usize,
        node_count: usize,
    },
    LikelihoodForInternalNode {
        node: usize,
    },
    DuplicateTipLikelihood {
        node: usize,
    },
    MissingTipLikelihood {
        node: usize,
    },
    TipLikelihoodsExcludedByStateConstraint {
        violations: Vec<(usize, usize)>,
    },
    MissingChildLikelihood {
        node: usize,
        child: usize,
    },
    NonBinaryCladogenesisNode {
        node: usize,
        child_count: usize,
    },
    MissingRootLikelihood {
        root: usize,
    },
    CladogenesisStateCountMismatch {
        q_states: usize,
        cladogenesis_states: usize,
    },
    StateCountMismatch {
        expected: usize,
        actual: usize,
    },
    PruningResultNodeCountMismatch {
        expected: usize,
        actual: usize,
    },
    NonFiniteLikelihood {
        node: usize,
        state: usize,
        value: f64,
    },
    NegativeLikelihood {
        node: usize,
        state: usize,
        value: f64,
    },
    NegativePosteriorMass {
        node: usize,
        state: usize,
        value: f64,
    },
    NonPositiveNodeLikelihood {
        node: usize,
        value: f64,
    },
    RootPriorLengthMismatch {
        expected: usize,
        actual: usize,
    },
    NonFiniteRootPrior {
        state: usize,
        value: f64,
    },
    NegativeRootPrior {
        state: usize,
        value: f64,
    },
    ZeroRootPriorMass,
    NonPositiveRootLikelihood {
        value: f64,
    },
    NonPositivePosteriorMass {
        node: usize,
        value: f64,
    },
    Propagation(PropagationError),
    Cladogenesis(CladogenesisError),
}

impl fmt::Display for PruningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroStates => write!(f, "pruning requires at least one state"),
            Self::NodeOutOfBounds { node, node_count } => {
                write!(
                    f,
                    "likelihood node {node} is out of bounds for {node_count} nodes"
                )
            }
            Self::LikelihoodForInternalNode { node } => {
                write!(
                    f,
                    "node {node} is internal, but was given as a tip likelihood"
                )
            }
            Self::DuplicateTipLikelihood { node } => {
                write!(f, "duplicate tip likelihood for node {node}")
            }
            Self::MissingTipLikelihood { node } => {
                write!(f, "missing tip likelihood for node {node}")
            }
            Self::TipLikelihoodsExcludedByStateConstraint { violations } => {
                let details = violations
                    .iter()
                    .map(|(node, stratum)| format!("node {node} in stratum {}", stratum + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "{} tip likelihood(s) have no positive mass in ranges allowed by the state constraint at their sampling ages: {details}",
                    violations.len()
                )
            }
            Self::MissingChildLikelihood { node, child } => {
                write!(
                    f,
                    "node {node} is missing child likelihood for child {child}"
                )
            }
            Self::NonBinaryCladogenesisNode { node, child_count } => write!(
                f,
                "cladogenesis pruning requires binary internal nodes; node {node} has {child_count} children"
            ),
            Self::MissingRootLikelihood { root } => {
                write!(f, "missing root likelihood for root node {root}")
            }
            Self::CladogenesisStateCountMismatch {
                q_states,
                cladogenesis_states,
            } => write!(
                f,
                "cladogenesis table state count mismatch: Q has {q_states}, table has {cladogenesis_states}"
            ),
            Self::StateCountMismatch { expected, actual } => {
                write!(
                    f,
                    "likelihood vector state count mismatch: expected {expected}, got {actual}"
                )
            }
            Self::PruningResultNodeCountMismatch { expected, actual } => write!(
                f,
                "pruning result node count mismatch: expected {expected}, got {actual}"
            ),
            Self::NonFiniteLikelihood { node, state, value } => write!(
                f,
                "likelihood for node {node}, state {state} must be finite, got {value}"
            ),
            Self::NegativeLikelihood { node, state, value } => write!(
                f,
                "likelihood for node {node}, state {state} must be non-negative, got {value}"
            ),
            Self::NegativePosteriorMass { node, state, value } => write!(
                f,
                "posterior mass at node {node}, state {state} must be non-negative, got {value}"
            ),
            Self::NonPositiveNodeLikelihood { node, value } => write!(
                f,
                "combined likelihood at node {node} must be positive, got {value}"
            ),
            Self::RootPriorLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "root prior length mismatch: expected {expected}, got {actual}"
                )
            }
            Self::NonFiniteRootPrior { state, value } => {
                write!(
                    f,
                    "root prior for state {state} must be finite, got {value}"
                )
            }
            Self::NegativeRootPrior { state, value } => write!(
                f,
                "root prior for state {state} must be non-negative, got {value}"
            ),
            Self::ZeroRootPriorMass => write!(f, "root prior must have positive total mass"),
            Self::NonPositiveRootLikelihood { value } => {
                write!(f, "root likelihood must be positive, got {value}")
            }
            Self::NonPositivePosteriorMass { node, value } => write!(
                f,
                "posterior mass at node {node} must be positive, got {value}"
            ),
            Self::Propagation(error) => write!(f, "branch propagation failed: {error}"),
            Self::Cladogenesis(error) => write!(f, "cladogenesis failed: {error}"),
        }
    }
}

impl Error for PruningError {}

impl From<PropagationError> for PruningError {
    fn from(value: PropagationError) -> Self {
        Self::Propagation(value)
    }
}

impl From<CladogenesisError> for PruningError {
    fn from(value: CladogenesisError) -> Self {
        Self::Cladogenesis(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cladogenesis::DecCladogeneticModel;
    use crate::q::{RateTransition, SparseQ};
    use crate::state::StateSpace;
    use crate::tree::{Edge, Tree};

    fn assert_close(left: f64, right: f64, tolerance: f64) {
        assert!(
            (left - right).abs() < tolerance,
            "values differ: left={left}, right={right}"
        );
    }

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
    fn zero_q_identical_tip_states_reduce_to_root_prior() {
        let tree = two_tip_tree(1.0, 1.0);
        let q = SparseQ::new(2, Vec::new());
        let result = prune_fixed_q(
            &tree,
            &q,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![1.0, 0.0],
                },
            ],
            RootPrior::Equal,
        )
        .unwrap();

        assert_close(result.log_likelihood, 0.5_f64.ln(), 1e-12);
        assert_close_slice(&result.root_likelihoods, &[1.0, 0.0], 1e-12);
        assert_eq!(result.scale_factors[2], 1.0);
    }

    #[test]
    fn symmetric_two_tip_tree_matches_closed_form() {
        let rate = 0.25;
        let branch_length = 2.0;
        let tree = two_tip_tree(branch_length, branch_length);
        let q = symmetric_two_state_q(rate);
        let result = prune_fixed_q(
            &tree,
            &q,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1.0],
                },
            ],
            RootPrior::Equal,
        )
        .unwrap();

        let decay = (-2.0_f64 * rate * branch_length).exp();
        let p_same = 0.5 + 0.5 * decay;
        let p_change = 0.5 - 0.5 * decay;
        let expected_likelihood = p_same * p_change;

        assert_close(result.log_likelihood, expected_likelihood.ln(), 1e-12);
        assert_close_slice(&result.root_likelihoods, &[0.5, 0.5], 1e-12);
    }

    #[test]
    fn flat_root_prior_uses_unnormalized_root_mass() {
        let tree = two_tip_tree(1.0, 1.0);
        let q = SparseQ::new(2, Vec::new());
        let result = prune_fixed_q(
            &tree,
            &q,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![1.0, 0.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(result.log_likelihood, 0.0, 1e-12);
    }

    #[test]
    fn incompatible_zero_q_tips_return_zero_node_likelihood_error() {
        let tree = two_tip_tree(0.0, 0.0);
        let q = SparseQ::new(2, Vec::new());
        let error = prune_fixed_q(
            &tree,
            &q,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1.0],
                },
            ],
            RootPrior::Equal,
        )
        .unwrap_err();

        assert_eq!(
            error,
            PruningError::NonPositiveNodeLikelihood {
                node: 2,
                value: 0.0
            }
        );
    }

    #[test]
    fn rejects_missing_tip_likelihood() {
        let tree = two_tip_tree(1.0, 1.0);
        let q = SparseQ::new(2, Vec::new());
        let error = prune_fixed_q(
            &tree,
            &q,
            &[TipLikelihood {
                node: 0,
                likelihoods: vec![1.0, 0.0],
            }],
            RootPrior::Equal,
        )
        .unwrap_err();

        assert_eq!(error, PruningError::MissingTipLikelihood { node: 1 });
    }

    #[test]
    fn dec_cladogenesis_pruning_maps_ab_root_to_a_and_b_tips() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = two_tip_tree(0.0, 0.0);
        let q = SparseQ::new(states.len(), Vec::new());
        let result = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1.0, 0.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(result.log_likelihood, (1.0_f64 / 6.0).ln(), 1e-12);
        assert_close_slice(&result.root_likelihoods, &[0.0, 0.0, 1.0], 1e-12);
    }

    #[test]
    fn dec_cladogenesis_pruning_maps_single_area_root_to_matching_tips() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = two_tip_tree(0.0, 0.0);
        let q = SparseQ::new(states.len(), Vec::new());
        let result = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![1.0, 0.0, 0.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();

        assert_close(result.log_likelihood, 0.0, 1e-12);
        assert_close_slice(&result.root_likelihoods, &[1.0, 0.0, 0.0], 1e-12);
    }

    #[test]
    fn direct_ancestor_node_uses_identity_instead_of_cladogenesis() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = two_tip_tree(1e-7, 1.0)
            .with_direct_ancestor_hooks_below(1e-6)
            .unwrap();
        let q = SparseQ::new(states.len(), Vec::new());
        let tip_likelihoods = [
            TipLikelihood {
                node: 0,
                likelihoods: vec![1.0, 0.0, 1.0],
            },
            TipLikelihood {
                node: 1,
                likelihoods: vec![1.0, 1.0, 0.0],
            },
        ];

        let direct =
            prune_with_cladogenesis(&tree, &q, &cladogenesis, &tip_likelihoods, RootPrior::Flat)
                .unwrap();
        let identity = prune_fixed_q(&tree, &q, &tip_likelihoods, RootPrior::Flat).unwrap();

        assert_close(direct.log_likelihood, identity.log_likelihood, 1e-12);
        assert_close_slice(&direct.root_likelihoods, &identity.root_likelihoods, 1e-12);
        assert_close_slice(&direct.root_likelihoods, &[1.0, 0.0, 0.0], 1e-12);

        let node_posteriors =
            cladogenetic_node_state_posteriors(&tree, &q, &cladogenesis, &direct, RootPrior::Flat)
                .unwrap();
        for posterior in node_posteriors {
            assert_close_slice(&posterior.probabilities, &[1.0, 0.0, 0.0], 1e-12);
        }

        let split_posteriors = cladogenetic_split_scenario_posteriors(
            &tree,
            &q,
            &cladogenesis,
            &direct,
            RootPrior::Flat,
        )
        .unwrap();
        assert!(split_posteriors.is_empty());
    }

    #[test]
    fn cladogenetic_node_state_posteriors_map_zero_branch_root_state() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = two_tip_tree(0.0, 0.0);
        let q = SparseQ::new(states.len(), Vec::new());
        let result = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1.0, 0.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();
        let posteriors =
            cladogenetic_node_state_posteriors(&tree, &q, &cladogenesis, &result, RootPrior::Flat)
                .unwrap();

        assert_eq!(posteriors.len(), tree.node_count());
        assert_eq!(posteriors[2].node, 2);
        assert_close_slice(&posteriors[2].probabilities, &[0.0, 0.0, 1.0], 1e-12);
    }

    #[test]
    fn cladogenetic_split_scenario_posteriors_map_zero_branch_split() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = two_tip_tree(0.0, 0.0);
        let q = SparseQ::new(states.len(), Vec::new());
        let result = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1.0, 0.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();
        let posteriors = cladogenetic_split_scenario_posteriors(
            &tree,
            &q,
            &cladogenesis,
            &result,
            RootPrior::Flat,
        )
        .unwrap();

        let ab_to_a_b = posteriors
            .iter()
            .find(|posterior| {
                posterior.node == 2
                    && posterior.ancestor == 2
                    && posterior.left == 0
                    && posterior.right == 1
            })
            .unwrap();
        assert_close(ab_to_a_b.probability, 1.0, 1e-12);
        assert_close(
            posteriors
                .iter()
                .filter(|posterior| posterior.node == 2)
                .map(|posterior| posterior.probability)
                .sum(),
            1.0,
            1e-12,
        );
    }

    #[test]
    fn split_posteriors_are_invariant_to_small_tip_likelihood_scale() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = two_tip_tree(0.0, 0.0);
        let q = SparseQ::new(states.len(), Vec::new());
        let result = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1e-20, 0.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1e-20, 0.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();
        let posteriors = cladogenetic_split_scenario_posteriors(
            &tree,
            &q,
            &cladogenesis,
            &result,
            RootPrior::Flat,
        )
        .unwrap();

        let ab_to_a_b = posteriors
            .iter()
            .find(|posterior| {
                posterior.node == 2
                    && posterior.ancestor == 2
                    && posterior.left == 0
                    && posterior.right == 1
            })
            .unwrap();
        assert_close(ab_to_a_b.probability, 1.0, 1e-12);
    }

    #[test]
    fn cladogenetic_node_state_posteriors_include_internal_nodes() {
        let states = StateSpace::new(3, 3, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = Tree::new(
            4,
            5,
            vec![
                Edge {
                    parent: 4,
                    child: 2,
                    length: 0.0,
                },
                Edge {
                    parent: 4,
                    child: 3,
                    length: 0.0,
                },
                Edge {
                    parent: 2,
                    child: 0,
                    length: 0.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 0.0,
                },
            ],
        )
        .unwrap();
        let q = SparseQ::new(states.len(), Vec::new());
        let mut a = vec![0.0; states.len()];
        a[states
            .index_of(crate::state::AreaSet::from_bits(0b001))
            .unwrap()] = 1.0;
        let mut b = vec![0.0; states.len()];
        b[states
            .index_of(crate::state::AreaSet::from_bits(0b010))
            .unwrap()] = 1.0;
        let mut c = vec![0.0; states.len()];
        c[states
            .index_of(crate::state::AreaSet::from_bits(0b100))
            .unwrap()] = 1.0;
        let result = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: a,
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: b,
                },
                TipLikelihood {
                    node: 3,
                    likelihoods: c,
                },
            ],
            RootPrior::Flat,
        )
        .unwrap();
        let posteriors =
            cladogenetic_node_state_posteriors(&tree, &q, &cladogenesis, &result, RootPrior::Flat)
                .unwrap();

        let ab = states
            .index_of(crate::state::AreaSet::from_bits(0b011))
            .unwrap();
        let abc = states
            .index_of(crate::state::AreaSet::from_bits(0b111))
            .unwrap();
        assert_close(posteriors[2].probabilities[ab], 1.0, 1e-12);
        assert_close(posteriors[4].probabilities[abc], 1.0, 1e-12);
        for posterior in &posteriors {
            assert_close(posterior.probabilities.iter().sum(), 1.0, 1e-12);
        }
    }

    #[test]
    fn cladogenesis_pruning_rejects_non_binary_internal_nodes() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let cladogenesis = DecCladogeneticModel::new().build_table(&states).unwrap();
        let tree = Tree::new(
            3,
            4,
            vec![
                Edge {
                    parent: 3,
                    child: 0,
                    length: 0.0,
                },
                Edge {
                    parent: 3,
                    child: 1,
                    length: 0.0,
                },
                Edge {
                    parent: 3,
                    child: 2,
                    length: 0.0,
                },
            ],
        )
        .unwrap();
        let q = SparseQ::new(states.len(), Vec::new());
        let error = prune_with_cladogenesis(
            &tree,
            &q,
            &cladogenesis,
            &[
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 0.0, 0.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 1.0, 0.0],
                },
                TipLikelihood {
                    node: 2,
                    likelihoods: vec![0.0, 0.0, 1.0],
                },
            ],
            RootPrior::Flat,
        )
        .unwrap_err();

        assert_eq!(
            error,
            PruningError::NonBinaryCladogenesisNode {
                node: 3,
                child_count: 3
            }
        );
    }

    fn two_tip_tree(left_length: f64, right_length: f64) -> Tree {
        Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: left_length,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: right_length,
                },
            ],
        )
        .unwrap()
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
