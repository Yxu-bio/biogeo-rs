use std::collections::{BTreeMap, HashSet};
use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const RANGE_TABLE_FORMAT: &str = "biogeo-range-table-v1";
pub const STRATA_IMPORT_FORMAT: &str = "biogeo-biogeobears-strata-import-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdjacencyRangeRule {
    AllPairs,
    EdgeCovered,
}

impl AdjacencyRangeRule {
    pub fn parse(value: &str) -> Result<Self, LegacyImportError> {
        match value.to_ascii_lowercase().as_str() {
            "all-pairs" => Ok(Self::AllPairs),
            "edge-covered" => Ok(Self::EdgeCovered),
            _ => Err(LegacyImportError::InvalidAdjacencyRangeRule(
                value.to_string(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::AllPairs => "all-pairs",
            Self::EdgeCovered => "edge-covered",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RangeSourceFormat {
    Auto,
    Lagrange,
    Csv,
}

impl RangeSourceFormat {
    pub fn parse(value: &str) -> Result<Self, LegacyImportError> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "lagrange" | "lagrange-data" | "data" => Ok(Self::Lagrange),
            "csv" | "csv-matrix" => Ok(Self::Csv),
            _ => Err(LegacyImportError::InvalidRangeFormat(value.to_string())),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Lagrange => "lagrange",
            Self::Csv => "csv",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalRangeTable {
    pub source_format: RangeSourceFormat,
    pub area_names: Vec<String>,
    pub rows: Vec<CanonicalRangeRow>,
    pub taxon_map_applied: usize,
    pub area_map_applied: usize,
}

impl CanonicalRangeTable {
    pub fn to_tsv(&self) -> String {
        let mut output = String::new();
        output.push_str("# format\t");
        output.push_str(RANGE_TABLE_FORMAT);
        output.push('\n');
        output.push_str("# source_format\t");
        output.push_str(self.source_format.as_str());
        output.push('\n');
        if self.taxon_map_applied > 0 {
            writeln!(output, "# taxon_map_applied\t{}", self.taxon_map_applied)
                .expect("writing to a String cannot fail");
        }
        if self.area_map_applied > 0 {
            writeln!(output, "# area_map_applied\t{}", self.area_map_applied)
                .expect("writing to a String cannot fail");
        }
        output.push_str("tip");
        for area in &self.area_names {
            output.push('\t');
            output.push_str(area);
        }
        output.push('\n');
        for row in &self.rows {
            output.push_str(&row.taxon);
            for value in &row.presence {
                output.push('\t');
                output.push(*value);
            }
            output.push('\n');
        }
        output
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalRangeRow {
    pub taxon: String,
    pub presence: Vec<char>,
}

pub fn import_range_table(
    input: &str,
    requested_format: RangeSourceFormat,
    taxon_column: Option<&str>,
) -> Result<CanonicalRangeTable, LegacyImportError> {
    let format = match requested_format {
        RangeSourceFormat::Auto => detect_range_format(input)?,
        explicit => explicit,
    };
    match format {
        RangeSourceFormat::Lagrange => parse_lagrange_ranges(input),
        RangeSourceFormat::Csv => parse_csv_ranges(input, taxon_column),
        RangeSourceFormat::Auto => unreachable!("auto range format must be resolved"),
    }
}

pub fn maybe_import_range_table(
    input: &str,
    taxon_column: Option<&str>,
) -> Result<Option<CanonicalRangeTable>, LegacyImportError> {
    match detect_range_format(input) {
        Ok(format) => {
            let table = import_range_table(input, format, taxon_column)?;
            validate_canonical_range_table(&table)?;
            Ok(Some(table))
        }
        Err(LegacyImportError::CannotDetectRangeFormat) => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn validate_canonical_range_table(
    table: &CanonicalRangeTable,
) -> Result<(), LegacyImportError> {
    if let Some(row) = table
        .rows
        .iter()
        .find(|row| row.taxon.chars().any(char::is_whitespace))
    {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "taxon {:?} contains whitespace; convert it with an explicit --taxon-map before analysis",
                row.taxon
            ),
        });
    }
    Ok(())
}

fn detect_range_format(input: &str) -> Result<RangeSourceFormat, LegacyImportError> {
    let first = input
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .ok_or(LegacyImportError::EmptyInput("range input"))?;
    if first.contains(',') {
        return Ok(RangeSourceFormat::Csv);
    }
    let fields = first.split_whitespace().collect::<Vec<_>>();
    if fields.len() >= 3
        && fields[0].parse::<usize>().is_ok()
        && fields[1].parse::<usize>().is_ok()
        && first.contains('(')
        && first.contains(')')
    {
        return Ok(RangeSourceFormat::Lagrange);
    }
    Err(LegacyImportError::CannotDetectRangeFormat)
}

fn parse_lagrange_ranges(input: &str) -> Result<CanonicalRangeTable, LegacyImportError> {
    let mut lines = input
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, trimmed))
        });
    let (header_line, header) = lines
        .next()
        .ok_or(LegacyImportError::EmptyInput("LAGRANGE range input"))?;
    let open = header
        .find('(')
        .ok_or_else(|| LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: "LAGRANGE header is missing '(' before area names".to_string(),
        })?;
    let close = header
        .rfind(')')
        .ok_or_else(|| LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: "LAGRANGE header is missing ')' after area names".to_string(),
        })?;
    if close <= open {
        return Err(LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: "LAGRANGE area-name parentheses are malformed".to_string(),
        });
    }
    let prefix = header[..open].split_whitespace().collect::<Vec<_>>();
    if prefix.len() != 2 {
        return Err(LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: "LAGRANGE header must start with '<taxa> <areas>'".to_string(),
        });
    }
    let expected_taxa = parse_positive_count(prefix[0], header_line, "taxon count")?;
    let expected_areas = parse_positive_count(prefix[1], header_line, "area count")?;
    let area_names = header[open + 1..close]
        .split_whitespace()
        .map(canonical_area_name)
        .collect::<Result<Vec<_>, _>>()?;
    validate_area_names(&area_names, Some(header_line))?;
    if area_names.len() != expected_areas {
        return Err(LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: format!(
                "LAGRANGE header declares {expected_areas} areas but lists {} names",
                area_names.len()
            ),
        });
    }

    let mut rows = Vec::with_capacity(expected_taxa);
    let mut taxa = HashSet::with_capacity(expected_taxa);
    for (line_number, line) in lines {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 2 {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: "LAGRANGE rows must contain exactly a taxon and a 0/1 range code"
                    .to_string(),
            });
        }
        let taxon = fields[0].to_string();
        if !taxa.insert(taxon.clone()) {
            return Err(LegacyImportError::DuplicateTaxon {
                line: line_number,
                taxon,
            });
        }
        let presence = fields[1].chars().collect::<Vec<_>>();
        if presence.len() != expected_areas {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!(
                    "range code has {} cells, expected {expected_areas}",
                    presence.len()
                ),
            });
        }
        validate_presence(&presence, line_number, false)?;
        rows.push(CanonicalRangeRow { taxon, presence });
    }
    if rows.len() != expected_taxa {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "LAGRANGE header declares {expected_taxa} taxa but {} rows were found",
                rows.len()
            ),
        });
    }
    Ok(CanonicalRangeTable {
        source_format: RangeSourceFormat::Lagrange,
        area_names,
        rows,
        taxon_map_applied: 0,
        area_map_applied: 0,
    })
}

