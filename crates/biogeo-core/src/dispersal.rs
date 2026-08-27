use std::error::Error;
use std::fmt;

use crate::constraints::RangeStateConstraint;

#[derive(Clone, Debug, PartialEq)]
pub struct DispersalMultiplierMatrix {
    num_areas: usize,
    values: Vec<f64>,
}

impl DispersalMultiplierMatrix {
    pub fn new(num_areas: usize, values: Vec<f64>) -> Result<Self, DispersalMatrixError> {
        if num_areas == 0 {
            return Err(DispersalMatrixError::ZeroAreas);
        }
        let expected = num_areas
            .checked_mul(num_areas)
            .ok_or(DispersalMatrixError::DimensionOverflow { num_areas })?;
        if values.len() != expected {
            return Err(DispersalMatrixError::ValueCountMismatch {
                num_areas,
                expected,
                actual: values.len(),
            });
        }

        for (index, value) in values.iter().copied().enumerate() {
            let from = index / num_areas;
            let to = index % num_areas;
            if !value.is_finite() {
                return Err(DispersalMatrixError::NonFiniteMultiplier { from, to, value });
            }
            if value < 0.0 {
                return Err(DispersalMatrixError::NegativeMultiplier { from, to, value });
            }
        }

        Ok(Self { num_areas, values })
    }

    pub fn num_areas(&self) -> usize {
        self.num_areas
    }

