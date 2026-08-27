use std::error::Error;
use std::fmt;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use biogeo_core::{
    CladePlacementScope, FOSSIL_PLACEMENT_RNG_PROTOCOL, FossilAttachment, FossilPlacementError,
    FossilPlacementSpec, ParsedNewickTree, format_newick, place_fossils_randomly,
};

pub const FOSSIL_PLACEMENT_MANIFEST_FORMAT: &str = "biogeo-fossil-placement-manifest-v1";
pub const FOSSIL_PLACEMENT_SET_FORMAT: &str = "biogeo-fossil-placement-set-v1";
const SOURCE_TREE_FILE: &str = "source-tree.nwk";
const SOURCE_MANIFEST_FILE: &str = "source-manifest.tsv";
const METADATA_FILE: &str = "metadata.tsv";
const PLACEMENTS_FILE: &str = "placements.tsv";
const TREES_DIRECTORY: &str = "trees";

static NEXT_STAGING_DIRECTORY: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq)]
pub struct FossilPlacementRunConfig {
    pub tree_path: PathBuf,
    pub tree_name: Option<String>,
    pub manifest_path: PathBuf,
    pub output_dir: PathBuf,
    pub replicates: usize,
    pub seed: u64,
    pub direct_ancestor_hook_length: f64,
}

pub fn parse_manifest(
    input: &str,
    path: &Path,
) -> Result<Vec<FossilPlacementSpec>, FossilPlacementCliError> {
    let mut lines = input.lines().enumerate().filter_map(|(index, line)| {
        let trimmed = line.trim();
        (!trimmed.is_empty() && !trimmed.starts_with('#')).then_some((index + 1, trimmed))
    });
    let (format_line, format) = lines
        .next()
        .ok_or_else(|| invalid_manifest(path, 1, "manifest is empty"))?;
    let format = format.trim_start_matches('\u{feff}');
    if format != FOSSIL_PLACEMENT_MANIFEST_FORMAT {
        return Err(invalid_manifest(
            path,
            format_line,
            format!("expected format {FOSSIL_PLACEMENT_MANIFEST_FORMAT:?}, found {format:?}"),
        ));
    }
    let (header_line, header) = lines.next().ok_or_else(|| {
        invalid_manifest(path, format_line + 1, "missing fossil placement header")
    })?;
    let expected = "fossil_id\tmin_age\tmax_age\tattachment\tstem_or_crown\tclade_tips";
    if header != expected {
        return Err(invalid_manifest(
            path,
            header_line,
            format!(
                "header must be exactly: {}",
                expected.replace('\t', "<TAB>")
            ),
        ));
    }

    let mut specs = Vec::new();
    for (line_number, line) in lines {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 6 {
            return Err(invalid_manifest(
                path,
                line_number,
                format!("expected 6 tab-separated fields, found {}", fields.len()),
            ));
        }
        let fossil_label = decode_field(fields[0]).map_err(|message| {
            invalid_manifest(path, line_number, format!("invalid fossil_id: {message}"))
        })?;
        let min_age = parse_age(path, line_number, "min_age", fields[1])?;
        let max_age = parse_age(path, line_number, "max_age", fields[2])?;
        let attachment = match fields[3] {
            "side_branch" => FossilAttachment::SideBranch,
            "direct_ancestor" | "ancestor" => FossilAttachment::DirectAncestor,
            value => {
                return Err(invalid_manifest(
                    path,
                    line_number,
                    format!("attachment must be side_branch or direct_ancestor, found {value:?}"),
                ));
            }
        };
        let scope = match fields[4] {
            "stem" => CladePlacementScope::Stem,
            "crown" => CladePlacementScope::Crown,
            "both" => CladePlacementScope::Both,
            value => {
                return Err(invalid_manifest(
                    path,
                    line_number,
                    format!("stem_or_crown must be stem, crown, or both, found {value:?}"),
                ));
            }
        };
        let clade_tip_labels = fields[5]
            .split(',')
            .map(decode_field)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|message| {
                invalid_manifest(path, line_number, format!("invalid clade_tips: {message}"))
            })?;
        specs.push(FossilPlacementSpec {
            fossil_label,
            min_age,
            max_age,
            attachment,
            scope,
            clade_tip_labels,
        });
    }
    if specs.is_empty() {
        return Err(invalid_manifest(
            path,
            header_line + 1,
            "manifest contains no fossil rows",
        ));
    }
    Ok(specs)
}