fn parse_csv_ranges(
    input: &str,
    requested_taxon_column: Option<&str>,
) -> Result<CanonicalRangeTable, LegacyImportError> {
    let records = parse_csv_records(input)?;
    let header = records
        .first()
        .ok_or(LegacyImportError::EmptyInput("CSV range input"))?;
    if header.len() < 2 {
        return Err(LegacyImportError::InvalidInput {
            line: Some(1),
            message: "CSV range input needs a taxon column and at least one area column"
                .to_string(),
        });
    }
    let taxon_index = resolve_taxon_column(header, requested_taxon_column)?;
    if taxon_index + 1 >= header.len() {
        return Err(LegacyImportError::InvalidInput {
            line: Some(1),
            message: "CSV range input has no area columns after the taxon column".to_string(),
        });
    }
    let area_names = header[taxon_index + 1..]
        .iter()
        .map(|name| canonical_area_name(name))
        .collect::<Result<Vec<_>, _>>()?;
    validate_area_names(&area_names, Some(1))?;

    let mut rows = Vec::with_capacity(records.len().saturating_sub(1));
    let mut taxa = HashSet::with_capacity(records.len().saturating_sub(1));
    for (row_index, record) in records.iter().enumerate().skip(1) {
        let line_number = row_index + 1;
        if record.len() != header.len() {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!(
                    "CSV row has {} columns, expected {}",
                    record.len(),
                    header.len()
                ),
            });
        }
        let taxon = record[taxon_index].trim().to_string();
        if taxon.is_empty() {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: "CSV taxon name is empty".to_string(),
            });
        }
        if !taxa.insert(taxon.clone()) {
            return Err(LegacyImportError::DuplicateTaxon {
                line: line_number,
                taxon,
            });
        }
        let presence = record[taxon_index + 1..]
            .iter()
            .map(|value| {
                let value = value.trim();
                let mut chars = value.chars();
                match (chars.next(), chars.next()) {
                    (Some(value), None) => Ok(value),
                    _ => Err(LegacyImportError::InvalidInput {
                        line: Some(line_number),
                        message: format!("range cell {value:?} is not 0, 1, or ?"),
                    }),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        validate_presence(&presence, line_number, true)?;
        rows.push(CanonicalRangeRow { taxon, presence });
    }
    if rows.is_empty() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "CSV range input contains no taxon rows".to_string(),
        });
    }
    Ok(CanonicalRangeTable {
        source_format: RangeSourceFormat::Csv,
        area_names,
        rows,
        taxon_map_applied: 0,
        area_map_applied: 0,
    })
}

pub fn apply_taxon_map(
    table: &mut CanonicalRangeTable,
    input: &str,
) -> Result<(), LegacyImportError> {
    let mut lines = input
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, line))
        });
    let (header_line, header) = lines
        .next()
        .ok_or(LegacyImportError::EmptyInput("taxon map"))?;
    let header_fields = header.split('\t').map(str::trim).collect::<Vec<_>>();
    if header_fields != ["source_taxon", "target_taxon"] {
        return Err(LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: "taxon map header must be 'source_taxon<TAB>target_taxon'".to_string(),
        });
    }

    let mut mappings = BTreeMap::new();
    let mut targets = HashSet::new();
    for (line_number, line) in lines {
        let fields = line.split('\t').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 2 || fields.iter().any(|value| value.is_empty()) {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: "taxon map rows must contain non-empty source and target names separated by one tab"
                    .to_string(),
            });
        }
        if fields[1].chars().any(char::is_whitespace) {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!(
                    "mapped taxon {:?} contains whitespace, which canonical range tables cannot represent",
                    fields[1]
                ),
            });
        }
        if mappings
            .insert(fields[0].to_string(), (fields[1].to_string(), line_number))
            .is_some()
        {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!("duplicate taxon-map source {:?}", fields[0]),
            });
        }
        if !targets.insert(fields[1].to_string()) {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!("duplicate taxon-map target {:?}", fields[1]),
            });
        }
    }
    if mappings.is_empty() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "taxon map contains no mappings".to_string(),
        });
    }

    let source_taxa = table
        .rows
        .iter()
        .map(|row| row.taxon.as_str())
        .collect::<HashSet<_>>();
    if let Some((source, (_, line))) = mappings
        .iter()
        .find(|(source, _)| !source_taxa.contains(source.as_str()))
    {
        return Err(LegacyImportError::InvalidInput {
            line: Some(*line),
            message: format!("taxon-map source {source:?} is absent from the range input"),
        });
    }

    let mut final_taxa = HashSet::with_capacity(table.rows.len());
    for row in &mut table.rows {
        if let Some((target, _)) = mappings.get(&row.taxon) {
            row.taxon.clone_from(target);
        }
        if row.taxon.chars().any(char::is_whitespace) {
            return Err(LegacyImportError::InvalidInput {
                line: None,
                message: format!(
                    "taxon {:?} contains whitespace; provide --taxon-map with a canonical target name",
                    row.taxon
                ),
            });
        }
        if !final_taxa.insert(row.taxon.clone()) {
            return Err(LegacyImportError::InvalidInput {
                line: None,
                message: format!("taxon mapping creates duplicate taxon {:?}", row.taxon),
            });
        }
    }
    table.taxon_map_applied = mappings.len();
    Ok(())
}