    pub fn get(&self, from: usize, to: usize) -> f64 {
        self.values[from * self.num_areas + to]
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    pub fn distance_power_checked(&self, exponent: f64) -> Result<Self, DispersalMatrixError> {
        if !exponent.is_finite() {
            return Err(DispersalMatrixError::NonFiniteExponent { exponent });
        }

        let mut values = Vec::with_capacity(self.values.len());
        for (index, value) in self.values.iter().copied().enumerate() {
            let from = index / self.num_areas;
            let to = index % self.num_areas;
            if from == to {
                values.push(1.0);
                continue;
            }
            let transformed = value.powf(exponent);
            if !transformed.is_finite() {
                return Err(DispersalMatrixError::NonFinitePower {
                    from,
                    to,
                    value,
                    exponent,
                });
            }
            values.push(transformed);
        }

        Self::new(self.num_areas, values)
    }

    pub fn elementwise_product(&self, other: &Self) -> Result<Self, DispersalMatrixError> {
        if self.num_areas != other.num_areas {
            return Err(DispersalMatrixError::ProductAreaCountMismatch {
                left_areas: self.num_areas,
                right_areas: other.num_areas,
            });
        }

        let mut values = Vec::with_capacity(self.values.len());
        for (index, (left, right)) in self.values.iter().zip(&other.values).enumerate() {
            let product = left * right;
            if !product.is_finite() {
                return Err(DispersalMatrixError::NonFiniteProduct {
                    from: index / self.num_areas,
                    to: index % self.num_areas,
                    left: *left,
                    right: *right,
                });
            }
            values.push(product);
        }

        Self::new(self.num_areas, values)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExtirpationMultiplierVector {
    values: Vec<f64>,
}

impl ExtirpationMultiplierVector {
    pub fn new(values: Vec<f64>) -> Result<Self, ExtirpationMultiplierError> {
        if values.is_empty() {
            return Err(ExtirpationMultiplierError::ZeroAreas);
        }
        for (area, value) in values.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(ExtirpationMultiplierError::NonFiniteMultiplier { area, value });
            }
            if value < 0.0 {
                return Err(ExtirpationMultiplierError::NegativeMultiplier { area, value });
            }
        }
        Ok(Self { values })
    }

    pub fn num_areas(&self) -> usize {
        self.values.len()
    }

    pub fn get(&self, area: usize) -> f64 {
        self.values[area]
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AreaSizeVector {
    values: Vec<f64>,
}

impl AreaSizeVector {
    pub fn new(values: Vec<f64>) -> Result<Self, AreaSizeError> {
        if values.is_empty() {
            return Err(AreaSizeError::ZeroAreas);
        }
        for (area, value) in values.iter().copied().enumerate() {
            if !value.is_finite() {
                return Err(AreaSizeError::NonFiniteSize { area, value });
            }
            if value <= 0.0 {
                return Err(AreaSizeError::NonPositiveSize { area, value });
            }
        }
        Ok(Self { values })
    }

    pub fn num_areas(&self) -> usize {
        self.values.len()
    }

    pub fn get(&self, area: usize) -> f64 {
        self.values[area]
    }

    pub fn values(&self) -> &[f64] {
        &self.values
    }

    pub fn is_uniform(&self) -> bool {
        self.values.iter().all(|value| *value == self.values[0])
    }

    pub fn extirpation_multipliers(
        &self,
        exponent: f64,
    ) -> Result<ExtirpationMultiplierVector, AreaSizeError> {
        if !exponent.is_finite() {
            return Err(AreaSizeError::NonFiniteExponent { exponent });
        }

        let mut values = Vec::with_capacity(self.values.len());
        for (area, size) in self.values.iter().copied().enumerate() {
            let multiplier = size.powf(exponent);
            if !multiplier.is_finite() {
                return Err(AreaSizeError::NonFinitePower {
                    area,
                    size,
                    exponent,
                });
            }
            values.push(multiplier);
        }
        ExtirpationMultiplierVector::new(values)
            .map_err(AreaSizeError::InvalidExtirpationMultipliers)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispersalTimeStratum {
    pub oldest_age: f64,
    pub multipliers: DispersalMultiplierMatrix,
}

impl DispersalTimeStratum {
    pub fn new(
        oldest_age: f64,
        multipliers: DispersalMultiplierMatrix,
    ) -> Result<Self, DispersalScheduleError> {
        if !oldest_age.is_finite() || oldest_age <= 0.0 {
            return Err(DispersalScheduleError::InvalidOldestAge {
                index: 0,
                oldest_age,
            });
        }
        Ok(Self {
            oldest_age,
            multipliers,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeStratifiedDispersal {
    strata: Vec<DispersalTimeStratum>,
}

impl TimeStratifiedDispersal {
    pub fn new(strata: Vec<DispersalTimeStratum>) -> Result<Self, DispersalScheduleError> {
        if strata.is_empty() {
            return Err(DispersalScheduleError::EmptyStrata);
        }

        let expected_areas = strata[0].multipliers.num_areas();
        let mut previous_age = 0.0;
        for (index, stratum) in strata.iter().enumerate() {
            if !stratum.oldest_age.is_finite() || stratum.oldest_age <= 0.0 {
                return Err(DispersalScheduleError::InvalidOldestAge {
                    index,
                    oldest_age: stratum.oldest_age,
                });
            }
            if stratum.oldest_age <= previous_age {
                return Err(DispersalScheduleError::NonIncreasingOldestAge {
                    index,
                    previous: previous_age,
                    current: stratum.oldest_age,
                });
            }
            if stratum.multipliers.num_areas() != expected_areas {
                return Err(DispersalScheduleError::AreaCountMismatch {
                    index,
                    expected: expected_areas,
                    actual: stratum.multipliers.num_areas(),
                });
            }
            previous_age = stratum.oldest_age;
        }

        Ok(Self { strata })
    }

    pub fn strata(&self) -> &[DispersalTimeStratum] {
        &self.strata
    }

    pub fn num_areas(&self) -> usize {
        self.strata[0].multipliers.num_areas()
    }

    pub fn oldest_age(&self) -> f64 {
        self.strata
            .last()
            .expect("validated time-stratified schedule is non-empty")
            .oldest_age
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnageneticTimeStratum {
    pub oldest_age: f64,
    pub dispersal_multipliers: Option<DispersalMultiplierMatrix>,
    pub extirpation_multipliers: Option<ExtirpationMultiplierVector>,
    pub state_constraint: Option<RangeStateConstraint>,
}

impl AnageneticTimeStratum {
    pub fn new(
        oldest_age: f64,
        dispersal_multipliers: Option<DispersalMultiplierMatrix>,
        extirpation_multipliers: Option<ExtirpationMultiplierVector>,
    ) -> Result<Self, DispersalScheduleError> {
        if !oldest_age.is_finite() || oldest_age <= 0.0 {
            return Err(DispersalScheduleError::InvalidOldestAge {
                index: 0,
                oldest_age,
            });
        }
        if let (Some(dispersal), Some(extirpation)) =
            (&dispersal_multipliers, &extirpation_multipliers)
            && dispersal.num_areas() != extirpation.num_areas()
        {
            return Err(DispersalScheduleError::AreaCountMismatch {
                index: 0,
                expected: dispersal.num_areas(),
                actual: extirpation.num_areas(),
            });
        }

        Ok(Self {
            oldest_age,
            dispersal_multipliers,
            extirpation_multipliers,
            state_constraint: None,
        })
    }

    pub fn with_state_constraint(
        mut self,
        constraint: RangeStateConstraint,
    ) -> Result<Self, DispersalScheduleError> {
        if let (Some(expected), Some(actual)) = (self.num_areas(), constraint.num_areas())
            && expected != actual
        {
            return Err(DispersalScheduleError::AreaCountMismatch {
                index: 0,
                expected,
                actual,
            });
        }
        self.state_constraint = Some(constraint);
        Ok(self)
    }

    fn num_areas(&self) -> Option<usize> {
        self.dispersal_multipliers
            .as_ref()
            .map(DispersalMultiplierMatrix::num_areas)
            .or_else(|| {
                self.extirpation_multipliers
                    .as_ref()
                    .map(ExtirpationMultiplierVector::num_areas)
            })
            .or_else(|| {
                self.state_constraint
                    .as_ref()
                    .and_then(RangeStateConstraint::num_areas)
            })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TimeStratifiedAnagenesis {
    strata: Vec<AnageneticTimeStratum>,
    num_areas: Option<usize>,
}

impl TimeStratifiedAnagenesis {
    pub fn new(strata: Vec<AnageneticTimeStratum>) -> Result<Self, DispersalScheduleError> {
        if strata.is_empty() {
            return Err(DispersalScheduleError::EmptyStrata);
        }

        let mut expected_areas = None;
        let mut previous_age = 0.0;
        for (index, stratum) in strata.iter().enumerate() {
            if !stratum.oldest_age.is_finite() || stratum.oldest_age <= 0.0 {
                return Err(DispersalScheduleError::InvalidOldestAge {
                    index,
                    oldest_age: stratum.oldest_age,
                });
            }
            if stratum.oldest_age <= previous_age {
                return Err(DispersalScheduleError::NonIncreasingOldestAge {
                    index,
                    previous: previous_age,
                    current: stratum.oldest_age,
                });
            }
            if let Some(actual) = stratum.num_areas() {
                if let Some(expected) = expected_areas {
                    if actual != expected {
                        return Err(DispersalScheduleError::AreaCountMismatch {
                            index,
                            expected,
                            actual,
                        });
                    }
                } else {
                    expected_areas = Some(actual);
                }
            }
            previous_age = stratum.oldest_age;
        }

        Ok(Self {
            strata,
            num_areas: expected_areas,
        })
    }

    pub fn strata(&self) -> &[AnageneticTimeStratum] {
        &self.strata
    }

    pub fn num_areas(&self) -> Option<usize> {
        self.num_areas
    }

    pub fn oldest_age(&self) -> f64 {
        self.strata
            .last()
            .expect("validated time-stratified schedule is non-empty")
            .oldest_age
    }

    pub fn stratum_index_at_age(&self, age: f64) -> usize {
        self.strata
            .iter()
            .position(|stratum| age <= stratum.oldest_age + 1e-12)
            .unwrap_or(self.strata.len() - 1)
    }
}

impl From<TimeStratifiedDispersal> for TimeStratifiedAnagenesis {
    fn from(schedule: TimeStratifiedDispersal) -> Self {
        let num_areas = schedule.num_areas();
        let strata = schedule
            .strata
            .into_iter()
            .map(|stratum| AnageneticTimeStratum {
                oldest_age: stratum.oldest_age,
                dispersal_multipliers: Some(stratum.multipliers),
                extirpation_multipliers: None,
                state_constraint: None,
            })
            .collect();
        Self {
            strata,
            num_areas: Some(num_areas),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DispersalStratumSpec {
    pub oldest_age: f64,
    pub matrix_path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnageneticStratumSpec {
    pub oldest_age: f64,
    pub dispersal_matrix_path: Option<String>,
    pub distance_matrix_path: Option<String>,
    pub environment_distance_matrix_path: Option<String>,
    pub area_sizes_path: Option<String>,
    pub areas_allowed_path: Option<String>,
    pub areas_adjacency_path: Option<String>,
    pub allowed_ranges_path: Option<String>,
}

pub fn parse_anagenetic_strata_table(
    input: &str,
) -> Result<Vec<AnageneticStratumSpec>, AnageneticStrataParseError> {
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

    let (header_line, header) = lines.next().ok_or(AnageneticStrataParseError::EmptyInput)?;
    let header_fields: Vec<&str> = header.split_whitespace().collect();
    let legacy_header = [
        "oldest_age",
        "matrix",
        "distance_matrix",
        "environment_distance_matrix",
        "area_sizes",
    ];
    let constrained_header = [
        "oldest_age",
        "matrix",
        "distance_matrix",
        "environment_distance_matrix",
        "area_sizes",
        "areas_allowed",
        "areas_adjacency",
    ];
    let explicit_ranges_header = [
        "oldest_age",
        "matrix",
        "distance_matrix",
        "environment_distance_matrix",
        "area_sizes",
        "areas_allowed",
        "areas_adjacency",
        "allowed_ranges",
    ];
    let constraint_columns = if header_fields == explicit_ranges_header {
        3
    } else if header_fields == constrained_header {
        2
    } else if header_fields == legacy_header {
        0
    } else {
        return Err(AnageneticStrataParseError::InvalidHeader {
            line: header_line,
            actual: header_fields.join(" "),
        });
    };
    let expected_columns = match constraint_columns {
        3 => explicit_ranges_header.len(),
        2 => constrained_header.len(),
        _ => legacy_header.len(),
    };

    let mut specs = Vec::new();
    let mut previous_age = 0.0;
    for (line_number, line) in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != expected_columns {
            return Err(AnageneticStrataParseError::WrongColumnCount {
                line: line_number,
                actual: fields.len(),
            });
        }
        let oldest_age =
            fields[0]
                .parse::<f64>()
                .map_err(|_| AnageneticStrataParseError::InvalidOldestAge {
                    line: line_number,
                    value: fields[0].to_string(),
                })?;
        if !oldest_age.is_finite() || oldest_age <= previous_age {
            return Err(AnageneticStrataParseError::NonIncreasingOldestAge {
                line: line_number,
                previous: previous_age,
                current: oldest_age,
            });
        }
        specs.push(AnageneticStratumSpec {
            oldest_age,
            dispersal_matrix_path: optional_stratum_path(fields[1]),
            distance_matrix_path: optional_stratum_path(fields[2]),
            environment_distance_matrix_path: optional_stratum_path(fields[3]),
            area_sizes_path: optional_stratum_path(fields[4]),
            areas_allowed_path: (constraint_columns >= 2)
                .then(|| optional_stratum_path(fields[5]))
                .flatten(),
            areas_adjacency_path: (constraint_columns >= 2)
                .then(|| optional_stratum_path(fields[6]))
                .flatten(),
            allowed_ranges_path: (constraint_columns == 3)
                .then(|| optional_stratum_path(fields[7]))
                .flatten(),
        });
        previous_age = oldest_age;
    }

    if specs.is_empty() {
        return Err(AnageneticStrataParseError::NoStrata);
    }
    Ok(specs)
}

fn optional_stratum_path(value: &str) -> Option<String> {
    if value == "-" || value.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(value.to_string())
    }
}

pub fn parse_dispersal_strata_table(
    input: &str,
) -> Result<Vec<DispersalStratumSpec>, DispersalStrataParseError> {
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

    let (header_line, header) = lines.next().ok_or(DispersalStrataParseError::EmptyInput)?;
    let header_fields: Vec<&str> = header.split_whitespace().collect();
    if header_fields != ["oldest_age", "matrix"] {
        return Err(DispersalStrataParseError::InvalidHeader {
            line: header_line,
            actual: header_fields.join(" "),
        });
    }

    let mut specs = Vec::new();
    for (line_number, line) in lines {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 2 {
            return Err(DispersalStrataParseError::WrongColumnCount {
                line: line_number,
                actual: fields.len(),
            });
        }
        let oldest_age =
            fields[0]
                .parse::<f64>()
                .map_err(|_| DispersalStrataParseError::InvalidOldestAge {
                    line: line_number,
                    value: fields[0].to_string(),
                })?;
        specs.push(DispersalStratumSpec {
            oldest_age,
            matrix_path: fields[1].to_string(),
        });
    }

    if specs.is_empty() {
        return Err(DispersalStrataParseError::NoStrata);
    }
    Ok(specs)
}

pub fn parse_dispersal_multipliers_table(
    input: &str,
    area_names: &[String],
) -> Result<DispersalMultiplierMatrix, DispersalMatrixParseError> {
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

    let (header_line, header) = lines.next().ok_or(DispersalMatrixParseError::EmptyInput)?;
    let header_fields: Vec<&str> = header.split_whitespace().collect();
    let expected_columns = area_names.len() + 1;
    if header_fields.len() != expected_columns {
        return Err(DispersalMatrixParseError::WrongColumnCount {
            line: header_line,
            expected: expected_columns,
            actual: header_fields.len(),
        });
    }
    if header_fields[0] != "from" {
        return Err(DispersalMatrixParseError::InvalidHeaderFirstColumn {
            line: header_line,
            value: header_fields[0].to_string(),
        });
    }
    for (area_index, (actual, expected)) in header_fields[1..].iter().zip(area_names).enumerate() {
        if *actual != expected {
            return Err(DispersalMatrixParseError::HeaderAreaMismatch {
                line: header_line,
                column: area_index + 2,
                expected: expected.clone(),
                actual: (*actual).to_string(),
            });
        }
    }

    let mut values = Vec::with_capacity(area_names.len() * area_names.len());
    let mut row_count = 0;
    for (line_number, line) in lines {
        if row_count >= area_names.len() {
            return Err(DispersalMatrixParseError::TooManyRows {
                expected: area_names.len(),
                line: line_number,
            });
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != expected_columns {
            return Err(DispersalMatrixParseError::WrongColumnCount {
                line: line_number,
                expected: expected_columns,
                actual: fields.len(),
            });
        }
        if fields[0] != area_names[row_count] {
            return Err(DispersalMatrixParseError::RowAreaMismatch {
                line: line_number,
                expected: area_names[row_count].clone(),
                actual: fields[0].to_string(),
            });
        }

        for (column, field) in fields[1..].iter().enumerate() {
            let value =
                field
                    .parse::<f64>()
                    .map_err(|_| DispersalMatrixParseError::InvalidMultiplier {
                        line: line_number,
                        column: column + 2,
                        value: (*field).to_string(),
                    })?;
            values.push(value);
        }
        row_count += 1;
    }

    if row_count != area_names.len() {
        return Err(DispersalMatrixParseError::WrongRowCount {
            expected: area_names.len(),
            actual: row_count,
        });
    }

    DispersalMultiplierMatrix::new(area_names.len(), values)
        .map_err(DispersalMatrixParseError::Matrix)
}

pub fn parse_extirpation_multipliers_table(
    input: &str,
    area_names: &[String],
) -> Result<ExtirpationMultiplierVector, ExtirpationMultiplierParseError> {
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
        .ok_or(ExtirpationMultiplierParseError::EmptyInput)?;
    let header_fields: Vec<&str> = header.split_whitespace().collect();
    if header_fields != ["area", "multiplier"] {
        return Err(ExtirpationMultiplierParseError::InvalidHeader {
            line: header_line,
            actual: header_fields.join(" "),
        });
    }

    let mut values = Vec::with_capacity(area_names.len());
    for (row_index, (line_number, line)) in lines.enumerate() {
        if row_index >= area_names.len() {
            return Err(ExtirpationMultiplierParseError::TooManyRows {
                expected: area_names.len(),
                line: line_number,
            });
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 2 {
            return Err(ExtirpationMultiplierParseError::WrongColumnCount {
                line: line_number,
                actual: fields.len(),
            });
        }
        if fields[0] != area_names[row_index] {
            return Err(ExtirpationMultiplierParseError::AreaMismatch {
                line: line_number,
                expected: area_names[row_index].clone(),
                actual: fields[0].to_string(),
            });
        }
        let value = fields[1].parse::<f64>().map_err(|_| {
            ExtirpationMultiplierParseError::InvalidMultiplier {
                line: line_number,
                value: fields[1].to_string(),
            }
        })?;
        values.push(value);
    }

    if values.len() != area_names.len() {
        return Err(ExtirpationMultiplierParseError::WrongRowCount {
            expected: area_names.len(),
            actual: values.len(),
        });
    }

    ExtirpationMultiplierVector::new(values).map_err(ExtirpationMultiplierParseError::Vector)
}

pub fn parse_area_sizes_table(
    input: &str,
    area_names: &[String],
) -> Result<AreaSizeVector, AreaSizeParseError> {
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

    let (header_line, header) = lines.next().ok_or(AreaSizeParseError::EmptyInput)?;
    let header_fields: Vec<&str> = header.split_whitespace().collect();
    if header_fields != ["area", "size"] {
        return Err(AreaSizeParseError::InvalidHeader {
            line: header_line,
            actual: header_fields.join(" "),
        });
    }

    let mut values = Vec::with_capacity(area_names.len());
    for (row_index, (line_number, line)) in lines.enumerate() {
        if row_index >= area_names.len() {
            return Err(AreaSizeParseError::TooManyRows {
                expected: area_names.len(),
                line: line_number,
            });
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() != 2 {
            return Err(AreaSizeParseError::WrongColumnCount {
                line: line_number,
                actual: fields.len(),
            });
        }
        if fields[0] != area_names[row_index] {
            return Err(AreaSizeParseError::AreaMismatch {
                line: line_number,
                expected: area_names[row_index].clone(),
                actual: fields[0].to_string(),
            });
        }
        let value = fields[1]
            .parse::<f64>()
            .map_err(|_| AreaSizeParseError::InvalidSize {
                line: line_number,
                value: fields[1].to_string(),
            })?;
        values.push(value);
    }

    if values.len() != area_names.len() {
        return Err(AreaSizeParseError::WrongRowCount {
            expected: area_names.len(),
            actual: values.len(),
        });
    }

    AreaSizeVector::new(values).map_err(AreaSizeParseError::Vector)
}

#[derive(Clone, Debug, PartialEq)]
pub enum DispersalMatrixError {
    ZeroAreas,
    DimensionOverflow {
        num_areas: usize,
    },
    ValueCountMismatch {
        num_areas: usize,
        expected: usize,
        actual: usize,
    },
    NonFiniteMultiplier {
        from: usize,
        to: usize,
        value: f64,
    },
    NegativeMultiplier {
        from: usize,
        to: usize,
        value: f64,
    },
    NonFiniteExponent {
        exponent: f64,
    },
    NonFinitePower {
        from: usize,
        to: usize,
        value: f64,
        exponent: f64,
    },
    ProductAreaCountMismatch {
        left_areas: usize,
        right_areas: usize,
    },
    NonFiniteProduct {
        from: usize,
        to: usize,
        left: f64,
        right: f64,
    },
    AreaCountMismatch {
        matrix_areas: usize,
        state_space_areas: usize,
    },
}

impl fmt::Display for DispersalMatrixError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAreas => write!(
                f,
                "dispersal multiplier matrix must contain at least one area"
            ),
            Self::DimensionOverflow { num_areas } => write!(
                f,
                "dispersal multiplier matrix dimension overflows for {num_areas} areas"
            ),
            Self::ValueCountMismatch {
                num_areas,
                expected,
                actual,
            } => write!(
                f,
                "dispersal multiplier matrix for {num_areas} areas needs {expected} values, got {actual}"
            ),
            Self::NonFiniteMultiplier { from, to, value } => write!(
                f,
                "dispersal multiplier [{from},{to}] must be finite, got {value}"
            ),
            Self::NegativeMultiplier { from, to, value } => write!(
                f,
                "dispersal multiplier [{from},{to}] must be non-negative, got {value}"
            ),
            Self::NonFiniteExponent { exponent } => {
                write!(
                    f,
                    "dispersal matrix exponent must be finite, got {exponent}"
                )
            }
            Self::NonFinitePower {
                from,
                to,
                value,
                exponent,
            } => write!(
                f,
                "dispersal value [{from},{to}]={value} raised to exponent {exponent} is not finite"
            ),
            Self::ProductAreaCountMismatch {
                left_areas,
                right_areas,
            } => write!(
                f,
                "cannot combine dispersal matrices with {left_areas} and {right_areas} areas"
            ),
            Self::NonFiniteProduct {
                from,
                to,
                left,
                right,
            } => write!(
                f,
                "dispersal matrix product [{from},{to}] is not finite: {left} * {right}"
            ),
            Self::AreaCountMismatch {
                matrix_areas,
                state_space_areas,
            } => write!(
                f,
                "dispersal matrix has {matrix_areas} areas but state space has {state_space_areas}"
            ),
        }
    }
}

impl Error for DispersalMatrixError {}

#[derive(Clone, Debug, PartialEq)]
pub enum AreaSizeError {
    ZeroAreas,
    NonFiniteSize {
        area: usize,
        value: f64,
    },
    NonPositiveSize {
        area: usize,
        value: f64,
    },
    NonFiniteExponent {
        exponent: f64,
    },
    NonFinitePower {
        area: usize,
        size: f64,
        exponent: f64,
    },
    InvalidExtirpationMultipliers(ExtirpationMultiplierError),
}

impl fmt::Display for AreaSizeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAreas => write!(f, "area-size vector must contain at least one area"),
            Self::NonFiniteSize { area, value } => {
                write!(f, "size for area {area} must be finite, got {value}")
            }
            Self::NonPositiveSize { area, value } => {
                write!(f, "size for area {area} must be positive, got {value}")
            }
            Self::NonFiniteExponent { exponent } => {
                write!(f, "area-size exponent must be finite, got {exponent}")
            }
            Self::NonFinitePower {
                area,
                size,
                exponent,
            } => write!(
                f,
                "area {area} size {size} raised to exponent {exponent} is not finite"
            ),
            Self::InvalidExtirpationMultipliers(error) => write!(f, "{error}"),
        }
    }
}

impl Error for AreaSizeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidExtirpationMultipliers(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExtirpationMultiplierError {
    ZeroAreas,
    NonFiniteMultiplier {
        area: usize,
        value: f64,
    },
    NegativeMultiplier {
        area: usize,
        value: f64,
    },
    AreaCountMismatch {
        multiplier_areas: usize,
        state_space_areas: usize,
    },
}

impl fmt::Display for ExtirpationMultiplierError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroAreas => write!(
                f,
                "extirpation multiplier vector must contain at least one area"
            ),
            Self::NonFiniteMultiplier { area, value } => write!(
                f,
                "extirpation multiplier for area {area} must be finite, got {value}"
            ),
            Self::NegativeMultiplier { area, value } => write!(
                f,
                "extirpation multiplier for area {area} must be non-negative, got {value}"
            ),
            Self::AreaCountMismatch {
                multiplier_areas,
                state_space_areas,
            } => write!(
                f,
                "extirpation multipliers have {multiplier_areas} areas but state space has {state_space_areas}"
            ),
        }
    }
}

impl Error for ExtirpationMultiplierError {}

#[derive(Clone, Debug, PartialEq)]
pub enum DispersalScheduleError {
    EmptyStrata,
    InvalidOldestAge {
        index: usize,
        oldest_age: f64,
    },
    NonIncreasingOldestAge {
        index: usize,
        previous: f64,
        current: f64,
    },
    AreaCountMismatch {
        index: usize,
        expected: usize,
        actual: usize,
    },
    StateSpaceAreaCountMismatch {
        schedule_areas: usize,
        state_space_areas: usize,
    },
    DoesNotCoverRoot {
        oldest_age: f64,
        root_age: f64,
    },
    RequiresBranchContext,
}

impl fmt::Display for DispersalScheduleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyStrata => write!(f, "time-stratified anagenesis needs at least one stratum"),
            Self::InvalidOldestAge { index, oldest_age } => write!(
                f,
                "anagenetic stratum {index} oldest age must be finite and positive, got {oldest_age}"
            ),
            Self::NonIncreasingOldestAge {
                index,
                previous,
                current,
            } => write!(
                f,
                "anagenetic stratum {index} oldest age {current} must be greater than previous boundary {previous}"
            ),
            Self::AreaCountMismatch {
                index,
                expected,
                actual,
            } => write!(
                f,
                "anagenetic stratum {index} has {actual} areas, expected {expected}"
            ),
            Self::StateSpaceAreaCountMismatch {
                schedule_areas,
                state_space_areas,
            } => write!(
                f,
                "time-stratified anagenesis has {schedule_areas} areas but state space has {state_space_areas}"
            ),
            Self::DoesNotCoverRoot {
                oldest_age,
                root_age,
            } => write!(
                f,
                "oldest anagenetic boundary {oldest_age} does not cover tree root age {root_age}"
            ),
            Self::RequiresBranchContext => write!(
                f,
                "time-stratified anagenesis requires branch-aware likelihood evaluation"
            ),
        }
    }
}

impl Error for DispersalScheduleError {}

#[derive(Clone, Debug, PartialEq)]
pub enum DispersalStrataParseError {
    EmptyInput,
    InvalidHeader { line: usize, actual: String },
    WrongColumnCount { line: usize, actual: usize },
    InvalidOldestAge { line: usize, value: String },
    NoStrata,
}

impl fmt::Display for DispersalStrataParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "dispersal strata table is empty"),
            Self::InvalidHeader { line, actual } => write!(
                f,
                "dispersal strata header on line {line} must be 'oldest_age matrix', got {actual:?}"
            ),
            Self::WrongColumnCount { line, actual } => write!(
                f,
                "dispersal stratum on line {line} has {actual} columns, expected 2"
            ),
            Self::InvalidOldestAge { line, value } => write!(
                f,
                "dispersal stratum oldest age on line {line} is not a number: {value:?}"
            ),
            Self::NoStrata => write!(f, "dispersal strata table contains no strata"),
        }
    }
}

impl Error for DispersalStrataParseError {}

#[derive(Clone, Debug, PartialEq)]
pub enum AnageneticStrataParseError {
    EmptyInput,
    InvalidHeader {
        line: usize,
        actual: String,
    },
    WrongColumnCount {
        line: usize,
        actual: usize,
    },
    InvalidOldestAge {
        line: usize,
        value: String,
    },
    NonIncreasingOldestAge {
        line: usize,
        previous: f64,
        current: f64,
    },
    NoStrata,
}

impl fmt::Display for AnageneticStrataParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "anagenetic strata table is empty"),
            Self::InvalidHeader { line, actual } => write!(
                f,
                "anagenetic strata header on line {line} must be 'oldest_age matrix distance_matrix environment_distance_matrix area_sizes' with optional trailing 'areas_allowed areas_adjacency' and 'allowed_ranges', got {actual:?}"
            ),
            Self::WrongColumnCount { line, actual } => write!(
                f,
                "anagenetic stratum on line {line} has {actual} columns, expected 5, 7, or 8"
            ),
            Self::InvalidOldestAge { line, value } => write!(
                f,
                "anagenetic stratum oldest age on line {line} is not a number: {value:?}"
            ),
            Self::NonIncreasingOldestAge {
                line,
                previous,
                current,
            } => write!(
                f,
                "anagenetic stratum oldest age on line {line} is {current}, expected a finite value greater than {previous}"
            ),
            Self::NoStrata => write!(f, "anagenetic strata table contains no strata"),
        }
    }
}

