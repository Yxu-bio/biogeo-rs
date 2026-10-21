use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use biogeo_core::source_attribution::{
    ROUTE_ALGORITHM, RouteCandidate, SOURCE_ALGORITHM, SOURCE_RNG_PROTOCOL, SourceEventId,
    route_probabilities, sample_source, source_probabilities,
};
use biogeo_core::{AreaSet, DispersalMultiplierMatrix};
use rayon::prelude::*;

pub const FORMAT: &str = "biogeo-bsm-source-attribution-v1";
pub const RUN_FORMAT: &str = "biogeo-bsm-source-attribution-run-v1";
pub const INSPECT_FORMAT: &str = "biogeo-bsm-source-inspection-v1";
pub const PROBABILITY_FORMAT: &str = "biogeo-bsm-source-attribution-v2";
pub const PROBABILITY_RUN_FORMAT: &str = "biogeo-bsm-source-attribution-run-v2";
pub const PROBABILITY_INSPECT_FORMAT: &str = "biogeo-bsm-source-inspection-v2";
const CHUNK_ROWS: usize = 4096;
const PROBABILITY_CHUNK_ROWS: usize = 256;
const ROUTE_HEADER: &str =
    "sample\tevent_id\tsource_area_index\ttarget_area_index\tconditional_probability";
const HEADER: &str = "sample\tevent_id\tevent_scope\tevent_kind\tedge\tnode\tevent_ordinal\tsegment\tq_index\ttime_from_parent\tage_before_present\tsource_state_index\ttarget_state_index\tfounder_daughter_order\tsource_area_index\ttarget_area_index\tassignment_status\treason\tconditional_probability\tpositive_candidates";
const FILE_HEADER: &str = "chunk\tfile\trows\tbytes\tfingerprint";
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    fit: PathBuf,
    bsm: PathBuf,
    output: PathBuf,
    seed: Option<u64>,
    threads: Option<usize>,
    inspect: bool,
    deep: bool,
    resume: bool,
    probabilities: bool,
}

impl Config {
    fn chunk_rows(&self) -> usize {
        if self.probabilities {
            PROBABILITY_CHUNK_ROWS
        } else {
            CHUNK_ROWS
        }
    }
}

pub fn parse(args: impl IntoIterator<Item = String>, inspect: bool) -> Result<Config> {
    let mut values = BTreeMap::new();
    let mut deep = false;
    let mut resume = false;
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--deep" && inspect {
            deep = true;
            continue;
        }
        if arg == "--resume" && !inspect {
            resume = true;
            continue;
        }
        let allowed = [
            "--analysis-result",
            "--bsm-result",
            if inspect {
                "--source-result"
            } else {
                "--output-dir"
            },
        ];
        if !allowed.contains(&arg.as_str())
            && (inspect || !["--seed", "--threads", "--allocation-mode"].contains(&arg.as_str()))
        {
            return Err(format!("unknown source allocation option: {arg}"));
        }
        let value = args
            .next()
            .ok_or_else(|| format!("missing value for {arg}"))?;
        if values.insert(arg.clone(), value).is_some() {
            return Err(format!("duplicate option: {arg}"));
        }
    }
    let required = |key: &str| {
        values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing {key}"))
    };
    let threads = match values
        .get("--threads")
        .map(String::as_str)
        .unwrap_or("auto")
    {
        "auto" => None,
        text => Some(
            text.parse::<usize>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or("--threads must be auto or a positive integer")?,
        ),
    };
    let probabilities = match values
        .get("--allocation-mode")
        .map(String::as_str)
        .unwrap_or("sample")
    {
        "sample" => false,
        "probabilities" => true,
        _ => return Err("--allocation-mode must be sample or probabilities".into()),
    };
    if probabilities && values.contains_key("--seed") {
        return Err("probabilities mode uses no RNG; omit --seed".into());
    }
    Ok(Config {
        fit: required("--analysis-result")?.into(),
        bsm: required("--bsm-result")?.into(),
        output: required(if inspect {
            "--source-result"
        } else {
            "--output-dir"
        })?
        .into(),
        seed: if inspect || probabilities {
            None
        } else {
            Some(
                required("--seed")?
                    .parse()
                    .map_err(|_| "--seed must be an unsigned 64-bit integer")?,
            )
        },
        threads,
        inspect,
        deep,
        resume,
        probabilities,
    })
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}
fn keys(text: &str) -> Result<BTreeMap<String, String>> {
    let mut keys = BTreeMap::new();
    for line in text.lines() {
        let (key, value) = line.split_once('\t').ok_or("expected key/value TSV")?;
        if keys.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("duplicate key: {key}"));
        }
    }
    Ok(keys)
}
fn fingerprint(path: &Path) -> Result<(u64, String)> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut hash = 0xcbf29ce484222325u64;
    let mut count = 0;
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        count += n as u64;
        for byte in &buffer[..n] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
    }
    Ok((count, format!("{hash:016x}")))
}
fn hash(text: &str) -> String {
    crate::analysis_result::stable_fingerprint(text.as_bytes())
}