pub fn apply_area_map(
    table: &mut CanonicalRangeTable,
    input: &str,
) -> Result<(), LegacyImportError> {
    let mut lines = input
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, line))
        });
    let (header_line, header) = lines
        .next()
        .ok_or(LegacyImportError::EmptyInput("area map"))?;
    let header_fields = header.split('\t').map(str::trim).collect::<Vec<_>>();
    if header_fields != ["source_area", "target_area"] {
        return Err(LegacyImportError::InvalidInput {
            line: Some(header_line),
            message: "area map header must be 'source_area<TAB>target_area'".to_string(),
        });
    }

    let mut mappings = BTreeMap::new();
    for (line_number, line) in lines {
        let fields = line.split('\t').map(str::trim).collect::<Vec<_>>();
        if fields.len() != 2 || fields.iter().any(|value| value.is_empty()) {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: "area map rows must contain non-empty source and target names separated by one tab"
                    .to_string(),
            });
        }
        let source = canonical_area_name(fields[0])?;
        let target = canonical_area_name(fields[1])?;
        if mappings
            .insert(source.clone(), (target, line_number))
            .is_some()
        {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!("duplicate area-map source {source:?}"),
            });
        }
    }
    if mappings.is_empty() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "area map contains no mappings".to_string(),
        });
    }
    if let Some((source, (_, line))) = mappings
        .iter()
        .find(|(source, _)| !table.area_names.contains(source))
    {
        return Err(LegacyImportError::InvalidInput {
            line: Some(*line),
            message: format!("area-map source {source:?} is absent from the range input"),
        });
    }

    let mut final_names = HashSet::with_capacity(table.area_names.len());
    for area in &mut table.area_names {
        if let Some((target, _)) = mappings.get(area) {
            area.clone_from(target);
        }
        if !final_names.insert(area.clone()) {
            return Err(LegacyImportError::InvalidInput {
                line: None,
                message: format!("area mapping creates duplicate area {area:?}"),
            });
        }
    }
    table.area_map_applied = mappings.len();
    Ok(())
}

fn parse_csv_records(input: &str) -> Result<Vec<Vec<String>>, LegacyImportError> {
    let input = input.trim_start_matches('\u{feff}');
    if input.trim().is_empty() {
        return Err(LegacyImportError::EmptyInput("CSV range input"));
    }
    let chars = input.chars().collect::<Vec<_>>();
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut line = 1;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        if in_quotes {
            if ch == '"' {
                if chars.get(index + 1) == Some(&'"') {
                    field.push('"');
                    index += 1;
                } else {
                    in_quotes = false;
                    after_quote = true;
                }
            } else {
                if ch == '\n' {
                    line += 1;
                }
                field.push(ch);
            }
        } else if after_quote {
            match ch {
                ',' => {
                    record.push(std::mem::take(&mut field));
                    after_quote = false;
                }
                '\r' if chars.get(index + 1) == Some(&'\n') => {}
                '\r' | '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    after_quote = false;
                    if ch == '\n' {
                        line += 1;
                    }
                }
                value if value.is_whitespace() => {}
                _ => {
                    return Err(LegacyImportError::InvalidInput {
                        line: Some(line),
                        message: "unexpected character after a closing CSV quote".to_string(),
                    });
                }
            }
        } else {
            match ch {
                '"' if field.is_empty() => in_quotes = true,
                '"' => {
                    return Err(LegacyImportError::InvalidInput {
                        line: Some(line),
                        message: "CSV quote must begin at the start of a field".to_string(),
                    });
                }
                ',' => record.push(std::mem::take(&mut field)),
                '\r' if chars.get(index + 1) == Some(&'\n') => {}
                '\r' | '\n' => {
                    record.push(std::mem::take(&mut field));
                    records.push(std::mem::take(&mut record));
                    if ch == '\n' {
                        line += 1;
                    }
                }
                _ => field.push(ch),
            }
        }
        index += 1;
    }
    if in_quotes {
        return Err(LegacyImportError::InvalidInput {
            line: Some(line),
            message: "unterminated quoted CSV field".to_string(),
        });
    }
    if !field.is_empty() || !record.is_empty() || input.ends_with(',') {
        record.push(field);
        records.push(record);
    }
    while records
        .last()
        .is_some_and(|row| row.iter().all(|value| value.trim().is_empty()))
    {
        records.pop();
    }
    Ok(records)
}

