use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt;

use crate::dec::TipRange;
use crate::newick::ParsedNewickTree;
use crate::pruning::TipLikelihood;
use crate::state::{AreaSet, StateSpace};

#[derive(Clone, Debug, PartialEq)]
pub struct TipDetectionCounts {
    pub node: usize,
    pub label: String,
    pub detections: Vec<f64>,
    pub controls: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DetectionData {
    pub area_names: Vec<String>,
    pub tips: Vec<TipDetectionCounts>,
}

impl DetectionData {
    pub fn observed_tip_ranges(&self) -> Vec<TipRange> {
        self.tips
            .iter()
            .map(|tip| {
                let bits = tip
                    .detections
                    .iter()
                    .enumerate()
                    .filter(|(_, count)| **count > 0.0)
                    .fold(0_u64, |bits, (area, _)| bits | (1_u64 << area));
                TipRange {
                    node: tip.node,
                    range: AreaSet::from_bits(bits),
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectionModel {
    pub mean_frequency: f64,
    pub detection_probability: f64,
    pub false_detection_probability: f64,
}

impl DetectionModel {
    pub fn new(
        mean_frequency: f64,
        detection_probability: f64,
        false_detection_probability: f64,
    ) -> Result<Self, DetectionModelError> {
        for (name, value) in [
            ("mf", mean_frequency),
            ("dp", detection_probability),
            ("fdp", false_detection_probability),
        ] {
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                return Err(DetectionModelError::InvalidProbability { name, value });
            }
        }

        Ok(Self {
            mean_frequency,
            detection_probability,
            false_detection_probability,
        })
    }

    pub fn tip_likelihoods(
        &self,
        data: &DetectionData,
        states: &StateSpace,
    ) -> Result<Vec<TipLikelihood>, DetectionModelError> {
        if data.area_names.len() != usize::from(states.num_areas()) {
            return Err(DetectionModelError::AreaCountMismatch {
                data_areas: data.area_names.len(),
                state_areas: usize::from(states.num_areas()),
            });
        }

        data.tips
            .iter()
            .map(|tip| self.tip_likelihood(tip, states))
            .collect()
    }

    fn tip_likelihood(
        &self,
        tip: &TipDetectionCounts,
        states: &StateSpace,
    ) -> Result<TipLikelihood, DetectionModelError> {
        let log_likelihoods = states
            .states()
            .iter()
            .map(|state| {
                if state.is_empty() {
                    return f64::NEG_INFINITY;
                }

                tip.detections
                    .iter()
                    .zip(&tip.controls)
                    .enumerate()
                    .map(|(area, (detections, controls))| {
                        self.area_log_likelihood(state.contains(area as u8), *detections, *controls)
                    })
                    .sum::<f64>()
            })
            .collect::<Vec<_>>();

        let max_log_likelihood = log_likelihoods
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        if !max_log_likelihood.is_finite() {
            return Err(DetectionModelError::NoFiniteStateLikelihood {
                node: tip.node,
                label: tip.label.clone(),
            });
        }

        let likelihoods = log_likelihoods
            .into_iter()
            .map(|log_likelihood| {
                if log_likelihood == f64::NEG_INFINITY {
                    0.0
                } else {
                    (log_likelihood - max_log_likelihood).exp()
                }
            })
            .collect();

        Ok(TipLikelihood {
            node: tip.node,
            likelihoods,
        })
    }

    fn area_log_likelihood(&self, truly_present: bool, detections: f64, controls: f64) -> f64 {
        if controls == 0.0 {
            return 0.0;
        }

        let detected_probability = if truly_present {
            self.mean_frequency * self.detection_probability
                + (1.0 - self.mean_frequency) * self.false_detection_probability
        } else {
            self.false_detection_probability
        }
        .clamp(0.0, 1.0);
        let not_detected_probability = (1.0 - detected_probability).clamp(0.0, 1.0);
        let non_target_detections = controls - detections;

        count_log_probability(detections, detected_probability)
            + count_log_probability(non_target_detections, not_detected_probability)
    }
}

fn count_log_probability(count: f64, probability: f64) -> f64 {
    if count == 0.0 {
        0.0
    } else if probability == 0.0 {
        f64::NEG_INFINITY
    } else {
        count * probability.ln()
    }
}

pub fn parse_detection_data(
    detections_input: &str,
    controls_input: &str,
    tree: &ParsedNewickTree,
) -> Result<DetectionData, DetectionDataParseError> {
    let detections = parse_count_table(detections_input, "detections", tree)?;
    let controls = parse_count_table(controls_input, "controls", tree)?;

    if detections.area_names != controls.area_names {
        return Err(DetectionDataParseError::AreaNamesMismatch {
            detections: detections.area_names,
            controls: controls.area_names,
        });
    }

    let mut tips = Vec::with_capacity(detections.rows.len());
    for (detections_row, controls_row) in detections.rows.into_iter().zip(controls.rows) {
        debug_assert_eq!(detections_row.node, controls_row.node);
        for (area, (detection_count, control_count)) in detections_row
            .counts
            .iter()
            .zip(&controls_row.counts)
            .enumerate()
        {
            if detection_count > control_count {
                return Err(DetectionDataParseError::DetectionExceedsControl {
                    tip: detections_row.label.clone(),
                    area: detections.area_names[area].clone(),
                    detections: *detection_count,
                    controls: *control_count,
                });
            }
        }
        tips.push(TipDetectionCounts {
            node: detections_row.node,
            label: detections_row.label,
            detections: detections_row.counts,
            controls: controls_row.counts,
        });
    }

    Ok(DetectionData {
        area_names: detections.area_names,
        tips,
    })
}

#[derive(Clone, Debug, PartialEq)]
struct CountTable {
    area_names: Vec<String>,
    rows: Vec<CountRow>,
}

#[derive(Clone, Debug, PartialEq)]
struct CountRow {
    node: usize,
    label: String,
    counts: Vec<f64>,
}

fn parse_count_table(
    input: &str,
    table: &'static str,
    tree: &ParsedNewickTree,
) -> Result<CountTable, DetectionDataParseError> {
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
        .ok_or(DetectionDataParseError::EmptyInput { table })?;
    let header_fields = split_count_fields(header);
    let area_fields = match header_fields.first().copied() {
        Some("tip" | "Tip" | "TIP" | "otu" | "OTU") => &header_fields[1..],
        _ => &header_fields[..],
    };
    if area_fields.is_empty() {
        return Err(DetectionDataParseError::HeaderNeedsAreas {
            table,
            line: header_line,
        });
    }
    if area_fields.len() > 64 {
        return Err(DetectionDataParseError::TooManyAreas {
            table,
            count: area_fields.len(),
        });
    }

    let mut seen_areas = HashSet::new();
    let mut area_names = Vec::with_capacity(area_fields.len());
    for area in area_fields {
        if !seen_areas.insert(*area) {
            return Err(DetectionDataParseError::DuplicateArea {
                table,
                line: header_line,
                area: (*area).to_owned(),
            });
        }
        area_names.push((*area).to_owned());
    }

    let mut seen_nodes = HashSet::new();
    let mut rows = Vec::with_capacity(tree.tip_labels.len());
    let tip_node_by_label = tree
        .tip_labels
        .iter()
        .map(|tip| (tip.label.as_str(), tip.node))
        .collect::<HashMap<_, _>>();
    for (line_number, line) in lines {
        let fields = split_count_fields(line);
        let expected = area_names.len() + 1;
        if fields.len() != expected {
            return Err(DetectionDataParseError::WrongColumnCount {
                table,
                line: line_number,
                expected,
                actual: fields.len(),
            });
        }

        let label = fields[0];
        let node = tip_node_by_label.get(label).copied().ok_or_else(|| {
            DetectionDataParseError::UnknownTip {
                table,
                line: line_number,
                label: label.to_owned(),
            }
        })?;
        if !seen_nodes.insert(node) {
            return Err(DetectionDataParseError::DuplicateTip {
                table,
                line: line_number,
                label: label.to_owned(),
            });
        }

        let mut counts = Vec::with_capacity(area_names.len());
        for (area, field) in fields[1..].iter().enumerate() {
            let count =
                field
                    .parse::<f64>()
                    .map_err(|_| DetectionDataParseError::InvalidCount {
                        table,
                        line: line_number,
                        column: area + 2,
                        value: (*field).to_owned(),
                    })?;
            if !count.is_finite() {
                return Err(DetectionDataParseError::NonFiniteCount {
                    table,
                    line: line_number,
                    column: area + 2,
                    value: count,
                });
            }
            if count < 0.0 {
                return Err(DetectionDataParseError::NegativeCount {
                    table,
                    line: line_number,
                    column: area + 2,
                    value: count,
                });
            }
            counts.push(count);
        }
        rows.push(CountRow {
            node,
            label: label.to_owned(),
            counts,
        });
    }

    if let Some(missing) = tree
        .tip_labels
        .iter()
        .find(|tip| !seen_nodes.contains(&tip.node))
    {
        return Err(DetectionDataParseError::MissingTip {
            table,
            label: missing.label.clone(),
        });
    }

    rows.sort_by_key(|row| row.node);
    Ok(CountTable { area_names, rows })
}

fn split_count_fields(line: &str) -> Vec<&str> {
    if line.contains('\t') {
        line.split('\t').map(str::trim).collect()
    } else {
        line.split_whitespace().collect()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DetectionDataParseError {
    EmptyInput {
        table: &'static str,
    },
    HeaderNeedsAreas {
        table: &'static str,
        line: usize,
    },
    TooManyAreas {
        table: &'static str,
        count: usize,
    },
    DuplicateArea {
        table: &'static str,
        line: usize,
        area: String,
    },
    WrongColumnCount {
        table: &'static str,
        line: usize,
        expected: usize,
        actual: usize,
    },
    UnknownTip {
        table: &'static str,
        line: usize,
        label: String,
    },
    DuplicateTip {
        table: &'static str,
        line: usize,
        label: String,
    },
    MissingTip {
        table: &'static str,
        label: String,
    },
    InvalidCount {
        table: &'static str,
        line: usize,
        column: usize,
        value: String,
    },
    NonFiniteCount {
        table: &'static str,
        line: usize,
        column: usize,
        value: f64,
    },
    NegativeCount {
        table: &'static str,
        line: usize,
        column: usize,
        value: f64,
    },
    AreaNamesMismatch {
        detections: Vec<String>,
        controls: Vec<String>,
    },
    DetectionExceedsControl {
        tip: String,
        area: String,
        detections: f64,
        controls: f64,
    },
}

impl fmt::Display for DetectionDataParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput { table } => write!(f, "{table} count table is empty"),
            Self::HeaderNeedsAreas { table, line } => write!(
                f,
                "{table} count table header on line {line} must include at least one area"
            ),
            Self::TooManyAreas { table, count } => write!(
                f,
                "{table} count table has {count} areas, but bitset ranges support at most 64"
            ),
            Self::DuplicateArea { table, line, area } => write!(
                f,
                "{table} count table header on line {line} repeats area {area:?}"
            ),
            Self::WrongColumnCount {
                table,
                line,
                expected,
                actual,
            } => write!(
                f,
                "{table} count row on line {line} has {actual} columns, expected {expected}"
            ),
            Self::UnknownTip { table, line, label } => write!(
                f,
                "{table} count row on line {line} refers to unknown tip {label:?}"
            ),
            Self::DuplicateTip { table, line, label } => {
                write!(f, "{table} count row on line {line} repeats tip {label:?}")
            }
            Self::MissingTip { table, label } => {
                write!(f, "{table} count table is missing tree tip {label:?}")
            }
            Self::InvalidCount {
                table,
                line,
                column,
                value,
            } => write!(
                f,
                "{table} count on line {line}, column {column} is not numeric: {value:?}"
            ),
            Self::NonFiniteCount {
                table,
                line,
                column,
                value,
            } => write!(
                f,
                "{table} count on line {line}, column {column} must be finite, got {value}"
            ),
            Self::NegativeCount {
                table,
                line,
                column,
                value,
            } => write!(
                f,
                "{table} count on line {line}, column {column} must be non-negative, got {value}"
            ),
            Self::AreaNamesMismatch {
                detections,
                controls,
            } => write!(
                f,
                "detections and controls area columns differ: detections={detections:?}, controls={controls:?}"
            ),
            Self::DetectionExceedsControl {
                tip,
                area,
                detections,
                controls,
            } => write!(
                f,
                "detection count exceeds its inclusive control count for tip {tip:?}, area {area:?}: {detections} > {controls}"
            ),
        }
    }
}

impl Error for DetectionDataParseError {}

#[derive(Clone, Debug, PartialEq)]
pub enum DetectionModelError {
    InvalidProbability {
        name: &'static str,
        value: f64,
    },
    AreaCountMismatch {
        data_areas: usize,
        state_areas: usize,
    },
    NoFiniteStateLikelihood {
        node: usize,
        label: String,
    },
}

impl fmt::Display for DetectionModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProbability { name, value } => {
                write!(
                    f,
                    "detection parameter {name} must be finite and in [0, 1], got {value}"
                )
            }
            Self::AreaCountMismatch {
                data_areas,
                state_areas,
            } => write!(
                f,
                "detection data has {data_areas} areas, but the state space has {state_areas}"
            ),
            Self::NoFiniteStateLikelihood { node, label } => write!(
                f,
                "detection data for tip {label:?} (node {node}) has zero likelihood under every non-null state"
            ),
        }
    }
}

impl Error for DetectionModelError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::newick::parse_newick;