struct Input {
    fit: crate::analysis_result::LoadedAnalysisResult,
    restored: crate::RestoredParameterAnalysis,
    bundles: Vec<PathBuf>,
    binding: String,
    ages: Vec<f64>,
}

impl Input {
    fn load(config: &Config) -> Result<Self> {
        let fit =
            crate::analysis_result::load_analysis_result(&config.fit).map_err(|e| e.to_string())?;
        let restored = crate::restore_parameter_analysis(&fit).map_err(|e| e.to_string())?;
        let bundles = crate::bsm_inspect::source_bundles(&config.bsm).map_err(|e| e.to_string())?;
        let metadata = keys(&read(&config.bsm.join("metadata.tsv"))?)?;
        if metadata.get("analysis_result_fingerprint") != Some(&fit.fingerprint) {
            return Err("BSM analysis_result_fingerprint does not match the saved fit (a saved-fit model-bsm result is required)".into());
        }
        let context = crate::BsmFormattingContext::new(
            &restored.parsed_tree,
            &restored.parsed_ranges,
            &restored.states,
            &restored.model,
            crate::BsmOutputLevel::Compact,
        )
        .map_err(|e| e.to_string())?;
        let mut dictionary = String::new();
        let mut files = vec![config.bsm.join("metadata.tsv")];
        for (name, expected) in crate::format_bsm_reference_tables(&context, &restored.model) {
            let actual = read(&config.bsm.join(name))?;
            if actual != expected {
                return Err(format!("BSM {name} does not match the saved fit"));
            }
            dictionary.push_str(&format!("{name}\t{}\n", hash(&actual)));
            files.push(config.bsm.join(name));
        }
        for bundle in &bundles {
            for name in [
                "node_states.tsv",
                "cladogenetic_splits.tsv",
                "branch_segments.tsv",
                "sample_event_counts.tsv",
                "sample_period_event_counts.tsv",
                "sample_state_occupancy.tsv",
                "sample_period_state_occupancy.tsv",
                "anagenetic_events.tsv",
            ] {
                files.push(bundle.join(name));
            }
        }
        let mut content = String::new();
        for path in files {
            let (bytes, fp) = fingerprint(&path)?;
            let relative = path
                .strip_prefix(&config.bsm)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            content.push_str(&format!("{relative}\t{bytes}\t{fp}\n"));
        }
        let binding = format!(
            "analysis_result_fingerprint\t{}\nbsm_content_fingerprint\t{}\ndictionary_fingerprint\t{}\n",
            fit.fingerprint,
            hash(&content),
            hash(&dictionary)
        );
        let ages = restored.parsed_tree.tree.node_ages_from_present();
        Ok(Self {
            fit,
            restored,
            bundles,
            binding,
            ages,
        })
    }

    fn matrix(&self, q: usize) -> Result<Option<&DispersalMultiplierMatrix>> {
        let model = &self.restored.model;
        if let Some(schedule) = model.anagenesis.time_stratified_anagenesis() {
            Ok(schedule
                .strata()
                .get(q)
                .ok_or("event q_index outside fitted schedule")?
                .dispersal_multipliers
                .as_ref())
        } else if q == 0 {
            Ok(model.anagenesis.dispersal_multipliers())
        } else {
            Err("nonzero q_index in a homogeneous model".into())
        }
    }
}

#[derive(Clone)]
struct Event {
    sample: u64,
    id: String,
    scope: &'static str,
    kind: String,
    edge: Option<usize>,
    node: Option<usize>,
    ordinal: Option<u64>,
    segment: Option<usize>,
    q: usize,
    time: Option<f64>,
    age: f64,
    source_state: usize,
    target_state: Option<usize>,
    daughter: Option<usize>,
}

fn optional<T: std::fmt::Display>(value: Option<T>) -> String {
    value.map_or_else(|| "NA".into(), |x| x.to_string())
}