fn resolve_taxon_column(
    header: &[String],
    requested: Option<&str>,
) -> Result<usize, LegacyImportError> {
    if let Some(requested) = requested {
        return unique_case_insensitive_column(header, requested)?.ok_or_else(|| {
            LegacyImportError::InvalidInput {
                line: Some(1),
                message: format!("CSV taxon column {requested:?} was not found"),
            }
        });
    }
    for candidate in ["name", "tip", "taxon", "species"] {
        if let Some(index) = unique_case_insensitive_column(header, candidate)? {
            return Ok(index);
        }
    }
    Err(LegacyImportError::InvalidInput {
        line: Some(1),
        message: "could not identify a CSV taxon column; use --taxon-column".to_string(),
    })
}

fn unique_case_insensitive_column(
    header: &[String],
    requested: &str,
) -> Result<Option<usize>, LegacyImportError> {
    let matches = header
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            value
                .trim()
                .eq_ignore_ascii_case(requested)
                .then_some(index)
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(None),
        [index] => Ok(Some(*index)),
        _ => Err(LegacyImportError::InvalidInput {
            line: Some(1),
            message: format!("CSV column {requested:?} occurs more than once"),
        }),
    }
}

fn canonical_area_name(name: &str) -> Result<String, LegacyImportError> {
    let mut canonical = String::new();
    let mut previous_was_separator = false;
    for ch in name.trim().chars() {
        if ch.is_whitespace() {
            if !previous_was_separator {
                canonical.push('_');
                previous_was_separator = true;
            }
        } else {
            canonical.push(ch);
            previous_was_separator = false;
        }
    }
    if canonical.is_empty() {
        return Err(LegacyImportError::InvalidInput {
            line: Some(1),
            message: "area names cannot be empty".to_string(),
        });
    }
    Ok(canonical)
}

fn validate_area_names(
    area_names: &[String],
    line: Option<usize>,
) -> Result<(), LegacyImportError> {
    if area_names.is_empty() {
        return Err(LegacyImportError::InvalidInput {
            line,
            message: "at least one area is required".to_string(),
        });
    }
    if area_names.len() > 64 {
        return Err(LegacyImportError::InvalidInput {
            line,
            message: format!("{} areas exceed the engine limit of 64", area_names.len()),
        });
    }
    let mut seen = HashSet::with_capacity(area_names.len());
    for name in area_names {
        if !seen.insert(name) {
            return Err(LegacyImportError::InvalidInput {
                line,
                message: format!("duplicate area name {name:?} after canonicalization"),
            });
        }
    }
    Ok(())
}

fn validate_presence(
    presence: &[char],
    line: usize,
    allow_unknown: bool,
) -> Result<(), LegacyImportError> {
    for (column, value) in presence.iter().copied().enumerate() {
        if value != '0' && value != '1' && !(allow_unknown && value == '?') {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line),
                message: format!(
                    "range cell {} contains {value:?}, expected 0 or 1{}",
                    column + 1,
                    if allow_unknown { " or ?" } else { "" }
                ),
            });
        }
    }
    Ok(())
}

fn parse_positive_count(value: &str, line: usize, label: &str) -> Result<usize, LegacyImportError> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| LegacyImportError::InvalidInput {
            line: Some(line),
            message: format!("invalid {label} {value:?}"),
        })?;
    if parsed == 0 {
        return Err(LegacyImportError::InvalidInput {
            line: Some(line),
            message: format!("{label} must be positive"),
        });
    }
    Ok(parsed)
}

#[derive(Clone, Debug)]
struct MatrixBlock {
    area_names: Vec<String>,
    values: Vec<Vec<f64>>,
}

