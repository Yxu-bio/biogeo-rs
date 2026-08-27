use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;

use crate::tree::{Edge, Tree, TreeError};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TipLabel {
    pub node: usize,
    pub label: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InternalNodeLabel {
    pub node: usize,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParsedNewickTree {
    pub tree: Tree,
    pub tip_labels: Vec<TipLabel>,
    pub internal_node_labels: Vec<InternalNodeLabel>,
}

impl ParsedNewickTree {
    pub fn tip_node(&self, label: &str) -> Option<usize> {
        self.tip_labels
            .iter()
            .find(|tip| tip.label == label)
            .map(|tip| tip.node)
    }

    pub fn node_label(&self, node: usize) -> Option<&str> {
        self.tip_labels
            .iter()
            .find(|tip| tip.node == node)
            .map(|tip| tip.label.as_str())
            .or_else(|| {
                self.internal_node_labels
                    .iter()
                    .find(|label| label.node == node)
                    .map(|label| label.label.as_str())
            })
    }

    pub fn with_direct_ancestor_hooks_below(
        mut self,
        min_branch_length: f64,
    ) -> Result<Self, TreeError> {
        self.tree = self
            .tree
            .with_direct_ancestor_hooks_below(min_branch_length)?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MissingBranchLengthPolicy {
    Reject,
    Fill(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NewickParseOptions {
    pub missing_branch_lengths: MissingBranchLengthPolicy,
}

impl Default for NewickParseOptions {
    fn default() -> Self {
        Self {
            missing_branch_lengths: MissingBranchLengthPolicy::Reject,
        }
    }
}

pub fn parse_newick(input: &str) -> Result<ParsedNewickTree, NewickError> {
    parse_newick_with_options(input, NewickParseOptions::default())
}

pub fn format_newick(parsed: &ParsedNewickTree) -> String {
    let mut output = String::new();
    format_subtree(parsed, parsed.tree.root(), None, &mut output);
    output.push(';');
    output
}

fn format_subtree(
    parsed: &ParsedNewickTree,
    node: usize,
    branch_length: Option<f64>,
    output: &mut String,
) {
    let children = parsed
        .tree
        .children(node)
        .expect("a parsed-tree node is inside the tree");
    if children.is_empty() {
        let label = parsed
            .tip_labels
            .iter()
            .find(|label| label.node == node)
            .expect("every parsed-tree tip has a label");
        format_newick_label(&label.label, output);
    } else {
        output.push('(');
        for (index, child) in children.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            format_subtree(parsed, child.node, Some(child.length), output);
        }
        output.push(')');
        if let Some(label) = parsed
            .internal_node_labels
            .iter()
            .find(|label| label.node == node)
        {
            format_newick_label(&label.label, output);
        }
    }
    if let Some(branch_length) = branch_length {
        write!(output, ":{branch_length}")
            .expect("writing a branch length to a String cannot fail");
    }
}

fn format_newick_label(label: &str, output: &mut String) {
    let requires_quotes = label.is_empty()
        || label.chars().any(|ch| {
            ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | ',' | ':' | ';' | '\'')
        });
    if !requires_quotes {
        output.push_str(label);
        return;
    }

    output.push('\'');
    for ch in label.chars() {
        if ch == '\'' {
            output.push('\'');
        }
        output.push(ch);
    }
    output.push('\'');
}

pub fn parse_newick_with_options(
    input: &str,
    options: NewickParseOptions,
) -> Result<ParsedNewickTree, NewickError> {
    validate_options(options)?;
    let mut parser = Parser::new(input);
    let root = parser.parse_subtree()?;
    parser.skip_ignored()?;
    parser.expect_char(';')?;
    parser.skip_ignored()?;
    if !parser.is_at_end() {
        return Err(NewickError::TrailingInput {
            position: parser.position(),
        });
    }
    if let Some(length) = root.length {
        return Err(NewickError::UnsupportedRootBranchLength {
            position: root.position,
            length,
        });
    }

    let mut edges = Vec::new();
    let mut tip_labels = Vec::new();
    let mut internal_node_labels = Vec::new();
    let mut next_node = 0;
    let root_id = assign_node_ids(
        &root,
        &mut next_node,
        &mut edges,
        &mut tip_labels,
        &mut internal_node_labels,
        options.missing_branch_lengths,
    )?;
    check_duplicate_tip_labels(&tip_labels)?;
    let tree = Tree::new(root_id, next_node, edges)?;

    Ok(ParsedNewickTree {
        tree,
        tip_labels,
        internal_node_labels,
    })
}

fn validate_options(options: NewickParseOptions) -> Result<(), NewickError> {
    if let MissingBranchLengthPolicy::Fill(length) = options.missing_branch_lengths
        && (!length.is_finite() || length < 0.0)
    {
        return Err(NewickError::InvalidMissingBranchLengthFill { length });
    }
    Ok(())
}

fn assign_node_ids(
    node: &RawNode,
    next_node: &mut usize,
    edges: &mut Vec<Edge>,
    tip_labels: &mut Vec<TipLabel>,
    internal_node_labels: &mut Vec<InternalNodeLabel>,
    missing_branch_lengths: MissingBranchLengthPolicy,
) -> Result<usize, NewickError> {
    if node.children.is_empty() {
        let label = node.label.clone().ok_or(NewickError::MissingTipLabel {
            position: node.position,
        })?;
        let node_id = *next_node;
        *next_node += 1;
        tip_labels.push(TipLabel {
            node: node_id,
            label,
        });
        return Ok(node_id);
    }

    let mut child_ids = Vec::with_capacity(node.children.len());
    for child in &node.children {
        let child_id = assign_node_ids(
            child,
            next_node,
            edges,
            tip_labels,
            internal_node_labels,
            missing_branch_lengths,
        )?;
        let length = match (child.length, missing_branch_lengths) {
            (Some(length), _) => length,
            (None, MissingBranchLengthPolicy::Fill(length)) => length,
            (None, MissingBranchLengthPolicy::Reject) => {
                return Err(NewickError::RequiredBranchLengthMissing {
                    position: child.position,
                    label: child.label.clone(),
                });
            }
        };
        child_ids.push((child_id, length));
    }

    let node_id = *next_node;
    *next_node += 1;
    if let Some(label) = &node.label {
        internal_node_labels.push(InternalNodeLabel {
            node: node_id,
            label: label.clone(),
        });
    }
    for (child_id, length) in child_ids {
        edges.push(Edge {
            parent: node_id,
            child: child_id,
            length,
        });
    }

    Ok(node_id)
}

fn check_duplicate_tip_labels(tip_labels: &[TipLabel]) -> Result<(), NewickError> {
    let mut seen = HashSet::new();
    for tip in tip_labels {
        if !seen.insert(tip.label.as_str()) {
            return Err(NewickError::DuplicateTipLabel {
                label: tip.label.clone(),
            });
        }
    }

    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
struct RawNode {
    label: Option<String>,
    length: Option<f64>,
    children: Vec<RawNode>,
    position: usize,
}

struct Parser<'a> {
    input: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, position: 0 }
    }

    fn parse_subtree(&mut self) -> Result<RawNode, NewickError> {
        self.skip_ignored()?;
        let position = self.position;

        if self.peek_char() == Some('(') {
            self.position += 1;
            let mut children = Vec::new();
            loop {
                children.push(self.parse_subtree()?);
                self.skip_ignored()?;
                match self.peek_char() {
                    Some(',') => {
                        self.position += 1;
                    }
                    Some(')') => {
                        self.position += 1;
                        break;
                    }
                    Some(found) => {
                        return Err(NewickError::UnexpectedCharacter {
                            position: self.position,
                            expected: "',' or ')'",
                            found,
                        });
                    }
                    None => {
                        return Err(NewickError::UnexpectedEnd {
                            position: self.position,
                            expected: "')'",
                        });
                    }
                }
            }

            let label = self.parse_optional_label()?;
            let length = self.parse_optional_length()?;
            Ok(RawNode {
                label,
                length,
                children,
                position,
            })
        } else {
            let label = self.parse_optional_label()?;
            if label.as_deref().is_none_or(str::is_empty) {
                return Err(NewickError::MissingTipLabel { position });
            }
            let length = self.parse_optional_length()?;
            Ok(RawNode {
                label,
                length,
                children: Vec::new(),
                position,
            })
        }
    }

    fn parse_optional_label(&mut self) -> Result<Option<String>, NewickError> {
        self.skip_ignored()?;
        if self.peek_char() == Some('\'') {
            return self.parse_quoted_label().map(Some);
        }

        let start = self.position;

        while let Some(ch) = self.peek_char() {
            if matches!(ch, '(' | ')' | ',' | ':' | ';' | '[' | '\'') || ch.is_whitespace() {
                break;
            }
            self.position += ch.len_utf8();
        }

        if self.position == start {
            Ok(None)
        } else {
            Ok(Some(self.input[start..self.position].to_string()))
        }
    }

    fn parse_quoted_label(&mut self) -> Result<String, NewickError> {
        let start = self.position;
        self.position += 1;
        let mut label = String::new();

        while let Some(ch) = self.peek_char() {
            self.position += ch.len_utf8();
            if ch != '\'' {
                label.push(ch);
                continue;
            }
            if self.peek_char() == Some('\'') {
                self.position += 1;
                label.push('\'');
                continue;
            }
            return Ok(label);
        }

        Err(NewickError::UnterminatedQuotedLabel { position: start })
    }

    fn parse_optional_length(&mut self) -> Result<Option<f64>, NewickError> {
        self.skip_ignored()?;
        if self.peek_char() != Some(':') {
            return Ok(None);
        }
        self.position += 1;
        self.skip_ignored()?;

        let start = self.position;
        while let Some(ch) = self.peek_char() {
            if matches!(ch, '(' | ')' | ',' | ';' | '[') || ch.is_whitespace() {
                break;
            }
            self.position += ch.len_utf8();
        }

        if self.position == start {
            return Err(NewickError::MissingBranchLength { position: start });
        }

        let raw = &self.input[start..self.position];
        let length = raw
            .parse::<f64>()
            .map_err(|_| NewickError::InvalidBranchLength {
                position: start,
                value: raw.to_string(),
            })?;
        if !length.is_finite() {
            return Err(NewickError::InvalidBranchLength {
                position: start,
                value: raw.to_string(),
            });
        }

        Ok(Some(length))
    }

    fn expect_char(&mut self, expected: char) -> Result<(), NewickError> {
        match self.peek_char() {
            Some(ch) if ch == expected => {
                self.position += ch.len_utf8();
                Ok(())
            }
            Some(found) => Err(NewickError::UnexpectedCharacter {
                position: self.position,
                expected: "';'",
                found,
            }),
            None => Err(NewickError::UnexpectedEnd {
                position: self.position,
                expected: "';'",
            }),
        }
    }

    fn skip_ignored(&mut self) -> Result<(), NewickError> {
        loop {
            while let Some(ch) = self.peek_char() {
                if !ch.is_whitespace() {
                    break;
                }
                self.position += ch.len_utf8();
            }
            if self.peek_char() != Some('[') {
                return Ok(());
            }
            self.skip_comment()?;
        }
    }

    fn skip_comment(&mut self) -> Result<(), NewickError> {
        let start = self.position;
        self.position += 1;
        let mut depth = 1usize;
        while let Some(ch) = self.peek_char() {
            self.position += ch.len_utf8();
            match ch {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
                _ => {}
            }
        }

        Err(NewickError::UnterminatedComment { position: start })
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }

    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }

    fn position(&self) -> usize {
        self.position
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NewickError {
    MissingTipLabel {
        position: usize,
    },
    RequiredBranchLengthMissing {
        position: usize,
        label: Option<String>,
    },
    MissingBranchLength {
        position: usize,
    },
    InvalidBranchLength {
        position: usize,
        value: String,
    },
    InvalidMissingBranchLengthFill {
        length: f64,
    },
    UnsupportedRootBranchLength {
        position: usize,
        length: f64,
    },
    UnterminatedQuotedLabel {
        position: usize,
    },
    UnterminatedComment {
        position: usize,
    },
    UnexpectedCharacter {
        position: usize,
        expected: &'static str,
        found: char,
    },
    UnexpectedEnd {
        position: usize,
        expected: &'static str,
    },
    TrailingInput {
        position: usize,
    },
    DuplicateTipLabel {
        label: String,
    },
    Tree(TreeError),
}

impl fmt::Display for NewickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingTipLabel { position } => {
                write!(f, "missing tip label at byte position {position}")
            }
            Self::RequiredBranchLengthMissing { position, label } => {
                if let Some(label) = label {
                    write!(
                        f,
                        "branch length is required for node {label:?} at byte position {position}"
                    )
                } else {
                    write!(
                        f,
                        "branch length is required for the internal node at byte position {position}"
                    )
                }
            }
            Self::MissingBranchLength { position } => {
                write!(f, "missing branch length at byte position {position}")
            }
            Self::InvalidBranchLength { position, value } => write!(
                f,
                "invalid branch length {value:?} at byte position {position}"
            ),
            Self::InvalidMissingBranchLengthFill { length } => write!(
                f,
                "missing-branch-length fill must be finite and non-negative, got {length}"
            ),
            Self::UnsupportedRootBranchLength { position, length } => write!(
                f,
                "root branch length {length} at byte position {position} is unsupported because the likelihood engine has no root-edge process"
            ),
            Self::UnterminatedQuotedLabel { position } => {
                write!(f, "unterminated quoted label at byte position {position}")
            }
            Self::UnterminatedComment { position } => {
                write!(f, "unterminated Newick comment at byte position {position}")
            }
            Self::UnexpectedCharacter {
                position,
                expected,
                found,
            } => write!(
                f,
                "unexpected character {found:?} at byte position {position}; expected {expected}"
            ),
            Self::UnexpectedEnd { position, expected } => write!(
                f,
                "unexpected end at byte position {position}; expected {expected}"
            ),
            Self::TrailingInput { position } => {
                write!(
                    f,
                    "trailing input after Newick tree at byte position {position}"
                )
            }
            Self::DuplicateTipLabel { label } => write!(f, "duplicate tip label {label:?}"),
            Self::Tree(error) => write!(f, "invalid parsed Newick tree: {error}"),
        }
    }
}