impl Event {
    fn allocate(&self, input: &Input, seed: u64) -> Result<String> {
        let states = &input.restored.states;
        let source = states
            .get(self.source_state)
            .ok_or("invalid source state index")?;
        let target = self
            .target_state
            .map(|i| states.get(i).ok_or("invalid target state index"))
            .transpose()?;
        let mut source_area = None;
        let mut target_area = None;
        let mut probability = None;
        let mut positive = 0;
        let (status, reason) = match self.kind.as_str() {
            "range_expansion" | "founder_event" => {
                let target = target.ok_or("missing target state")?;
                let target_bits = if self.kind == "range_expansion" {
                    target.bits() & !source.bits()
                } else {
                    target.bits()
                };
                if self.kind == "founder_event" && target_bits.count_ones() != 1 {
                    ("undetermined", "unsupported_multi_area_founder_target")
                } else {
                    if target_bits.count_ones() != 1 {
                        return Err("expansion must gain exactly one area".into());
                    }
                    let area = target_bits.trailing_zeros() as u8;
                    target_area = Some(area);
                    let candidates = source_probabilities(
                        source,
                        area,
                        states.num_areas(),
                        input.matrix(self.q)?,
                    )
                    .map_err(str::to_owned)?;
                    positive = candidates.len();
                    let id = if let Some(edge) = self.edge {
                        SourceEventId::Anagenetic {
                            sample: self.sample,
                            edge: edge as u64,
                            ordinal: self.ordinal.unwrap(),
                        }
                    } else {
                        SourceEventId::Cladogenetic {
                            sample: self.sample,
                            node: self.node.unwrap() as u64,
                        }
                    };
                    if let Some(selected) = sample_source(&candidates, seed, id) {
                        source_area = Some(selected.area);
                        probability = Some(selected.probability);
                        (
                            if positive == 1 {
                                "unique_candidate"
                            } else {
                                "weighted_sampled"
                            },
                            "NA",
                        )
                    } else {
                        ("undetermined", "zero_total_weight")
                    }
                }
            }
            "range_switching" => {
                let target = target.ok_or("missing switching target")?;
                if source.size() != 1 || target.size() != 1 || source == target {
                    return Err("invalid switching transition".into());
                }
                source_area = Some(source.bits().trailing_zeros() as u8);
                target_area = Some(target.bits().trailing_zeros() as u8);
                probability = Some(1.0);
                positive = 1;
                ("state_transition", "NA")
            }
            "local_extirpation" | "range_copying" | "subset_sympatry" | "vicariance" => {
                ("not_applicable", "event_has_no_dispersal_source")
            }
            _ => return Err(format!("unsupported event kind {}", self.kind)),
        };
        Ok(self.format_assignment(
            source_area,
            target_area,
            probability,
            positive,
            status,
            reason,
        ))
    }

    fn allocate_probabilities(&self, input: &Input) -> Result<(String, String)> {
        let states = &input.restored.states;
        let source = states
            .get(self.source_state)
            .ok_or("invalid source state index")?;
        let target = self
            .target_state
            .map(|i| states.get(i).ok_or("invalid target state index"))
            .transpose()?;
        let mut routes = Vec::new();
        let (status, reason) = match self.kind.as_str() {
            "range_expansion" | "founder_event" => {
                let target = target.ok_or("missing target state")?;
                let target = if self.kind == "range_expansion" {
                    let gained = target.bits() & !source.bits();
                    if gained.count_ones() != 1 {
                        return Err("expansion must gain exactly one area".into());
                    }
                    AreaSet::from_bits(gained)
                } else {
                    target
                };
                routes =
                    route_probabilities(source, target, states.num_areas(), input.matrix(self.q)?)
                        .map_err(str::to_owned)?;
                if routes.is_empty() {
                    ("undetermined", "zero_total_weight")
                } else {
                    ("probability_distribution", "NA")
                }
            }
            "range_switching" => {
                let target = target.ok_or("missing switching target")?;
                if source.size() != 1 || target.size() != 1 || source == target {
                    return Err("invalid switching transition".into());
                }
                routes.push(RouteCandidate {
                    source_area: source.bits().trailing_zeros() as u8,
                    target_area: target.bits().trailing_zeros() as u8,
                    probability: 1.0,
                });
                ("state_transition", "NA")
            }
            "local_extirpation" | "range_copying" | "subset_sympatry" | "vicariance" => {
                ("not_applicable", "event_has_no_dispersal_source")
            }
            _ => return Err(format!("unsupported event kind {}", self.kind)),
        };
        let mut text = String::new();
        for route in &routes {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{:.17e}\n",
                self.sample, self.id, route.source_area, route.target_area, route.probability
            ));
        }
        Ok((
            self.format_assignment(None, None, None, routes.len(), status, reason),
            text,
        ))
    }

    fn format_assignment(
        &self,
        source_area: Option<u8>,
        target_area: Option<u8>,
        probability: Option<f64>,
        positive: usize,
        status: &str,
        reason: &str,
    ) -> String {
        format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.17}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.sample,
            self.id,
            self.scope,
            self.kind,
            optional(self.edge),
            optional(self.node),
            optional(self.ordinal),
            optional(self.segment),
            self.q,
            self.time
                .map_or_else(|| "NA".into(), |x| format!("{x:.17}")),
            self.age,
            self.source_state,
            optional(self.target_state),
            optional(self.daughter),
            optional(source_area),
            optional(target_area),
            status,
            reason,
            probability.map_or_else(|| "NA".into(), |x| format!("{x:.17}")),
            positive
        )
    }
}