pub fn run(
    config: &FossilPlacementRunConfig,
    parsed_tree: &ParsedNewickTree,
    manifest_input: &str,
) -> Result<String, FossilPlacementCliError> {
    if config.replicates == 0 {
        return Err(FossilPlacementCliError::ZeroReplicates);
    }
    if config.output_dir.exists() {
        return Err(FossilPlacementCliError::OutputDirectoryExists(
            config.output_dir.clone(),
        ));
    }
    let specs = parse_manifest(manifest_input, &config.manifest_path)?;
    let parent = config.output_dir.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|source| FossilPlacementCliError::Io {
        path: parent.to_path_buf(),
        source,
    })?;
    let output_name = config
        .output_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("fossil-placement");
    let sequence = NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed);
    let staging = parent.join(format!(
        ".{output_name}.staging-{}-{sequence}",
        std::process::id()
    ));
    if staging.exists() {
        return Err(FossilPlacementCliError::OutputDirectoryExists(staging));
    }
    fs::create_dir(&staging).map_err(|source| FossilPlacementCliError::Io {
        path: staging.clone(),
        source,
    })?;

    let initialized = (|| {
        fs::create_dir(staging.join(TREES_DIRECTORY)).map_err(|source| {
            FossilPlacementCliError::Io {
                path: staging.join(TREES_DIRECTORY),
                source,
            }
        })?;
        let source_tree = format!("{}\n", format_newick(parsed_tree));
        write_new(&staging.join(SOURCE_TREE_FILE), source_tree.as_bytes())?;
        write_new(
            &staging.join(SOURCE_MANIFEST_FILE),
            manifest_input.as_bytes(),
        )?;

        let mut placements = String::from(
            "replicate\ttree_file\tfossil_index\tfossil_id\tmin_age\tmax_age\tfossil_age\tattachment_age\tfossil_branch_length\tattachment\tstem_or_crown\tclade_tips\tselected_branch_child_clade\n",
        );
        for replicate_index in 0..config.replicates {
            let replicate = replicate_index + 1;
            let replicate_seed = derive_replicate_seed(config.seed, replicate_index as u64);
            let result = place_fossils_randomly(
                parsed_tree,
                &specs,
                replicate_seed,
                config.direct_ancestor_hook_length,
            )?;
            let tree_file = format!("{TREES_DIRECTORY}/tree-{replicate:06}.nwk");
            let newick = format!("{}\n", format_newick(&result.tree));
            write_new(&staging.join(&tree_file), newick.as_bytes())?;
            for (fossil_index, (spec, record)) in specs.iter().zip(&result.records).enumerate() {
                writeln!(
                    placements,
                    "{}\t{}\t{}\t{}\t{:.17}\t{:.17}\t{:.17}\t{:.17}\t{:.17}\t{}\t{}\t{}\t{}",
                    replicate,
                    tree_file,
                    fossil_index + 1,
                    encode_field(&record.fossil_label),
                    spec.min_age,
                    spec.max_age,
                    record.fossil_age,
                    record.attachment_age,
                    record.fossil_branch_length,
                    record.attachment.as_str(),
                    record.scope.as_str(),
                    spec.clade_tip_labels
                        .iter()
                        .map(|label| encode_list_item(label))
                        .collect::<Vec<_>>()
                        .join(","),
                    encode_field(&clade_label(&result.tree, record.selected_child)),
                )
                .expect("writing fossil placement rows to a String cannot fail");
            }
        }
        write_new(&staging.join(PLACEMENTS_FILE), placements.as_bytes())?;

        let metadata = format!(
            "format\t{FOSSIL_PLACEMENT_SET_FORMAT}\nstatus\tcomplete\nreplicates\t{}\nfossils_per_tree\t{}\nmaster_seed\t{}\nrng_protocol\t{}\ndirect_ancestor_hook_length\t{:.17}\ndirect_ancestor_likelihood_setting\t--min-branch-length must be greater than the hook length\nsource_tree\t{}\nsource_manifest\t{}\nplacements\t{}\ntrees_directory\t{}\nfield_encoding\tpercent_for_percent_tab_cr_lf_comma_in_lists\n",
            config.replicates,
            specs.len(),
            config.seed,
            FOSSIL_PLACEMENT_RNG_PROTOCOL,
            config.direct_ancestor_hook_length,
            SOURCE_TREE_FILE,
            SOURCE_MANIFEST_FILE,
            PLACEMENTS_FILE,
            TREES_DIRECTORY,
        );
        write_new(&staging.join(METADATA_FILE), metadata.as_bytes())?;
        crate::fs_retry::rename(&staging, &config.output_dir).map_err(|source| {
            FossilPlacementCliError::Io {
                path: config.output_dir.clone(),
                source,
            }
        })?;
        Ok(metadata)
    })();
    if initialized.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    initialized
}

