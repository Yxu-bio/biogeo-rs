use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

pub const BSM_INSPECTION_FORMAT: &str = "biogeo-bsm-inspection-v1";

const TABLE_FILES: [&str; 8] = [
    "node_states.tsv",
    "cladogenetic_splits.tsv",
    "branch_segments.tsv",
    "sample_event_counts.tsv",
    "sample_period_event_counts.tsv",
    "sample_state_occupancy.tsv",
    "sample_period_state_occupancy.tsv",
    "anagenetic_events.tsv",
];
const REFERENCE_FILES: [&str; 5] = [
    "areas.tsv",
    "states.tsv",
    "nodes.tsv",
    "edges.tsv",
    "periods.tsv",
];
const CHECKPOINTS: &str = "checkpoints";
const SHARDS: &str = "shards";
const IN_PROGRESS: &str = "in-progress";

const LEGACY_HEADERS: [&str; 8] = [
    "sample\tnode\tlabel\tkind\tclade\tstate_index\trange_bits\trange",
    "sample\tnode\tlabel\tkind\tclade\tleft_clade\tright_clade\tancestor_state_index\tancestor_range_bits\tancestor_range\tleft_state_index\tleft_range_bits\tleft_range\tright_state_index\tright_range_bits\tright_range\tscenario_weight",
    "sample\tedge\tparent\tparent_clade\tchild\tchild_clade\tsegment\tq_index\tstart_time_from_parent\tend_time_from_parent\tstart_state_index\tstart_range_bits\tstart_range\tend_state_index\tend_range_bits\tend_range\tendpoint_probability\tvirtual_jump_count\tevent_count",
    "sample\tanagenetic_total\trange_expansion\tlocal_extirpation\tcladogenetic_total\trange_copying\tsubset_sympatry\tvicariance\tfounder_event\ttotal_branch_time",
    "sample\tq_index\tanagenetic_event_count\tevent_fraction",
    "sample\tstate_index\trange_bits\trange\toccupancy_time\toccupancy_fraction",
    "sample\tq_index\tstate_index\trange_bits\trange\toccupancy_time",
    "sample\tedge\tparent\tparent_clade\tchild\tchild_clade\tsegment\tq_index\ttime_from_parent\tevent_kind\tparameter\tarea_index\tarea\tfrom_state_index\tfrom_range_bits\tfrom_range\tto_state_index\tto_range_bits\tto_range",
];

const V2_EVENT_COUNTS_HEADER: &str = "sample\tanagenetic_total\trange_expansion\tlocal_extirpation\trange_switching\tcladogenetic_total\trange_copying\tsubset_sympatry\tvicariance\tfounder_event\ttotal_branch_time\tsegment_count\tconstrained_segment_count\tminimum_endpoint_probability\tmaximum_virtual_jump_count\tmaximum_anagenetic_events_per_segment\tforbidden_state_transitions\tforbidden_state_endpoints\tforbidden_state_time";

const COMPACT_HEADERS: [&str; 8] = [
    "sample\tnode\tstate_index",
    "sample\tnode\tancestor_state_index\tleft_state_index\tright_state_index\tscenario_weight",
    "sample\tedge\tsegment\tq_index\tstart_time_from_parent\tend_time_from_parent\tstart_state_index\tend_state_index\tendpoint_probability\tvirtual_jump_count\tevent_count",
    V2_EVENT_COUNTS_HEADER,
    "sample\tq_index\tanagenetic_event_count\tevent_fraction",
    "sample\tstate_index\toccupancy_time\toccupancy_fraction",
    "sample\tq_index\tstate_index\toccupancy_time",
    "sample\tedge\tsegment\tq_index\ttime_from_parent\tevent_kind\tparameter\tarea_index\tfrom_state_index\tto_state_index",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Layout {
    Monolithic,
    Sharded,
}

impl Layout {
    fn as_str(self) -> &'static str {
        match self {
            Self::Monolithic => "monolithic",
            Self::Sharded => "sharded",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OutputLevel {
    Legacy,
    Full,
    Compact,
    Summary,
}

impl OutputLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::Full => "full",
            Self::Compact => "compact",
            Self::Summary => "summary",
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct FormatInfo {
    format: &'static str,
    layout: Layout,
    level: OutputLevel,
    is_v2: bool,
    path_details: bool,
    sparse_occupancy: bool,
}

impl FormatInfo {
    fn parse(value: &str) -> Option<Self> {
        let (format, layout, level) = match value {
            "biogeo-bsm-tsv-v1" => ("biogeo-bsm-tsv-v1", Layout::Monolithic, OutputLevel::Legacy),
            "biogeo-bsm-sharded-tsv-v1" => (
                "biogeo-bsm-sharded-tsv-v1",
                Layout::Sharded,
                OutputLevel::Legacy,
            ),
            "biogeo-bsm-full-tsv-v2" => (
                "biogeo-bsm-full-tsv-v2",
                Layout::Monolithic,
                OutputLevel::Full,
            ),
            "biogeo-bsm-full-sharded-tsv-v2" => (
                "biogeo-bsm-full-sharded-tsv-v2",
                Layout::Sharded,
                OutputLevel::Full,
            ),
            "biogeo-bsm-compact-tsv-v2" => (
                "biogeo-bsm-compact-tsv-v2",
                Layout::Monolithic,
                OutputLevel::Compact,
            ),
            "biogeo-bsm-compact-sharded-tsv-v2" => (
                "biogeo-bsm-compact-sharded-tsv-v2",
                Layout::Sharded,
                OutputLevel::Compact,
            ),
            "biogeo-bsm-summary-tsv-v2" => (
                "biogeo-bsm-summary-tsv-v2",
                Layout::Monolithic,
                OutputLevel::Summary,
            ),
            "biogeo-bsm-summary-sharded-tsv-v2" => (
                "biogeo-bsm-summary-sharded-tsv-v2",
                Layout::Sharded,
                OutputLevel::Summary,
            ),
            _ => return None,
        };
        Some(Self {
            format,
            layout,
            level,
            is_v2: level != OutputLevel::Legacy,
            path_details: level != OutputLevel::Summary,
            sparse_occupancy: matches!(level, OutputLevel::Compact | OutputLevel::Summary),
        })
    }

    fn header(self, table: usize) -> &'static str {
        match self.level {
            OutputLevel::Legacy => LEGACY_HEADERS[table],
            OutputLevel::Full if table == 3 => V2_EVENT_COUNTS_HEADER,
            OutputLevel::Full => LEGACY_HEADERS[table],
            OutputLevel::Compact | OutputLevel::Summary => COMPACT_HEADERS[table],
        }
    }
}

#[derive(Clone, Debug)]
struct TableBundle {
    directory: PathBuf,
    sample_start: usize,
    sample_end: usize,
}

#[derive(Clone, Debug)]
struct Metadata {
    format: FormatInfo,
    run_status: String,
    completed_samples: usize,
    requested_samples: usize,
    completed_events: usize,
    states: usize,
    areas: usize,
    run_fingerprint: String,
}

#[derive(Clone, Debug, Default)]
struct ReferenceData {
    nodes: usize,
    edges: usize,
    periods: usize,
    state_bits: Vec<u64>,
    edge_lengths: Vec<f64>,
}

#[derive(Clone, Debug)]
struct DeepResult {
    data_rows: u64,
    periods: usize,
    diagnostic_violations: usize,
}

#[derive(Clone, Debug)]
pub struct InspectionReport {
    pub bsm_format: String,
    pub output_level: String,
    pub layout: String,
    pub run_status: String,
    pub completed_samples: usize,
    pub requested_samples: usize,
    pub completed_anagenetic_events: usize,
    pub shards: usize,
    pub states: usize,
    pub areas: usize,
    pub nodes: Option<usize>,
    pub edges: Option<usize>,
    pub periods: Option<usize>,
    pub path_details: bool,
    pub sparse_occupancy: bool,
    pub deep: bool,
    pub files_checked: usize,
    pub data_rows_checked: Option<u64>,
    pub event_count_validation: &'static str,
    pub occupancy_validation: &'static str,
    pub path_validation: &'static str,
    pub state_constraint_validation: &'static str,
    pub diagnostic_violations: Option<usize>,
}

#[derive(Debug)]
pub enum BsmInspectionError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Invalid {
        path: PathBuf,
        message: String,
    },
}

impl fmt::Display for BsmInspectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "failed to read {}: {source}", path.display())
            }
            Self::Invalid { path, message } => {
                write!(f, "invalid BSM result at {}: {message}", path.display())
            }
        }
    }
}