struct Row<'a> {
    columns: &'a BTreeMap<String, usize>,
    values: Vec<&'a str>,
}
impl Row<'_> {
    fn text(&self, key: &str) -> Result<&str> {
        self.columns
            .get(key)
            .and_then(|i| self.values.get(*i))
            .copied()
            .ok_or_else(|| format!("missing event field {key}"))
    }
    fn integer(&self, key: &str) -> Result<usize> {
        self.text(key)?
            .parse()
            .map_err(|_| format!("invalid event integer {key}"))
    }
    fn number(&self, key: &str) -> Result<f64> {
        self.text(key)?
            .parse()
            .map_err(|_| format!("invalid event number {key}"))
    }
}

fn visit_events(input: &Input, mut consume: impl FnMut(Event) -> Result<()>) -> Result<()> {
    for (scope, file) in [
        ("anagenetic", "anagenetic_events.tsv"),
        ("cladogenetic", "cladogenetic_splits.tsv"),
    ] {
        let mut current_sample = None;
        let mut ordinals = BTreeMap::<usize, u64>::new();
        for bundle in &input.bundles {
            let path = bundle.join(file);
            let mut reader =
                BufReader::new(File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?);
            let mut line = String::new();
            reader.read_line(&mut line).map_err(|e| e.to_string())?;
            let columns: BTreeMap<_, _> = line
                .trim_end_matches(['\n', '\r'])
                .split('\t')
                .enumerate()
                .map(|(i, name)| (name.to_owned(), i))
                .collect();
            loop {
                line.clear();
                if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
                    break;
                }
                let row = Row {
                    columns: &columns,
                    values: line.trim_end_matches(['\n', '\r']).split('\t').collect(),
                };
                let sample = row.integer("sample")? as u64;
                let kind = row.text("event_kind")?.to_owned();
                let event = if scope == "anagenetic" {
                    if current_sample != Some(sample) {
                        ordinals.clear();
                        current_sample = Some(sample);
                    }
                    let edge = row.integer("edge")?;
                    let ordinal = ordinals.entry(edge).or_default();
                    let event_ordinal = *ordinal;
                    *ordinal += 1;
                    let time = row.number("time_from_parent")?;
                    let parent = input
                        .restored
                        .parsed_tree
                        .tree
                        .edges()
                        .get(edge)
                        .ok_or("invalid event edge")?
                        .parent;
                    Event {
                        sample,
                        id: format!("s{sample}:a:e{edge}:i{event_ordinal}"),
                        scope,
                        kind,
                        edge: Some(edge),
                        node: None,
                        ordinal: Some(event_ordinal),
                        segment: Some(row.integer("segment")?),
                        q: row.integer("q_index")?,
                        time: Some(time),
                        age: input.ages[parent] - time,
                        source_state: row.integer("from_state_index")?,
                        target_state: Some(row.integer("to_state_index")?),
                        daughter: None,
                    }
                } else {
                    let node = row.integer("node")?;
                    let age = *input.ages.get(node).ok_or("invalid split node")?;
                    let q = input
                        .restored
                        .model
                        .anagenesis
                        .time_stratified_anagenesis()
                        .map_or(0, |s| s.stratum_index_at_age(age));
                    let daughter = if kind == "founder_event" {
                        Some(row.integer("founder_daughter_order")?)
                    } else {
                        None
                    };
                    let target_state = match daughter {
                        Some(0) => Some(row.integer("left_state_index")?),
                        Some(1) => Some(row.integer("right_state_index")?),
                        Some(_) => return Err("invalid founder daughter order".into()),
                        None => None,
                    };
                    Event {
                        sample,
                        id: format!("s{sample}:c:n{node}"),
                        scope,
                        kind,
                        edge: None,
                        node: Some(node),
                        ordinal: None,
                        segment: None,
                        q,
                        time: None,
                        age,
                        source_state: row.integer("ancestor_state_index")?,
                        target_state,
                        daughter,
                    }
                };
                consume(event)?;
            }
        }
    }
    Ok(())
}