pub fn import_biogeobears_strata(
    boundaries_path: &Path,
    dispersal_path: Option<&Path>,
    adjacency_path: Option<&Path>,
    adjacency_range_rule: AdjacencyRangeRule,
    max_range_size: Option<usize>,
    output_dir: &Path,
) -> Result<StrataImportSummary, LegacyImportError> {
    let boundaries_input = read_text(boundaries_path)?;
    let dispersal_input = dispersal_path.map(read_text).transpose()?;
    let adjacency_input = adjacency_path.map(read_text).transpose()?;
    let boundaries = parse_time_boundaries(&boundaries_input)?;
    let dispersal = dispersal_input
        .as_deref()
        .map(|input| parse_matrix_blocks(input, MatrixKind::Dispersal))
        .transpose()?;
    let adjacency = adjacency_input
        .as_deref()
        .map(|input| parse_matrix_blocks(input, MatrixKind::Adjacency))
        .transpose()?;
    if dispersal.is_none() && adjacency.is_none() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "provide --dispersal-matrices, --adjacency-matrices, or both".to_string(),
        });
    }
    if let Some(dispersal) = dispersal.as_ref()
        && boundaries.len() != dispersal.len()
    {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "{} time boundaries do not match {} dispersal matrices",
                boundaries.len(),
                dispersal.len()
            ),
        });
    }
    if adjacency_range_rule == AdjacencyRangeRule::EdgeCovered && adjacency.is_none() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "adjacency range rule edge-covered requires --adjacency-matrices".to_string(),
        });
    }
    if adjacency_range_rule == AdjacencyRangeRule::EdgeCovered && max_range_size.is_none() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "adjacency range rule edge-covered requires --max-range-size".to_string(),
        });
    }
    if adjacency_range_rule == AdjacencyRangeRule::AllPairs && max_range_size.is_some() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: "--max-range-size is only used with --adjacency-range-rule edge-covered"
                .to_string(),
        });
    }
    if let Some(adjacency) = adjacency.as_ref()
        && adjacency.len() != boundaries.len()
    {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "{} time boundaries do not match {} adjacency matrices",
                boundaries.len(),
                adjacency.len()
            ),
        });
    }
    let area_names = dispersal
        .as_ref()
        .and_then(|blocks| blocks.first())
        .or_else(|| adjacency.as_ref().and_then(|blocks| blocks.first()))
        .expect("at least one parsed matrix source exists")
        .area_names
        .clone();
    if let Some(dispersal) = dispersal.as_ref() {
        for (index, block) in dispersal.iter().enumerate() {
            ensure_same_areas(&area_names, &block.area_names, "dispersal", index + 1)?;
        }
    }
    if let Some(adjacency) = adjacency.as_ref() {
        for (index, block) in adjacency.iter().enumerate() {
            ensure_same_areas(&area_names, &block.area_names, "adjacency", index + 1)?;
        }
    }

    let allowed_ranges = if adjacency_range_rule == AdjacencyRangeRule::EdgeCovered {
        let max_range_size = max_range_size.expect("edge-covered max range size was validated");
        Some(
            adjacency
                .as_ref()
                .expect("edge-covered adjacency was validated")
                .iter()
                .map(|block| edge_covered_allowed_ranges(block, max_range_size))
                .collect::<Result<Vec<_>, _>>()?,
        )
    } else {
        None
    };

    let staging = create_staging_directory(output_dir)?;
    let write_result = write_strata_directory(
        &staging,
        StrataDirectoryWrite {
            boundaries: &boundaries,
            dispersal: dispersal.as_deref(),
            adjacency: adjacency.as_deref(),
            allowed_ranges: allowed_ranges.as_deref(),
            area_names: &area_names,
            boundaries_source: boundaries_path,
            dispersal_source: dispersal_path,
            adjacency_source: adjacency_path,
            adjacency_range_rule,
            max_range_size,
        },
    );
    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    if let Err(source) = crate::fs_retry::rename(&staging, output_dir) {
        let _ = fs::remove_dir_all(&staging);
        return Err(LegacyImportError::Io {
            path: output_dir.to_path_buf(),
            source,
        });
    }
    Ok(StrataImportSummary {
        output_dir: output_dir.to_path_buf(),
        area_names,
        strata: boundaries.len(),
        has_dispersal: dispersal.is_some(),
        has_adjacency: adjacency.is_some(),
        adjacency_range_rule,
        max_range_size,
        allowed_range_counts: allowed_ranges
            .unwrap_or_default()
            .into_iter()
            .map(|states| states.len())
            .collect(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StrataImportSummary {
    pub output_dir: PathBuf,
    pub area_names: Vec<String>,
    pub strata: usize,
    pub has_dispersal: bool,
    pub has_adjacency: bool,
    pub adjacency_range_rule: AdjacencyRangeRule,
    pub max_range_size: Option<usize>,
    pub allowed_range_counts: Vec<usize>,
}

impl StrataImportSummary {
    pub fn to_tsv(&self) -> String {
        format!(
            "format\t{STRATA_IMPORT_FORMAT}\nstatus\tcomplete\noutput_dir\t{}\nareas\t{}\narea_names\t{}\nstrata\t{}\nhas_dispersal\t{}\nhas_adjacency\t{}\nadjacency_range_rule\t{}\nmax_range_size\t{}\nallowed_range_counts\t{}\nstrata_file\tstrata.tsv\nmetadata_file\tmetadata.tsv\n",
            self.output_dir.display(),
            self.area_names.len(),
            self.area_names.join(","),
            self.strata,
            self.has_dispersal,
            self.has_adjacency,
            self.adjacency_range_rule.as_str(),
            self.max_range_size
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            self.allowed_range_counts
                .iter()
                .map(usize::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MatrixKind {
    Dispersal,
    Adjacency,
}

impl MatrixKind {
    fn label(self) -> &'static str {
        match self {
            Self::Dispersal => "dispersal",
            Self::Adjacency => "adjacency",
        }
    }
}

fn parse_time_boundaries(input: &str) -> Result<Vec<f64>, LegacyImportError> {
    let mut boundaries = Vec::new();
    let mut previous = 0.0;
    for (index, line) in input.trim_start_matches('\u{feff}').lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let boundary = trimmed
            .parse::<f64>()
            .map_err(|_| LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!("invalid time boundary {trimmed:?}"),
            })?;
        if !boundary.is_finite() || boundary <= previous {
            return Err(LegacyImportError::InvalidInput {
                line: Some(line_number),
                message: format!(
                    "time boundary {boundary} must be finite and greater than {previous}"
                ),
            });
        }
        boundaries.push(boundary);
        previous = boundary;
    }
    if boundaries.is_empty() {
        return Err(LegacyImportError::EmptyInput("time boundaries"));
    }
    Ok(boundaries)
}

fn parse_matrix_blocks(
    input: &str,
    kind: MatrixKind,
) -> Result<Vec<MatrixBlock>, LegacyImportError> {
    let lines = input
        .trim_start_matches('\u{feff}')
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim();
            (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, trimmed))
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        return Err(LegacyImportError::EmptyInput(kind.label()));
    }
    let mut blocks = Vec::new();
    let mut cursor = 0;
    while cursor < lines.len() {
        let (header_line, header) = lines[cursor];
        if header.eq_ignore_ascii_case("END") {
            if cursor + 1 != lines.len() {
                return Err(LegacyImportError::InvalidInput {
                    line: Some(header_line),
                    message: format!(
                        "{} matrix END marker must be the final content",
                        kind.label()
                    ),
                });
            }
            break;
        }
        let area_names = header
            .split_whitespace()
            .map(canonical_area_name)
            .collect::<Result<Vec<_>, _>>()?;
        validate_area_names(&area_names, Some(header_line))?;
        cursor += 1;
        if lines.len().saturating_sub(cursor) < area_names.len() {
            return Err(LegacyImportError::InvalidInput {
                line: Some(header_line),
                message: format!(
                    "{} matrix block needs {} rows",
                    kind.label(),
                    area_names.len()
                ),
            });
        }
        let mut values = Vec::with_capacity(area_names.len());
        for row_index in 0..area_names.len() {
            let (line_number, row) = lines[cursor];
            cursor += 1;
            let fields = row.split_whitespace().collect::<Vec<_>>();
            if fields.len() != area_names.len() {
                return Err(LegacyImportError::InvalidInput {
                    line: Some(line_number),
                    message: format!(
                        "{} matrix row {} has {} values, expected {}",
                        kind.label(),
                        row_index + 1,
                        fields.len(),
                        area_names.len()
                    ),
                });
            }
            let row_values = fields
                .iter()
                .enumerate()
                .map(|(column, value)| {
                    let parsed =
                        value
                            .parse::<f64>()
                            .map_err(|_| LegacyImportError::InvalidInput {
                                line: Some(line_number),
                                message: format!(
                                    "invalid {} matrix value {value:?} at column {}",
                                    kind.label(),
                                    column + 1
                                ),
                            })?;
                    if !parsed.is_finite() || parsed < 0.0 {
                        return Err(LegacyImportError::InvalidInput {
                            line: Some(line_number),
                            message: format!(
                                "{} matrix value must be finite and non-negative",
                                kind.label()
                            ),
                        });
                    }
                    if kind == MatrixKind::Adjacency && parsed != 0.0 && parsed != 1.0 {
                        return Err(LegacyImportError::InvalidInput {
                            line: Some(line_number),
                            message: "adjacency matrix values must be 0 or 1".to_string(),
                        });
                    }
                    Ok(parsed)
                })
                .collect::<Result<Vec<_>, _>>()?;
            values.push(row_values);
        }
        blocks.push(MatrixBlock { area_names, values });
    }
    Ok(blocks)
}

fn ensure_same_areas(
    expected: &[String],
    actual: &[String],
    kind: &str,
    block: usize,
) -> Result<(), LegacyImportError> {
    if expected != actual {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "{kind} matrix block {block} area names differ from the reference matrix block"
            ),
        });
    }
    Ok(())
}