impl Error for AnageneticStrataParseError {}

#[derive(Clone, Debug, PartialEq)]
pub enum DispersalMatrixParseError {
    EmptyInput,
    WrongColumnCount {
        line: usize,
        expected: usize,
        actual: usize,
    },
    InvalidHeaderFirstColumn {
        line: usize,
        value: String,
    },
    HeaderAreaMismatch {
        line: usize,
        column: usize,
        expected: String,
        actual: String,
    },
    RowAreaMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    TooManyRows {
        expected: usize,
        line: usize,
    },
    WrongRowCount {
        expected: usize,
        actual: usize,
    },
    InvalidMultiplier {
        line: usize,
        column: usize,
        value: String,
    },
    Matrix(DispersalMatrixError),
}

impl fmt::Display for DispersalMatrixParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "dispersal multiplier table is empty"),
            Self::WrongColumnCount {
                line,
                expected,
                actual,
            } => write!(
                f,
                "dispersal multiplier row on line {line} has {actual} columns, expected {expected}"
            ),
            Self::InvalidHeaderFirstColumn { line, value } => write!(
                f,
                "dispersal multiplier header first column on line {line} must be 'from', got {value:?}"
            ),
            Self::HeaderAreaMismatch {
                line,
                column,
                expected,
                actual,
            } => write!(
                f,
                "dispersal multiplier header on line {line}, column {column} names area {actual:?}, expected {expected:?}"
            ),
            Self::RowAreaMismatch {
                line,
                expected,
                actual,
            } => write!(
                f,
                "dispersal multiplier row on line {line} names source area {actual:?}, expected {expected:?}"
            ),
            Self::TooManyRows { expected, line } => write!(
                f,
                "dispersal multiplier table has more than {expected} data rows; extra row starts on line {line}"
            ),
            Self::WrongRowCount { expected, actual } => write!(
                f,
                "dispersal multiplier table has {actual} data rows, expected {expected}"
            ),
            Self::InvalidMultiplier {
                line,
                column,
                value,
            } => write!(
                f,
                "dispersal multiplier on line {line}, column {column} is not a number: {value:?}"
            ),
            Self::Matrix(error) => write!(f, "invalid dispersal multiplier matrix: {error}"),
        }
    }
}