    fn assert_close(left: f64, right: f64, tolerance: f64) {
        assert!(
            (left - right).abs() <= tolerance,
            "values differ: left={left}, right={right}, tolerance={tolerance}"
        );
    }

    #[test]
    fn parses_official_biogeobears_count_layout_and_tree_order() {
        let tree = parse_newick("(A:1,B:1);").unwrap();
        let data = parse_detection_data(
            "\tK\tO\nB\t0\t2\nA\t3\t0\n",
            "\tK\tO\nA\t5\t4\nB\t4\t6\n",
            &tree,
        )
        .unwrap();

        assert_eq!(data.area_names, vec!["K", "O"]);
        assert_eq!(data.tips[0].label, "A");
        assert_eq!(data.tips[0].detections, vec![3.0, 0.0]);
        assert_eq!(data.tips[1].label, "B");
        assert_eq!(
            data.observed_tip_ranges(),
            vec![
                TipRange {
                    node: 0,
                    range: AreaSet::from_bits(0b01),
                },
                TipRange {
                    node: 1,
                    range: AreaSet::from_bits(0b10),
                },
            ]
        );
    }

    #[test]
    fn tabular_count_tables_preserve_spaces_in_tip_and_area_labels() {
        let tree = parse_newick("('Taxon A':1,'O''Brien':1);").unwrap();
        let data = parse_detection_data(
            "tip\tNorth Area\tSouth Area\nTaxon A\t1\t0\nO'Brien\t0\t1\n",
            "tip\tNorth Area\tSouth Area\nTaxon A\t2\t2\nO'Brien\t2\t2\n",
            &tree,
        )
        .unwrap();

        assert_eq!(data.area_names, ["North Area", "South Area"]);
        assert_eq!(data.tips[0].label, "Taxon A");
        assert_eq!(data.tips[1].label, "O'Brien");
    }