fn create_staging_directory(output_dir: &Path) -> Result<PathBuf, LegacyImportError> {
    if output_dir.exists() {
        return Err(LegacyImportError::OutputDirectoryExists(
            output_dir.to_path_buf(),
        ));
    }
    let parent = output_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| LegacyImportError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let name = output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("strata");
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let staging = parent.join(format!(".{name}.staging-{}-{nonce}", std::process::id()));
    fs::create_dir(&staging).map_err(|source| LegacyImportError::Io {
        path: staging.clone(),
        source,
    })?;
    Ok(staging)
}

struct StrataDirectoryWrite<'a> {
    boundaries: &'a [f64],
    dispersal: Option<&'a [MatrixBlock]>,
    adjacency: Option<&'a [MatrixBlock]>,
    allowed_ranges: Option<&'a [Vec<biogeo_core::AreaSet>]>,
    area_names: &'a [String],
    boundaries_source: &'a Path,
    dispersal_source: Option<&'a Path>,
    adjacency_source: Option<&'a Path>,
    adjacency_range_rule: AdjacencyRangeRule,
    max_range_size: Option<usize>,
}

fn write_strata_directory(
    directory: &Path,
    context: StrataDirectoryWrite<'_>,
) -> Result<(), LegacyImportError> {
    let StrataDirectoryWrite {
        boundaries,
        dispersal,
        adjacency,
        allowed_ranges,
        area_names,
        boundaries_source,
        dispersal_source,
        adjacency_source,
        adjacency_range_rule,
        max_range_size,
    } = context;
    if let Some(dispersal) = dispersal {
        for (index, block) in dispersal.iter().enumerate() {
            write_matrix(
                &directory.join(format!("dispersal-{:03}.tsv", index + 1)),
                block,
                MatrixKind::Dispersal,
            )?;
        }
    }
    if let Some(adjacency) = adjacency {
        for (index, block) in adjacency.iter().enumerate() {
            write_matrix(
                &directory.join(format!("adjacency-{:03}.tsv", index + 1)),
                block,
                MatrixKind::Adjacency,
            )?;
        }
    }
    if let Some(allowed_ranges) = allowed_ranges {
        for (index, states) in allowed_ranges.iter().enumerate() {
            write_allowed_ranges(
                &directory.join(format!("allowed-ranges-{:03}.tsv", index + 1)),
                area_names,
                states,
            )?;
        }
    }

    let mut strata = String::new();
    if allowed_ranges.is_some() {
        strata.push_str(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\tallowed_ranges\n",
        );
    } else if adjacency.is_some() {
        strata.push_str(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\n",
        );
    } else {
        strata.push_str(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\n",
        );
    }
    for (index, boundary) in boundaries.iter().enumerate() {
        let matrix = if dispersal.is_some() {
            format!("dispersal-{:03}.tsv", index + 1)
        } else {
            "none".to_string()
        };
        if allowed_ranges.is_some() {
            writeln!(
                strata,
                "{boundary}\t{matrix}\tnone\tnone\tnone\tnone\tnone\tallowed-ranges-{:03}.tsv",
                index + 1
            )
            .expect("writing to a String cannot fail");
        } else if adjacency.is_some() {
            writeln!(
                strata,
                "{boundary}\t{matrix}\tnone\tnone\tnone\tnone\tadjacency-{:03}.tsv",
                index + 1
            )
            .expect("writing to a String cannot fail");
        } else {
            writeln!(strata, "{boundary}\t{matrix}\tnone\tnone\tnone")
                .expect("writing to a String cannot fail");
        }
    }
    write_text(&directory.join("strata.tsv"), &strata)?;

    let mut metadata = format!(
        "key\tvalue\nformat\t{STRATA_IMPORT_FORMAT}\nstatus\tcomplete\nareas\t{}\narea_names\t{}\nstrata\t{}\nhas_dispersal\t{}\nhas_adjacency\t{}\nadjacency_range_rule\t{}\nmax_range_size\t{}\nboundaries_source\t{}\n",
        area_names.len(),
        area_names.join(","),
        boundaries.len(),
        dispersal.is_some(),
        adjacency.is_some(),
        adjacency_range_rule.as_str(),
        max_range_size
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string()),
        boundaries_source.display()
    );
    if let Some(source) = dispersal_source {
        writeln!(metadata, "dispersal_source\t{}", source.display())
            .expect("writing to a String cannot fail");
    }
    if let Some(source) = adjacency_source {
        writeln!(metadata, "adjacency_source\t{}", source.display())
            .expect("writing to a String cannot fail");
    }
    write_text(&directory.join("metadata.tsv"), &metadata)
}

