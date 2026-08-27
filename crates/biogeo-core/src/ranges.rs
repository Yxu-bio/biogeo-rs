use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::dec::TipRange;
use crate::newick::ParsedNewickTree;
use crate::pruning::TipLikelihood;
use crate::state::{AreaSet, StateSpace};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TipRangeConstraint {
    pub node: usize,
    pub required: AreaSet,
    pub forbidden: AreaSet,
    pub unknown: AreaSet,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedTipRanges {
    pub area_names: Vec<String>,
    pub tip_ranges: Vec<TipRange>,
    pub ambiguity_constraints: Option<Vec<TipRangeConstraint>>,
}

impl ParsedTipRanges {
    pub fn from_exact(area_names: Vec<String>, tip_ranges: Vec<TipRange>) -> Self {
        Self {
            area_names,
            tip_ranges,
            ambiguity_constraints: None,
        }
    }

    pub fn has_ambiguities(&self) -> bool {
        self.ambiguity_constraints.is_some()
    }

    pub fn ambiguous_tip_count(&self) -> usize {
        self.ambiguity_constraints
            .as_deref()
            .map(|constraints| {
                constraints
                    .iter()
                    .filter(|constraint| !constraint.unknown.is_empty())
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn unknown_cell_count(&self) -> usize {
        self.ambiguity_constraints
            .as_deref()
            .map(|constraints| {
                constraints
                    .iter()
                    .map(|constraint| usize::from(constraint.unknown.size()))
                    .sum()
            })
            .unwrap_or(0)
    }

    pub fn all_unknown_tip_count(&self) -> usize {
        let num_areas = self.area_names.len() as u8;
        self.ambiguity_constraints
            .as_deref()
            .map(|constraints| {
                constraints
                    .iter()
                    .filter(|constraint| constraint.unknown.size() == num_areas)
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn exact_null_tip_count(&self) -> usize {
        match self.ambiguity_constraints.as_deref() {
            Some(constraints) => constraints
                .iter()
                .filter(|constraint| {
                    constraint.required.is_empty() && constraint.unknown.is_empty()
                })
                .count(),
            None => self
                .tip_ranges
                .iter()
                .filter(|tip| tip.range.is_empty())
                .count(),
        }
    }

    pub fn maximum_possible_range_size(&self) -> u8 {
        match self.ambiguity_constraints.as_deref() {
            Some(constraints) => constraints
                .iter()
                .map(|constraint| constraint.required.size() + constraint.unknown.size())
                .max()
                .unwrap_or(0),
            None => self
                .tip_ranges
                .iter()
                .map(|tip| tip.range.size())
                .max()
                .unwrap_or(0),
        }
    }

    pub fn tip_likelihoods(
        &self,
        states: &StateSpace,
    ) -> Result<Vec<TipLikelihood>, RangeLikelihoodError> {
        if self.area_names.len() != usize::from(states.num_areas()) {
            return Err(RangeLikelihoodError::AreaCountMismatch {
                range_areas: self.area_names.len(),
                state_areas: usize::from(states.num_areas()),
            });
        }

        match self.ambiguity_constraints.as_deref() {
            None => self
                .tip_ranges
                .iter()
                .map(|tip| exact_tip_likelihood(states, tip.node, tip.range))
                .collect(),
            Some(constraints) => constraints
                .iter()
                .map(|constraint| ambiguous_tip_likelihood(states, *constraint))
                .collect(),
        }
    }
}

pub fn parse_tip_ranges_table(
    input: &str,
    tree: &ParsedNewickTree,
) -> Result<ParsedTipRanges, RangeParseError> {
    parse_tip_ranges_table_impl(input, tree, false)
}

pub fn parse_tip_ranges_table_with_ambiguities(
    input: &str,
    tree: &ParsedNewickTree,
) -> Result<ParsedTipRanges, RangeParseError> {
    parse_tip_ranges_table_impl(input, tree, true)
}

fn parse_tip_ranges_table_impl(
    input: &str,
    tree: &ParsedNewickTree,
    allow_ambiguities: bool,
) -> Result<ParsedTipRanges, RangeParseError> {
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

    let (header_line, header) = lines.next().ok_or(RangeParseError::EmptyInput)?;
    let header_fields = split_fields(header);
    if header_fields.len() < 2 {
        return Err(RangeParseError::HeaderNeedsAreas { line: header_line });
    }
    if header_fields[0] != "tip" {
        return Err(RangeParseError::InvalidHeaderFirstColumn {
            line: header_line,
            value: header_fields[0].to_string(),
        });
    }

    let area_names: Vec<String> = header_fields[1..]
        .iter()
        .map(|field| (*field).to_string())
        .collect();
    if area_names.len() > 64 {
        return Err(RangeParseError::TooManyAreas {
            count: area_names.len(),
        });
    }
    let mut seen_area_names = HashSet::new();
    for area_name in &area_names {
        if !seen_area_names.insert(area_name.as_str()) {
            return Err(RangeParseError::DuplicateAreaName {
                line: header_line,
                name: area_name.clone(),
            });
        }
    }

    let mut tip_ranges = Vec::new();
    let mut constraints = Vec::new();
    let mut has_ambiguities = false;
    let mut first_line_by_node = HashMap::new();
    let tip_node_by_label = tree
        .tip_labels
        .iter()
        .map(|tip| (tip.label.as_str(), tip.node))
        .collect::<HashMap<_, _>>();
    for (line_number, line) in lines {
        let fields = split_fields(line);
        if fields.len() != area_names.len() + 1 {
            return Err(RangeParseError::WrongColumnCount {
                line: line_number,
                expected: area_names.len() + 1,
                actual: fields.len(),
            });
        }

        let tip_label = fields[0];
        let node = tip_node_by_label.get(tip_label).copied().ok_or_else(|| {
            RangeParseError::UnknownTip {
                line: line_number,
                label: tip_label.to_string(),
            }
        })?;
        if let Some(first_line) = first_line_by_node.insert(node, line_number) {
            return Err(RangeParseError::DuplicateTip {
                line: line_number,
                first_line,
                label: tip_label.to_string(),
            });
        }

        let mut required_bits = 0_u64;
        let mut forbidden_bits = 0_u64;
        let mut unknown_bits = 0_u64;
        for (area_index, value) in fields[1..].iter().enumerate() {
            match *value {
                "0" => forbidden_bits |= 1_u64 << area_index,
                "1" => required_bits |= 1_u64 << area_index,
                "?" if allow_ambiguities => {
                    unknown_bits |= 1_u64 << area_index;
                    has_ambiguities = true;
                }
                _ => {
                    return Err(RangeParseError::InvalidPresenceValue {
                        line: line_number,
                        column: area_index + 2,
                        value: (*value).to_string(),
                        allow_ambiguities,
                    });
                }
            }
        }

        tip_ranges.push(TipRange {
            node,
            range: AreaSet::from_bits(required_bits),
        });
        constraints.push(TipRangeConstraint {
            node,
            required: AreaSet::from_bits(required_bits),
            forbidden: AreaSet::from_bits(forbidden_bits),
            unknown: AreaSet::from_bits(unknown_bits),
        });
    }

    let missing_labels = tree
        .tip_labels
        .iter()
        .filter(|tip| !first_line_by_node.contains_key(&tip.node))
        .map(|tip| tip.label.clone())
        .collect::<Vec<_>>();
    if !missing_labels.is_empty() {
        return Err(RangeParseError::MissingTips {
            labels: missing_labels,
        });
    }

    Ok(ParsedTipRanges {
        area_names,
        tip_ranges,
        ambiguity_constraints: has_ambiguities.then_some(constraints),
    })
}

fn exact_tip_likelihood(
    states: &StateSpace,
    node: usize,
    range: AreaSet,
) -> Result<TipLikelihood, RangeLikelihoodError> {
    let state_index = states
        .index_of(range)
        .ok_or(RangeLikelihoodError::NoCompatibleRange {
            node,
            required_bits: range.bits(),
            forbidden_bits: 0,
        })?;
    let mut likelihoods = vec![0.0; states.len()];
    likelihoods[state_index] = 1.0;
    Ok(TipLikelihood { node, likelihoods })
}

fn ambiguous_tip_likelihood(
    states: &StateSpace,
    constraint: TipRangeConstraint,
) -> Result<TipLikelihood, RangeLikelihoodError> {
    if constraint.unknown.is_empty() {
        return exact_tip_likelihood(states, constraint.node, constraint.required);
    }

    let all_areas_unknown = constraint.unknown.size() == states.num_areas();
    let required_bits = constraint.required.bits();
    let forbidden_bits = constraint.forbidden.bits();
    let likelihoods = states
        .states()
        .iter()
        .map(|state| {
            let bits = state.bits();
            let compatible = if all_areas_unknown {
                true
            } else {
                !state.is_empty()
                    && bits & required_bits == required_bits
                    && bits & forbidden_bits == 0
            };
            if compatible { 1.0 } else { 0.0 }
        })
        .collect::<Vec<_>>();
    if likelihoods.iter().all(|likelihood| *likelihood == 0.0) {
        return Err(RangeLikelihoodError::NoCompatibleRange {
            node: constraint.node,
            required_bits,
            forbidden_bits,
        });
    }
    Ok(TipLikelihood {
        node: constraint.node,
        likelihoods,
    })
}

fn split_fields(line: &str) -> Vec<&str> {
    if line.contains('\t') {
        line.split('\t').map(str::trim).collect()
    } else {
        line.split_whitespace().collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeParseError {
    EmptyInput,
    HeaderNeedsAreas {
        line: usize,
    },
    InvalidHeaderFirstColumn {
        line: usize,
        value: String,
    },
    TooManyAreas {
        count: usize,
    },
    DuplicateAreaName {
        line: usize,
        name: String,
    },
    WrongColumnCount {
        line: usize,
        expected: usize,
        actual: usize,
    },
    UnknownTip {
        line: usize,
        label: String,
    },
    DuplicateTip {
        line: usize,
        first_line: usize,
        label: String,
    },
    MissingTips {
        labels: Vec<String>,
    },
    InvalidPresenceValue {
        line: usize,
        column: usize,
        value: String,
        allow_ambiguities: bool,
    },
}

impl fmt::Display for RangeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "tip range table is empty"),
            Self::HeaderNeedsAreas { line } => {
                write!(
                    f,
                    "tip range header on line {line} must include at least one area"
                )
            }
            Self::InvalidHeaderFirstColumn { line, value } => write!(
                f,
                "tip range header first column on line {line} must be 'tip', got {value:?}"
            ),
            Self::TooManyAreas { count } => write!(
                f,
                "tip range table has {count} areas, but bitset ranges support at most 64"
            ),
            Self::DuplicateAreaName { line, name } => write!(
                f,
                "tip range header on line {line} contains duplicate area name {name:?}"
            ),
            Self::WrongColumnCount {
                line,
                expected,
                actual,
            } => write!(
                f,
                "tip range row on line {line} has {actual} columns, expected {expected}"
            ),
            Self::UnknownTip { line, label } => {
                write!(
                    f,
                    "tip range row on line {line} refers to unknown tip {label:?}"
                )
            }
            Self::DuplicateTip {
                line,
                first_line,
                label,
            } => write!(
                f,
                "tip range row on line {line} duplicates tip {label:?} first listed on line {first_line}"
            ),
            Self::MissingTips { labels } => write!(
                f,
                "tip range table is missing {} tree tip(s): {}",
                labels.len(),
                labels.join(", ")
            ),
            Self::InvalidPresenceValue {
                line,
                column,
                value,
                allow_ambiguities,
            } => {
                let expected = if *allow_ambiguities {
                    "0, 1, or ?"
                } else {
                    "0 or 1"
                };
                write!(
                    f,
                    "tip range value on line {line}, column {column} must be {expected}, got {value:?}"
                )
            }
        }
    }
}

impl Error for RangeParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeLikelihoodError {
    AreaCountMismatch {
        range_areas: usize,
        state_areas: usize,
    },
    NoCompatibleRange {
        node: usize,
        required_bits: u64,
        forbidden_bits: u64,
    },
}

impl fmt::Display for RangeLikelihoodError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AreaCountMismatch {
                range_areas,
                state_areas,
            } => write!(
                f,
                "tip range table has {range_areas} areas but the state space has {state_areas}"
            ),
            Self::NoCompatibleRange {
                node,
                required_bits,
                forbidden_bits,
            } => write!(
                f,
                "tip observation at node {node} has no compatible state (required bits {required_bits:#x}, forbidden bits {forbidden_bits:#x})"
            ),
        }
    }
}

impl Error for RangeLikelihoodError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newick::parse_newick;

    #[test]
    fn parses_tip_range_table() {
        let tree = parse_newick("(A:0,B:0);").unwrap();
        let parsed = parse_tip_ranges_table("tip\tX\tY\nA\t1\t0\nB\t0\t1\n", &tree).unwrap();

        assert_eq!(parsed.area_names, vec!["X", "Y"]);
        assert_eq!(
            parsed.tip_ranges,
            vec![
                TipRange {
                    node: 0,
                    range: AreaSet::from_bits(0b01)
                },
                TipRange {
                    node: 1,
                    range: AreaSet::from_bits(0b10)
                }
            ]
        );
    }

    #[test]
    fn tabular_range_table_preserves_spaces_in_tip_and_area_labels() {
        let tree = parse_newick("('Taxon A':0,'O''Brien':0);").unwrap();
        let parsed = parse_tip_ranges_table(
            "tip\tNorth Area\tSouth Area\nTaxon A\t1\t0\nO'Brien\t0\t1\n",
            &tree,
        )
        .unwrap();

        assert_eq!(parsed.area_names, ["North Area", "South Area"]);
        assert_eq!(parsed.tip_ranges.len(), 2);
    }

    #[test]
    fn rejects_unknown_tip() {
        let tree = parse_newick("(A:0,B:0);").unwrap();
        let error = parse_tip_ranges_table("tip\tX\tY\nC\t1\t0\n", &tree).unwrap_err();

        assert_eq!(
            error,
            RangeParseError::UnknownTip {
                line: 2,
                label: "C".to_string()
            }
        );
    }

    #[test]
    fn rejects_invalid_presence_value() {
        let tree = parse_newick("(A:0,B:0);").unwrap();
        let error = parse_tip_ranges_table("tip\tX\tY\nA\t1\t2\n", &tree).unwrap_err();

        assert_eq!(
            error,
            RangeParseError::InvalidPresenceValue {
                line: 2,
                column: 3,
                value: "2".to_string(),
                allow_ambiguities: false
            }
        );
    }

    #[test]
    fn parses_biogeobears_question_mark_constraints_only_when_enabled() {
        let tree = parse_newick("((human:1,chimp:1):1,gorilla:2);").unwrap();
        let input = "tip A B C\nhuman ? ? ?\nchimp 0 ? 1\ngorilla 1 ? 0\n";

        assert_eq!(
            parse_tip_ranges_table(input, &tree).unwrap_err(),
            RangeParseError::InvalidPresenceValue {
                line: 2,
                column: 2,
                value: "?".to_string(),
                allow_ambiguities: false
            }
        );

        let parsed = parse_tip_ranges_table_with_ambiguities(input, &tree).unwrap();
        assert!(parsed.has_ambiguities());
        assert_eq!(parsed.ambiguous_tip_count(), 3);
        assert_eq!(parsed.unknown_cell_count(), 5);
        assert_eq!(parsed.all_unknown_tip_count(), 1);
        assert_eq!(
            parsed.ambiguity_constraints,
            Some(vec![
                TipRangeConstraint {
                    node: 0,
                    required: AreaSet::EMPTY,
                    forbidden: AreaSet::EMPTY,
                    unknown: AreaSet::from_bits(0b111),
                },
                TipRangeConstraint {
                    node: 1,
                    required: AreaSet::from_bits(0b100),
                    forbidden: AreaSet::from_bits(0b001),
                    unknown: AreaSet::from_bits(0b010),
                },
                TipRangeConstraint {
                    node: 3,
                    required: AreaSet::from_bits(0b001),
                    forbidden: AreaSet::from_bits(0b100),
                    unknown: AreaSet::from_bits(0b010),
                },
            ])
        );
    }

    #[test]
    fn ambiguity_tip_likelihoods_match_biogeobears_1_1_3() {
        let tree = parse_newick("((human:1,chimp:1):1,gorilla:2);").unwrap();
        let parsed = parse_tip_ranges_table_with_ambiguities(
            "tip A B C\nhuman ? ? ?\nchimp 0 ? 1\ngorilla 1 ? 0\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(3, 2, true).unwrap();
        let likelihoods = parsed.tip_likelihoods(&states).unwrap();

        assert_eq!(
            likelihoods,
            vec![
                TipLikelihood {
                    node: 0,
                    likelihoods: vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
                },
                TipLikelihood {
                    node: 1,
                    likelihoods: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
                },
                TipLikelihood {
                    node: 3,
                    likelihoods: vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                },
            ]
        );
    }

    #[test]
    fn ambiguity_likelihoods_reject_constraints_excluded_by_the_state_space() {
        let tree = parse_newick("(A:0,B:0);").unwrap();
        let parsed =
            parse_tip_ranges_table_with_ambiguities("tip X Y Z\nA 1 1 ?\nB 0 ? 0\n", &tree)
                .unwrap();
        let states = StateSpace::new(3, 1, false).unwrap();

        assert_eq!(
            parsed.tip_likelihoods(&states).unwrap_err(),
            RangeLikelihoodError::NoCompatibleRange {
                node: 0,
                required_bits: 0b011,
                forbidden_bits: 0,
            }
        );
    }

    #[test]
    fn rejects_duplicate_area_names_and_tip_rows() {
        let tree = parse_newick("(A:0,B:0);").unwrap();
        assert_eq!(
            parse_tip_ranges_table("tip X X\nA 1 0\nB 0 1\n", &tree).unwrap_err(),
            RangeParseError::DuplicateAreaName {
                line: 1,
                name: "X".to_string()
            }
        );
        assert_eq!(
            parse_tip_ranges_table("tip X Y\nA 1 0\nA 0 1\nB 0 1\n", &tree).unwrap_err(),
            RangeParseError::DuplicateTip {
                line: 3,
                first_line: 2,
                label: "A".to_string()
            }
        );
    }

    #[test]
    fn reports_all_missing_tree_tips_in_tree_order() {
        let tree = parse_newick("((A:0,B:0):0,C:0);").unwrap();
        let error = parse_tip_ranges_table("tip X Y\nB 0 1\n", &tree).unwrap_err();

        assert_eq!(
            error,
            RangeParseError::MissingTips {
                labels: vec!["A".to_string(), "C".to_string()]
            }
        );
    }

    #[test]
    fn accepts_utf8_bom_before_comments() {
        let tree = parse_newick("(A:0,B:0);").unwrap();
        let parsed = parse_tip_ranges_table(
            "\u{feff}# format\tbiogeo-range-table-v1\ntip X Y\nA 1 0\nB 0 1\n",
            &tree,
        )
        .unwrap();

        assert_eq!(parsed.area_names, ["X", "Y"]);
        assert_eq!(parsed.tip_ranges.len(), 2);
    }
}