    #[test]
    fn rejects_detection_counts_above_inclusive_controls() {
        let tree = parse_newick("(A:1,B:1);").unwrap();
        let error = parse_detection_data("K\nA\t2\nB\t0\n", "K\nA\t1\nB\t1\n", &tree).unwrap_err();

        assert_eq!(
            error,
            DetectionDataParseError::DetectionExceedsControl {
                tip: "A".to_owned(),
                area: "K".to_owned(),
                detections: 2.0,
                controls: 1.0,
            }
        );
    }

    #[test]
    fn rejects_mismatched_area_columns_and_missing_tips() {
        let tree = parse_newick("(A:1,B:1);").unwrap();
        let missing = parse_detection_data("K\nA\t1\n", "K\nA\t1\nB\t1\n", &tree).unwrap_err();
        assert_eq!(
            missing,
            DetectionDataParseError::MissingTip {
                table: "detections",
                label: "B".to_owned(),
            }
        );

        let mismatch =
            parse_detection_data("K\nA\t1\nB\t0\n", "O\nA\t1\nB\t1\n", &tree).unwrap_err();
        assert_eq!(
            mismatch,
            DetectionDataParseError::AreaNamesMismatch {
                detections: vec!["K".to_owned()],
                controls: vec!["O".to_owned()],
            }
        );
    }