fn edge_covered_allowed_ranges(
    block: &MatrixBlock,
    max_range_size: usize,
) -> Result<Vec<biogeo_core::AreaSet>, LegacyImportError> {
    if max_range_size == 0 || max_range_size > block.area_names.len() {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "max range size {max_range_size} must be between 1 and the {} imported areas",
                block.area_names.len()
            ),
        });
    }
    if block.area_names.len() > 64 {
        return Err(LegacyImportError::InvalidInput {
            line: None,
            message: format!(
                "explicit allowed ranges support at most 64 areas, got {}",
                block.area_names.len()
            ),
        });
    }
    let states =
        biogeo_core::StateSpace::new(block.area_names.len() as u8, max_range_size as u8, true)
            .map_err(|error| LegacyImportError::InvalidInput {
                line: None,
                message: error.to_string(),
            })?;
    Ok(states
        .states()
        .iter()
        .copied()
        .filter(|state| {
            if state.size() <= 1 {
                return true;
            }
            (0..block.area_names.len())
                .filter(|area| state.contains(*area as u8))
                .all(|area| {
                    (0..block.area_names.len()).any(|other| {
                        area != other
                            && state.contains(other as u8)
                            && block.values[area][other] == 1.0
                    })
                })
        })
        .collect())
}

fn write_allowed_ranges(
    path: &Path,
    area_names: &[String],
    states: &[biogeo_core::AreaSet],
) -> Result<(), LegacyImportError> {
    let mut output = format!(
        "# format\tbiogeo-allowed-ranges-v1\nrange\t{}\n",
        area_names.join("\t")
    );
    for state in states {
        let selected = area_names
            .iter()
            .enumerate()
            .filter(|(index, _)| state.contains(*index as u8))
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>();
        let label = if selected.is_empty() {
            "_".to_string()
        } else {
            selected.join("+")
        };
        output.push_str(&label);
        for area in 0..area_names.len() {
            output.push('\t');
            output.push(if state.contains(area as u8) { '1' } else { '0' });
        }
        output.push('\n');
    }
    write_text(path, &output)
}

fn write_matrix(
    path: &Path,
    block: &MatrixBlock,
    kind: MatrixKind,
) -> Result<(), LegacyImportError> {
    let mut output = String::new();
    output.push_str("from");
    for area in &block.area_names {
        output.push('\t');
        output.push_str(area);
    }
    output.push('\n');
    for (area, row) in block.area_names.iter().zip(&block.values) {
        output.push_str(area);
        for value in row {
            output.push('\t');
            if kind == MatrixKind::Adjacency {
                output.push(if *value == 1.0 { '1' } else { '0' });
            } else {
                output.push_str(&value.to_string());
            }
        }
        output.push('\n');
    }
    write_text(path, &output)
}

