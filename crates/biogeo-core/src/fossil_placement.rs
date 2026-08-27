use std::collections::HashSet;
use std::error::Error;
use std::fmt;

use rand::{Rng, RngExt, SeedableRng};
use rand_chacha::ChaCha12Rng;

use crate::newick::{InternalNodeLabel, ParsedNewickTree, TipLabel};
use crate::tree::{Edge, Tree, TreeError};

pub const FOSSIL_PLACEMENT_RNG_PROTOCOL: &str = "biogeo-fossil-placement-chacha12-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FossilAttachment {
    SideBranch,
    DirectAncestor,
}

impl FossilAttachment {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SideBranch => "side_branch",
            Self::DirectAncestor => "direct_ancestor",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CladePlacementScope {
    Stem,
    Crown,
    Both,
}

impl CladePlacementScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stem => "stem",
            Self::Crown => "crown",
            Self::Both => "both",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FossilPlacementSpec {
    pub fossil_label: String,
    pub min_age: f64,
    pub max_age: f64,
    pub attachment: FossilAttachment,
    pub scope: CladePlacementScope,
    pub clade_tip_labels: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FossilPlacementRecord {
    pub fossil_label: String,
    pub fossil_age: f64,
    pub attachment_age: f64,
    pub attachment: FossilAttachment,
    pub scope: CladePlacementScope,
    pub constrained_mrca: usize,
    pub selected_parent: usize,
    pub selected_child: usize,
    pub inserted_node: usize,
    pub fossil_node: usize,
    pub fossil_branch_length: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FossilPlacementResult {
    pub tree: ParsedNewickTree,
    pub records: Vec<FossilPlacementRecord>,
}

pub fn place_fossils_randomly(
    tree: &ParsedNewickTree,
    specs: &[FossilPlacementSpec],
    seed: u64,
    direct_ancestor_hook_length: f64,
) -> Result<FossilPlacementResult, FossilPlacementError> {
    let mut rng = ChaCha12Rng::seed_from_u64(seed);
    place_fossils_with_rng(tree, specs, direct_ancestor_hook_length, &mut rng)
}

pub fn place_fossils_with_rng<R: Rng + ?Sized>(
    tree: &ParsedNewickTree,
    specs: &[FossilPlacementSpec],
    direct_ancestor_hook_length: f64,
    rng: &mut R,
) -> Result<FossilPlacementResult, FossilPlacementError> {
    if !direct_ancestor_hook_length.is_finite() || direct_ancestor_hook_length <= 0.0 {
        return Err(FossilPlacementError::InvalidHookLength(
            direct_ancestor_hook_length,
        ));
    }
    validate_spec_labels(tree, specs)?;

    let mut placed_tree = tree.clone();
    let mut records = Vec::with_capacity(specs.len());
    for spec in specs {
        validate_spec(spec)?;
        let record = place_one(&mut placed_tree, spec, direct_ancestor_hook_length, rng)?;
        records.push(record);
    }
    Ok(FossilPlacementResult {
        tree: placed_tree,
        records,
    })
}

fn validate_spec_labels(
    tree: &ParsedNewickTree,
    specs: &[FossilPlacementSpec],
) -> Result<(), FossilPlacementError> {
    let mut labels = tree
        .tip_labels
        .iter()
        .map(|tip| tip.label.as_str())
        .collect::<HashSet<_>>();
    for spec in specs {
        if spec.fossil_label.is_empty() {
            return Err(FossilPlacementError::EmptyFossilLabel);
        }
        if !labels.insert(spec.fossil_label.as_str()) {
            return Err(FossilPlacementError::DuplicateTipLabel(
                spec.fossil_label.clone(),
            ));
        }
    }
    Ok(())
}

fn validate_spec(spec: &FossilPlacementSpec) -> Result<(), FossilPlacementError> {
    if !spec.min_age.is_finite()
        || !spec.max_age.is_finite()
        || spec.min_age < 0.0
        || spec.max_age < spec.min_age
    {
        return Err(FossilPlacementError::InvalidAgeInterval {
            fossil_label: spec.fossil_label.clone(),
            min_age: spec.min_age,
            max_age: spec.max_age,
        });
    }
    if spec.clade_tip_labels.is_empty() {
        return Err(FossilPlacementError::EmptyCladeConstraint(
            spec.fossil_label.clone(),
        ));
    }
    let mut labels = HashSet::new();
    for label in &spec.clade_tip_labels {
        if !labels.insert(label) {
            return Err(FossilPlacementError::DuplicateCladeSpecifier {
                fossil_label: spec.fossil_label.clone(),
                tip_label: label.clone(),
            });
        }
    }
    if spec.clade_tip_labels.len() == 1 && spec.scope == CladePlacementScope::Crown {
        return Err(FossilPlacementError::SingletonCrownConstraint(
            spec.fossil_label.clone(),
        ));
    }
    Ok(())
}

fn place_one<R: Rng + ?Sized>(
    parsed: &mut ParsedNewickTree,
    spec: &FossilPlacementSpec,
    hook_length: f64,
    rng: &mut R,
) -> Result<FossilPlacementRecord, FossilPlacementError> {
    let constraint_nodes = spec
        .clade_tip_labels
        .iter()
        .map(|label| {
            parsed
                .tip_node(label)
                .ok_or_else(|| FossilPlacementError::UnknownCladeTip {
                    fossil_label: spec.fossil_label.clone(),
                    tip_label: label.clone(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let parent_edges = parent_edges(&parsed.tree);
    let mrca = most_recent_common_ancestor(&parsed.tree, &parent_edges, &constraint_nodes)
        .expect("a nonempty set of tree nodes has an MRCA");
    let candidate_edges = constrained_edges(
        &parsed.tree,
        &parent_edges,
        mrca,
        constraint_nodes.len(),
        spec.scope,
        &spec.fossil_label,
    )?;
    let ages = parsed.tree.node_ages_from_present();
    let required_offset = match spec.attachment {
        FossilAttachment::SideBranch => 0.0,
        FossilAttachment::DirectAncestor => hook_length,
    };
    let oldest_feasible = candidate_edges
        .iter()
        .map(|edge_index| ages[parsed.tree.edges()[*edge_index].parent] - required_offset)
        .fold(f64::NEG_INFINITY, f64::max);
    let upper = spec.max_age.min(open_upper_bound(oldest_feasible));
    if upper < spec.min_age {
        return Err(FossilPlacementError::NoFeasibleAge {
            fossil_label: spec.fossil_label.clone(),
            min_age: spec.min_age,
            max_age: spec.max_age,
            oldest_feasible,
        });
    }
    let fossil_age = sample_closed_interval(spec.min_age, upper, rng);

    let (edge_index, attachment_age) = match spec.attachment {
        FossilAttachment::SideBranch => {
            let masses = candidate_edges
                .iter()
                .map(|edge_index| {
                    let edge = parsed.tree.edges()[*edge_index];
                    let lower = ages[edge.child].max(fossil_age);
                    (ages[edge.parent] - lower).max(0.0)
                })
                .collect::<Vec<_>>();
            let selected = sample_weighted_index(&masses, rng).ok_or_else(|| {
                FossilPlacementError::NoFeasibleBranch {
                    fossil_label: spec.fossil_label.clone(),
                    fossil_age,
                }
            })?;
            let edge_index = candidate_edges[selected];
            let edge = parsed.tree.edges()[edge_index];
            let lower = ages[edge.child].max(fossil_age);
            let attachment_age = sample_half_open_interval(lower, ages[edge.parent], rng);
            (edge_index, attachment_age)
        }
        FossilAttachment::DirectAncestor => {
            let attachment_age = fossil_age + hook_length;
            let masses = candidate_edges
                .iter()
                .map(|edge_index| {
                    let edge = parsed.tree.edges()[*edge_index];
                    if ages[edge.parent] > attachment_age && attachment_age >= ages[edge.child] {
                        edge.length
                    } else {
                        0.0
                    }
                })
                .collect::<Vec<_>>();
            let selected = sample_weighted_index(&masses, rng).ok_or_else(|| {
                FossilPlacementError::NoFeasibleBranch {
                    fossil_label: spec.fossil_label.clone(),
                    fossil_age,
                }
            })?;
            (candidate_edges[selected], attachment_age)
        }
    };

    insert_fossil(parsed, spec, mrca, edge_index, fossil_age, attachment_age)
}

fn parent_edges(tree: &Tree) -> Vec<Option<usize>> {
    let mut result = vec![None; tree.node_count()];
    for (edge_index, edge) in tree.edges().iter().enumerate() {
        result[edge.child] = Some(edge_index);
    }
    result
}

fn most_recent_common_ancestor(
    tree: &Tree,
    parent_edges: &[Option<usize>],
    nodes: &[usize],
) -> Option<usize> {
    let mut common = ancestor_path(tree, parent_edges, *nodes.first()?);
    for node in &nodes[1..] {
        let ancestors = ancestor_path(tree, parent_edges, *node)
            .into_iter()
            .collect::<HashSet<_>>();
        common.retain(|candidate| ancestors.contains(candidate));
    }
    common.into_iter().next()
}

fn ancestor_path(tree: &Tree, parent_edges: &[Option<usize>], mut node: usize) -> Vec<usize> {
    let mut path = vec![node];
    while node != tree.root() {
        let edge = tree.edges()[parent_edges[node].expect("every non-root node has a parent")];
        node = edge.parent;
        path.push(node);
    }
    path
}

fn constrained_edges(
    tree: &Tree,
    parent_edges: &[Option<usize>],
    mrca: usize,
    specifier_count: usize,
    scope: CladePlacementScope,
    fossil_label: &str,
) -> Result<Vec<usize>, FossilPlacementError> {
    let mut result = Vec::new();
    let include_stem = scope != CladePlacementScope::Crown;
    let include_crown = scope != CladePlacementScope::Stem && specifier_count > 1;
    if include_stem {
        if let Some(edge_index) = parent_edges[mrca] {
            result.push(edge_index);
        } else if !include_crown {
            return Err(FossilPlacementError::RootHasNoStem(
                fossil_label.to_string(),
            ));
        }
    }
    if include_crown {
        let mut stack = vec![mrca];
        while let Some(parent) = stack.pop() {
            for child in tree.children(parent).expect("node is inside tree") {
                result.push(child.edge_index);
                stack.push(child.node);
            }
        }
    }
    if result.is_empty() {
        return Err(FossilPlacementError::NoConstraintBranches(
            fossil_label.to_string(),
        ));
    }
    Ok(result)
}

fn insert_fossil(
    parsed: &mut ParsedNewickTree,
    spec: &FossilPlacementSpec,
    mrca: usize,
    edge_index: usize,
    fossil_age: f64,
    attachment_age: f64,
) -> Result<FossilPlacementRecord, FossilPlacementError> {
    let ages = parsed.tree.node_ages_from_present();
    let selected = parsed.tree.edges()[edge_index];
    let inserted_node = parsed.tree.node_count();
    let fossil_node = inserted_node + 1;
    let mut edges = Vec::with_capacity(parsed.tree.edges().len() + 2);
    for (index, edge) in parsed.tree.edges().iter().copied().enumerate() {
        if index == edge_index {
            edges.push(Edge {
                parent: selected.parent,
                child: inserted_node,
                length: ages[selected.parent] - attachment_age,
            });
            edges.push(Edge {
                parent: inserted_node,
                child: selected.child,
                length: attachment_age - ages[selected.child],
            });
            edges.push(Edge {
                parent: inserted_node,
                child: fossil_node,
                length: attachment_age - fossil_age,
            });
        } else {
            edges.push(edge);
        }
    }
    let fossil_branch_length = attachment_age - fossil_age;
    let tree = Tree::new(parsed.tree.root(), fossil_node + 1, edges)?;
    parsed.tree = tree;
    parsed.tip_labels.push(TipLabel {
        node: fossil_node,
        label: spec.fossil_label.clone(),
    });
    // The inserted attachment point intentionally has no synthetic node label.
    parsed.internal_node_labels = parsed
        .internal_node_labels
        .iter()
        .filter(|label| label.node != inserted_node)
        .cloned()
        .collect::<Vec<InternalNodeLabel>>();

    Ok(FossilPlacementRecord {
        fossil_label: spec.fossil_label.clone(),
        fossil_age,
        attachment_age,
        attachment: spec.attachment,
        scope: spec.scope,
        constrained_mrca: mrca,
        selected_parent: selected.parent,
        selected_child: selected.child,
        inserted_node,
        fossil_node,
        fossil_branch_length,
    })
}

fn sample_weighted_index<R: Rng + ?Sized>(weights: &[f64], rng: &mut R) -> Option<usize> {
    let total = weights.iter().copied().sum::<f64>();
    if !total.is_finite() || total <= 0.0 {
        return None;
    }
    let threshold = rng.random::<f64>() * total;
    let mut cumulative = 0.0;
    for (index, weight) in weights.iter().copied().enumerate() {
        cumulative += weight;
        if threshold < cumulative && weight > 0.0 {
            return Some(index);
        }
    }
    weights.iter().rposition(|weight| *weight > 0.0)
}

fn sample_closed_interval<R: Rng + ?Sized>(min: f64, max: f64, rng: &mut R) -> f64 {
    if min == max {
        min
    } else {
        min + rng.random::<f64>() * (max - min)
    }
}

fn sample_half_open_interval<R: Rng + ?Sized>(min: f64, max: f64, rng: &mut R) -> f64 {
    debug_assert!(max > min);
    let draw = rng.random::<f64>().max(f64::EPSILON);
    min + draw * (max - min)
}

fn open_upper_bound(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    value - f64::EPSILON * value.abs().max(1.0)
}

#[derive(Debug)]
pub enum FossilPlacementError {
    InvalidHookLength(f64),
    EmptyFossilLabel,
    DuplicateTipLabel(String),
    InvalidAgeInterval {
        fossil_label: String,
        min_age: f64,
        max_age: f64,
    },
    EmptyCladeConstraint(String),
    DuplicateCladeSpecifier {
        fossil_label: String,
        tip_label: String,
    },
    SingletonCrownConstraint(String),
    UnknownCladeTip {
        fossil_label: String,
        tip_label: String,
    },
    RootHasNoStem(String),
    NoConstraintBranches(String),
    NoFeasibleAge {
        fossil_label: String,
        min_age: f64,
        max_age: f64,
        oldest_feasible: f64,
    },
    NoFeasibleBranch {
        fossil_label: String,
        fossil_age: f64,
    },
    Tree(TreeError),
}

impl fmt::Display for FossilPlacementError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHookLength(value) => write!(
                f,
                "direct-ancestor hook length must be finite and greater than zero, got {value}"
            ),
            Self::EmptyFossilLabel => write!(f, "fossil labels must not be empty"),
            Self::DuplicateTipLabel(label) => {
                write!(
                    f,
                    "fossil label {label:?} duplicates another tree tip label"
                )
            }
            Self::InvalidAgeInterval {
                fossil_label,
                min_age,
                max_age,
            } => write!(
                f,
                "fossil {fossil_label:?} has invalid age interval [{min_age}, {max_age}]"
            ),
            Self::EmptyCladeConstraint(label) => {
                write!(f, "fossil {label:?} must specify at least one clade tip")
            }
            Self::DuplicateCladeSpecifier {
                fossil_label,
                tip_label,
            } => write!(
                f,
                "fossil {fossil_label:?} repeats clade tip specifier {tip_label:?}"
            ),
            Self::SingletonCrownConstraint(label) => write!(
                f,
                "fossil {label:?} cannot use crown placement with a one-tip clade constraint"
            ),
            Self::UnknownCladeTip {
                fossil_label,
                tip_label,
            } => write!(
                f,
                "fossil {fossil_label:?} refers to unknown clade tip {tip_label:?}"
            ),
            Self::RootHasNoStem(label) => {
                write!(f, "fossil {label:?} requests the stem of the root clade")
            }
            Self::NoConstraintBranches(label) => write!(
                f,
                "fossil {label:?} has no branches under its clade and stem/crown constraint"
            ),
            Self::NoFeasibleAge {
                fossil_label,
                min_age,
                max_age,
                oldest_feasible,
            } => write!(
                f,
                "fossil {fossil_label:?} age interval [{min_age}, {max_age}] does not overlap feasible constrained-tree ages (oldest feasible age below {oldest_feasible})"
            ),
            Self::NoFeasibleBranch {
                fossil_label,
                fossil_age,
            } => write!(
                f,
                "fossil {fossil_label:?} has no feasible constrained branch at sampled age {fossil_age}"
            ),
            Self::Tree(error) => write!(f, "fossil placement produced an invalid tree: {error}"),
        }
    }
}

impl Error for FossilPlacementError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Tree(error) => Some(error),
            _ => None,
        }
    }
}

impl From<TreeError> for FossilPlacementError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{format_newick, parse_newick};

    fn source_tree() -> ParsedNewickTree {
        parse_newick("((A:2,B:2):2,(C:2,D:2):2);").unwrap()
    }

    fn spec(
        label: &str,
        attachment: FossilAttachment,
        scope: CladePlacementScope,
        clade: &[&str],
    ) -> FossilPlacementSpec {
        FossilPlacementSpec {
            fossil_label: label.to_string(),
            min_age: 0.5,
            max_age: 1.5,
            attachment,
            scope,
            clade_tip_labels: clade.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    #[test]
    fn seeded_side_branch_placement_is_reproducible_and_age_valid() {
        let placed = place_fossils_randomly(
            &source_tree(),
            &[spec(
                "F",
                FossilAttachment::SideBranch,
                CladePlacementScope::Crown,
                &["A", "B"],
            )],
            17,
            1e-7,
        )
        .unwrap();
        let repeated = place_fossils_randomly(
            &source_tree(),
            &[spec(
                "F",
                FossilAttachment::SideBranch,
                CladePlacementScope::Crown,
                &["A", "B"],
            )],
            17,
            1e-7,
        )
        .unwrap();

        assert_eq!(placed, repeated);
        assert_eq!(placed.tree.tip_labels.len(), 5);
        let record = &placed.records[0];
        assert!((0.5..=1.5).contains(&record.fossil_age));
        assert!(record.attachment_age >= record.fossil_age);
        assert!(
            record.selected_child == source_tree().tip_node("A").unwrap()
                || record.selected_child == source_tree().tip_node("B").unwrap()
        );
        assert_eq!(
            parse_newick(&format_newick(&placed.tree))
                .unwrap()
                .tip_labels
                .len(),
            5
        );
    }

    #[test]
    fn direct_ancestor_has_exact_fossil_age_and_hook_length() {
        let hook = 1e-7;
        let placed = place_fossils_randomly(
            &source_tree(),
            &[spec(
                "F",
                FossilAttachment::DirectAncestor,
                CladePlacementScope::Stem,
                &["A"],
            )],
            91,
            hook,
        )
        .unwrap();
        let record = &placed.records[0];
        assert!((record.fossil_branch_length - hook).abs() < 1e-14);
        let ages = placed.tree.tree.node_ages_from_present();
        assert!((ages[record.fossil_node] - record.fossil_age).abs() < 1e-12);
    }

    #[test]
    fn sequential_constraints_can_reference_an_earlier_fossil() {
        let mut first = spec(
            "F1",
            FossilAttachment::SideBranch,
            CladePlacementScope::Stem,
            &["A"],
        );
        first.min_age = 0.2;
        first.max_age = 0.2;
        let mut second = spec(
            "F2",
            FossilAttachment::SideBranch,
            CladePlacementScope::Both,
            &["A", "F1"],
        );
        second.min_age = 0.1;
        second.max_age = 0.1;

        let placed = place_fossils_randomly(&source_tree(), &[first, second], 3, 1e-7).unwrap();
        assert!(placed.tree.tip_node("F1").is_some());
        assert!(placed.tree.tip_node("F2").is_some());
    }

    #[test]
    fn rejects_singleton_crown_and_root_stem() {
        let singleton = place_fossils_randomly(
            &source_tree(),
            &[spec(
                "F",
                FossilAttachment::SideBranch,
                CladePlacementScope::Crown,
                &["A"],
            )],
            1,
            1e-7,
        )
        .unwrap_err();
        assert!(matches!(
            singleton,
            FossilPlacementError::SingletonCrownConstraint(_)
        ));

        let root_stem = place_fossils_randomly(
            &source_tree(),
            &[spec(
                "F",
                FossilAttachment::SideBranch,
                CladePlacementScope::Stem,
                &["A", "C"],
            )],
            1,
            1e-7,
        )
        .unwrap_err();
        assert!(matches!(root_stem, FossilPlacementError::RootHasNoStem(_)));
    }
}