impl Error for BsmInspectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Invalid { .. } => None,
        }
    }
}

fn invalid(path: impl Into<PathBuf>, message: impl Into<String>) -> BsmInspectionError {
    BsmInspectionError::Invalid {
        path: path.into(),
        message: message.into(),
    }
}

fn read_text(path: &Path) -> Result<String, BsmInspectionError> {
    fs::read_to_string(path).map_err(|source| BsmInspectionError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn parse_key_values(path: &Path) -> Result<BTreeMap<String, String>, BsmInspectionError> {
    let text = read_text(path)?;
    let mut lines = text.lines();
    if lines.next() != Some("key\tvalue") {
        return Err(invalid(path, "missing key/value header"));
    }
    let mut values = BTreeMap::new();
    for (offset, line) in lines.enumerate() {
        let line_number = offset + 2;
        let (key, value) = line
            .split_once('\t')
            .ok_or_else(|| invalid(path, format!("line {line_number} is not a key/value row")))?;
        if key.is_empty() || values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(invalid(
                path,
                format!("line {line_number} has an empty or duplicate key {key:?}"),
            ));
        }
    }
    Ok(values)
}

fn required<'a>(
    path: &Path,
    values: &'a BTreeMap<String, String>,
    key: &str,
) -> Result<&'a str, BsmInspectionError> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| invalid(path, format!("missing required metadata key {key:?}")))
}

fn parse_usize_at(path: &Path, name: &str, value: &str) -> Result<usize, BsmInspectionError> {
    value
        .parse::<usize>()
        .map_err(|error| invalid(path, format!("invalid {name} value {value:?}: {error}")))
}

fn parse_u64_at(path: &Path, name: &str, value: &str) -> Result<u64, BsmInspectionError> {
    value
        .parse::<u64>()
        .map_err(|error| invalid(path, format!("invalid {name} value {value:?}: {error}")))
}

fn parse_f64_at(path: &Path, name: &str, value: &str) -> Result<f64, BsmInspectionError> {
    let parsed = value
        .parse::<f64>()
        .map_err(|error| invalid(path, format!("invalid {name} value {value:?}: {error}")))?;
    if !parsed.is_finite() {
        return Err(invalid(
            path,
            format!("{name} must be finite, got {value:?}"),
        ));
    }
    Ok(parsed)
}

fn load_metadata(root: &Path) -> Result<Metadata, BsmInspectionError> {
    if !root.is_dir() {
        return Err(invalid(root, "result path is not a directory"));
    }
    let path = root.join("metadata.tsv");
    let values = parse_key_values(&path)?;
    let format_value = required(&path, &values, "format")?;
    let format = FormatInfo::parse(format_value).ok_or_else(|| {
        invalid(
            &path,
            format!("unsupported BSM directory format {format_value:?}"),
        )
    })?;
    let run_status = required(&path, &values, "status")?.to_string();
    if !matches!(
        run_status.as_str(),
        "complete" | "incomplete" | "cancelled" | "time_limit" | "event_limit"
    ) {
        return Err(invalid(
            &path,
            format!("unsupported BSM run status {run_status:?}"),
        ));
    }
    let completed_samples = parse_usize_at(
        &path,
        "completed_samples",
        required(&path, &values, "completed_samples")?,
    )?;
    let requested_samples = parse_usize_at(&path, "samples", required(&path, &values, "samples")?)?;
    if completed_samples > requested_samples {
        return Err(invalid(
            &path,
            "completed_samples exceeds requested samples",
        ));
    }
    if run_status == "complete" && completed_samples != requested_samples {
        return Err(invalid(
            &path,
            "complete result does not contain every requested sample",
        ));
    }
    let completed_events = parse_usize_at(
        &path,
        "completed_anagenetic_events",
        required(&path, &values, "completed_anagenetic_events")?,
    )?;
    let states = parse_usize_at(&path, "states", required(&path, &values, "states")?)?;
    let areas = parse_usize_at(&path, "areas", required(&path, &values, "areas")?)?;
    let run_fingerprint = required(&path, &values, "run_fingerprint")?.to_string();
    if run_fingerprint.len() != 16 || !run_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(invalid(
            &path,
            "run_fingerprint is not 16 hexadecimal digits",
        ));
    }
    if format.is_v2 {
        let expected = [
            ("output_level", format.level.as_str()),
            (
                "path_details",
                if format.path_details { "true" } else { "false" },
            ),
            (
                "sparse_occupancy",
                if format.sparse_occupancy {
                    "true"
                } else {
                    "false"
                },
            ),
        ];
        for (key, expected_value) in expected {
            let actual = required(&path, &values, key)?;
            if actual != expected_value {
                return Err(invalid(
                    &path,
                    format!("{key} is {actual:?}, expected {expected_value:?}"),
                ));
            }
        }
    }
    Ok(Metadata {
        format,
        run_status,
        completed_samples,
        requested_samples,
        completed_events,
        states,
        areas,
        run_fingerprint,
    })
}

