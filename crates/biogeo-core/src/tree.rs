use std::error::Error;
use std::fmt;

const TIP_AGE_RELATIVE_TOLERANCE: f64 = 1e-9;
const TIP_AGE_ABSOLUTE_TOLERANCE: f64 = 1e-12;

pub fn default_tip_age_tolerance(root_age: f64) -> f64 {
    root_age.abs() * TIP_AGE_RELATIVE_TOLERANCE + TIP_AGE_ABSOLUTE_TOLERANCE
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub parent: usize,
    pub child: usize,
    pub length: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TreeChild {
    pub node: usize,
    pub edge_index: usize,
    pub length: f64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeEvent {
    Cladogenesis,
    DirectAncestor,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tree {
    root: usize,
    node_count: usize,
    edges: Vec<Edge>,
    children: Vec<Vec<TreeChild>>,
    postorder_internal_nodes: Vec<usize>,
    tip_nodes: Vec<usize>,
    node_events: Vec<NodeEvent>,
    direct_ancestor_threshold: f64,
    direct_ancestor_hook_edges: Vec<usize>,
}

impl Tree {
    pub fn new(root: usize, node_count: usize, edges: Vec<Edge>) -> Result<Self, TreeError> {
        validate_basic_shape(root, node_count, &edges)?;

        let mut parent_by_node = vec![None; node_count];
        let mut children = vec![Vec::new(); node_count];

        for (edge_index, edge) in edges.iter().copied().enumerate() {
            if parent_by_node[edge.child].is_some() {
                return Err(TreeError::MultipleParents { node: edge.child });
            }
            parent_by_node[edge.child] = Some(edge.parent);
            children[edge.parent].push(TreeChild {
                node: edge.child,
                edge_index,
                length: edge.length,
            });
        }

        if parent_by_node[root].is_some() {
            return Err(TreeError::RootHasParent { root });
        }

        for (node, parent) in parent_by_node.iter().enumerate() {
            if node != root && parent.is_none() {
                return Err(TreeError::DisconnectedNode { node });
            }
        }

        let mut visit_state = vec![VisitState::Unvisited; node_count];
        let mut postorder_internal_nodes = Vec::new();
        visit_postorder(
            root,
            &children,
            &mut visit_state,
            &mut postorder_internal_nodes,
        )?;

        if let Some((node, _)) = visit_state
            .iter()
            .enumerate()
            .find(|(_, state)| **state != VisitState::Done)
        {
            return Err(TreeError::DisconnectedNode { node });
        }

        let tip_nodes = (0..node_count)
            .filter(|node| children[*node].is_empty())
            .collect();

        Ok(Self {
            root,
            node_count,
            edges,
            children,
            postorder_internal_nodes,
            tip_nodes,
            node_events: vec![NodeEvent::Cladogenesis; node_count],
            direct_ancestor_threshold: 0.0,
            direct_ancestor_hook_edges: Vec::new(),
        })
    }

    pub fn with_direct_ancestor_hooks_below(
        mut self,
        min_branch_length: f64,
    ) -> Result<Self, TreeError> {
        if !min_branch_length.is_finite() || min_branch_length < 0.0 {
            return Err(TreeError::InvalidDirectAncestorThreshold {
                value: min_branch_length,
            });
        }

        self.node_events.fill(NodeEvent::Cladogenesis);
        self.direct_ancestor_threshold = min_branch_length;
        self.direct_ancestor_hook_edges.clear();
        for (edge_index, edge) in self.edges.iter().enumerate() {
            if edge.length < min_branch_length {
                self.node_events[edge.parent] = NodeEvent::DirectAncestor;
                self.direct_ancestor_hook_edges.push(edge_index);
            }
        }
        Ok(self)
    }

    pub fn root(&self) -> usize {
        self.root
    }

    pub fn node_count(&self) -> usize {
        self.node_count
    }

    pub fn edges(&self) -> &[Edge] {
        &self.edges
    }

    pub fn children(&self, node: usize) -> Option<&[TreeChild]> {
        self.children.get(node).map(Vec::as_slice)
    }

    pub fn is_tip(&self, node: usize) -> bool {
        self.children
            .get(node)
            .is_some_and(|children| children.is_empty())
    }

    pub fn tip_nodes(&self) -> &[usize] {
        &self.tip_nodes
    }

    pub fn postorder_internal_nodes(&self) -> &[usize] {
        &self.postorder_internal_nodes
    }

    pub fn node_event(&self, node: usize) -> Option<NodeEvent> {
        self.node_events.get(node).copied()
    }

    pub fn is_direct_ancestor_node(&self, node: usize) -> bool {
        self.node_event(node) == Some(NodeEvent::DirectAncestor)
    }

    pub fn direct_ancestor_hook_edges(&self) -> &[usize] {
        &self.direct_ancestor_hook_edges
    }

    pub fn direct_ancestor_threshold(&self) -> f64 {
        self.direct_ancestor_threshold
    }

    pub fn cladogenesis_node_count(&self) -> usize {
        self.postorder_internal_nodes
            .iter()
            .filter(|node| !self.is_direct_ancestor_node(**node))
            .count()
    }

    pub fn node_ages_from_present(&self) -> Vec<f64> {
        let mut depths = vec![0.0; self.node_count];
        let mut stack = vec![self.root];
        while let Some(parent) = stack.pop() {
            for child in &self.children[parent] {
                depths[child.node] = depths[parent] + child.length;
                stack.push(child.node);
            }
        }

        let present_depth = self
            .tip_nodes
            .iter()
            .map(|node| depths[*node])
            .fold(0.0, f64::max);
        depths
            .into_iter()
            .map(|depth| {
                let age = present_depth - depth;
                if age.abs() <= 1e-12 { 0.0 } else { age }
            })
            .collect()
    }
}

fn validate_basic_shape(root: usize, node_count: usize, edges: &[Edge]) -> Result<(), TreeError> {
    if node_count == 0 {
        return Err(TreeError::ZeroNodes);
    }
    if root >= node_count {
        return Err(TreeError::NodeOutOfBounds {
            node: root,
            node_count,
        });
    }

    for (edge_index, edge) in edges.iter().enumerate() {
        if edge.parent >= node_count {
            return Err(TreeError::NodeOutOfBounds {
                node: edge.parent,
                node_count,
            });
        }
        if edge.child >= node_count {
            return Err(TreeError::NodeOutOfBounds {
                node: edge.child,
                node_count,
            });
        }
        if edge.parent == edge.child {
            return Err(TreeError::SelfEdge {
                node: edge.parent,
                edge_index,
            });
        }
        if !edge.length.is_finite() {
            return Err(TreeError::NonFiniteEdgeLength {
                edge_index,
                length: edge.length,
            });
        }
        if edge.length < 0.0 {
            return Err(TreeError::NegativeEdgeLength {
                edge_index,
                length: edge.length,
            });
        }
    }

    Ok(())
}

fn visit_postorder(
    node: usize,
    children: &[Vec<TreeChild>],
    visit_state: &mut [VisitState],
    postorder_internal_nodes: &mut Vec<usize>,
) -> Result<(), TreeError> {
    match visit_state[node] {
        VisitState::Visiting => return Err(TreeError::Cycle { node }),
        VisitState::Done => return Ok(()),
        VisitState::Unvisited => {}
    }

    visit_state[node] = VisitState::Visiting;
    for child in &children[node] {
        visit_postorder(child.node, children, visit_state, postorder_internal_nodes)?;
    }
    visit_state[node] = VisitState::Done;

    if !children[node].is_empty() {
        postorder_internal_nodes.push(node);
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VisitState {
    Unvisited,
    Visiting,
    Done,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TreeError {
    ZeroNodes,
    NodeOutOfBounds { node: usize, node_count: usize },
    SelfEdge { node: usize, edge_index: usize },
    NonFiniteEdgeLength { edge_index: usize, length: f64 },
    NegativeEdgeLength { edge_index: usize, length: f64 },
    MultipleParents { node: usize },
    RootHasParent { root: usize },
    DisconnectedNode { node: usize },
    Cycle { node: usize },
    InvalidDirectAncestorThreshold { value: f64 },
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroNodes => write!(f, "tree must contain at least one node"),
            Self::NodeOutOfBounds { node, node_count } => {
                write!(
                    f,
                    "tree node {node} is out of bounds for {node_count} nodes"
                )
            }
            Self::SelfEdge { node, edge_index } => {
                write!(
                    f,
                    "tree edge {edge_index} has node {node} as both parent and child"
                )
            }
            Self::NonFiniteEdgeLength { edge_index, length } => {
                write!(
                    f,
                    "tree edge {edge_index} length must be finite, got {length}"
                )
            }
            Self::NegativeEdgeLength { edge_index, length } => {
                write!(
                    f,
                    "tree edge {edge_index} length must be non-negative, got {length}"
                )
            }
            Self::MultipleParents { node } => write!(f, "tree node {node} has multiple parents"),
            Self::RootHasParent { root } => write!(f, "tree root node {root} has a parent"),
            Self::DisconnectedNode { node } => write!(f, "tree node {node} is disconnected"),
            Self::Cycle { node } => write!(f, "tree contains a cycle involving node {node}"),
            Self::InvalidDirectAncestorThreshold { value } => write!(
                f,
                "direct-ancestor branch-length threshold must be finite and non-negative, got {value}"
            ),
        }
    }
}

impl Error for TreeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_postorder_for_rooted_tree() {
        let tree = example_tree();

        assert_eq!(tree.root(), 4);
        assert_eq!(tree.node_count(), 5);
        assert_eq!(tree.tip_nodes(), &[0, 1, 2]);
        assert_eq!(tree.postorder_internal_nodes(), &[3, 4]);
        assert!(tree.is_tip(0));
        assert!(!tree.is_tip(3));
    }

    #[test]
    fn rejects_disconnected_nodes() {
        let error = Tree::new(
            2,
            4,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap_err();

        assert_eq!(error, TreeError::DisconnectedNode { node: 3 });
    }

    #[test]
    fn rejects_multiple_parents() {
        let error = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 1,
                    child: 0,
                    length: 1.0,
                },
            ],
        )
        .unwrap_err();

        assert_eq!(error, TreeError::MultipleParents { node: 0 });
    }

    #[test]
    fn computes_node_ages_for_ultrametric_tree() {
        let tree = example_tree();

        assert_eq!(tree.node_ages_from_present(), vec![0.0, 0.0, 0.0, 1.0, 1.5]);
    }

    #[test]
    fn shorter_root_to_tip_path_represents_an_older_tip() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 0.6,
                },
            ],
        )
        .unwrap();

        assert_eq!(tree.node_ages_from_present(), vec![0.0, 0.4, 1.0]);
    }

    #[test]
    fn default_tip_age_tolerance_scales_with_tree_age() {
        assert!((default_tip_age_tolerance(123.5) - 1.23501e-7).abs() < 1e-20);
        assert_eq!(default_tip_age_tolerance(0.0), 1e-12);
    }

    #[test]
    fn marks_parent_of_strictly_shorter_branch_as_direct_ancestor() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1e-7,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1e-6,
                },
            ],
        )
        .unwrap()
        .with_direct_ancestor_hooks_below(1e-6)
        .unwrap();

        assert_eq!(tree.node_event(2), Some(NodeEvent::DirectAncestor));
        assert_eq!(tree.direct_ancestor_hook_edges(), &[0]);
        assert_eq!(tree.cladogenesis_node_count(), 0);
    }

    #[test]
    fn threshold_zero_disables_direct_ancestor_detection() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 0.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap()
        .with_direct_ancestor_hooks_below(0.0)
        .unwrap();

        assert_eq!(tree.node_event(2), Some(NodeEvent::Cladogenesis));
        assert!(tree.direct_ancestor_hook_edges().is_empty());
        assert_eq!(tree.cladogenesis_node_count(), 1);
    }

    fn example_tree() -> Tree {
        Tree::new(
            4,
            5,
            vec![
                Edge {
                    parent: 4,
                    child: 3,
                    length: 0.5,
                },
                Edge {
                    parent: 3,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 3,
                    child: 1,
                    length: 1.0,
                },
                Edge {
                    parent: 4,
                    child: 2,
                    length: 1.5,
                },
            ],
        )
        .unwrap()
    }
}