fn commit(path: &Path, text: &str, check_only: bool) -> Result<()> {
    if path.exists() {
        if read(path)? != text {
            return Err(format!("content mismatch: {}", path.display()));
        }
    } else if check_only {
        return Err(format!("missing committed file: {}", path.display()));
    } else {
        let partial = path.with_extension("tsv.partial");
        fs::write(&partial, text).map_err(|e| e.to_string())?;
        fs::rename(partial, path).map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn metadata(input: &Input, seed: u64, probabilities: bool) -> String {
    let (format, algorithm, rng, seed, chunk_rows, multi_rule) = if probabilities {
        (
            PROBABILITY_FORMAT,
            ROUTE_ALGORITHM,
            "none",
            "NA".to_owned(),
            PROBABILITY_CHUNK_ROWS,
            "joint_source_target_conditional_probability",
        )
    } else {
        (
            FORMAT,
            SOURCE_ALGORITHM,
            SOURCE_RNG_PROTOCOL,
            seed.to_string(),
            CHUNK_ROWS,
            "undetermined_no_joint_target_model",
        )
    };
    format!(
        "format\t{format}\nalgorithm\t{algorithm}\nrng_protocol\t{rng}\nsource_seed\t{seed}\nfingerprint_algorithm\tfnv1a64\n{}model_fingerprint\t{}\nevent_id_protocol\tsample-scope-location-ordinal-v1\nrow_order\tanagenetic_then_cladogenetic_preserving_input_table_order\nchunk_rows\t{chunk_rows}\nsource_semantics\tconditional_auxiliary_allocation_not_new_history\nweight_semantics\teffective_fitted_dmat_source_row_target_column\nanagenetic_period_rule\tstored_segment_q_index\ncladogenetic_period_rule\tengine_stratum_index_at_node_age_younger_at_boundary_tolerance_1e-12\nmulti_area_founder_rule\t{multi_rule}\nzero_weight_rule\tundetermined_no_uniform_fallback\nnode_edge_area_state_mapping\tidentity_to_bound_bsm_and_fit\ntime_units\tinput_tree_branch_length_units\n",
        input.binding, input.fit.manifest.model_fingerprint
    )
}

struct Writer<'a> {
    config: &'a Config,
    input: &'a Input,
    pool: rayon::ThreadPool,
    seed: u64,
    pending: Vec<Event>,
    files: String,
    route_files: String,
    route_rows: usize,
    chunks: usize,
    rows: usize,
    statuses: BTreeMap<String, usize>,
    reasons: BTreeMap<String, usize>,
}

impl Writer<'_> {
    fn flush(&mut self) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let allocated: Vec<Result<(String, String)>> = self.pool.install(|| {
            self.pending
                .par_iter()
                .map(|event| {
                    if self.config.probabilities {
                        event.allocate_probabilities(self.input)
                    } else {
                        event
                            .allocate(self.input, self.seed)
                            .map(|row| (row, String::new()))
                    }
                })
                .collect()
        });
        let mut text = format!("{HEADER}\n");
        let mut route_text = format!("{ROUTE_HEADER}\n");
        for row in allocated {
            let (row, routes) = row?;
            route_text.push_str(&routes);
            let fields: Vec<_> = row.trim_end().split('\t').collect();
            *self.statuses.entry(fields[16].to_owned()).or_default() += 1;
            if fields[17] != "NA" {
                *self.reasons.entry(fields[17].to_owned()).or_default() += 1;
            }
            text.push_str(&row);
        }
        let relative = format!("shards/sources-{:08}.tsv", self.chunks);
        commit(
            &self.config.output.join(&relative),
            &text,
            self.config.inspect,
        )?;
        self.files.push_str(&format!(
            "{}\t{relative}\t{}\t{}\t{}\n",
            self.chunks,
            self.pending.len(),
            text.len(),
            hash(&text)
        ));
        if self.config.probabilities {
            let relative = format!("shards/route-candidates-{:08}.tsv", self.chunks);
            commit(
                &self.config.output.join(&relative),
                &route_text,
                self.config.inspect,
            )?;
            let count = route_text.lines().count() - 1;
            self.route_rows += count;
            self.route_files.push_str(&format!(
                "{}\t{relative}\t{count}\t{}\t{}\n",
                self.chunks,
                route_text.len(),
                hash(&route_text)
            ));
        }
        self.chunks += 1;
        self.rows += self.pending.len();
        self.pending.clear();
        Ok(())
    }
    fn finish(mut self, metadata: &str) -> Result<(usize, usize)> {
        self.flush()?;
        validate_chunk_names(&self.config.output, self.chunks, self.config.probabilities)?;
        commit(
            &self.config.output.join("files.tsv"),
            &self.files,
            self.config.inspect,
        )?;
        let mut complete = format!(
            "status\tcompleted\nevents\t{}\nchunks\t{}\nmetadata_fingerprint\t{}\nfiles_fingerprint\t{}\n",
            self.rows,
            self.chunks,
            hash(metadata),
            hash(&self.files)
        );
        if self.config.probabilities {
            commit(
                &self.config.output.join("route-files.tsv"),
                &self.route_files,
                self.config.inspect,
            )?;
            complete.push_str(&format!(
                "route_rows\t{}\nroute_files_fingerprint\t{}\n",
                self.route_rows,
                hash(&self.route_files)
            ));
        }
        for (status, count) in &self.statuses {
            complete.push_str(&format!("status.{status}\t{count}\n"));
        }
        for (reason, count) in &self.reasons {
            complete.push_str(&format!("reason.{reason}\t{count}\n"));
        }
        commit(
            &self.config.output.join("complete.tsv"),
            &complete,
            self.config.inspect,
        )?;
        validate_output(self.config, metadata)?;
        Ok((self.rows, *self.statuses.get("undetermined").unwrap_or(&0)))
    }
}