    #[test]
    fn reproduces_biogeobears_relative_tip_likelihood_formula() {
        let tree = parse_newick("(A:1,B:1);").unwrap();
        let data = parse_detection_data(
            "K\tO\nA\t1\t0\nB\t0\t1\n",
            "K\tO\nA\t2\t2\nB\t2\t2\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let model = DetectionModel::new(0.2, 0.8, 0.1).unwrap();
        let likelihoods = model.tip_likelihoods(&data, &states).unwrap();

        let present_detect: f64 = 0.2 * 0.8 + 0.8 * 0.1;
        let present_non_detect = 1.0 - present_detect;
        let absent_detect = 0.1_f64;
        let absent_non_detect = 0.9_f64;
        let log_k = present_detect.ln() + present_non_detect.ln() + 2.0 * absent_non_detect.ln();
        let log_o = absent_detect.ln() + absent_non_detect.ln() + 2.0 * present_non_detect.ln();
        let log_ko = present_detect.ln() + present_non_detect.ln() + 2.0 * present_non_detect.ln();
        let max_log = log_k.max(log_o).max(log_ko);

        assert_eq!(likelihoods[0].likelihoods[0], 0.0);
        assert_close(
            likelihoods[0].likelihoods[1],
            (log_k - max_log).exp(),
            1e-14,
        );
        assert_close(
            likelihoods[0].likelihoods[2],
            (log_o - max_log).exp(),
            1e-14,
        );
        assert_close(
            likelihoods[0].likelihoods[3],
            (log_ko - max_log).exp(),
            1e-14,
        );
    }

    #[test]
    fn perfect_detection_can_recover_a_one_hot_tip_range() {
        let tree = parse_newick("(A:1,B:1);").unwrap();
        let data = parse_detection_data(
            "K\tO\nA\t1\t0\nB\t0\t1\n",
            "K\tO\nA\t1\t1\nB\t1\t1\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let model = DetectionModel::new(1.0, 1.0, 0.0).unwrap();
        let likelihoods = model.tip_likelihoods(&data, &states).unwrap();

        assert_eq!(likelihoods[0].likelihoods, vec![0.0, 1.0, 0.0, 0.0]);
        assert_eq!(likelihoods[1].likelihoods, vec![0.0, 0.0, 1.0, 0.0]);
    }

    #[test]
    fn zero_controls_are_uninformative_for_every_non_null_state() {
        let tree = parse_newick("(A:1,B:1);").unwrap();
        let data = parse_detection_data(
            "K\tO\nA\t0\t0\nB\t0\t0\n",
            "K\tO\nA\t0\t0\nB\t0\t0\n",
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(2, 2, true).unwrap();
        let model = DetectionModel::new(0.1, 1.0, 0.0).unwrap();
        let likelihoods = model.tip_likelihoods(&data, &states).unwrap();

        assert_eq!(likelihoods[0].likelihoods, vec![0.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn rejects_detection_parameters_outside_probabilities() {
        assert_eq!(
            DetectionModel::new(0.1, 1.1, 0.0),
            Err(DetectionModelError::InvalidProbability {
                name: "dp",
                value: 1.1,
            })
        );
    }
}