fn read_text(path: &Path) -> Result<String, LegacyImportError> {
    fs::read_to_string(path).map_err(|source| LegacyImportError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn write_text(path: &Path, contents: &str) -> Result<(), LegacyImportError> {
    let mut file = fs::File::create(path).map_err(|source| LegacyImportError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    file.write_all(contents.as_bytes())
        .map_err(|source| LegacyImportError::Io {
            path: path.to_path_buf(),
            source,
        })
}

#[derive(Debug)]
pub enum LegacyImportError {
    EmptyInput(&'static str),
    InvalidRangeFormat(String),
    InvalidAdjacencyRangeRule(String),
    CannotDetectRangeFormat,
    InvalidInput {
        line: Option<usize>,
        message: String,
    },
    DuplicateTaxon {
        line: usize,
        taxon: String,
    },
    OutputDirectoryExists(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for LegacyImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyInput(kind) => write!(f, "{kind} is empty"),
            Self::InvalidRangeFormat(value) => write!(
                f,
                "unknown range input format {value:?}; expected auto, lagrange, or csv"
            ),
            Self::InvalidAdjacencyRangeRule(value) => write!(
                f,
                "unknown adjacency range rule {value:?}; expected all-pairs or edge-covered"
            ),
            Self::CannotDetectRangeFormat => write!(
                f,
                "could not detect range input format; use --input-format lagrange or csv"
            ),
            Self::InvalidInput {
                line: Some(line),
                message,
            } => write!(f, "invalid legacy input on line {line}: {message}"),
            Self::InvalidInput {
                line: None,
                message,
            } => write!(f, "invalid legacy input: {message}"),
            Self::DuplicateTaxon { line, taxon } => {
                write!(f, "duplicate taxon {taxon:?} on line {line}")
            }
            Self::OutputDirectoryExists(path) => write!(
                f,
                "strata output directory already exists; refusing to overwrite {}",
                path.display()
            ),
            Self::Io { path, source } => write!(f, "I/O failed for {}: {source}", path.display()),
        }
    }
}

impl Error for LegacyImportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_lagrange_data_to_canonical_tsv() {
        let input = "3 3 (A B C)\nTaxon_1 100\nTaxon_2 011\nTaxon_3 001\n";
        let table = import_range_table(input, RangeSourceFormat::Auto, None).unwrap();
        assert_eq!(table.source_format, RangeSourceFormat::Lagrange);
        assert_eq!(table.area_names, ["A", "B", "C"]);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[1].presence, ['0', '1', '1']);
        assert!(table.to_tsv().contains("tip\tA\tB\tC\nTaxon_1\t1\t0\t0\n"));
    }

    #[test]
    fn applies_strict_explicit_taxon_mapping() {
        let mut table = import_range_table(
            "Name,A\nNewGenus_bucki_EX2455_DZUP549431,1\nOther,0\n",
            RangeSourceFormat::Csv,
            None,
        )
        .unwrap();
        apply_taxon_map(
            &mut table,
            "source_taxon\ttarget_taxon\nNewGenus_bucki_EX2455_DZUP549431\tNeoponera_bucki\n",
        )
        .unwrap();

        assert_eq!(table.rows[0].taxon, "Neoponera_bucki");
        assert_eq!(table.taxon_map_applied, 1);
        assert!(table.to_tsv().contains("# taxon_map_applied\t1\n"));
    }

    #[test]
    fn canonical_validation_requires_mapping_for_whitespace_taxa() {
        let mut table =
            import_range_table("Name,A\nTaxon one,1\n", RangeSourceFormat::Csv, None).unwrap();
        assert!(
            validate_canonical_range_table(&table)
                .unwrap_err()
                .to_string()
                .contains("--taxon-map")
        );

        apply_taxon_map(
            &mut table,
            "source_taxon\ttarget_taxon\nTaxon one\tTaxon_one\n",
        )
        .unwrap();
        validate_canonical_range_table(&table).unwrap();
    }

    #[test]
    fn applies_explicit_area_mapping_after_csv_canonicalization() {
        let mut table = import_range_table(
            "Name,Eastern Palearctic,Western Palearctic\nA,1,0\n",
            RangeSourceFormat::Csv,
            None,
        )
        .unwrap();
        apply_area_map(
            &mut table,
            "source_area\ttarget_area\nEastern Palearctic\tE\nWestern_Palearctic\tW\n",
        )
        .unwrap();

        assert_eq!(table.area_names, ["E", "W"]);
        assert_eq!(table.area_map_applied, 2);
        assert!(table.to_tsv().contains("# area_map_applied\t2\n"));
    }

    #[test]
    fn taxon_mapping_rejects_unknown_sources_and_collisions() {
        let mut table =
            import_range_table("Name,A\nA,1\nB,0\n", RangeSourceFormat::Csv, None).unwrap();
        let unknown =
            apply_taxon_map(&mut table, "source_taxon\ttarget_taxon\nMissing\tC\n").unwrap_err();
        assert!(unknown.to_string().contains("absent"));

        let collision =
            apply_taxon_map(&mut table, "source_taxon\ttarget_taxon\nA\tB\n").unwrap_err();
        assert!(collision.to_string().contains("duplicate taxon"));
    }

    #[test]
    fn edge_covered_ranges_allow_connected_chains_but_not_disconnected_pairs() {
        let block = MatrixBlock {
            area_names: vec!["A".to_string(), "B".to_string(), "C".to_string()],
            values: vec![
                vec![1.0, 1.0, 0.0],
                vec![1.0, 1.0, 1.0],
                vec![0.0, 1.0, 1.0],
            ],
        };
        let states = edge_covered_allowed_ranges(&block, 3).unwrap();
        assert!(states.contains(&biogeo_core::AreaSet::from_bits(0b111)));
        assert!(!states.contains(&biogeo_core::AreaSet::from_bits(0b101)));
        assert_eq!(states.len(), 7);
    }

    #[test]
    fn imports_csv_name_column_and_canonicalizes_area_whitespace() {
        let input = "ID,Name,Area One,Area Two\n1,Taxon_1,1,0\n2,\"Taxon_2\",0,?\n";
        let table = import_range_table(input, RangeSourceFormat::Csv, None).unwrap();
        assert_eq!(table.area_names, ["Area_One", "Area_Two"]);
        assert_eq!(table.rows[1].taxon, "Taxon_2");
        assert_eq!(table.rows[1].presence, ['0', '?']);
    }

    #[test]
    fn csv_parser_handles_escaped_quotes_and_rejects_unclosed_quotes() {
        let records = parse_csv_records("Name,A\n\"Taxon \"\"one\"\"\",1\n").unwrap();
        assert_eq!(records[1][0], "Taxon \"one\"");
        let error = parse_csv_records("Name,A\n\"Taxon,1\n").unwrap_err();
        assert!(error.to_string().contains("unterminated"));
    }

    #[test]
    fn imports_block_matrices_and_rejects_count_mismatch() {
        let dispersal = "A B\n1 0.5\n0.5 1\n\nA B\n1 0.1\n0.1 1\n\nEND\n";
        let blocks = parse_matrix_blocks(dispersal, MatrixKind::Dispersal).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[1].values[0][1], 0.1);
        let boundaries = parse_time_boundaries("1\n2\n").unwrap();
        assert_eq!(boundaries, [1.0, 2.0]);
    }

    #[test]
    fn rejects_non_binary_adjacency() {
        let error = parse_matrix_blocks("A B\n1 0.5\n0 1\n", MatrixKind::Adjacency).unwrap_err();
        assert!(error.to_string().contains("must be 0 or 1"));
    }

    #[test]
    fn rejects_content_after_matrix_end_marker() {
        let error =
            parse_matrix_blocks("A B\n1 1\n1 1\nEND\nA B\n1 1\n1 1\n", MatrixKind::Adjacency)
                .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("END marker must be the final content")
        );
    }
}