fn clade_label(tree: &ParsedNewickTree, node: usize) -> String {
    let mut labels = Vec::new();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        let children = tree
            .tree
            .children(current)
            .expect("placement record node is inside the generated tree");
        if children.is_empty() {
            labels.push(
                tree.node_label(current)
                    .expect("every generated-tree tip has a label")
                    .to_string(),
            );
        } else {
            stack.extend(children.iter().map(|child| child.node));
        }
    }
    labels.sort();
    labels.join("+")
}

fn derive_replicate_seed(master_seed: u64, replicate_index: u64) -> u64 {
    let mut value = master_seed ^ replicate_index.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

fn parse_age(
    path: &Path,
    line: usize,
    column: &str,
    value: &str,
) -> Result<f64, FossilPlacementCliError> {
    value.parse::<f64>().map_err(|error| {
        invalid_manifest(
            path,
            line,
            format!("invalid {column} value {value:?}: {error}"),
        )
    })
}

fn decode_field(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            decoded.push(bytes[index]);
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len() {
            return Err("truncated percent escape".to_string());
        }
        let high = decode_hex(bytes[index + 1])
            .ok_or_else(|| "percent escape contains a non-hex digit".to_string())?;
        let low = decode_hex(bytes[index + 2])
            .ok_or_else(|| "percent escape contains a non-hex digit".to_string())?;
        decoded.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(decoded).map_err(|_| "decoded value is not UTF-8".to_string())
}

fn decode_hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn encode_field(value: &str) -> String {
    encode_bytes(value, false)
}

fn encode_list_item(value: &str) -> String {
    encode_bytes(value, true)
}

fn encode_bytes(value: &str, encode_comma: bool) -> String {
    let mut output = Vec::with_capacity(value.len());
    for byte in value.bytes() {
        if matches!(byte, b'%' | b'\t' | b'\r' | b'\n') || encode_comma && byte == b',' {
            output.push(b'%');
            output.extend_from_slice(format!("{byte:02X}").as_bytes());
        } else {
            output.push(byte);
        }
    }
    String::from_utf8(output).expect("field encoding preserves UTF-8")
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), FossilPlacementCliError> {
    if path.exists() {
        return Err(FossilPlacementCliError::OutputFileExists(
            path.to_path_buf(),
        ));
    }
    fs::write(path, bytes).map_err(|source| FossilPlacementCliError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn invalid_manifest(
    path: &Path,
    line: usize,
    message: impl Into<String>,
) -> FossilPlacementCliError {
    FossilPlacementCliError::InvalidManifest {
        path: path.to_path_buf(),
        line,
        message: message.into(),
    }
}

#[derive(Debug)]
pub enum FossilPlacementCliError {
    InvalidManifest {
        path: PathBuf,
        line: usize,
        message: String,
    },
    ZeroReplicates,
    OutputDirectoryExists(PathBuf),
    OutputFileExists(PathBuf),
    Placement(FossilPlacementError),
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

impl fmt::Display for FossilPlacementCliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidManifest {
                path,
                line,
                message,
            } => write!(
                f,
                "invalid fossil-placement manifest {} at line {line}: {message}",
                path.display()
            ),
            Self::ZeroReplicates => write!(f, "fossil placement requires at least one replicate"),
            Self::OutputDirectoryExists(path) => write!(
                f,
                "fossil-placement output directory already exists: {}",
                path.display()
            ),
            Self::OutputFileExists(path) => write!(
                f,
                "fossil-placement output file already exists: {}",
                path.display()
            ),
            Self::Placement(error) => write!(f, "fossil placement failed: {error}"),
            Self::Io { path, source } => {
                write!(
                    f,
                    "fossil-placement I/O failed for {}: {source}",
                    path.display()
                )
            }
        }
    }
}

impl Error for FossilPlacementCliError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Placement(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<FossilPlacementError> for FossilPlacementCliError {
    fn from(value: FossilPlacementError) -> Self {
        Self::Placement(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use biogeo_core::parse_newick;

    #[test]
    fn parses_versioned_manifest_and_percent_encoded_tip_lists() {
        let input = concat!(
            "biogeo-fossil-placement-manifest-v1\n",
            "fossil_id\tmin_age\tmax_age\tattachment\tstem_or_crown\tclade_tips\n",
            "F%25\t1\t2.5\tdirect_ancestor\tboth\tA%2C1,B\n"
        );
        let specs = parse_manifest(input, Path::new("fossils.tsv")).unwrap();
        assert_eq!(specs[0].fossil_label, "F%");
        assert_eq!(specs[0].clade_tip_labels, ["A,1", "B"]);
        assert_eq!(specs[0].attachment, FossilAttachment::DirectAncestor);
    }

    #[test]
    fn writes_non_overwriting_reproducible_placement_set() {
        let base = std::env::temp_dir().join(format!(
            "biogeo-fossil-placement-test-{}-{}",
            std::process::id(),
            NEXT_STAGING_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        let output = base.join("result");
        let manifest = concat!(
            "biogeo-fossil-placement-manifest-v1\n",
            "fossil_id\tmin_age\tmax_age\tattachment\tstem_or_crown\tclade_tips\n",
            "F\t0.5\t1.5\tside_branch\tcrown\tA,B\n"
        );
        let config = FossilPlacementRunConfig {
            tree_path: PathBuf::from("tree.nwk"),
            tree_name: None,
            manifest_path: PathBuf::from("fossils.tsv"),
            output_dir: output.clone(),
            replicates: 2,
            seed: 7,
            direct_ancestor_hook_length: 1e-7,
        };
        let tree = parse_newick("((A:2,B:2):1,C:3);").unwrap();
        let metadata = run(&config, &tree, manifest).unwrap();
        assert!(metadata.starts_with("format\tbiogeo-fossil-placement-set-v1\n"));
        assert!(output.join("trees/tree-000001.nwk").is_file());
        assert!(output.join("trees/tree-000002.nwk").is_file());
        assert!(
            fs::read_to_string(output.join(PLACEMENTS_FILE))
                .unwrap()
                .contains("\n1\ttrees/tree-000001.nwk\t1\tF\t")
        );
        assert!(matches!(
            run(&config, &tree, manifest),
            Err(FossilPlacementCliError::OutputDirectoryExists(_))
        ));
        let _ = fs::remove_dir_all(base);
    }
}