fn directory_entries(path: &Path) -> Result<BTreeSet<String>, BsmInspectionError> {
    fs::read_dir(path)
        .map_err(|source| BsmInspectionError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .map(|entry| {
            entry
                .map_err(|source| BsmInspectionError::Io {
                    path: path.to_path_buf(),
                    source,
                })
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
        })
        .collect()
}

fn expected_root_entries(info: FormatInfo) -> BTreeSet<String> {
    let mut expected = BTreeSet::from(["metadata.tsv".to_string()]);
    if info.is_v2 {
        expected.extend(REFERENCE_FILES.map(str::to_string));
    }
    match info.layout {
        Layout::Monolithic => {
            expected.insert(CHECKPOINTS.to_string());
            expected.extend(TABLE_FILES.map(str::to_string));
        }
        Layout::Sharded => {
            expected.extend([
                "manifest.tsv".to_string(),
                SHARDS.to_string(),
                IN_PROGRESS.to_string(),
            ]);
        }
    }
    expected
}

#[derive(Clone, Debug)]
struct Checkpoint {
    completed_samples: usize,
    completed_events: Option<usize>,
    fingerprint: String,
    table_lengths: [u64; 8],
}

fn load_latest_checkpoint(directory: &Path) -> Result<Checkpoint, BsmInspectionError> {
    let checkpoint_dir = directory.join(CHECKPOINTS);
    if !checkpoint_dir.is_dir() {
        return Err(invalid(&checkpoint_dir, "missing checkpoint directory"));
    }
    let mut latest: Option<(usize, PathBuf)> = None;
    for name in directory_entries(&checkpoint_dir)? {
        let Some(index) = name
            .strip_prefix("checkpoint-")
            .and_then(|value| value.strip_suffix(".tsv"))
            .and_then(|value| value.parse::<usize>().ok())
        else {
            return Err(invalid(
                checkpoint_dir.join(name),
                "unexpected checkpoint entry",
            ));
        };
        if latest.as_ref().is_none_or(|(current, _)| index > *current) {
            latest = Some((index, checkpoint_dir.join(name)));
        }
    }
    let (name_samples, path) = latest.ok_or_else(|| invalid(&checkpoint_dir, "no checkpoint"))?;
    let values = parse_key_values(&path)?;
    let format = required(&path, &values, "format")?;
    if !matches!(
        format,
        "biogeo-bsm-checkpoint-v1" | "biogeo-bsm-checkpoint-v2"
    ) {
        return Err(invalid(
            &path,
            format!("unsupported checkpoint format {format:?}"),
        ));
    }
    let completed_samples = parse_usize_at(
        &path,
        "completed_samples",
        required(&path, &values, "completed_samples")?,
    )?;
    if completed_samples != name_samples {
        return Err(invalid(
            &path,
            "checkpoint file name and completed_samples disagree",
        ));
    }
    let completed_events = values
        .get("completed_anagenetic_events")
        .map(|value| parse_usize_at(&path, "completed_anagenetic_events", value))
        .transpose()?;
    if format == "biogeo-bsm-checkpoint-v2" && completed_events.is_none() {
        return Err(invalid(&path, "v2 checkpoint lacks cumulative event count"));
    }
    let fingerprint = required(&path, &values, "run_fingerprint")?.to_string();
    let mut table_lengths = [0; 8];
    for (index, file_name) in TABLE_FILES.iter().enumerate() {
        table_lengths[index] =
            parse_u64_at(&path, file_name, required(&path, &values, file_name)?)?;
    }
    let allowed = BTreeSet::from_iter(
        [
            "format",
            "completed_samples",
            "completed_anagenetic_events",
            "run_fingerprint",
        ]
        .into_iter()
        .chain(TABLE_FILES),
    );
    if let Some(unknown) = values.keys().find(|key| !allowed.contains(key.as_str())) {
        return Err(invalid(
            &path,
            format!("unknown checkpoint key {unknown:?}"),
        ));
    }
    Ok(Checkpoint {
        completed_samples,
        completed_events,
        fingerprint,
        table_lengths,
    })
}

fn validate_bundle(
    directory: &Path,
    info: FormatInfo,
    expected_samples: usize,
    expected_events: Option<usize>,
    fingerprint: &str,
    validate_entries: bool,
) -> Result<Checkpoint, BsmInspectionError> {
    if validate_entries {
        let expected = BTreeSet::from_iter(
            std::iter::once(CHECKPOINTS.to_string()).chain(TABLE_FILES.map(str::to_string)),
        );
        let actual = directory_entries(directory)?;
        if actual != expected {
            return Err(invalid(
                directory,
                format!("table directory entries differ: found {actual:?}, expected {expected:?}"),
            ));
        }
    }
    let checkpoint = load_latest_checkpoint(directory)?;
    if checkpoint.completed_samples != expected_samples {
        return Err(invalid(
            directory,
            format!(
                "latest checkpoint ends at sample {}, expected {expected_samples}",
                checkpoint.completed_samples
            ),
        ));
    }
    if checkpoint.fingerprint != fingerprint {
        return Err(invalid(directory, "checkpoint run_fingerprint mismatch"));
    }
    if expected_events.is_some() && checkpoint.completed_events != expected_events {
        return Err(invalid(
            directory,
            "checkpoint cumulative event count mismatch",
        ));
    }
    for (index, file_name) in TABLE_FILES.iter().enumerate() {
        let path = directory.join(file_name);
        let length = fs::metadata(&path)
            .map_err(|source| BsmInspectionError::Io {
                path: path.clone(),
                source,
            })?
            .len();
        if length != checkpoint.table_lengths[index] {
            return Err(invalid(
                &path,
                format!(
                    "file length {length} differs from checkpoint length {}",
                    checkpoint.table_lengths[index]
                ),
            ));
        }
        validate_header(&path, info.header(index))?;
    }
    Ok(checkpoint)
}

fn validate_header(path: &Path, expected: &str) -> Result<(), BsmInspectionError> {
    let file = fs::File::open(path).map_err(|source| BsmInspectionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut reader = BufReader::new(file);
    let mut header = String::new();
    reader
        .read_line(&mut header)
        .map_err(|source| BsmInspectionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let header = header.trim_end_matches(['\r', '\n']);
    if header != expected {
        return Err(invalid(
            path,
            format!("table header is {header:?}, expected {expected:?}"),
        ));
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct ManifestRow {
    index: usize,
    start: usize,
    end: usize,
    events: usize,
    cumulative_events: usize,
    directory: String,
    lengths: [u64; 8],
}

#[derive(Clone, Debug)]
struct Manifest {
    completed_samples: usize,
    completed_events: usize,
    rows: Vec<ManifestRow>,
}

fn load_manifest(root: &Path, metadata: &Metadata) -> Result<Manifest, BsmInspectionError> {
    let path = root.join("manifest.tsv");
    let text = read_text(&path)?;
    let marker = "\nshards\n";
    let (preamble, table) = text
        .split_once(marker)
        .ok_or_else(|| invalid(&path, "missing shards section"))?;
    let mut values = BTreeMap::new();
    let mut lines = preamble.lines();
    if lines.next() != Some("key\tvalue") {
        return Err(invalid(&path, "manifest lacks key/value header"));
    }
    for (offset, line) in lines.enumerate() {
        let (key, value) = line
            .split_once('\t')
            .ok_or_else(|| invalid(&path, format!("invalid preamble line {}", offset + 2)))?;
        if values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(invalid(&path, format!("duplicate manifest key {key:?}")));
        }
    }
    if required(&path, &values, "format")? != "biogeo-bsm-shard-manifest-v1"
        || required(&path, &values, "run_fingerprint")? != metadata.run_fingerprint
    {
        return Err(invalid(
            &path,
            "manifest format or run fingerprint mismatch",
        ));
    }
    let samples = parse_usize_at(&path, "samples", required(&path, &values, "samples")?)?;
    if samples != metadata.requested_samples {
        return Err(invalid(&path, "manifest requested sample count mismatch"));
    }
    let completed_shards = parse_usize_at(
        &path,
        "completed_shards",
        required(&path, &values, "completed_shards")?,
    )?;
    let completed_samples = parse_usize_at(
        &path,
        "completed_samples",
        required(&path, &values, "completed_samples")?,
    )?;
    let completed_events = parse_usize_at(
        &path,
        "completed_anagenetic_events",
        required(&path, &values, "completed_anagenetic_events")?,
    )?;
    let expected_header = std::iter::once(
        "shard_index\tsample_start\tsample_end_exclusive\tsample_count\tanagenetic_events\tcumulative_anagenetic_events\tdirectory".to_string(),
    )
    .chain(TABLE_FILES.map(|name| format!("{name}_bytes")))
    .collect::<Vec<_>>()
    .join("\t");
    let mut rows_iter = table.lines();
    if rows_iter.next() != Some(expected_header.as_str()) {
        return Err(invalid(&path, "shard table header mismatch"));
    }
    let mut rows = Vec::new();
    let mut expected_start = 0;
    let mut previous_events = 0_usize;
    for (offset, line) in rows_iter.enumerate() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 15 {
            return Err(invalid(
                &path,
                format!(
                    "shard row {} has {} fields, expected 15",
                    offset + 11,
                    fields.len()
                ),
            ));
        }
        let mut lengths = [0_u64; 8];
        for (index, value) in fields[7..].iter().enumerate() {
            lengths[index] = parse_u64_at(&path, TABLE_FILES[index], value)?;
        }
        let row = ManifestRow {
            index: parse_usize_at(&path, "shard_index", fields[0])?,
            start: parse_usize_at(&path, "sample_start", fields[1])?,
            end: parse_usize_at(&path, "sample_end_exclusive", fields[2])?,
            events: parse_usize_at(&path, "anagenetic_events", fields[4])?,
            cumulative_events: parse_usize_at(&path, "cumulative_anagenetic_events", fields[5])?,
            directory: fields[6].to_string(),
            lengths,
        };
        let sample_count = parse_usize_at(&path, "sample_count", fields[3])?;
        if row.index != rows.len()
            || row.start != expected_start
            || row.end <= row.start
            || sample_count != row.end - row.start
            || row.cumulative_events
                != previous_events
                    .checked_add(row.events)
                    .ok_or_else(|| invalid(&path, "manifest event count overflow"))?
            || row.directory != format!("shards/shard-{:020}-{:020}", row.start, row.end)
        {
            return Err(invalid(&path, format!("inconsistent shard row {offset}")));
        }
        expected_start = row.end;
        previous_events = row.cumulative_events;
        rows.push(row);
    }
    if rows.len() != completed_shards
        || expected_start != completed_samples
        || previous_events != completed_events
    {
        return Err(invalid(&path, "manifest summary and shard rows disagree"));
    }
    Ok(Manifest {
        completed_samples,
        completed_events,
        rows,
    })
}

fn load_bundles(
    root: &Path,
    metadata: &Metadata,
) -> Result<(Vec<TableBundle>, usize), BsmInspectionError> {
    match metadata.format.layout {
        Layout::Monolithic => {
            let checkpoint = validate_bundle(
                root,
                metadata.format,
                metadata.completed_samples,
                Some(metadata.completed_events),
                &metadata.run_fingerprint,
                false,
            )?;
            if checkpoint.completed_events != Some(metadata.completed_events) {
                return Err(invalid(
                    root,
                    "metadata and checkpoint event totals disagree",
                ));
            }
            Ok((
                vec![TableBundle {
                    directory: root.to_path_buf(),
                    sample_start: 0,
                    sample_end: metadata.completed_samples,
                }],
                1,
            ))
        }
        Layout::Sharded => {
            let manifest = load_manifest(root, metadata)?;
            let mut bundles = Vec::new();
            for row in &manifest.rows {
                let directory = root.join(Path::new(&row.directory));
                let checkpoint = validate_bundle(
                    &directory,
                    metadata.format,
                    row.end,
                    Some(row.cumulative_events),
                    &metadata.run_fingerprint,
                    true,
                )?;
                if checkpoint.table_lengths != row.lengths {
                    return Err(invalid(
                        &directory,
                        "manifest and checkpoint table lengths disagree",
                    ));
                }
                bundles.push(TableBundle {
                    directory,
                    sample_start: row.start,
                    sample_end: row.end,
                });
            }
            let in_progress_root = root.join(IN_PROGRESS);
            let in_progress_entries = directory_entries(&in_progress_root)?;
            if in_progress_entries.len() > 1 {
                return Err(invalid(
                    &in_progress_root,
                    "more than one in-progress shard exists",
                ));
            }
            if let Some(name) = in_progress_entries.into_iter().next() {
                let expected_prefix = format!("shard-{:020}-", manifest.completed_samples);
                if !name.starts_with(&expected_prefix) {
                    return Err(invalid(
                        in_progress_root.join(&name),
                        "in-progress shard does not follow the published prefix",
                    ));
                }
                let directory = in_progress_root.join(name);
                validate_bundle(
                    &directory,
                    metadata.format,
                    metadata.completed_samples,
                    Some(metadata.completed_events),
                    &metadata.run_fingerprint,
                    true,
                )?;
                bundles.push(TableBundle {
                    directory,
                    sample_start: manifest.completed_samples,
                    sample_end: metadata.completed_samples,
                });
            }
            if bundles.last().map_or(0, |bundle| bundle.sample_end) != metadata.completed_samples {
                return Err(invalid(
                    root,
                    "metadata completed_samples does not match shard prefix",
                ));
            }
            if metadata.run_status == "complete"
                && (manifest.completed_samples != metadata.requested_samples
                    || manifest.completed_events != metadata.completed_events
                    || bundles.len() != manifest.rows.len())
            {
                return Err(invalid(root, "complete sharded result has staged data"));
            }
            Ok((bundles, manifest.rows.len()))
        }
    }
}

fn validate_root_entries(root: &Path, info: FormatInfo) -> Result<(), BsmInspectionError> {
    let actual = directory_entries(root)?;
    let expected = expected_root_entries(info);
    if actual != expected {
        return Err(invalid(
            root,
            format!("root entries differ: found {actual:?}, expected {expected:?}"),
        ));
    }
    Ok(())
}

fn read_reference_table(
    path: &Path,
    expected_header: &str,
) -> Result<Vec<Vec<String>>, BsmInspectionError> {
    let file = fs::File::open(path).map_err(|source| BsmInspectionError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut lines = BufReader::new(file).lines();
    let header = lines
        .next()
        .transpose()
        .map_err(|source| BsmInspectionError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .ok_or_else(|| invalid(path, "empty reference table"))?;
    if header != expected_header {
        return Err(invalid(
            path,
            format!("reference header mismatch: {header:?}"),
        ));
    }
    lines
        .enumerate()
        .map(|(offset, line)| {
            line.map_err(|source| BsmInspectionError::Io {
                path: path.to_path_buf(),
                source,
            })
            .and_then(|line| {
                if line.is_empty() {
                    Err(invalid(path, format!("empty row at line {}", offset + 2)))
                } else {
                    Ok(line.split('\t').map(str::to_string).collect())
                }
            })
        })
        .collect()
}

fn load_references(root: &Path, metadata: &Metadata) -> Result<ReferenceData, BsmInspectionError> {
    if !metadata.format.is_v2 {
        return Ok(ReferenceData::default());
    }
    let areas = read_reference_table(&root.join("areas.tsv"), "area_index\tarea")?;
    if areas.len() != metadata.areas {
        return Err(invalid(root.join("areas.tsv"), "area count mismatch"));
    }
    for (index, row) in areas.iter().enumerate() {
        if row.len() != 2
            || parse_usize_at(&root.join("areas.tsv"), "area_index", &row[0])? != index
        {
            return Err(invalid(root.join("areas.tsv"), "non-contiguous area index"));
        }
    }
    let states = read_reference_table(&root.join("states.tsv"), "state_index\trange_bits\trange")?;
    if states.len() != metadata.states {
        return Err(invalid(root.join("states.tsv"), "state count mismatch"));
    }
    let mut state_bits = Vec::with_capacity(states.len());
    let mut unique_bits = BTreeSet::new();
    for (index, row) in states.iter().enumerate() {
        if row.len() != 3
            || parse_usize_at(&root.join("states.tsv"), "state_index", &row[0])? != index
        {
            return Err(invalid(
                root.join("states.tsv"),
                "non-contiguous state index",
            ));
        }
        let bits = parse_u64_at(&root.join("states.tsv"), "range_bits", &row[1])?;
        if !unique_bits.insert(bits) {
            return Err(invalid(root.join("states.tsv"), "duplicate range bitset"));
        }
        state_bits.push(bits);
    }
    let nodes = read_reference_table(&root.join("nodes.tsv"), "node\tlabel\tkind")?;
    for (index, row) in nodes.iter().enumerate() {
        if row.len() != 3
            || parse_usize_at(&root.join("nodes.tsv"), "node", &row[0])? != index
            || !matches!(row[2].as_str(), "root" | "internal" | "tip")
        {
            return Err(invalid(
                root.join("nodes.tsv"),
                "invalid node reference row",
            ));
        }
    }
    let edges = read_reference_table(&root.join("edges.tsv"), "edge\tparent\tchild\tlength")?;
    let mut edge_lengths = Vec::with_capacity(edges.len());
    for (index, row) in edges.iter().enumerate() {
        let length = if row.len() == 4 {
            parse_f64_at(&root.join("edges.tsv"), "length", &row[3])?
        } else {
            -1.0
        };
        if row.len() != 4
            || parse_usize_at(&root.join("edges.tsv"), "edge", &row[0])? != index
            || parse_usize_at(&root.join("edges.tsv"), "parent", &row[1])? >= nodes.len()
            || parse_usize_at(&root.join("edges.tsv"), "child", &row[2])? >= nodes.len()
            || length < 0.0
        {
            return Err(invalid(
                root.join("edges.tsv"),
                "invalid edge reference row",
            ));
        }
        edge_lengths.push(length);
    }
    let periods = read_reference_table(
        &root.join("periods.tsv"),
        "q_index\toldest_age\thas_state_constraint\tallowed_state_count",
    )?;
    for (index, row) in periods.iter().enumerate() {
        if row.len() != 4
            || parse_usize_at(&root.join("periods.tsv"), "q_index", &row[0])? != index
            || (row[1] != "unbounded"
                && parse_f64_at(&root.join("periods.tsv"), "oldest_age", &row[1]).is_err())
            || !matches!(row[2].as_str(), "true" | "false")
            || parse_usize_at(&root.join("periods.tsv"), "allowed_state_count", &row[3])?
                > states.len()
        {
            return Err(invalid(
                root.join("periods.tsv"),
                "invalid period reference row",
            ));
        }
    }
    Ok(ReferenceData {
        nodes: nodes.len(),
        edges: edges.len(),
        periods: periods.len(),
        state_bits,
        edge_lengths,
    })
}

fn for_each_row<F>(
    bundles: &[TableBundle],
    table_index: usize,
    expected_header: &str,
    mut callback: F,
) -> Result<u64, BsmInspectionError>
where
    F: FnMut(&Path, usize, &[&str]) -> Result<(), BsmInspectionError>,
{
    let expected_fields = expected_header.split('\t').count();
    let mut rows = 0_u64;
    for bundle in bundles {
        let path = bundle.directory.join(TABLE_FILES[table_index]);
        let file = fs::File::open(&path).map_err(|source| BsmInspectionError::Io {
            path: path.clone(),
            source,
        })?;
        for (offset, line) in BufReader::new(file).lines().enumerate() {
            let line_number = offset + 1;
            let line = line.map_err(|source| BsmInspectionError::Io {
                path: path.clone(),
                source,
            })?;
            if line_number == 1 {
                if line != expected_header {
                    return Err(invalid(&path, "table header changed during deep scan"));
                }
                continue;
            }
            if line.is_empty() {
                return Err(invalid(
                    &path,
                    format!("empty data row at line {line_number}"),
                ));
            }
            let fields = line.split('\t').collect::<Vec<_>>();
            if fields.len() != expected_fields {
                return Err(invalid(
                    &path,
                    format!(
                        "line {line_number} has {} fields, expected {expected_fields}",
                        fields.len()
                    ),
                ));
            }
            callback(&path, line_number, &fields)?;
            rows = rows
                .checked_add(1)
                .ok_or_else(|| invalid(&path, "data row count overflow"))?;
        }
    }
    Ok(rows)
}

fn row_usize(
    path: &Path,
    line: usize,
    name: &str,
    value: &str,
) -> Result<usize, BsmInspectionError> {
    parse_usize_at(path, &format!("{name} on line {line}"), value)
}

fn row_f64(path: &Path, line: usize, name: &str, value: &str) -> Result<f64, BsmInspectionError> {
    parse_f64_at(path, &format!("{name} on line {line}"), value)
}

fn close_enough(actual: f64, expected: f64) -> bool {
    (actual - expected).abs() <= 1e-8_f64.max(expected.abs() * 1e-10)
}

fn validate_deep(
    metadata: &Metadata,
    references: &ReferenceData,
    bundles: &[TableBundle],
) -> Result<DeepResult, BsmInspectionError> {
    let mut rows = 0_u64;
    let mut branch_times = vec![0.0; metadata.completed_samples];
    let mut anagenetic_counts = vec![0_usize; metadata.completed_samples];
    let mut segment_counts = vec![0_usize; metadata.completed_samples];
    let mut diagnostic_violations = 0_usize;
    let mut expected_sample = 0;
    rows += for_each_row(
        bundles,
        3,
        metadata.format.header(3),
        |path, line, fields| {
            let sample = row_usize(path, line, "sample", fields[0])?;
            if sample != expected_sample || sample >= metadata.completed_samples {
                return Err(invalid(
                    path,
                    format!(
                        "sample_event_counts is not a contiguous 0-based sample sequence at line {line}"
                    ),
                ));
            }
            expected_sample += 1;
            let anagenetic = row_usize(path, line, "anagenetic_total", fields[1])?;
            let expansion = row_usize(path, line, "range_expansion", fields[2])?;
            let extirpation = row_usize(path, line, "local_extirpation", fields[3])?;
            let (switching, clado_index) = if metadata.format.is_v2 {
                (row_usize(path, line, "range_switching", fields[4])?, 5)
            } else {
                (0, 4)
            };
            let component_total = expansion
                .checked_add(extirpation)
                .and_then(|sum| sum.checked_add(switching))
                .ok_or_else(|| invalid(path, "anagenetic component count overflow"))?;
            if anagenetic != component_total {
                return Err(invalid(
                    path,
                    format!("anagenetic event components disagree at line {line}"),
                ));
            }
            let cladogenetic = row_usize(path, line, "cladogenetic_total", fields[clado_index])?;
            let clado_sum = (1..=4).try_fold(0_usize, |sum, offset| {
                sum.checked_add(row_usize(
                    path,
                    line,
                    "cladogenetic component",
                    fields[clado_index + offset],
                )?)
                .ok_or_else(|| invalid(path, "cladogenetic count overflow"))
            })?;
            if cladogenetic != clado_sum {
                return Err(invalid(
                    path,
                    format!("cladogenetic event components disagree at line {line}"),
                ));
            }
            let branch_time = row_f64(path, line, "total_branch_time", fields[clado_index + 5])?;
            if branch_time < 0.0 {
                return Err(invalid(
                    path,
                    format!("negative total branch time at line {line}"),
                ));
            }
            branch_times[sample] = branch_time;
            anagenetic_counts[sample] = anagenetic;
            if metadata.format.is_v2 {
                segment_counts[sample] = row_usize(path, line, "segment_count", fields[11])?;
                let minimum = fields[13];
                if minimum != "NA" {
                    let probability = row_f64(path, line, "minimum_endpoint_probability", minimum)?;
                    if probability <= 0.0 || probability > 1.0 + 1e-10 {
                        return Err(invalid(
                            path,
                            format!("invalid minimum endpoint probability at line {line}"),
                        ));
                    }
                }
                let transitions = row_usize(path, line, "forbidden_state_transitions", fields[16])?;
                let endpoints = row_usize(path, line, "forbidden_state_endpoints", fields[17])?;
                let forbidden_time = row_f64(path, line, "forbidden_state_time", fields[18])?;
                if forbidden_time < 0.0 {
                    return Err(invalid(
                        path,
                        format!("negative forbidden state time at line {line}"),
                    ));
                }
                diagnostic_violations = diagnostic_violations
                    .checked_add(transitions)
                    .and_then(|sum| sum.checked_add(endpoints))
                    .and_then(|sum| sum.checked_add(usize::from(forbidden_time > 1e-12)))
                    .ok_or_else(|| invalid(path, "diagnostic count overflow"))?;
            }
            Ok(())
        },
    )?;
    if expected_sample != metadata.completed_samples {
        return Err(invalid(
            &bundles[0].directory,
            "sample_event_counts does not cover the committed sample prefix",
        ));
    }
    let total_events = anagenetic_counts.iter().try_fold(0_usize, |sum, count| {
        sum.checked_add(*count)
            .ok_or_else(|| invalid(&bundles[0].directory, "event total overflow"))
    })?;
    if total_events != metadata.completed_events {
        return Err(invalid(
            &bundles[0].directory,
            "metadata and sample event totals disagree",
        ));
    }
    if diagnostic_violations != 0 {
        return Err(invalid(
            &bundles[0].directory,
            format!("found {diagnostic_violations} state-constraint diagnostic violation(s)"),
        ));
    }

    let mut inferred_periods = references.periods;
    let mut period_event_sums = vec![0_usize; metadata.completed_samples];
    let mut period_row_counts = vec![0_usize; metadata.completed_samples];
    let mut previous_key = None;
    rows += for_each_row(
        bundles,
        4,
        metadata.format.header(4),
        |path, line, fields| {
            let sample = row_usize(path, line, "sample", fields[0])?;
            let q_index = row_usize(path, line, "q_index", fields[1])?;
            if sample >= metadata.completed_samples {
                return Err(invalid(
                    path,
                    format!("sample index out of range at line {line}"),
                ));
            }
            if references.periods > 0 && q_index >= references.periods {
                return Err(invalid(
                    path,
                    format!("q_index out of range at line {line}"),
                ));
            }
            if previous_key.is_some_and(|key| (sample, q_index) <= key) {
                return Err(invalid(
                    path,
                    format!("period event keys are duplicated or unsorted at line {line}"),
                ));
            }
            previous_key = Some((sample, q_index));
            inferred_periods = inferred_periods.max(
                q_index
                    .checked_add(1)
                    .ok_or_else(|| invalid(path, "q_index overflow"))?,
            );
            let count = row_usize(path, line, "anagenetic_event_count", fields[2])?;
            let fraction = row_f64(path, line, "event_fraction", fields[3])?;
            if !(0.0..=1.0 + 1e-10).contains(&fraction) {
                return Err(invalid(
                    path,
                    format!("event fraction out of range at line {line}"),
                ));
            }
            period_event_sums[sample] = period_event_sums[sample]
                .checked_add(count)
                .ok_or_else(|| invalid(path, "period event count overflow"))?;
            period_row_counts[sample] += 1;
            let expected_fraction = if anagenetic_counts[sample] == 0 {
                0.0
            } else {
                count as f64 / anagenetic_counts[sample] as f64
            };
            if !close_enough(fraction, expected_fraction) {
                return Err(invalid(
                    path,
                    format!("event fraction disagrees with its count at line {line}"),
                ));
            }
            Ok(())
        },
    )?;
    if period_event_sums != anagenetic_counts
        || period_row_counts
            .iter()
            .any(|count| *count != inferred_periods)
    {
        return Err(invalid(
            &bundles[0].directory,
            "period event totals do not match per-sample event totals",
        ));
    }

    for (table_index, occupancy_name) in [(5, "state occupancy"), (6, "period-state occupancy")] {
        let mut sums = vec![0.0_f64; metadata.completed_samples];
        let mut fraction_sums = vec![0.0_f64; metadata.completed_samples];
        let mut row_counts = vec![0_usize; metadata.completed_samples];
        let mut previous = None;
        rows += for_each_row(
            bundles,
            table_index,
            metadata.format.header(table_index),
            |path, line, fields| {
                let sample = row_usize(path, line, "sample", fields[0])?;
                if sample >= metadata.completed_samples {
                    return Err(invalid(
                        path,
                        format!("sample index out of range at line {line}"),
                    ));
                }
                let (q_index, state_index, time_index, key) = if table_index == 5 {
                    let state = row_usize(path, line, "state_index", fields[1])?;
                    let time = if metadata.format.level == OutputLevel::Full
                        || metadata.format.level == OutputLevel::Legacy
                    {
                        4
                    } else {
                        2
                    };
                    (None, state, time, (sample, 0, state))
                } else {
                    let q = row_usize(path, line, "q_index", fields[1])?;
                    let state = row_usize(path, line, "state_index", fields[2])?;
                    let time = if metadata.format.level == OutputLevel::Full
                        || metadata.format.level == OutputLevel::Legacy
                    {
                        5
                    } else {
                        3
                    };
                    (Some(q), state, time, (sample, q, state))
                };
                if state_index >= metadata.states || q_index.is_some_and(|q| q >= inferred_periods)
                {
                    return Err(invalid(
                        path,
                        format!("occupancy reference out of range at line {line}"),
                    ));
                }
                if previous.is_some_and(|old| key <= old) {
                    return Err(invalid(
                        path,
                        format!("{occupancy_name} keys are duplicated or unsorted at line {line}"),
                    ));
                }
                previous = Some(key);
                let time = row_f64(path, line, "occupancy_time", fields[time_index])?;
                if time < 0.0 || (metadata.format.sparse_occupancy && time == 0.0) {
                    return Err(invalid(
                        path,
                        format!("invalid occupancy time at line {line}"),
                    ));
                }
                sums[sample] += time;
                row_counts[sample] += 1;
                if table_index == 5 {
                    let fraction =
                        row_f64(path, line, "occupancy_fraction", fields[time_index + 1])?;
                    let expected_fraction = if branch_times[sample] == 0.0 {
                        0.0
                    } else {
                        time / branch_times[sample]
                    };
                    if !(0.0..=1.0 + 1e-10).contains(&fraction)
                        || !close_enough(fraction, expected_fraction)
                    {
                        return Err(invalid(
                            path,
                            format!("occupancy fraction disagrees with time at line {line}"),
                        ));
                    }
                    fraction_sums[sample] += fraction;
                }
                Ok(())
            },
        )?;
        for (sample, (actual, expected)) in sums.iter().zip(&branch_times).enumerate() {
            if !close_enough(*actual, *expected) {
                return Err(invalid(
                    &bundles[0].directory,
                    format!(
                        "{occupancy_name} for sample {sample} sums to {actual}, expected {expected}"
                    ),
                ));
            }
            if !metadata.format.sparse_occupancy {
                let expected_rows = if table_index == 5 {
                    metadata.states
                } else {
                    inferred_periods
                        .checked_mul(metadata.states)
                        .ok_or_else(|| {
                            invalid(&bundles[0].directory, "occupancy row count overflow")
                        })?
                };
                if row_counts[sample] != expected_rows {
                    return Err(invalid(
                        &bundles[0].directory,
                        format!(
                            "{occupancy_name} for sample {sample} has {} rows, expected {expected_rows}",
                            row_counts[sample]
                        ),
                    ));
                }
            }
            if table_index == 5 && *expected > 0.0 && !close_enough(fraction_sums[sample], 1.0) {
                return Err(invalid(
                    &bundles[0].directory,
                    format!(
                        "state occupancy fractions for sample {sample} sum to {}",
                        fraction_sums[sample]
                    ),
                ));
            }
        }
    }

    if metadata.format.path_details {
        rows += validate_path_tables(
            metadata,
            references,
            bundles,
            &anagenetic_counts,
            &segment_counts,
        )?;
    } else {
        for table_index in [0, 1, 2, 7] {
            let path_rows = for_each_row(
                bundles,
                table_index,
                metadata.format.header(table_index),
                |path, line, _| {
                    Err(invalid(
                        path,
                        format!("summary path table contains data at line {line}"),
                    ))
                },
            )?;
            rows += path_rows;
        }
    }

    Ok(DeepResult {
        data_rows: rows,
        periods: inferred_periods,
        diagnostic_violations,
    })
}

struct TsvRowReader {
    path: PathBuf,
    lines: std::io::Lines<BufReader<fs::File>>,
    line_number: usize,
    expected_fields: usize,
}

impl TsvRowReader {
    fn open(path: PathBuf, expected_header: &str) -> Result<Self, BsmInspectionError> {
        let file = fs::File::open(&path).map_err(|source| BsmInspectionError::Io {
            path: path.clone(),
            source,
        })?;
        let mut lines = BufReader::new(file).lines();
        let header = lines
            .next()
            .transpose()
            .map_err(|source| BsmInspectionError::Io {
                path: path.clone(),
                source,
            })?
            .ok_or_else(|| invalid(&path, "empty table"))?;
        if header != expected_header {
            return Err(invalid(
                &path,
                "table header changed during event-chain scan",
            ));
        }
        Ok(Self {
            path,
            lines,
            line_number: 1,
            expected_fields: expected_header.split('\t').count(),
        })
    }

    fn next(&mut self) -> Result<Option<(usize, Vec<String>)>, BsmInspectionError> {
        let Some(line) = self.lines.next() else {
            return Ok(None);
        };
        self.line_number += 1;
        let line = line.map_err(|source| BsmInspectionError::Io {
            path: self.path.clone(),
            source,
        })?;
        if line.is_empty() {
            return Err(invalid(
                &self.path,
                format!("empty data row at line {}", self.line_number),
            ));
        }
        let fields = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        if fields.len() != self.expected_fields {
            return Err(invalid(
                &self.path,
                format!(
                    "line {} has {} fields, expected {}",
                    self.line_number,
                    fields.len(),
                    self.expected_fields
                ),
            ));
        }
        Ok(Some((self.line_number, fields)))
    }
}

#[derive(Clone, Copy, Debug)]
struct SegmentChainRow {
    key: (usize, usize, usize),
    q_index: usize,
    start_time: f64,
    end_time: f64,
    start_state: usize,
    end_state: usize,
    event_count: usize,
}

#[derive(Clone, Copy, Debug)]
struct EventChainRow {
    key: (usize, usize, usize),
    q_index: usize,
    time: f64,
    from_state: usize,
    to_state: usize,
}

fn parse_segment_chain_row(
    path: &Path,
    line: usize,
    fields: &[String],
    level: OutputLevel,
) -> Result<SegmentChainRow, BsmInspectionError> {
    let (segment, q, start_time, end_time, start_state, end_state, event_count) =
        if level == OutputLevel::Compact {
            (2, 3, 4, 5, 6, 7, 10)
        } else {
            (6, 7, 8, 9, 10, 13, 18)
        };
    Ok(SegmentChainRow {
        key: (
            row_usize(path, line, "sample", &fields[0])?,
            row_usize(path, line, "edge", &fields[1])?,
            row_usize(path, line, "segment", &fields[segment])?,
        ),
        q_index: row_usize(path, line, "q_index", &fields[q])?,
        start_time: row_f64(path, line, "start_time_from_parent", &fields[start_time])?,
        end_time: row_f64(path, line, "end_time_from_parent", &fields[end_time])?,
        start_state: row_usize(path, line, "start_state_index", &fields[start_state])?,
        end_state: row_usize(path, line, "end_state_index", &fields[end_state])?,
        event_count: row_usize(path, line, "event_count", &fields[event_count])?,
    })
}

fn parse_event_chain_row(
    path: &Path,
    line: usize,
    fields: &[String],
    level: OutputLevel,
) -> Result<EventChainRow, BsmInspectionError> {
    let (segment, q, time, from_state, to_state) = if level == OutputLevel::Compact {
        (2, 3, 4, 8, 9)
    } else {
        (6, 7, 8, 13, 16)
    };
    Ok(EventChainRow {
        key: (
            row_usize(path, line, "sample", &fields[0])?,
            row_usize(path, line, "edge", &fields[1])?,
            row_usize(path, line, "segment", &fields[segment])?,
        ),
        q_index: row_usize(path, line, "q_index", &fields[q])?,
        time: row_f64(path, line, "time_from_parent", &fields[time])?,
        from_state: row_usize(path, line, "from_state_index", &fields[from_state])?,
        to_state: row_usize(path, line, "to_state_index", &fields[to_state])?,
    })
}

fn validate_referenced_edge_end(
    path: &Path,
    previous: SegmentChainRow,
    references: &ReferenceData,
) -> Result<(), BsmInspectionError> {
    if references.edge_lengths.is_empty() {
        return Ok(());
    }
    let expected = references
        .edge_lengths
        .get(previous.key.1)
        .ok_or_else(|| invalid(path, "segment edge index is outside the reference table"))?;
    if !close_enough(previous.end_time, *expected) {
        return Err(invalid(
            path,
            format!(
                "edge {} for sample {} does not end at its referenced length",
                previous.key.1, previous.key.0
            ),
        ));
    }
    Ok(())
}

fn validate_segment_event_chains(
    metadata: &Metadata,
    references: &ReferenceData,
    bundles: &[TableBundle],
) -> Result<(), BsmInspectionError> {
    for bundle in bundles {
        let segment_path = bundle.directory.join(TABLE_FILES[2]);
        let event_path = bundle.directory.join(TABLE_FILES[7]);
        let mut segments = TsvRowReader::open(segment_path.clone(), metadata.format.header(2))?;
        let mut events = TsvRowReader::open(event_path.clone(), metadata.format.header(7))?;
        let mut next_event = events
            .next()?
            .map(|(line, fields)| {
                parse_event_chain_row(&event_path, line, &fields, metadata.format.level)
                    .map(|row| (line, row))
            })
            .transpose()?;
        let mut previous_segment: Option<SegmentChainRow> = None;
        let mut expected_sample = bundle.sample_start;
        let mut expected_edge = 0_usize;

        while let Some((line, fields)) = segments.next()? {
            let segment =
                parse_segment_chain_row(&segment_path, line, &fields, metadata.format.level)?;
            let (sample, edge, segment_index) = segment.key;
            if sample < bundle.sample_start || sample >= bundle.sample_end {
                return Err(invalid(
                    &segment_path,
                    format!("segment sample lies outside its table bundle at line {line}"),
                ));
            }
            let same_edge = previous_segment
                .is_some_and(|previous| (previous.key.0, previous.key.1) == (sample, edge));
            if same_edge {
                let previous = previous_segment.expect("same_edge requires a previous segment");
                let expected_segment = previous
                    .key
                    .2
                    .checked_add(1)
                    .ok_or_else(|| invalid(&segment_path, "segment index overflow"))?;
                if segment_index != expected_segment
                    || !close_enough(segment.start_time, previous.end_time)
                    || segment.start_state != previous.end_state
                {
                    return Err(invalid(
                        &segment_path,
                        format!("non-contiguous segment chain at line {line}"),
                    ));
                }
            } else {
                if let Some(previous) = previous_segment {
                    if segment.key <= previous.key {
                        return Err(invalid(
                            &segment_path,
                            format!("segment keys are duplicated or unsorted at line {line}"),
                        ));
                    }
                    validate_referenced_edge_end(&segment_path, previous, references)?;
                }
                if segment_index != 0 || !close_enough(segment.start_time, 0.0) {
                    return Err(invalid(
                        &segment_path,
                        format!("first segment of an edge is invalid at line {line}"),
                    ));
                }
                if references.edges > 0 {
                    if (sample, edge) != (expected_sample, expected_edge) {
                        return Err(invalid(
                            &segment_path,
                            format!("edge coverage is incomplete or unsorted at line {line}"),
                        ));
                    }
                    expected_edge = expected_edge
                        .checked_add(1)
                        .ok_or_else(|| invalid(&segment_path, "edge index overflow"))?;
                    if expected_edge == references.edges {
                        expected_edge = 0;
                        expected_sample = expected_sample
                            .checked_add(1)
                            .ok_or_else(|| invalid(&segment_path, "sample index overflow"))?;
                    }
                }
            }

            let mut current_state = segment.start_state;
            let mut consumed = 0_usize;
            let mut previous_time = None;
            while next_event.is_some_and(|(_, event)| event.key == segment.key) {
                let (event_line, event) = next_event.take().expect("matching event should exist");
                if event.q_index != segment.q_index
                    || event.time < segment.start_time - 1e-10
                    || event.time > segment.end_time + 1e-10
                    || previous_time.is_some_and(|time| event.time < time)
                    || event.from_state != current_state
                {
                    return Err(invalid(
                        &event_path,
                        format!("event does not follow its segment chain at line {event_line}"),
                    ));
                }
                current_state = event.to_state;
                previous_time = Some(event.time);
                consumed = consumed
                    .checked_add(1)
                    .ok_or_else(|| invalid(&event_path, "segment event count overflow"))?;
                next_event = events
                    .next()?
                    .map(|(line, fields)| {
                        parse_event_chain_row(&event_path, line, &fields, metadata.format.level)
                            .map(|row| (line, row))
                    })
                    .transpose()?;
            }
            if next_event.is_some_and(|(_, event)| event.key < segment.key) {
                return Err(invalid(
                    &event_path,
                    "event keys are duplicated, unsorted, or reference a missing segment",
                ));
            }
            if consumed != segment.event_count || current_state != segment.end_state {
                return Err(invalid(
                    &segment_path,
                    format!("segment event chain disagrees with its endpoint at line {line}"),
                ));
            }
            previous_segment = Some(segment);
        }

        if let Some(previous) = previous_segment {
            validate_referenced_edge_end(&segment_path, previous, references)?;
        }
        if references.edges > 0 && (expected_sample != bundle.sample_end || expected_edge != 0) {
            return Err(invalid(
                &segment_path,
                "branch segments do not cover every referenced edge in the sample bundle",
            ));
        }
        if let Some((line, _)) = next_event {
            return Err(invalid(
                &event_path,
                format!("event at line {line} has no matching branch segment"),
            ));
        }
    }
    Ok(())
}

fn validate_path_tables(
    metadata: &Metadata,
    references: &ReferenceData,
    bundles: &[TableBundle],
    anagenetic_counts: &[usize],
    expected_segment_counts: &[usize],
) -> Result<u64, BsmInspectionError> {
    let mut rows = 0_u64;
    let mut node_counts = vec![0_usize; metadata.completed_samples];
    let mut previous_node_key = None;
    rows += for_each_row(
        bundles,
        0,
        metadata.format.header(0),
        |path, line, fields| {
            let sample = row_usize(path, line, "sample", fields[0])?;
            let node = row_usize(path, line, "node", fields[1])?;
            let state_column = if metadata.format.level == OutputLevel::Compact {
                2
            } else {
                5
            };
            let state = row_usize(path, line, "state_index", fields[state_column])?;
            if sample >= metadata.completed_samples
                || state >= metadata.states
                || (references.nodes > 0 && node >= references.nodes)
            {
                return Err(invalid(
                    path,
                    format!("node-state reference out of range at line {line}"),
                ));
            }
            if previous_node_key.is_some_and(|key| (sample, node) <= key) {
                return Err(invalid(
                    path,
                    format!("node-state keys are duplicated or unsorted at line {line}"),
                ));
            }
            previous_node_key = Some((sample, node));
            node_counts[sample] += 1;
            Ok(())
        },
    )?;
    if references.nodes > 0 && node_counts.iter().any(|count| *count != references.nodes) {
        return Err(invalid(
            &bundles[0].directory,
            "node_states does not contain exactly one row per sample and referenced node",
        ));
    }

    let mut previous_split_key = None;
    rows += for_each_row(
        bundles,
        1,
        metadata.format.header(1),
        |path, line, fields| {
            let sample = row_usize(path, line, "sample", fields[0])?;
            let node = row_usize(path, line, "node", fields[1])?;
            let (state_columns, weight_column) = if metadata.format.level == OutputLevel::Compact {
                ([2, 3, 4], 5)
            } else {
                ([7, 10, 13], 16)
            };
            if sample >= metadata.completed_samples
                || (references.nodes > 0 && node >= references.nodes)
                || state_columns.iter().any(|column| {
                    fields[*column]
                        .parse::<usize>()
                        .map_or(true, |state| state >= metadata.states)
                })
                || row_f64(path, line, "scenario_weight", fields[weight_column])? <= 0.0
            {
                return Err(invalid(
                    path,
                    format!("invalid cladogenetic split at line {line}"),
                ));
            }
            if previous_split_key.is_some_and(|key| (sample, node) <= key) {
                return Err(invalid(
                    path,
                    format!("split keys are duplicated or unsorted at line {line}"),
                ));
            }
            previous_split_key = Some((sample, node));
            Ok(())
        },
    )?;

    let mut segment_counts = vec![0_usize; metadata.completed_samples];
    let mut segment_events = vec![0_usize; metadata.completed_samples];
    rows += for_each_row(
        bundles,
        2,
        metadata.format.header(2),
        |path, line, fields| {
            let sample = row_usize(path, line, "sample", fields[0])?;
            let edge = row_usize(path, line, "edge", fields[1])?;
            let (
                q_column,
                start_time_column,
                end_time_column,
                start_state_column,
                end_state_column,
                probability_column,
                event_count_column,
            ) = if metadata.format.level == OutputLevel::Compact {
                (3, 4, 5, 6, 7, 8, 10)
            } else {
                (7, 8, 9, 10, 13, 16, 18)
            };
            let q_index = row_usize(path, line, "q_index", fields[q_column])?;
            let start_time = row_f64(
                path,
                line,
                "start_time_from_parent",
                fields[start_time_column],
            )?;
            let end_time = row_f64(path, line, "end_time_from_parent", fields[end_time_column])?;
            let start_state =
                row_usize(path, line, "start_state_index", fields[start_state_column])?;
            let end_state = row_usize(path, line, "end_state_index", fields[end_state_column])?;
            let probability = row_f64(
                path,
                line,
                "endpoint_probability",
                fields[probability_column],
            )?;
            let event_count = row_usize(path, line, "event_count", fields[event_count_column])?;
            if sample >= metadata.completed_samples
                || (references.edges > 0 && edge >= references.edges)
                || (references.periods > 0 && q_index >= references.periods)
                || start_time < 0.0
                || end_time < start_time
                || start_state >= metadata.states
                || end_state >= metadata.states
                || probability <= 0.0
                || probability > 1.0 + 1e-10
            {
                return Err(invalid(
                    path,
                    format!("invalid branch segment at line {line}"),
                ));
            }
            segment_counts[sample] += 1;
            segment_events[sample] = segment_events[sample]
                .checked_add(event_count)
                .ok_or_else(|| invalid(path, "segment event count overflow"))?;
            Ok(())
        },
    )?;
    if segment_events != anagenetic_counts
        || (metadata.format.is_v2 && segment_counts != expected_segment_counts)
    {
        return Err(invalid(
            &bundles[0].directory,
            "branch segment counts do not match sample summaries",
        ));
    }

    let mut event_counts = vec![0_usize; metadata.completed_samples];
    rows += for_each_row(
        bundles,
        7,
        metadata.format.header(7),
        |path, line, fields| {
            let sample = row_usize(path, line, "sample", fields[0])?;
            let edge = row_usize(path, line, "edge", fields[1])?;
            let (
                q_column,
                time_column,
                kind_column,
                parameter_column,
                area_column,
                from_column,
                to_column,
            ) = if metadata.format.level == OutputLevel::Compact {
                (3, 4, 5, 6, 7, 8, 9)
            } else {
                (7, 8, 9, 10, 11, 13, 16)
            };
            let q_index = row_usize(path, line, "q_index", fields[q_column])?;
            let time = row_f64(path, line, "time_from_parent", fields[time_column])?;
            let area = row_usize(path, line, "area_index", fields[area_column])?;
            let from = row_usize(path, line, "from_state_index", fields[from_column])?;
            let to = row_usize(path, line, "to_state_index", fields[to_column])?;
            let expected_parameter = match fields[kind_column] {
                "range_expansion" => "d",
                "local_extirpation" => "e",
                "range_switching" => "a",
                other => {
                    return Err(invalid(
                        path,
                        format!("unknown event kind {other:?} at line {line}"),
                    ));
                }
            };
            if fields[parameter_column] != expected_parameter
                || sample >= metadata.completed_samples
                || (references.edges > 0 && edge >= references.edges)
                || (references.periods > 0 && q_index >= references.periods)
                || time < 0.0
                || area >= metadata.areas
                || from >= metadata.states
                || to >= metadata.states
            {
                return Err(invalid(
                    path,
                    format!("invalid anagenetic event at line {line}"),
                ));
            }
            if !references.state_bits.is_empty() {
                let from_bits = references.state_bits[from];
                let to_bits = references.state_bits[to];
                let area_bit = 1_u64.checked_shl(area as u32).ok_or_else(|| {
                    invalid(path, format!("area bit overflows u64 at line {line}"))
                })?;
                let valid_transition = match fields[kind_column] {
                    "range_expansion" => {
                        from_bits & area_bit == 0 && to_bits == from_bits | area_bit
                    }
                    "local_extirpation" => {
                        from_bits & area_bit != 0 && to_bits == from_bits & !area_bit
                    }
                    "range_switching" => {
                        from_bits.count_ones() == 1
                            && to_bits.count_ones() == 1
                            && from_bits != to_bits
                            && to_bits == area_bit
                    }
                    _ => unreachable!(),
                };
                if !valid_transition {
                    return Err(invalid(
                        path,
                        format!("event transition semantics fail at line {line}"),
                    ));
                }
            }
            event_counts[sample] += 1;
            Ok(())
        },
    )?;
    if event_counts != anagenetic_counts {
        return Err(invalid(
            &bundles[0].directory,
            "anagenetic event rows do not match sample summaries",
        ));
    }
    validate_segment_event_chains(metadata, references, bundles)?;
    Ok(rows)
}

pub fn inspect(root: &Path, deep: bool) -> Result<InspectionReport, BsmInspectionError> {
    let metadata = load_metadata(root)?;
    validate_root_entries(root, metadata.format)?;
    let references = load_references(root, &metadata)?;
    let (bundles, completed_shards) = load_bundles(root, &metadata)?;
    if bundles.is_empty() && metadata.completed_samples != 0 {
        return Err(invalid(root, "no table bundle covers committed samples"));
    }
    let files_checked = 1
        + usize::from(metadata.format.is_v2) * REFERENCE_FILES.len()
        + bundles.len() * (TABLE_FILES.len() + 1)
        + usize::from(metadata.format.layout == Layout::Sharded);
    let deep_result = if deep && metadata.completed_samples == 0 {
        Some(DeepResult {
            data_rows: 0,
            periods: references.periods,
            diagnostic_violations: 0,
        })
    } else {
        deep.then(|| validate_deep(&metadata, &references, &bundles))
            .transpose()?
    };
    let periods = deep_result
        .as_ref()
        .map(|result| result.periods)
        .or((references.periods > 0).then_some(references.periods));
    Ok(InspectionReport {
        bsm_format: metadata.format.format.to_string(),
        output_level: metadata.format.level.as_str().to_string(),
        layout: metadata.format.layout.as_str().to_string(),
        run_status: metadata.run_status,
        completed_samples: metadata.completed_samples,
        requested_samples: metadata.requested_samples,
        completed_anagenetic_events: metadata.completed_events,
        shards: completed_shards,
        states: metadata.states,
        areas: metadata.areas,
        nodes: (references.nodes > 0).then_some(references.nodes),
        edges: (references.edges > 0).then_some(references.edges),
        periods,
        path_details: metadata.format.path_details,
        sparse_occupancy: metadata.format.sparse_occupancy,
        deep,
        files_checked,
        data_rows_checked: deep_result.as_ref().map(|result| result.data_rows),
        event_count_validation: if deep { "passed" } else { "not_requested" },
        occupancy_validation: if deep { "passed" } else { "not_requested" },
        path_validation: if !deep {
            "not_requested"
        } else if metadata.format.path_details {
            "passed"
        } else {
            "not_applicable"
        },
        state_constraint_validation: if !deep {
            "not_requested"
        } else if metadata.format.is_v2 {
            "passed"
        } else {
            "not_available"
        },
        diagnostic_violations: deep_result
            .as_ref()
            .filter(|_| metadata.format.is_v2)
            .map(|result| result.diagnostic_violations),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_all_published_bsm_directory_formats() {
        for format in [
            "biogeo-bsm-tsv-v1",
            "biogeo-bsm-sharded-tsv-v1",
            "biogeo-bsm-full-tsv-v2",
            "biogeo-bsm-full-sharded-tsv-v2",
            "biogeo-bsm-compact-tsv-v2",
            "biogeo-bsm-compact-sharded-tsv-v2",
            "biogeo-bsm-summary-tsv-v2",
            "biogeo-bsm-summary-sharded-tsv-v2",
        ] {
            assert_eq!(FormatInfo::parse(format).unwrap().format, format);
        }
        assert!(FormatInfo::parse("future-bsm-v99").is_none());
    }
}