fn validate_output(config: &Config, metadata: &str) -> Result<(usize, usize)> {
    let files = read(&config.output.join("files.tsv"))?;
    let complete = keys(&read(&config.output.join("complete.tsv"))?)?;
    if complete.get("status").map(String::as_str) != Some("completed")
        || complete.get("metadata_fingerprint") != Some(&hash(metadata))
        || complete.get("files_fingerprint") != Some(&hash(&files))
    {
        return Err("invalid source completion marker or manifest fingerprint".into());
    }
    let mut rows = 0usize;
    let mut chunks = 0;
    let mut lines = files.lines();
    if lines.next() != Some(FILE_HEADER) {
        return Err("invalid source file manifest header".into());
    }
    for line in lines {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 5
            || fields[0] != chunks.to_string()
            || fields[1] != format!("shards/sources-{chunks:08}.tsv")
        {
            return Err("invalid source chunk identity".into());
        }
        let path = config.output.join(fields[1]);
        let (bytes, fp) = fingerprint(&path)?;
        if fields[3] != bytes.to_string() || fields[4] != fp {
            return Err(format!(
                "source chunk checksum mismatch: {}",
                path.display()
            ));
        }
        let text = read(&path)?;
        if text.lines().next() != Some(HEADER) {
            return Err("invalid source chunk header".into());
        }
        let count = text.lines().count() - 1;
        if count == 0 || count > config.chunk_rows() || fields[2] != count.to_string() {
            return Err("invalid source chunk row count".into());
        }
        rows += count;
        chunks += 1;
    }
    if complete.get("events") != Some(&rows.to_string())
        || complete.get("chunks") != Some(&chunks.to_string())
    {
        return Err("source completion counts mismatch".into());
    }
    validate_chunk_names(&config.output, chunks, config.probabilities)?;
    if config.probabilities {
        validate_routes(config, &complete, chunks)?;
    }
    let mut allowed_statuses = vec![
        "weighted_sampled",
        "unique_candidate",
        "state_transition",
        "not_applicable",
        "undetermined",
    ];
    if config.probabilities {
        allowed_statuses.push("probability_distribution");
    }
    let mut status_total = 0;
    for (key, value) in &complete {
        if let Some(status) = key.strip_prefix("status.") {
            if !allowed_statuses.contains(&status) {
                return Err(format!("unknown assignment status {status}"));
            }
            status_total += value.parse::<usize>().map_err(|e| e.to_string())?;
        }
    }
    if status_total != rows {
        return Err("source assignment status totals mismatch".into());
    }
    Ok((
        rows,
        complete
            .get("status.undetermined")
            .map_or(Ok(0), |n| n.parse::<usize>().map_err(|e| e.to_string()))?,
    ))
}