impl Error for DispersalMatrixParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Matrix(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum AreaSizeParseError {
    EmptyInput,
    InvalidHeader {
        line: usize,
        actual: String,
    },
    WrongColumnCount {
        line: usize,
        actual: usize,
    },
    AreaMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    TooManyRows {
        expected: usize,
        line: usize,
    },
    WrongRowCount {
        expected: usize,
        actual: usize,
    },
    InvalidSize {
        line: usize,
        value: String,
    },
    Vector(AreaSizeError),
}

impl fmt::Display for AreaSizeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "area-size table is empty"),
            Self::InvalidHeader { line, actual } => write!(
                f,
                "area-size header on line {line} must be 'area size', got {actual:?}"
            ),
            Self::WrongColumnCount { line, actual } => write!(
                f,
                "area-size row on line {line} has {actual} columns, expected 2"
            ),
            Self::AreaMismatch {
                line,
                expected,
                actual,
            } => write!(
                f,
                "area-size row on line {line} names area {actual:?}, expected {expected:?}"
            ),
            Self::TooManyRows { expected, line } => write!(
                f,
                "area-size table has more than {expected} data rows; extra row starts on line {line}"
            ),
            Self::WrongRowCount { expected, actual } => write!(
                f,
                "area-size table has {actual} data rows, expected {expected}"
            ),
            Self::InvalidSize { line, value } => {
                write!(f, "area size on line {line} is not a number: {value:?}")
            }
            Self::Vector(error) => write!(f, "invalid area-size vector: {error}"),
        }
    }
}

