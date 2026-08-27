use crate::analysis_result::{decode_field, encode_field, stable_fingerprint};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

pub const INPUT_BUNDLE_FORMAT_VERSION: &str = "biogeo-input-bundle-v1";
pub const METADATA_FILE: &str = "metadata.tsv";
pub const FILES_FILE: &str = "files.tsv";

const INPUT_KIND: &str = "input";
const DEPENDENCY_KIND: &str = "dependency";
const PROVENANCE_KIND: &str = "provenance";

#[derive(Clone, Copy, Debug)]
pub struct InputBundleSpec<'a> {
    pub role: &'a str,
    pub path: &'a Path,
    pub required_for_replay: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InputBundleFileRecord {
    pub id: String,
    pub kind: String,
    pub role: String,
    pub parent_role: Option<String>,
    pub relative_path: PathBuf,
    pub path: PathBuf,
    pub required_for_replay: bool,
    pub bytes: u64,
    pub fingerprint: String,
    pub source_bytes: u64,
    pub source_fingerprint: String,
}

#[derive(Clone, Debug)]
pub struct LoadedInputBundle {
    pub root: PathBuf,
    pub fingerprint: String,
    pub files: BTreeMap<String, InputBundleFileRecord>,
    pub top_level_inputs: BTreeMap<String, InputBundleFileRecord>,
}

impl LoadedInputBundle {
    pub fn dependency_count(&self) -> usize {
        self.files
            .values()
            .filter(|record| record.kind == DEPENDENCY_KIND)
            .count()
    }

    pub fn provenance_count(&self) -> usize {
        self.files
            .values()
            .filter(|record| record.kind == PROVENANCE_KIND)
            .count()
    }
}

#[derive(Clone, Debug)]
struct ManifestRecord {
    id: String,
    kind: String,
    role: String,
    parent_role: Option<String>,
    relative_path: PathBuf,
    required_for_replay: bool,
    bytes: u64,
    fingerprint: String,
    source_bytes: u64,
    source_fingerprint: String,
}

struct BundleBuilder<'a> {
    root: &'a Path,
    records: Vec<ManifestRecord>,
}

struct FileWrite<'a> {
    id: String,
    kind: &'static str,
    role: String,
    parent_role: Option<String>,
    relative_path: PathBuf,
    required_for_replay: bool,
    source_bytes: &'a [u8],
    bundled_bytes: &'a [u8],
}

struct DependencyWrite<'a> {
    source_base: &'a Path,
    raw_path: &'a str,
    input_index: usize,
    row_index: usize,
    field: &'static str,
    parent_role: &'a str,
    required_for_replay: bool,
}