fn validate_routes(
    config: &Config,
    complete: &BTreeMap<String, String>,
    chunks: usize,
) -> Result<()> {
    let manifest = read(&config.output.join("route-files.tsv"))?;
    if complete.get("route_files_fingerprint") != Some(&hash(&manifest)) {
        return Err("route manifest fingerprint mismatch".into());
    }
    let mut lines = manifest.lines();
    if lines.next() != Some(FILE_HEADER) {
        return Err("invalid route manifest header".into());
    }
    let mut total = 0;
    for chunk in 0..chunks {
        let fields: Vec<_> = lines
            .next()
            .ok_or("missing route chunk")?
            .split('\t')
            .collect();
        let relative = format!("shards/route-candidates-{chunk:08}.tsv");
        if fields.len() != 5 || fields[0] != chunk.to_string() || fields[1] != relative {
            return Err("invalid route chunk identity".into());
        }
        let path = config.output.join(&relative);
        let (bytes, fp) = fingerprint(&path)?;
        if fields[3] != bytes.to_string() || fields[4] != fp {
            return Err("route chunk checksum mismatch".into());
        }
        let text = read(&path)?;
        let mut routes = text.lines();
        if routes.next() != Some(ROUTE_HEADER) {
            return Err("invalid route chunk header".into());
        }
        let events = read(&config.output.join(format!("shards/sources-{chunk:08}.tsv")))?;
        let mut expected = BTreeMap::new();
        for event in events.lines().skip(1) {
            let event: Vec<_> = event.split('\t').collect();
            if event.len() != 20 || event[14..16].iter().any(|v| *v != "NA") || event[18] != "NA" {
                return Err("invalid probability event row".into());
            }
            let count = event[19].parse::<usize>().map_err(|e| e.to_string())?;
            let applicable = matches!(event[16], "probability_distribution" | "state_transition");
            if applicable != (count > 0) || (event[16] == "state_transition" && count != 1) {
                return Err("route count inconsistent with event status".into());
            }
            if expected
                .insert(
                    (event[0], event[1]),
                    (count, 0usize, 0.0f64, std::collections::BTreeSet::new()),
                )
                .is_some()
            {
                return Err("duplicate probability event identity".into());
            }
        }
        let mut count = 0;
        for route in routes {
            let row: Vec<_> = route.split('\t').collect();
            if row.len() != 5 {
                return Err("invalid route row".into());
            }
            let source = row[2].parse::<u8>().map_err(|e| e.to_string())?;
            let target = row[3].parse::<u8>().map_err(|e| e.to_string())?;
            let probability = row[4].parse::<f64>().map_err(|e| e.to_string())?;
            if source >= 64
                || target >= 64
                || source == target
                || !probability.is_finite()
                || probability <= 0.0
                || probability > 1.0
            {
                return Err("invalid route areas or probability".into());
            }
            let entry = expected
                .get_mut(&(row[0], row[1]))
                .ok_or("route has no matching event")?;
            if !entry.3.insert((source, target)) {
                return Err("duplicate candidate route".into());
            }
            entry.1 += 1;
            entry.2 += probability;
            count += 1;
        }
        for (expected_count, count, sum, _) in expected.values() {
            if expected_count != count || (*count > 0 && (sum - 1.0).abs() > 1e-12) {
                return Err("conditional route counts or probability sum mismatch".into());
            }
        }
        if fields[2] != count.to_string() {
            return Err("route manifest row count mismatch".into());
        }
        total += count;
    }
    if lines.next().is_some() || complete.get("route_rows") != Some(&total.to_string()) {
        return Err("route completion counts mismatch".into());
    }
    Ok(())
}

fn validate_chunk_names(output: &Path, chunks: usize, probabilities: bool) -> Result<()> {
    for entry in fs::read_dir(output.join("shards")).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let name = path.file_name().unwrap().to_string_lossy();
        let prefix = if probabilities && name.starts_with("route-candidates-") {
            "route-candidates-"
        } else {
            "sources-"
        };
        let index = name
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(".tsv"))
            .and_then(|s| s.parse::<usize>().ok());
        if !index.is_some_and(|i| i < chunks && name == format!("{prefix}{i:08}.tsv")) {
            return Err(format!("unexpected source chunk {}", path.display()));
        }
    }
    Ok(())
}

fn resolved_destination(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return path.canonicalize().map_err(|e| e.to_string());
    }
    let parent = path.parent().ok_or("invalid output parent")?;
    Ok(resolved_destination(parent)?.join(path.file_name().ok_or("invalid output directory")?))
}