impl Error for AreaSizeParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Vector(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExtirpationMultiplierParseError {
    EmptyInput,
    InvalidHeader {
        line: usize,
        actual: String,
    },
    WrongColumnCount {
        line: usize,
        actual: usize,
    },
    AreaMismatch {
        line: usize,
        expected: String,
        actual: String,
    },
    TooManyRows {
        expected: usize,
        line: usize,
    },
    WrongRowCount {
        expected: usize,
        actual: usize,
    },
    InvalidMultiplier {
        line: usize,
        value: String,
    },
    Vector(ExtirpationMultiplierError),
}

impl fmt::Display for ExtirpationMultiplierParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "extirpation multiplier table is empty"),
            Self::InvalidHeader { line, actual } => write!(
                f,
                "extirpation multiplier header on line {line} must be 'area multiplier', got {actual:?}"
            ),
            Self::WrongColumnCount { line, actual } => write!(
                f,
                "extirpation multiplier row on line {line} has {actual} columns, expected 2"
            ),
            Self::AreaMismatch {
                line,
                expected,
                actual,
            } => write!(
                f,
                "extirpation multiplier row on line {line} names area {actual:?}, expected {expected:?}"
            ),
            Self::TooManyRows { expected, line } => write!(
                f,
                "extirpation multiplier table has more than {expected} data rows; extra row starts on line {line}"
            ),
            Self::WrongRowCount { expected, actual } => write!(
                f,
                "extirpation multiplier table has {actual} data rows, expected {expected}"
            ),
            Self::InvalidMultiplier { line, value } => write!(
                f,
                "extirpation multiplier on line {line} is not a number: {value:?}"
            ),
            Self::Vector(error) => write!(f, "invalid extirpation multiplier vector: {error}"),
        }
    }
}