impl Error for NewickError {}

impl From<TreeError> for NewickError {
    fn from(value: TreeError) -> Self {
        Self::Tree(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_two_tip_tree() {
        let parsed = parse_newick("(A:0,B:0);").unwrap();

        assert_eq!(parsed.tree.root(), 2);
        assert_eq!(parsed.tree.node_count(), 3);
        assert_eq!(parsed.tip_node("A"), Some(0));
        assert_eq!(parsed.tip_node("B"), Some(1));
        assert_eq!(parsed.tree.postorder_internal_nodes(), &[2]);
    }

    #[test]
    fn canonical_newick_format_round_trips_labels_topology_and_lengths() {
        let parsed = parse_newick(
            "[source](('Homo sapiens':0.91,'O''Brien, fossil':1)'inner node':1,gorilla:2)'root';",
        )
        .unwrap();
        let formatted = format_newick(&parsed);

        assert_eq!(
            formatted,
            "(('Homo sapiens':0.91,'O''Brien, fossil':1)'inner node':1,gorilla:2)root;"
        );
        assert_eq!(parse_newick(&formatted).unwrap(), parsed);
    }

    #[test]
    fn parses_nested_tree_with_internal_branch_lengths() {
        let parsed = parse_newick("((A:1,B:1):0.5,C:1.5);").unwrap();

        assert_eq!(parsed.tip_node("A"), Some(0));
        assert_eq!(parsed.tip_node("B"), Some(1));
        assert_eq!(parsed.tip_node("C"), Some(3));
        assert_eq!(parsed.tree.root(), 4);
        assert_eq!(parsed.tree.postorder_internal_nodes(), &[2, 4]);
        assert_eq!(parsed.tree.edges().len(), 4);
    }

    #[test]
    fn parses_quoted_unicode_labels_and_escaped_quotes() {
        let parsed =
            parse_newick("('Homo sapiens':0.91,'Pan, troglodytes':1,'O''Brien（化石）':2);")
                .unwrap();

        assert_eq!(parsed.tip_node("Homo sapiens"), Some(0));
        assert_eq!(parsed.tip_node("Pan, troglodytes"), Some(1));
        assert_eq!(parsed.tip_node("O'Brien（化石）"), Some(2));
    }

    #[test]
    fn ignores_balanced_comments_between_tokens() {
        let parsed = parse_newick(
            "[&R] ((A[&tip=true]:1, B:1)'inner node'[outer[nested]]:1, C:2)'root node'[x];",
        )
        .unwrap();

        assert_eq!(parsed.tip_node("A"), Some(0));
        assert_eq!(parsed.tip_node("B"), Some(1));
        assert_eq!(parsed.tip_node("C"), Some(3));
        assert_eq!(parsed.node_label(2), Some("inner node"));
        assert_eq!(parsed.node_label(4), Some("root node"));
    }

    #[test]
    fn rejects_missing_non_root_branch_lengths_by_default() {
        let error = parse_newick("(A,B:1);").unwrap_err();

        assert_eq!(
            error,
            NewickError::RequiredBranchLengthMissing {
                position: 1,
                label: Some("A".to_string()),
            }
        );
    }

    #[test]
    fn explicitly_fills_missing_non_root_branch_lengths() {
        let parsed = parse_newick_with_options(
            "((A,B:0.5),C:2);",
            NewickParseOptions {
                missing_branch_lengths: MissingBranchLengthPolicy::Fill(0.25),
            },
        )
        .unwrap();

        let edge_to_a = parsed
            .tree
            .edges()
            .iter()
            .find(|edge| edge.child == parsed.tip_node("A").unwrap())
            .unwrap();
        let inner = parsed.tree.children(parsed.tree.root()).unwrap()[0].node;
        let edge_to_inner = parsed
            .tree
            .edges()
            .iter()
            .find(|edge| edge.child == inner)
            .unwrap();
        assert_eq!(edge_to_a.length, 0.25);
        assert_eq!(edge_to_inner.length, 0.25);
    }

    #[test]
    fn rejects_invalid_missing_branch_length_fill() {
        let error = parse_newick_with_options(
            "(A,B);",
            NewickParseOptions {
                missing_branch_lengths: MissingBranchLengthPolicy::Fill(-1.0),
            },
        )
        .unwrap_err();

        assert_eq!(
            error,
            NewickError::InvalidMissingBranchLengthFill { length: -1.0 }
        );
    }

    #[test]
    fn rejects_root_branch_length_instead_of_silently_ignoring_it() {
        let error = parse_newick("(A:1,B:1):0.5;").unwrap_err();

        assert_eq!(
            error,
            NewickError::UnsupportedRootBranchLength {
                position: 0,
                length: 0.5,
            }
        );
    }

    #[test]
    fn rejects_unterminated_quoted_label_and_comment() {
        assert!(matches!(
            parse_newick("('A:1,B:1);"),
            Err(NewickError::UnterminatedQuotedLabel { position: 1 })
        ));
        assert!(matches!(
            parse_newick("(A:1[comment,B:1);"),
            Err(NewickError::UnterminatedComment { position: 4 })
        ));
    }

    #[test]
    fn rejects_duplicate_tip_labels() {
        let error = parse_newick("(A:0,A:0);").unwrap_err();

        assert_eq!(
            error,
            NewickError::DuplicateTipLabel {
                label: "A".to_string()
            }
        );
    }

    #[test]
    fn rejects_missing_semicolon() {
        let error = parse_newick("(A:0,B:0)").unwrap_err();

        assert!(matches!(error, NewickError::UnexpectedEnd { .. }));
    }
}