pub fn run(
    mut config: Config,
    cancellation: Option<biogeo_core::ExecutionCancellationToken>,
) -> Result<String> {
    let check_cancelled = || {
        if cancellation
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
        {
            Err(
                "source allocation cancelled; committed chunks are retained for --resume"
                    .to_string(),
            )
        } else {
            Ok(())
        }
    };
    check_cancelled()?;
    let input = Input::load(&config)?;
    check_cancelled()?;
    if config.inspect {
        config.probabilities = match keys(&read(&config.output.join("metadata.tsv"))?)?
            .get("format")
            .map(String::as_str)
        {
            Some(FORMAT) => false,
            Some(PROBABILITY_FORMAT) => true,
            _ => return Err("unsupported source attribution format".into()),
        };
    }
    let seed = if config.probabilities {
        0
    } else {
        match config.seed {
            Some(seed) => seed,
            None => keys(&read(&config.output.join("metadata.tsv"))?)?
                .get("source_seed")
                .ok_or("missing source_seed")?
                .parse::<u64>()
                .map_err(|e| e.to_string())?,
        }
    };
    let expected = metadata(&input, seed, config.probabilities);
    if !config.inspect
        && !config.resume
        && config.output.exists()
        && fs::read_dir(&config.output)
            .map_err(|e| e.to_string())?
            .next()
            .is_some()
    {
        return Err(
            "output directory must be empty; use --resume only for matching source allocation"
                .into(),
        );
    }
    if config.inspect || config.resume {
        if read(&config.output.join("metadata.tsv"))? != expected {
            return Err("source metadata, seed, algorithm or input binding mismatch".into());
        }
    } else {
        let absolute = std::env::current_dir()
            .map_err(|e| e.to_string())?
            .join(&config.output);
        let resolved = resolved_destination(&absolute)?;
        for source in [&config.fit, &config.bsm] {
            if resolved.starts_with(source.canonicalize().map_err(|e| e.to_string())?) {
                return Err("source allocation output must be outside the read-only fit and BSM directories".into());
            }
        }
        fs::create_dir_all(config.output.join("shards")).map_err(|e| e.to_string())?;
        commit(&config.output.join("metadata.tsv"), &expected, false)?;
    }
    let threads = config
        .threads
        .unwrap_or_else(|| std::thread::available_parallelism().map_or(1, usize::from))
        .min(config.chunk_rows());
    let (rows, undetermined) = if config.inspect && !config.deep {
        validate_output(&config, &expected)?
    } else {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| e.to_string())?;
        let mut writer = Writer {
            config: &config,
            input: &input,
            pool,
            seed,
            pending: Vec::with_capacity(config.chunk_rows()),
            files: format!("{FILE_HEADER}\n"),
            route_files: format!("{FILE_HEADER}\n"),
            route_rows: 0,
            chunks: 0,
            rows: 0,
            statuses: BTreeMap::new(),
            reasons: BTreeMap::new(),
        };
        visit_events(&input, |event| {
            if writer.pending.is_empty() {
                check_cancelled()?;
            }
            writer.pending.push(event);
            if writer.pending.len() == config.chunk_rows() {
                writer.flush()?;
            }
            Ok(())
        })?;
        check_cancelled()?;
        writer.finish(&expected)?
    };
    let seed = if config.probabilities {
        "NA".into()
    } else {
        seed.to_string()
    };
    Ok(format!(
        "format\t{}\nstatus\t{}\nsource_seed\t{seed}\nevents\t{rows}\nundetermined_events\t{undetermined}\nactual_workers\t{}\ndeep\t{}\n",
        if config.inspect {
            if config.probabilities {
                PROBABILITY_INSPECT_FORMAT
            } else {
                INSPECT_FORMAT
            }
        } else {
            if config.probabilities {
                PROBABILITY_RUN_FORMAT
            } else {
                RUN_FORMAT
            }
        },
        if config.inspect { "valid" } else { "completed" },
        if config.inspect && !config.deep {
            0
        } else {
            threads
        },
        config.deep
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probability_mode_requires_no_seed_and_sample_mode_still_requires_one() {
        let base = [
            "--analysis-result",
            "fit",
            "--bsm-result",
            "bsm",
            "--output-dir",
            "out",
        ];
        let parse_extra = |extra: &[&str]| {
            parse(
                base.iter()
                    .copied()
                    .chain(extra.iter().copied())
                    .map(str::to_owned),
                false,
            )
        };
        assert!(parse_extra(&[]).is_err());
        assert!(
            parse_extra(&["--allocation-mode", "probabilities"])
                .unwrap()
                .probabilities
        );
        assert!(parse_extra(&["--allocation-mode", "probabilities", "--seed", "9"]).is_err());
        assert!(
            !parse_extra(&["--allocation-mode", "sample", "--seed", "9"])
                .unwrap()
                .probabilities
        );
        assert!(parse_extra(&["--allocation-mode", "unknown"]).is_err());
    }
    #[test]
    fn cancellation_before_start_does_not_touch_inputs_or_output() {
        let config = parse(
            [
                "--analysis-result",
                "missing-fit",
                "--bsm-result",
                "missing-bsm",
                "--output-dir",
                "missing-output",
                "--seed",
                "1",
            ]
            .map(str::to_string),
            false,
        )
        .unwrap();
        let cancellation = biogeo_core::ExecutionCancellationToken::new();
        cancellation.cancel();
        assert!(
            run(config, Some(cancellation))
                .unwrap_err()
                .contains("cancelled")
        );
    }
}