impl Error for ExtirpationMultiplierParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Vector(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_directional_matrix() {
        let area_names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let parsed = parse_dispersal_multipliers_table(
            "from\tA\tB\tC\nA\t1\t0.5\t0\nB\t0.25\t1\t2\nC\t0\t3\t1\n",
            &area_names,
        )
        .unwrap();

        assert_eq!(parsed.num_areas(), 3);
        assert_eq!(parsed.get(0, 1), 0.5);
        assert_eq!(parsed.get(1, 0), 0.25);
        assert_eq!(parsed.get(2, 1), 3.0);
    }

    #[test]
    fn rejects_area_order_mismatch() {
        let area_names = vec!["A".to_string(), "B".to_string()];
        let error =
            parse_dispersal_multipliers_table("from\tB\tA\nA\t1\t1\nB\t1\t1\n", &area_names)
                .unwrap_err();

        assert_eq!(
            error,
            DispersalMatrixParseError::HeaderAreaMismatch {
                line: 1,
                column: 2,
                expected: "A".to_string(),
                actual: "B".to_string(),
            }
        );
    }

    #[test]
    fn rejects_negative_multiplier() {
        let area_names = vec!["A".to_string(), "B".to_string()];
        let error =
            parse_dispersal_multipliers_table("from\tA\tB\nA\t1\t-0.5\nB\t1\t1\n", &area_names)
                .unwrap_err();

        assert!(matches!(
            error,
            DispersalMatrixParseError::Matrix(DispersalMatrixError::NegativeMultiplier {
                from: 0,
                to: 1,
                value: -0.5
            })
        ));
    }

    #[test]
    fn composes_geographic_environment_and_manual_multipliers() {
        let distances = DispersalMultiplierMatrix::new(2, vec![0.0, 2.0, 4.0, 0.0]).unwrap();
        let environment = DispersalMultiplierMatrix::new(2, vec![0.0, 0.25, 4.0, 0.0]).unwrap();
        let manual = DispersalMultiplierMatrix::new(2, vec![1.0, 0.5, 0.25, 1.0]).unwrap();

        let effective = distances
            .distance_power_checked(-1.0)
            .unwrap()
            .elementwise_product(&environment.distance_power_checked(0.5).unwrap())
            .unwrap()
            .elementwise_product(&manual)
            .unwrap();

        assert_eq!(effective.values(), &[1.0, 0.125, 0.125, 1.0]);
    }

    #[test]
    fn rejects_zero_distance_with_negative_exponent() {
        let distances = DispersalMultiplierMatrix::new(2, vec![1.0, 0.0, 1.0, 1.0]).unwrap();
        let error = distances.distance_power_checked(-1.0).unwrap_err();

        assert!(matches!(
            error,
            DispersalMatrixError::NonFinitePower {
                from: 0,
                to: 1,
                value: 0.0,
                exponent: -1.0
            }
        ));
    }

    #[test]
    fn parses_named_extirpation_multipliers() {
        let area_names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let parsed = parse_extirpation_multipliers_table(
            "area\tmultiplier\nA\t1\nB\t0.5\nC\t2\n",
            &area_names,
        )
        .unwrap();

        assert_eq!(parsed.values(), &[1.0, 0.5, 2.0]);
    }

    #[test]
    fn parses_area_sizes_and_builds_biogeobears_u_multipliers() {
        let area_names = vec!["A".to_string(), "B".to_string(), "C".to_string()];
        let parsed =
            parse_area_sizes_table("area\tsize\nA\t0.5\nB\t1\nC\t2\n", &area_names).unwrap();

        assert_eq!(parsed.values(), &[0.5, 1.0, 2.0]);
        assert!(!parsed.is_uniform());
        assert_eq!(
            parsed.extirpation_multipliers(0.0).unwrap().values(),
            &[1.0, 1.0, 1.0]
        );
        assert_eq!(
            parsed.extirpation_multipliers(-1.0).unwrap().values(),
            &[2.0, 1.0, 0.5]
        );
    }

    #[test]
    fn rejects_non_positive_area_size() {
        let error = AreaSizeVector::new(vec![1.0, 0.0]).unwrap_err();

        assert_eq!(
            error,
            AreaSizeError::NonPositiveSize {
                area: 1,
                value: 0.0
            }
        );
    }

    #[test]
    fn rejects_non_finite_area_size_power() {
        let sizes = AreaSizeVector::new(vec![f64::MAX]).unwrap();
        let error = sizes.extirpation_multipliers(2.0).unwrap_err();

        assert!(matches!(
            error,
            AreaSizeError::NonFinitePower {
                area: 0,
                exponent: 2.0,
                ..
            }
        ));
    }

    #[test]
    fn validates_and_parses_time_strata() {
        let specs =
            parse_dispersal_strata_table("oldest_age\tmatrix\n0.5\tyoung.tsv\n2.0\told.tsv\n")
                .unwrap();
        assert_eq!(
            specs,
            vec![
                DispersalStratumSpec {
                    oldest_age: 0.5,
                    matrix_path: "young.tsv".to_string(),
                },
                DispersalStratumSpec {
                    oldest_age: 2.0,
                    matrix_path: "old.tsv".to_string(),
                },
            ]
        );

        let matrix = DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap();
        let schedule = TimeStratifiedDispersal::new(vec![
            DispersalTimeStratum::new(0.5, matrix.clone()).unwrap(),
            DispersalTimeStratum::new(2.0, matrix).unwrap(),
        ])
        .unwrap();
        assert_eq!(schedule.num_areas(), 2);
        assert_eq!(schedule.oldest_age(), 2.0);
    }

    #[test]
    fn parses_anagenetic_strata_with_optional_raw_inputs() {
        let specs = parse_anagenetic_strata_table(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\n\
             0.5\tyoung-manual.tsv\tyoung-distance.tsv\t-\tyoung-area.tsv\n\
             2.0\tnone\told-distance.tsv\told-environment.tsv\told-area.tsv\n",
        )
        .unwrap();

        assert_eq!(
            specs,
            vec![
                AnageneticStratumSpec {
                    oldest_age: 0.5,
                    dispersal_matrix_path: Some("young-manual.tsv".to_string()),
                    distance_matrix_path: Some("young-distance.tsv".to_string()),
                    environment_distance_matrix_path: None,
                    area_sizes_path: Some("young-area.tsv".to_string()),
                    areas_allowed_path: None,
                    areas_adjacency_path: None,
                    allowed_ranges_path: None,
                },
                AnageneticStratumSpec {
                    oldest_age: 2.0,
                    dispersal_matrix_path: None,
                    distance_matrix_path: Some("old-distance.tsv".to_string()),
                    environment_distance_matrix_path: Some("old-environment.tsv".to_string()),
                    area_sizes_path: Some("old-area.tsv".to_string()),
                    areas_allowed_path: None,
                    areas_adjacency_path: None,
                    allowed_ranges_path: None,
                },
            ]
        );
    }

    #[test]
    fn parses_anagenetic_strata_with_state_constraint_paths() {
        let specs = parse_anagenetic_strata_table(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\n\
             0.5\t-\t-\t-\t-\tyoung-allowed.tsv\t-\n\
             2.0\tnone\tnone\tnone\tnone\told-allowed.tsv\told-adjacency.tsv\n",
        )
        .unwrap();

        assert_eq!(
            specs[0].areas_allowed_path.as_deref(),
            Some("young-allowed.tsv")
        );
        assert_eq!(specs[0].areas_adjacency_path, None);
        assert_eq!(
            specs[1].areas_allowed_path.as_deref(),
            Some("old-allowed.tsv")
        );
        assert_eq!(
            specs[1].areas_adjacency_path.as_deref(),
            Some("old-adjacency.tsv")
        );
        assert_eq!(specs[1].allowed_ranges_path, None);
    }

    #[test]
    fn parses_anagenetic_strata_with_explicit_allowed_ranges() {
        let specs = parse_anagenetic_strata_table(
            "oldest_age matrix distance_matrix environment_distance_matrix area_sizes areas_allowed areas_adjacency allowed_ranges\n\
             2.0 none none none none none none allowed-ranges.tsv\n",
        )
        .unwrap();

        assert_eq!(
            specs[0].allowed_ranges_path.as_deref(),
            Some("allowed-ranges.tsv")
        );
    }

    #[test]
    fn time_stratified_anagenesis_validates_both_modifier_dimensions() {
        let dispersal = DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap();
        let extirpation = ExtirpationMultiplierVector::new(vec![1.0, 2.0]).unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![
            AnageneticTimeStratum::new(0.5, Some(dispersal.clone()), Some(extirpation.clone()))
                .unwrap(),
            AnageneticTimeStratum::new(2.0, Some(dispersal), Some(extirpation)).unwrap(),
        ])
        .unwrap();

        assert_eq!(schedule.num_areas(), Some(2));
        assert_eq!(schedule.oldest_age(), 2.0);

        let mismatched = AnageneticTimeStratum::new(
            1.0,
            Some(DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap()),
            Some(ExtirpationMultiplierVector::new(vec![1.0, 2.0, 3.0]).unwrap()),
        )
        .unwrap_err();
        assert_eq!(
            mismatched,
            DispersalScheduleError::AreaCountMismatch {
                index: 0,
                expected: 2,
                actual: 3,
            }
        );
    }

    #[test]
    fn rejects_non_increasing_time_boundaries() {
        let matrix = DispersalMultiplierMatrix::new(2, vec![1.0; 4]).unwrap();
        let error = TimeStratifiedDispersal::new(vec![
            DispersalTimeStratum::new(1.0, matrix.clone()).unwrap(),
            DispersalTimeStratum::new(0.5, matrix).unwrap(),
        ])
        .unwrap_err();

        assert_eq!(
            error,
            DispersalScheduleError::NonIncreasingOldestAge {
                index: 1,
                previous: 1.0,
                current: 0.5,
            }
        );
    }
}