impl<'a> BundleBuilder<'a> {
    fn add_file(&mut self, file: FileWrite<'_>) -> Result<(), InputBundleError> {
        let path = self.root.join(&file.relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| InputBundleError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        fs::write(&path, file.bundled_bytes).map_err(|source| InputBundleError::Io {
            path: path.clone(),
            source,
        })?;
        self.records.push(ManifestRecord {
            id: file.id,
            kind: file.kind.to_string(),
            role: file.role,
            parent_role: file.parent_role,
            relative_path: file.relative_path,
            required_for_replay: file.required_for_replay,
            bytes: file.bundled_bytes.len() as u64,
            fingerprint: stable_fingerprint(file.bundled_bytes),
            source_bytes: file.source_bytes.len() as u64,
            source_fingerprint: stable_fingerprint(file.source_bytes),
        });
        Ok(())
    }

    fn add_dependency(
        &mut self,
        dependency: DependencyWrite<'_>,
    ) -> Result<String, InputBundleError> {
        let candidate = PathBuf::from(dependency.raw_path);
        let source_path = if candidate.is_absolute() {
            candidate
        } else {
            dependency.source_base.join(candidate)
        };
        let canonical_path =
            fs::canonicalize(&source_path).map_err(|source| InputBundleError::Io {
                path: source_path.clone(),
                source,
            })?;
        let bytes = fs::read(&canonical_path).map_err(|source| InputBundleError::Io {
            path: canonical_path.clone(),
            source,
        })?;
        let extension = portable_extension(&canonical_path);
        let file_name = format!(
            "{:03}-{:03}-{}{extension}",
            dependency.input_index,
            dependency.row_index,
            sanitize_component(dependency.field)
        );
        let relative_path = PathBuf::from("files").join("dependencies").join(&file_name);
        self.add_file(FileWrite {
            id: format!(
                "dependency:{}:{}:{}",
                dependency.parent_role, dependency.row_index, dependency.field
            ),
            kind: DEPENDENCY_KIND,
            role: dependency.field.to_string(),
            parent_role: Some(dependency.parent_role.to_string()),
            relative_path,
            required_for_replay: dependency.required_for_replay,
            source_bytes: &bytes,
            bundled_bytes: &bytes,
        })?;
        Ok(format!("../dependencies/{file_name}"))
    }
}

pub fn write_input_bundle(
    root: &Path,
    specs: &[InputBundleSpec<'_>],
) -> Result<LoadedInputBundle, InputBundleError> {
    if root.exists() {
        return Err(InputBundleError::OutputExists(root.to_path_buf()));
    }
    fs::create_dir_all(root).map_err(|source| InputBundleError::Io {
        path: root.to_path_buf(),
        source,
    })?;

    let mut sorted: Vec<InputBundleSpec<'_>> = specs.to_vec();
    sorted.sort_by(|left, right| left.role.cmp(right.role));
    for pair in sorted.windows(2) {
        if pair[0].role == pair[1].role {
            return Err(InputBundleError::DuplicateInputRole(
                pair[0].role.to_string(),
            ));
        }
    }

    let mut builder = BundleBuilder {
        root,
        records: Vec::new(),
    };
    for (input_index, spec) in sorted.iter().enumerate() {
        let canonical_path =
            fs::canonicalize(spec.path).map_err(|source| InputBundleError::Io {
                path: spec.path.to_path_buf(),
                source,
            })?;
        let source_bytes = fs::read(&canonical_path).map_err(|source| InputBundleError::Io {
            path: canonical_path.clone(),
            source,
        })?;
        let extension = portable_extension(&canonical_path);
        let file_name = format!(
            "{input_index:03}-{}{extension}",
            sanitize_component(spec.role)
        );
        let relative_path = PathBuf::from("files").join("inputs").join(&file_name);
        let bundled_bytes = if spec.role == "dispersal_strata" {
            let rewritten = rewrite_strata_table(
                &mut builder,
                &canonical_path,
                &source_bytes,
                input_index,
                spec.role,
                spec.required_for_replay,
            )?;
            let provenance_name = format!(
                "{input_index:03}-{}.original{extension}",
                sanitize_component(spec.role)
            );
            builder.add_file(FileWrite {
                id: format!("provenance:{}", spec.role),
                kind: PROVENANCE_KIND,
                role: format!("{}_source", spec.role),
                parent_role: Some(spec.role.to_string()),
                relative_path: PathBuf::from("files")
                    .join("provenance")
                    .join(provenance_name),
                required_for_replay: false,
                source_bytes: &source_bytes,
                bundled_bytes: &source_bytes,
            })?;
            rewritten.into_bytes()
        } else {
            source_bytes.clone()
        };
        builder.add_file(FileWrite {
            id: format!("input:{}", spec.role),
            kind: INPUT_KIND,
            role: spec.role.to_string(),
            parent_role: None,
            relative_path,
            required_for_replay: spec.required_for_replay,
            source_bytes: &source_bytes,
            bundled_bytes: &bundled_bytes,
        })?;
    }

    builder
        .records
        .sort_by(|left, right| left.id.cmp(&right.id));
    let files_text = format_manifest(&builder.records)?;
    write_file(&root.join(FILES_FILE), files_text.as_bytes())?;
    let dependency_count = builder
        .records
        .iter()
        .filter(|record| record.kind == DEPENDENCY_KIND)
        .count();
    let provenance_count = builder
        .records
        .iter()
        .filter(|record| record.kind == PROVENANCE_KIND)
        .count();
    let metadata = format!(
        "key\tvalue\n\
format\t{}\n\
status\tcomplete\n\
path_mode\trelative\n\
files_file\t{}\n\
input_count\t{}\n\
dependency_count\t{}\n\
provenance_count\t{}\n\
file_count\t{}\n",
        INPUT_BUNDLE_FORMAT_VERSION,
        FILES_FILE,
        sorted.len(),
        dependency_count,
        provenance_count,
        builder.records.len(),
    );
    write_file(&root.join(METADATA_FILE), metadata.as_bytes())?;
    load_input_bundle(root)
}

fn rewrite_strata_table(
    builder: &mut BundleBuilder<'_>,
    source_path: &Path,
    source_bytes: &[u8],
    input_index: usize,
    parent_role: &str,
    required_for_replay: bool,
) -> Result<String, InputBundleError> {
    let input = std::str::from_utf8(source_bytes)
        .map_err(|_| InputBundleError::NonUtf8File(source_path.to_path_buf()))?;
    let source_base = source_path.parent().unwrap_or_else(|| Path::new("."));
    let header_fields = first_data_fields(input)
        .ok_or_else(|| InputBundleError::InvalidStrataHeader(source_path.to_path_buf()))?;
    if header_fields.len() == 2 {
        let specs = biogeo_core::parse_dispersal_strata_table(input)?;
        let mut output = String::from("oldest_age\tmatrix\n");
        for (row_index, spec) in specs.iter().enumerate() {
            let matrix = builder.add_dependency(DependencyWrite {
                source_base,
                raw_path: &spec.matrix_path,
                input_index,
                row_index,
                field: "matrix",
                parent_role,
                required_for_replay,
            })?;
            output.push_str(&format!("{}\t{matrix}\n", spec.oldest_age));
        }
        return Ok(output);
    }

    let constrained = header_fields.len() >= 7;
    let explicit_ranges = header_fields.len() == 8;
    let specs = biogeo_core::parse_anagenetic_strata_table(input)?;
    let mut output = if explicit_ranges {
        String::from(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\tallowed_ranges\n",
        )
    } else if constrained {
        String::from(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\n",
        )
    } else {
        String::from(
            "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\n",
        )
    };
    for (row_index, spec) in specs.iter().enumerate() {
        let matrix = bundle_optional_dependency(
            builder,
            source_base,
            spec.dispersal_matrix_path.as_deref(),
            input_index,
            row_index,
            "matrix",
            parent_role,
            required_for_replay,
        )?;
        let distance = bundle_optional_dependency(
            builder,
            source_base,
            spec.distance_matrix_path.as_deref(),
            input_index,
            row_index,
            "distance_matrix",
            parent_role,
            required_for_replay,
        )?;
        let environment = bundle_optional_dependency(
            builder,
            source_base,
            spec.environment_distance_matrix_path.as_deref(),
            input_index,
            row_index,
            "environment_distance_matrix",
            parent_role,
            required_for_replay,
        )?;
        let area_sizes = bundle_optional_dependency(
            builder,
            source_base,
            spec.area_sizes_path.as_deref(),
            input_index,
            row_index,
            "area_sizes",
            parent_role,
            required_for_replay,
        )?;
        output.push_str(&format!(
            "{}\t{matrix}\t{distance}\t{environment}\t{area_sizes}",
            spec.oldest_age
        ));
        if constrained {
            let areas_allowed = bundle_optional_dependency(
                builder,
                source_base,
                spec.areas_allowed_path.as_deref(),
                input_index,
                row_index,
                "areas_allowed",
                parent_role,
                required_for_replay,
            )?;
            let areas_adjacency = bundle_optional_dependency(
                builder,
                source_base,
                spec.areas_adjacency_path.as_deref(),
                input_index,
                row_index,
                "areas_adjacency",
                parent_role,
                required_for_replay,
            )?;
            output.push_str(&format!("\t{areas_allowed}\t{areas_adjacency}"));
            if explicit_ranges {
                let allowed_ranges = bundle_optional_dependency(
                    builder,
                    source_base,
                    spec.allowed_ranges_path.as_deref(),
                    input_index,
                    row_index,
                    "allowed_ranges",
                    parent_role,
                    required_for_replay,
                )?;
                output.push_str(&format!("\t{allowed_ranges}"));
            }
        }
        output.push('\n');
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn bundle_optional_dependency(
    builder: &mut BundleBuilder<'_>,
    source_base: &Path,
    raw_path: Option<&str>,
    input_index: usize,
    row_index: usize,
    field: &'static str,
    parent_role: &str,
    required_for_replay: bool,
) -> Result<String, InputBundleError> {
    raw_path
        .map(|raw_path| {
            builder.add_dependency(DependencyWrite {
                source_base,
                raw_path,
                input_index,
                row_index,
                field,
                parent_role,
                required_for_replay,
            })
        })
        .transpose()
        .map(|path| path.unwrap_or_else(|| "-".to_string()))
}

pub fn load_input_bundle(root: &Path) -> Result<LoadedInputBundle, InputBundleError> {
    let root = fs::canonicalize(root).map_err(|source| InputBundleError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let metadata_path = root.join(METADATA_FILE);
    let metadata_bytes = fs::read(&metadata_path).map_err(|source| InputBundleError::Io {
        path: metadata_path.clone(),
        source,
    })?;
    let metadata_text = std::str::from_utf8(&metadata_bytes)
        .map_err(|_| invalid_metadata(&metadata_path, "metadata is not UTF-8"))?;
    let metadata = parse_key_value_table(&metadata_path, metadata_text)?;
    require_value(
        &metadata,
        "format",
        &metadata_path,
        INPUT_BUNDLE_FORMAT_VERSION,
    )?;
    require_value(&metadata, "status", &metadata_path, "complete")?;
    require_value(&metadata, "path_mode", &metadata_path, "relative")?;
    require_value(&metadata, "files_file", &metadata_path, FILES_FILE)?;

    let files_path = root.join(FILES_FILE);
    let files_bytes = fs::read(&files_path).map_err(|source| InputBundleError::Io {
        path: files_path.clone(),
        source,
    })?;
    let files_text = std::str::from_utf8(&files_bytes)
        .map_err(|_| invalid_manifest(&files_path, "files table is not UTF-8"))?;
    let manifest_records = parse_manifest(&files_path, files_text)?;
    let expected_file_count = parse_usize(&metadata, "file_count", &metadata_path)?;
    if manifest_records.len() != expected_file_count {
        return Err(invalid_metadata(
            &metadata_path,
            "file_count does not match files.tsv",
        ));
    }

    let mut files = BTreeMap::new();
    let mut paths = BTreeMap::new();
    let mut top_level_inputs = BTreeMap::new();
    for record in manifest_records {
        let raw_path = portable_path_string(&record.relative_path)?;
        let relative_path = parse_portable_relative_path(&raw_path)?;
        let candidate = root.join(&relative_path);
        let canonical_path =
            fs::canonicalize(&candidate).map_err(|source| InputBundleError::Io {
                path: candidate.clone(),
                source,
            })?;
        if !canonical_path.starts_with(&root) {
            return Err(InputBundleError::PathEscapesBundle(candidate));
        }
        let bytes = fs::read(&canonical_path).map_err(|source| InputBundleError::Io {
            path: canonical_path.clone(),
            source,
        })?;
        let actual_fingerprint = stable_fingerprint(&bytes);
        if bytes.len() as u64 != record.bytes || actual_fingerprint != record.fingerprint {
            return Err(InputBundleError::FileChanged {
                id: record.id,
                path: canonical_path,
                expected_bytes: record.bytes,
                actual_bytes: bytes.len() as u64,
                expected_fingerprint: record.fingerprint,
                actual_fingerprint,
            });
        }
        if paths
            .insert(canonical_path.clone(), record.id.clone())
            .is_some()
        {
            return Err(invalid_manifest(
                &files_path,
                "multiple manifest records resolve to the same path",
            ));
        }
        let loaded = InputBundleFileRecord {
            id: record.id.clone(),
            kind: record.kind,
            role: record.role,
            parent_role: record.parent_role,
            relative_path,
            path: canonical_path,
            required_for_replay: record.required_for_replay,
            bytes: record.bytes,
            fingerprint: record.fingerprint,
            source_bytes: record.source_bytes,
            source_fingerprint: record.source_fingerprint,
        };
        if loaded.kind == INPUT_KIND {
            if loaded.parent_role.is_some() {
                return Err(invalid_manifest(
                    &files_path,
                    "top-level input records cannot have parent_role",
                ));
            }
            if top_level_inputs
                .insert(loaded.role.clone(), loaded.clone())
                .is_some()
            {
                return Err(invalid_manifest(
                    &files_path,
                    "duplicate top-level input role",
                ));
            }
        } else if loaded.parent_role.is_none() {
            return Err(invalid_manifest(
                &files_path,
                "dependency and provenance records require parent_role",
            ));
        }
        files.insert(loaded.id.clone(), loaded);
    }

    validate_manifest_counts(&metadata, &metadata_path, &files, &top_level_inputs)?;
    validate_parent_roles(&files_path, &files, &top_level_inputs)?;
    validate_strata_dependencies(&root, &files_path, &files, &top_level_inputs)?;
    let fingerprint = bundle_fingerprint(&metadata_bytes, &files_bytes, files.values())?;
    Ok(LoadedInputBundle {
        root,
        fingerprint,
        files,
        top_level_inputs,
    })
}

fn validate_manifest_counts(
    metadata: &BTreeMap<String, String>,
    metadata_path: &Path,
    files: &BTreeMap<String, InputBundleFileRecord>,
    top_level_inputs: &BTreeMap<String, InputBundleFileRecord>,
) -> Result<(), InputBundleError> {
    let expected_inputs = parse_usize(metadata, "input_count", metadata_path)?;
    let expected_dependencies = parse_usize(metadata, "dependency_count", metadata_path)?;
    let expected_provenance = parse_usize(metadata, "provenance_count", metadata_path)?;
    let actual_dependencies = files
        .values()
        .filter(|record| record.kind == DEPENDENCY_KIND)
        .count();
    let actual_provenance = files
        .values()
        .filter(|record| record.kind == PROVENANCE_KIND)
        .count();
    if expected_inputs != top_level_inputs.len()
        || expected_dependencies != actual_dependencies
        || expected_provenance != actual_provenance
    {
        return Err(invalid_metadata(
            metadata_path,
            "input/dependency/provenance counts do not match files.tsv",
        ));
    }
    Ok(())
}

fn validate_parent_roles(
    files_path: &Path,
    files: &BTreeMap<String, InputBundleFileRecord>,
    top_level_inputs: &BTreeMap<String, InputBundleFileRecord>,
) -> Result<(), InputBundleError> {
    for record in files.values().filter(|record| record.kind != INPUT_KIND) {
        let parent = record
            .parent_role
            .as_deref()
            .expect("non-input records were checked above");
        if !top_level_inputs.contains_key(parent) {
            return Err(invalid_manifest(
                files_path,
                format!("unknown parent_role {parent:?}"),
            ));
        }
    }
    Ok(())
}

fn validate_strata_dependencies(
    root: &Path,
    files_path: &Path,
    files: &BTreeMap<String, InputBundleFileRecord>,
    top_level_inputs: &BTreeMap<String, InputBundleFileRecord>,
) -> Result<(), InputBundleError> {
    let dependency_paths: BTreeSet<PathBuf> = files
        .values()
        .filter(|record| record.kind == DEPENDENCY_KIND)
        .map(|record| record.path.clone())
        .collect();
    let Some(strata) = top_level_inputs.get("dispersal_strata") else {
        if dependency_paths.is_empty() {
            return Ok(());
        }
        return Err(invalid_manifest(
            files_path,
            "dependency files exist without a dispersal_strata input",
        ));
    };
    let bytes = fs::read(&strata.path).map_err(|source| InputBundleError::Io {
        path: strata.path.clone(),
        source,
    })?;
    let input = std::str::from_utf8(&bytes)
        .map_err(|_| InputBundleError::NonUtf8File(strata.path.clone()))?;
    let mut raw_paths = Vec::new();
    let header_fields = first_data_fields(input)
        .ok_or_else(|| InputBundleError::InvalidStrataHeader(strata.path.clone()))?;
    if header_fields.len() == 2 {
        for spec in biogeo_core::parse_dispersal_strata_table(input)? {
            raw_paths.push(spec.matrix_path);
        }
    } else {
        for spec in biogeo_core::parse_anagenetic_strata_table(input)? {
            raw_paths.extend(
                [
                    spec.dispersal_matrix_path,
                    spec.distance_matrix_path,
                    spec.environment_distance_matrix_path,
                    spec.area_sizes_path,
                    spec.areas_allowed_path,
                    spec.areas_adjacency_path,
                    spec.allowed_ranges_path,
                ]
                .into_iter()
                .flatten(),
            );
        }
    }

    let base = strata.path.parent().unwrap_or(root);
    let mut referenced = BTreeSet::new();
    for raw_path in raw_paths {
        let candidate = base.join(raw_path);
        let canonical_path =
            fs::canonicalize(&candidate).map_err(|source| InputBundleError::Io {
                path: candidate.clone(),
                source,
            })?;
        if !canonical_path.starts_with(root) {
            return Err(InputBundleError::PathEscapesBundle(candidate));
        }
        if !dependency_paths.contains(&canonical_path) {
            return Err(invalid_manifest(
                files_path,
                format!(
                    "strata dependency {} is not declared as a dependency file",
                    canonical_path.display()
                ),
            ));
        }
        referenced.insert(canonical_path);
    }
    if referenced != dependency_paths {
        return Err(invalid_manifest(
            files_path,
            "files.tsv contains unreferenced strata dependencies",
        ));
    }
    Ok(())
}

fn bundle_fingerprint<'a>(
    metadata: &[u8],
    files_manifest: &[u8],
    records: impl Iterator<Item = &'a InputBundleFileRecord>,
) -> Result<String, InputBundleError> {
    let mut hash = Fnv64::new();
    hash.update(metadata);
    hash.update(files_manifest);
    for record in records {
        hash.update(record.id.as_bytes());
        let bytes = fs::read(&record.path).map_err(|source| InputBundleError::Io {
            path: record.path.clone(),
            source,
        })?;
        hash.update(&bytes);
    }
    Ok(hash.finish())
}

struct Fnv64(u64);

impl Fnv64 {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(self) -> String {
        format!("{:016x}", self.0)
    }
}

fn format_manifest(records: &[ManifestRecord]) -> Result<String, InputBundleError> {
    let mut output = String::from(
        "id\tkind\trole\tparent_role\tpath\trequired_for_replay\tbytes\tfingerprint\tsource_bytes\tsource_fingerprint\n",
    );
    for record in records {
        output.push_str(&encode_field(&record.id));
        output.push('\t');
        output.push_str(&record.kind);
        output.push('\t');
        output.push_str(&encode_field(&record.role));
        output.push('\t');
        output.push_str(
            &record
                .parent_role
                .as_deref()
                .map(encode_field)
                .unwrap_or_default(),
        );
        output.push('\t');
        output.push_str(&encode_field(&portable_path_string(&record.relative_path)?));
        output.push('\t');
        output.push_str(if record.required_for_replay {
            "true"
        } else {
            "false"
        });
        output.push('\t');
        output.push_str(&record.bytes.to_string());
        output.push('\t');
        output.push_str(&record.fingerprint);
        output.push('\t');
        output.push_str(&record.source_bytes.to_string());
        output.push('\t');
        output.push_str(&record.source_fingerprint);
        output.push('\n');
    }
    Ok(output)
}

fn parse_manifest(path: &Path, input: &str) -> Result<Vec<ManifestRecord>, InputBundleError> {
    let mut lines = input.lines();
    if lines.next()
        != Some(
            "id\tkind\trole\tparent_role\tpath\trequired_for_replay\tbytes\tfingerprint\tsource_bytes\tsource_fingerprint",
        )
    {
        return Err(invalid_manifest(path, "unexpected files header"));
    }
    let mut records = Vec::new();
    let mut ids = BTreeSet::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        if fields.len() != 10 {
            return Err(invalid_manifest(
                path,
                format!("line {} must contain ten fields", index + 2),
            ));
        }
        let id = decode_manifest_field(path, fields[0])?;
        if !ids.insert(id.clone()) {
            return Err(invalid_manifest(path, format!("duplicate id {id:?}")));
        }
        let kind = fields[1].to_string();
        if !matches!(
            kind.as_str(),
            INPUT_KIND | DEPENDENCY_KIND | PROVENANCE_KIND
        ) {
            return Err(invalid_manifest(path, "invalid file kind"));
        }
        let role = decode_manifest_field(path, fields[2])?;
        let parent_role = if fields[3].is_empty() {
            None
        } else {
            Some(decode_manifest_field(path, fields[3])?)
        };
        let relative_path = PathBuf::from(decode_manifest_field(path, fields[4])?);
        let required_for_replay = parse_manifest_bool(path, fields[5])?;
        let bytes = fields[6]
            .parse::<u64>()
            .map_err(|_| invalid_manifest(path, "invalid byte length"))?;
        validate_fingerprint(path, fields[7])?;
        let source_bytes = fields[8]
            .parse::<u64>()
            .map_err(|_| invalid_manifest(path, "invalid source byte length"))?;
        validate_fingerprint(path, fields[9])?;
        records.push(ManifestRecord {
            id,
            kind,
            role,
            parent_role,
            relative_path,
            required_for_replay,
            bytes,
            fingerprint: fields[7].to_ascii_lowercase(),
            source_bytes,
            source_fingerprint: fields[9].to_ascii_lowercase(),
        });
    }
    Ok(records)
}

fn parse_key_value_table(
    path: &Path,
    input: &str,
) -> Result<BTreeMap<String, String>, InputBundleError> {
    let mut lines = input.lines();
    if lines.next() != Some("key\tvalue") {
        return Err(invalid_metadata(path, "unexpected metadata header"));
    }
    let mut values = BTreeMap::new();
    for (index, line) in lines.enumerate() {
        if line.is_empty() {
            continue;
        }
        let Some((key, value)) = line.split_once('\t') else {
            return Err(invalid_metadata(
                path,
                format!("line {} has no tab separator", index + 2),
            ));
        };
        if key.is_empty() || values.insert(key.to_string(), value.to_string()).is_some() {
            return Err(invalid_metadata(
                path,
                format!("line {} has an empty or duplicate key", index + 2),
            ));
        }
    }
    Ok(values)
}

fn require_value(
    values: &BTreeMap<String, String>,
    key: &'static str,
    path: &Path,
    expected: &str,
) -> Result<(), InputBundleError> {
    let actual = values
        .get(key)
        .ok_or_else(|| invalid_metadata(path, format!("missing key {key:?}")))?;
    if actual != expected {
        return Err(invalid_metadata(
            path,
            format!("{key} must be {expected:?}, got {actual:?}"),
        ));
    }
    Ok(())
}

fn parse_usize(
    values: &BTreeMap<String, String>,
    key: &'static str,
    path: &Path,
) -> Result<usize, InputBundleError> {
    values
        .get(key)
        .ok_or_else(|| invalid_metadata(path, format!("missing key {key:?}")))?
        .parse::<usize>()
        .map_err(|_| invalid_metadata(path, format!("invalid integer for {key}")))
}

fn parse_manifest_bool(path: &Path, value: &str) -> Result<bool, InputBundleError> {
    match value {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => Err(invalid_manifest(path, "invalid boolean")),
    }
}

fn decode_manifest_field(path: &Path, value: &str) -> Result<String, InputBundleError> {
    decode_field(value).map_err(|message| invalid_manifest(path, message))
}

fn validate_fingerprint(path: &Path, value: &str) -> Result<(), InputBundleError> {
    if value.len() == 16 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(invalid_manifest(path, "invalid fingerprint"))
    }
}

fn parse_portable_relative_path(raw: &str) -> Result<PathBuf, InputBundleError> {
    if raw.is_empty()
        || raw.contains('\\')
        || raw.split('/').any(|part| {
            part.is_empty()
                || matches!(part, "." | "..")
                || !part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        })
    {
        return Err(InputBundleError::InvalidPortablePath(raw.to_string()));
    }
    let path = Path::new(raw);
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => clean.push(value),
            _ => return Err(InputBundleError::InvalidPortablePath(raw.to_string())),
        }
    }
    if clean.as_os_str().is_empty() {
        return Err(InputBundleError::InvalidPortablePath(raw.to_string()));
    }
    Ok(clean)
}

fn portable_path_string(path: &Path) -> Result<String, InputBundleError> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(value) = component else {
            return Err(InputBundleError::InvalidPortablePath(
                path.display().to_string(),
            ));
        };
        let value = value
            .to_str()
            .ok_or_else(|| InputBundleError::NonUtf8Path(path.to_path_buf()))?;
        parts.push(value);
    }
    if parts.is_empty() {
        return Err(InputBundleError::InvalidPortablePath(
            path.display().to_string(),
        ));
    }
    Ok(parts.join("/"))
}

fn portable_extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 16
                && value.bytes().all(|byte| byte.is_ascii_alphanumeric())
        })
        .map(|value| format!(".{}", value.to_ascii_lowercase()))
        .unwrap_or_else(|| ".dat".to_string())
}

fn sanitize_component(value: &str) -> String {
    let mut output: String = value
        .chars()
        .take(64)
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect();
    if output.is_empty() {
        output.push_str("file");
    }
    output
}

fn first_data_fields(input: &str) -> Option<Vec<&str>> {
    input
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split_whitespace().collect())
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), InputBundleError> {
    fs::write(path, bytes).map_err(|source| InputBundleError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn invalid_metadata(path: &Path, message: impl Into<String>) -> InputBundleError {
    InputBundleError::InvalidMetadata {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

fn invalid_manifest(path: &Path, message: impl Into<String>) -> InputBundleError {
    InputBundleError::InvalidManifest {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum InputBundleError {
    OutputExists(PathBuf),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    NonUtf8Path(PathBuf),
    NonUtf8File(PathBuf),
    DuplicateInputRole(String),
    InvalidPortablePath(String),
    PathEscapesBundle(PathBuf),
    InvalidStrataHeader(PathBuf),
    InvalidMetadata {
        path: PathBuf,
        message: String,
    },
    InvalidManifest {
        path: PathBuf,
        message: String,
    },
    FileChanged {
        id: String,
        path: PathBuf,
        expected_bytes: u64,
        actual_bytes: u64,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
    DispersalStrata(biogeo_core::DispersalStrataParseError),
    AnageneticStrata(biogeo_core::AnageneticStrataParseError),
}

impl fmt::Display for InputBundleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutputExists(path) => {
                write!(formatter, "input bundle already exists: {}", path.display())
            }
            Self::Io { path, source } => {
                write!(
                    formatter,
                    "input bundle I/O failed for {}: {source}",
                    path.display()
                )
            }
            Self::NonUtf8Path(path) => {
                write!(
                    formatter,
                    "input bundle path is not UTF-8: {}",
                    path.display()
                )
            }
            Self::NonUtf8File(path) => {
                write!(
                    formatter,
                    "input bundle text file is not UTF-8: {}",
                    path.display()
                )
            }
            Self::DuplicateInputRole(role) => {
                write!(formatter, "duplicate input bundle role {role:?}")
            }
            Self::InvalidPortablePath(path) => {
                write!(formatter, "invalid portable relative path {path:?}")
            }
            Self::PathEscapesBundle(path) => write!(
                formatter,
                "input bundle path escapes its root: {}",
                path.display()
            ),
            Self::InvalidStrataHeader(path) => write!(
                formatter,
                "cannot identify the dispersal strata format in {}",
                path.display()
            ),
            Self::InvalidMetadata { path, message } => write!(
                formatter,
                "invalid input bundle metadata {}: {message}",
                path.display()
            ),
            Self::InvalidManifest { path, message } => write!(
                formatter,
                "invalid input bundle manifest {}: {message}",
                path.display()
            ),
            Self::FileChanged {
                id,
                path,
                expected_bytes,
                actual_bytes,
                expected_fingerprint,
                actual_fingerprint,
            } => write!(
                formatter,
                "input bundle file {id:?} changed at {}: expected {expected_bytes} bytes/{expected_fingerprint}, got {actual_bytes} bytes/{actual_fingerprint}",
                path.display()
            ),
            Self::DispersalStrata(source) => write!(formatter, "invalid strata table: {source}"),
            Self::AnageneticStrata(source) => {
                write!(formatter, "invalid anagenetic strata table: {source}")
            }
        }
    }
}

impl Error for InputBundleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::DispersalStrata(source) => Some(source),
            Self::AnageneticStrata(source) => Some(source),
            _ => None,
        }
    }
}

impl From<biogeo_core::DispersalStrataParseError> for InputBundleError {
    fn from(value: biogeo_core::DispersalStrataParseError) -> Self {
        Self::DispersalStrata(value)
    }
}

impl From<biogeo_core::AnageneticStrataParseError> for InputBundleError {
    fn from(value: biogeo_core::AnageneticStrataParseError) -> Self {
        Self::AnageneticStrata(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "biogeo-input-bundle-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn bundles_extended_strata_dependencies_and_survives_source_removal() {
        let root = temp_dir("strata");
        let source = root.join("source");
        fs::create_dir_all(&source).unwrap();
        let tree = source.join("tree.nwk");
        let matrix = source.join("matrix.tsv");
        let allowed = source.join("allowed.tsv");
        let explicit_ranges = source.join("allowed-ranges.tsv");
        let strata = source.join("strata.tsv");
        fs::write(&tree, "(A:1,B:1);\n").unwrap();
        fs::write(&matrix, "area\tA\tB\nA\t1\t1\nB\t1\t1\n").unwrap();
        fs::write(&allowed, "A\tB\n1\t1\n").unwrap();
        fs::write(&explicit_ranges, "range\tA\tB\n_\t0\t0\nA\t1\t0\nB\t0\t1\n").unwrap();
        fs::write(
            &strata,
            format!(
                "oldest_age\tmatrix\tdistance_matrix\tenvironment_distance_matrix\tarea_sizes\tareas_allowed\tareas_adjacency\tallowed_ranges\n1\t{}\t-\t-\t-\t{}\t-\t{}\n",
                matrix.display(),
                allowed.display(),
                explicit_ranges.display()
            ),
        )
        .unwrap();

        let bundle_dir = root.join("bundle");
        let loaded = write_input_bundle(
            &bundle_dir,
            &[
                InputBundleSpec {
                    role: "tree",
                    path: &tree,
                    required_for_replay: true,
                },
                InputBundleSpec {
                    role: "dispersal_strata",
                    path: &strata,
                    required_for_replay: true,
                },
            ],
        )
        .unwrap();
        assert_eq!(loaded.top_level_inputs.len(), 2);
        assert_eq!(loaded.dependency_count(), 3);
        assert_eq!(loaded.provenance_count(), 1);
        let bundled_strata =
            fs::read_to_string(&loaded.top_level_inputs["dispersal_strata"].path).unwrap();
        assert!(bundled_strata.contains("../dependencies/"));
        assert!(!bundled_strata.contains(&source.display().to_string()));

        fs::remove_dir_all(&source).unwrap();
        let reloaded = load_input_bundle(&bundle_dir).unwrap();
        assert_eq!(reloaded.fingerprint, loaded.fingerprint);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn portable_manifest_paths_reject_parent_and_windows_separators() {
        assert!(parse_portable_relative_path("files/tree.nwk").is_ok());
        assert!(parse_portable_relative_path("../tree.nwk").is_err());
        assert!(parse_portable_relative_path("files\\tree.nwk").is_err());
        assert!(parse_portable_relative_path("/tree.nwk").is_err());
        assert!(parse_portable_relative_path("C:/tree.nwk").is_err());
        assert!(parse_portable_relative_path("files//tree.nwk").is_err());
    }
}
