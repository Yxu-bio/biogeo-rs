use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use biogeo_core::{NodeStatePosterior, ParsedNewickTree, SplitScenarioPosterior, StateSpace};

use crate::analysis_result::{
    LoadedAnalysisResult, decode_field, encode_field, stable_fingerprint,
};

pub const FORMAT: &str = "biogeo-single-model-posteriors-v1";
pub const DIRECTORY: &str = "posteriors";
const TOLERANCE: f64 = 1e-8;
const NODES: &str =
    "node_id\tkind\tlabel\tlabel_present\tis_root\tis_direct_ancestor\tfossil_status\tage";
const EDGES: &str = "edge_id\tparent_id\tchild_id\tlength\tchild_order\tis_direct_ancestor_hook";
const AREAS: &str = "area_index\tarea";
const STATES: &str = "state_index\trange_bits\trange\tis_null";
const PROBABILITIES: &str = "node_id\tstate_index\tprobability";
const SPLITS: &str = "node_id\tleft_node_id\tright_node_id\tancestor_state_index\tleft_state_index\tright_state_index\tevent\tprobability";

pub struct PosteriorInput<'a> {
    pub tree: &'a ParsedNewickTree,
    pub area_names: &'a [String],
    pub states: &'a StateSpace,
    pub ancestral: Option<&'a [NodeStatePosterior]>,
    pub splits: Option<&'a [SplitScenarioPosterior]>,
}

// The registered posterior artifact is committed together with its parent fitted result.
pub fn write(
    parent: &Path,
    input: &PosteriorInput<'_>,
    fit: &LoadedAnalysisResult,
) -> io::Result<()> {
    let root = parent.join(DIRECTORY);
    fs::create_dir(&root)?;
    let tree = &input.tree.tree;
    let ages = tree.node_ages_from_present();
    let age_tolerance = biogeo_core::default_tip_age_tolerance(ages[tree.root()]);
    let mut labels = vec![None; tree.node_count()];
    for tip in &input.tree.tip_labels {
        labels[tip.node] = Some(tip.label.as_str());
    }
    for node in &input.tree.internal_node_labels {
        labels[node.node] = Some(node.label.as_str());
    }
    let hooks: BTreeSet<_> = tree.direct_ancestor_hook_edges().iter().copied().collect();
    let hook_tips: BTreeSet<_> = hooks.iter().map(|edge| tree.edges()[*edge].child).collect();
    let mut files = Vec::new();
    save_table(&root, "nodes.tsv", NODES, &mut files, |out| {
        for node in 0..tree.node_count() {
            let kind = if tree.is_tip(node) { "tip" } else { "internal" };
            let fossil = if hook_tips.contains(&node) {
                "direct_ancestor_tip"
            } else if tree.is_tip(node) && ages[node] > age_tolerance {
                "non_extant_tip"
            } else {
                "not_identified"
            };
            writeln!(
                out,
                "{node}\t{kind}\t{}\t{}\t{}\t{}\t{fossil}\t{}",
                encode_field(labels[node].unwrap_or("")),
                labels[node].is_some(),
                node == tree.root(),
                tree.is_direct_ancestor_node(node),
                ages[node]
            )?;
        }
        Ok(())
    })?;
    let mut child_orders = vec![0; tree.node_count()];
    for node in 0..tree.node_count() {
        for (order, child) in tree.children(node).unwrap().iter().enumerate() {
            child_orders[child.node] = order;
        }
    }
    save_table(&root, "edges.tsv", EDGES, &mut files, |out| {
        for (id, edge) in tree.edges().iter().enumerate() {
            writeln!(
                out,
                "{id}\t{}\t{}\t{}\t{}\t{}",
                edge.parent,
                edge.child,
                edge.length,
                child_orders[edge.child],
                hooks.contains(&id)
            )?;
        }
        Ok(())
    })?;
    save_table(&root, "areas.tsv", AREAS, &mut files, |out| {
        for (id, name) in input.area_names.iter().enumerate() {
            writeln!(out, "{id}\t{}", encode_field(name))?;
        }
        Ok(())
    })?;
    save_table(&root, "states.tsv", STATES, &mut files, |out| {
        for (id, state) in input.states.states().iter().enumerate() {
            let label = range_label(state.bits(), input.area_names);
            writeln!(
                out,
                "{id}\t{}\t{}\t{}",
                state.bits(),
                encode_field(&label),
                state.is_empty()
            )?;
        }
        Ok(())
    })?;
    if let Some(posteriors) = input.ancestral {
        save_table(
            &root,
            "node_posteriors.tsv",
            PROBABILITIES,
            &mut files,
            |out| {
                for posterior in posteriors {
                    if tree.is_tip(posterior.node) {
                        continue;
                    }
                    for (state, probability) in posterior.probabilities.iter().enumerate() {
                        writeln!(out, "{}\t{state}\t{probability}", posterior.node)?;
                    }
                }
                Ok(())
            },
        )?;
    }
    if let Some(splits) = input.splits {
        save_table(&root, "split_posteriors.tsv", SPLITS, &mut files, |out| {
            for split in splits {
                let children = tree.children(split.node).unwrap();
                check(
                    children.len() == 2 && !tree.is_direct_ancestor_node(split.node),
                    "invalid split node",
                )?;
                let event = crate::model_average::classify_split_event(
                    split.node,
                    input
                        .states
                        .get(split.ancestor)
                        .ok_or_else(|| invalid("unknown ancestor state"))?
                        .bits(),
                    input
                        .states
                        .get(split.left)
                        .ok_or_else(|| invalid("unknown left state"))?
                        .bits(),
                    input
                        .states
                        .get(split.right)
                        .ok_or_else(|| invalid("unknown right state"))?
                        .bits(),
                )
                .map_err(invalid)?
                .as_str();
                writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{event}\t{}",
                    split.node,
                    children[0].node,
                    children[1].node,
                    split.ancestor,
                    split.left,
                    split.right,
                    split.probability
                )?;
            }
            Ok(())
        })?;
    }
    let mut manifest = String::from("file\tbytes\tfingerprint\n");
    for (name, bytes, fingerprint) in &files {
        use std::fmt::Write as _;
        writeln!(manifest, "{name}\t{bytes}\t{fingerprint}").unwrap();
    }
    fs::write(root.join("files.tsv"), &manifest)?;
    let area_bytes = fs::read(root.join("areas.tsv"))?;
    let mut state_identity = area_bytes;
    state_identity.extend(fs::read(root.join("states.tsv"))?);
    let metadata = format!(
        "key\tvalue\nformat\t{FORMAT}\nstatus\tcomplete\nengine_version\t{}\nmodel\tBIOGEOBEARS_LIKE_CONFIGURABLE\nmodel_fingerprint\t{}\nanalysis_result\t..\nanalysis_result_format\t{}\nanalysis_result_fingerprint\t{}\ntree_fingerprint\t{}\ninputs_fingerprint\t{}\nstate_space_fingerprint\t{}\nfiles_fingerprint\t{}\ninclude_null_range\t{}\nmax_range_size\t{}\nroot_prior\t{}\nroot_node_id\t{}\nnodes\t{}\ntips\t{}\nareas\t{}\nstates\t{}\nnode_posteriors\t{}\nsplit_posteriors\t{}\nposterior_location\tbranch_top_at_node\nnode_probability_rows\tinternal_nodes_all_states\nsplit_probability_rows\tcladogenesis_nodes_only\nordered_daughters\tinput_tree_child_order\nnode_id_scope\tthis_fitted_result\nfield_encoding\tpercent_for_percent_tab_cr_lf\nprobability_tolerance\t{TOLERANCE}\ntip_age_tolerance\t{age_tolerance}\nfossil_status_basis\tdirect_ancestor_hook_or_tip_age_not_external_annotation\n",
        env!("CARGO_PKG_VERSION"),
        fit.manifest.model_fingerprint,
        fit.format_version,
        fit.fingerprint,
        fit.manifest.inputs["tree"].fingerprint,
        stable_fingerprint(&fs::read(parent.join("inputs.tsv"))?),
        stable_fingerprint(&state_identity),
        stable_fingerprint(manifest.as_bytes()),
        input.states.include_null_range(),
        input.states.max_range_size(),
        fit.manifest.root_prior,
        tree.root(),
        tree.node_count(),
        tree.tip_nodes().len(),
        input.area_names.len(),
        input.states.len(),
        input.ancestral.is_some(),
        input.splits.is_some()
    );
    fs::write(root.join("metadata.tsv"), metadata)?;
    verify(parent, fit)
}

fn range_label(bits: u64, areas: &[String]) -> String {
    if bits == 0 {
        return "null".to_string();
    }
    areas
        .iter()
        .enumerate()
        .filter(|(i, _)| bits & (1u64 << i) != 0)
        .map(|(_, name)| name.as_str())
        .collect::<Vec<_>>()
        .join("+")
}

fn save_table(
    root: &Path,
    name: &'static str,
    header: &str,
    files: &mut Vec<(&'static str, u64, String)>,
    write_rows: impl FnOnce(&mut BufWriter<File>) -> io::Result<()>,
) -> io::Result<()> {
    let path = root.join(name);
    let mut out = BufWriter::new(File::create(&path)?);
    writeln!(out, "{header}")?;
    write_rows(&mut out)?;
    out.flush()?;
    let bytes = fs::read(path)?;
    files.push((name, bytes.len() as u64, stable_fingerprint(&bytes)));
    Ok(())
}

fn invalid(message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("single-model posteriors: {message}"),
    )
}

fn check(condition: bool, message: &str) -> io::Result<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

fn number<T: std::str::FromStr>(value: &str) -> io::Result<T> {
    value
        .parse()
        .map_err(|_| invalid(format!("invalid numeric or boolean value {value:?}")))
}

fn rows(
    path: &Path,
    header: &str,
    mut visit: impl FnMut(&[&str]) -> io::Result<()>,
) -> io::Result<()> {
    let mut input = BufReader::new(File::open(path)?);
    let mut line = String::new();
    input.read_line(&mut line)?;
    check(
        line.trim_end_matches(['\r', '\n']) == header,
        &format!("invalid header in {}", path.display()),
    )?;
    let columns = header.split('\t').count();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            break;
        }
        let fields = line
            .trim_end_matches(['\r', '\n'])
            .split('\t')
            .collect::<Vec<_>>();
        check(fields.len() == columns, "unexpected table column count")?;
        visit(&fields)?;
    }
    Ok(())
}

pub fn verify(parent: &Path, fit: &LoadedAnalysisResult) -> io::Result<()> {
    let root = parent.join(DIRECTORY);
    check(
        root.try_exists()? == fit.posteriors_available,
        "posterior directory availability mismatch",
    )?;
    if !fit.posteriors_available {
        return Ok(());
    }
    let mut metadata = BTreeMap::new();
    rows(&root.join("metadata.tsv"), "key\tvalue", |r| {
        check(
            metadata
                .insert(r[0].to_string(), r[1].to_string())
                .is_none(),
            "duplicate metadata key",
        )
    })?;
    let expected_keys = BTreeSet::from([
        "format",
        "status",
        "engine_version",
        "model",
        "model_fingerprint",
        "analysis_result",
        "analysis_result_format",
        "analysis_result_fingerprint",
        "tree_fingerprint",
        "inputs_fingerprint",
        "state_space_fingerprint",
        "files_fingerprint",
        "include_null_range",
        "max_range_size",
        "root_prior",
        "root_node_id",
        "nodes",
        "tips",
        "areas",
        "states",
        "node_posteriors",
        "split_posteriors",
        "posterior_location",
        "node_probability_rows",
        "split_probability_rows",
        "ordered_daughters",
        "node_id_scope",
        "field_encoding",
        "probability_tolerance",
        "tip_age_tolerance",
        "fossil_status_basis",
    ]);
    check(
        metadata.keys().map(String::as_str).collect::<BTreeSet<_>>() == expected_keys,
        "missing or unknown posterior metadata fields",
    )?;
    let get = |key: &str| -> io::Result<&str> {
        metadata
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| invalid(format!("missing metadata {key}")))
    };
    check(!get("engine_version")?.is_empty(), "missing engine version")?;
    for (key, expected) in [
        ("format", FORMAT),
        ("status", "complete"),
        ("analysis_result", ".."),
        ("model", "BIOGEOBEARS_LIKE_CONFIGURABLE"),
        ("analysis_result_format", fit.format_version.as_str()),
        ("analysis_result_fingerprint", fit.fingerprint.as_str()),
        ("model_fingerprint", fit.manifest.model_fingerprint.as_str()),
        (
            "tree_fingerprint",
            fit.manifest.inputs["tree"].fingerprint.as_str(),
        ),
        ("root_prior", fit.manifest.root_prior.as_str()),
        ("posterior_location", "branch_top_at_node"),
        ("node_probability_rows", "internal_nodes_all_states"),
        ("split_probability_rows", "cladogenesis_nodes_only"),
        ("ordered_daughters", "input_tree_child_order"),
        ("node_id_scope", "this_fitted_result"),
        ("field_encoding", "percent_for_percent_tab_cr_lf"),
        (
            "fossil_status_basis",
            "direct_ancestor_hook_or_tip_age_not_external_annotation",
        ),
    ] {
        check(get(key)? == expected, &format!("metadata mismatch: {key}"))?;
    }
    check(
        get("inputs_fingerprint")? == stable_fingerprint(&fs::read(parent.join("inputs.tsv"))?),
        "input identity mismatch",
    )?;
    check(
        number::<bool>(get("include_null_range")?)? == fit.manifest.include_null_range,
        "null range mismatch",
    )?;
    check(
        number::<u8>(get("max_range_size")?)? == fit.manifest.max_range_size,
        "max range mismatch",
    )?;
    check(
        number::<f64>(get("probability_tolerance")?)? == TOLERANCE,
        "unsupported probability tolerance",
    )?;
    let ancestral = number::<bool>(get("node_posteriors")?)?;
    let splits = number::<bool>(get("split_posteriors")?)?;
    check(ancestral || splits, "empty posterior artifact")?;
    let mut expected_files = BTreeSet::from(["nodes.tsv", "edges.tsv", "areas.tsv", "states.tsv"]);
    if ancestral {
        expected_files.insert("node_posteriors.tsv");
    }
    if splits {
        expected_files.insert("split_posteriors.tsv");
    }
    let mut expected_entries = expected_files
        .iter()
        .map(|name| (*name).to_string())
        .collect::<BTreeSet<_>>();
    expected_entries.extend(["metadata.tsv".to_string(), "files.tsv".to_string()]);
    let actual_entries = fs::read_dir(&root)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<io::Result<BTreeSet<_>>>()?;
    check(
        actual_entries == expected_entries,
        "missing or unknown posterior directory entries",
    )?;
    let files = fs::read(root.join("files.tsv"))?;
    check(
        get("files_fingerprint")? == stable_fingerprint(&files),
        "file manifest fingerprint mismatch",
    )?;
    rows(&root.join("files.tsv"), "file\tbytes\tfingerprint", |r| {
        check(
            expected_files.remove(r[0]),
            "unknown or duplicate posterior file",
        )?;
        let contents = fs::read(root.join(r[0]))?;
        check(
            contents.len() as u64 == number::<u64>(r[1])? && stable_fingerprint(&contents) == r[2],
            "posterior file changed",
        )
    })?;
    check(
        expected_files.is_empty(),
        "incomplete posterior file manifest",
    )?;
    check(
        ancestral || !root.join("node_posteriors.tsv").exists(),
        "undeclared node posteriors",
    )?;
    check(
        splits || !root.join("split_posteriors.tsv").exists(),
        "undeclared split posteriors",
    )?;
    let mut state_identity = fs::read(root.join("areas.tsv"))?;
    state_identity.extend(fs::read(root.join("states.tsv"))?);
    check(
        get("state_space_fingerprint")? == stable_fingerprint(&state_identity),
        "state space identity mismatch",
    )?;
    let mut areas = Vec::new();
    rows(&root.join("areas.tsv"), AREAS, |r| {
        check(
            number::<usize>(r[0])? == areas.len(),
            "non-contiguous area index",
        )?;
        areas.push(decode_field(r[1]).map_err(invalid)?);
        Ok(())
    })?;
    check(
        areas.len() == fit.manifest.areas && areas.len() == number::<usize>(get("areas")?)?,
        "area count mismatch",
    )?;
    check(
        areas.iter().collect::<BTreeSet<_>>().len() == areas.len(),
        "duplicate area names",
    )?;
    let state_space = StateSpace::new(
        u8::try_from(areas.len()).map_err(invalid)?,
        fit.manifest.max_range_size,
        fit.manifest.include_null_range,
    )
    .map_err(invalid)?;
    check(
        state_space.len() == fit.manifest.states
            && state_space.len() == number::<usize>(get("states")?)?,
        "state count mismatch",
    )?;
    let mut state_count = 0;
    rows(&root.join("states.tsv"), STATES, |r| {
        let state = state_space
            .get(state_count)
            .ok_or_else(|| invalid("extra state"))?;
        check(
            number::<usize>(r[0])? == state_count && number::<u64>(r[1])? == state.bits(),
            "state index or bitset mismatch",
        )?;
        check(
            decode_field(r[2]).map_err(invalid)? == range_label(state.bits(), &areas),
            "state display name mismatch",
        )?;
        check(
            number::<bool>(r[3])? == state.is_empty(),
            "null state marker mismatch",
        )?;
        state_count += 1;
        Ok(())
    })?;
    check(state_count == state_space.len(), "missing states")?;
    let count = number::<usize>(get("nodes")?)?;
    let root_id = number::<usize>(get("root_node_id")?)?;
    let mut nodes = Vec::new();
    let mut tip_names = BTreeSet::new();
    rows(&root.join("nodes.tsv"), NODES, |r| {
        let node = number::<usize>(r[0])?;
        check(
            node == nodes.len(),
            "node IDs must be unique and contiguous",
        )?;
        check(r[1] == "tip" || r[1] == "internal", "unknown node kind")?;
        let label = decode_field(r[2]).map_err(invalid)?;
        let label_present = number::<bool>(r[3])?;
        check(label_present || label.is_empty(), "absent label has text")?;
        if r[1] == "tip" {
            check(
                label_present && tip_names.insert(label),
                "missing or duplicate original tip label",
            )?;
        }
        check(
            number::<bool>(r[4])? == (node == root_id),
            "root marker mismatch",
        )?;
        let age = number::<f64>(r[7])?;
        check(age.is_finite() && age >= 0.0, "invalid node age")?;
        nodes.push((r[1] == "tip", number::<bool>(r[5])?, r[6].to_string(), age));
        Ok(())
    })?;
    check(nodes.len() == count && count > 0, "node count mismatch")?;
    check(
        tip_names.len() == fit.manifest.tips && tip_names.len() == number::<usize>(get("tips")?)?,
        "tip count mismatch",
    )?;
    let mut edges = Vec::new();
    let mut edge_info = Vec::new();
    rows(&root.join("edges.tsv"), EDGES, |r| {
        check(
            number::<usize>(r[0])? == edges.len(),
            "edge IDs must be unique and contiguous",
        )?;
        edges.push(biogeo_core::Edge {
            parent: number(r[1])?,
            child: number(r[2])?,
            length: number(r[3])?,
        });
        edge_info.push((number::<usize>(r[4])?, number::<bool>(r[5])?));
        Ok(())
    })?;
    let tree = biogeo_core::Tree::new(root_id, count, edges)
        .map_err(invalid)?
        .with_direct_ancestor_hooks_below(fit.manifest.min_branch_length)
        .map_err(invalid)?;
    let mut child_orders = vec![0; count];
    for node in 0..count {
        for (order, child) in tree.children(node).unwrap().iter().enumerate() {
            child_orders[child.node] = order;
        }
    }
    let hooks: BTreeSet<_> = tree.direct_ancestor_hook_edges().iter().copied().collect();
    let hook_tips: BTreeSet<_> = hooks.iter().map(|edge| tree.edges()[*edge].child).collect();
    for (id, edge) in tree.edges().iter().enumerate() {
        check(
            edge_info[id] == (child_orders[edge.child], hooks.contains(&id)),
            "child order or hook marker mismatch",
        )?;
    }
    let ages = tree.node_ages_from_present();
    let age_tolerance = biogeo_core::default_tip_age_tolerance(ages[root_id]);
    check(
        number::<f64>(get("tip_age_tolerance")?)? == age_tolerance,
        "tip age tolerance mismatch",
    )?;
    for (id, node) in nodes.iter().enumerate() {
        check(
            node.0 == tree.is_tip(id) && node.1 == tree.is_direct_ancestor_node(id),
            "node kind or direct ancestor mismatch",
        )?;
        check(node.3 == ages[id], "node age disagrees with edges")?;
        let fossil = if hook_tips.contains(&id) {
            "direct_ancestor_tip"
        } else if node.0 && ages[id] > age_tolerance {
            "non_extant_tip"
        } else {
            "not_identified"
        };
        check(node.2 == fossil, "fossil status disagrees with tree")?;
    }
    if ancestral {
        let mut counts = vec![0usize; count];
        let mut sums = vec![0.0; count];
        let mut previous = None;
        rows(&root.join("node_posteriors.tsv"), PROBABILITIES, |r| {
            let node = number::<usize>(r[0])?;
            let state = number::<usize>(r[1])?;
            check(
                node < count && !tree.is_tip(node) && state < state_count,
                "unknown posterior node or state",
            )?;
            check(
                previous.is_none_or(|p| p < (node, state)),
                "duplicate or unordered node posterior",
            )?;
            previous = Some((node, state));
            sums[node] += probability(r[2])?;
            counts[node] += 1;
            Ok(())
        })?;
        for node in tree.postorder_internal_nodes() {
            check(
                counts[*node] == state_count,
                "incomplete internal node probabilities",
            )?;
            check(
                (sums[*node] - 1.0).abs() <= TOLERANCE,
                "node probabilities do not sum to one",
            )?;
        }
    }
    if splits {
        let mut seen = BTreeSet::new();
        let mut sums = vec![0.0; count];
        rows(&root.join("split_posteriors.tsv"), SPLITS, |r| {
            let node = number::<usize>(r[0])?;
            check(
                node < count && !tree.is_tip(node) && !tree.is_direct_ancestor_node(node),
                "unknown split node",
            )?;
            let children = tree.children(node).unwrap();
            check(
                children.len() == 2
                    && number::<usize>(r[1])? == children[0].node
                    && number::<usize>(r[2])? == children[1].node,
                "split daughter order mismatch",
            )?;
            let ancestor = number::<usize>(r[3])?;
            let left = number::<usize>(r[4])?;
            let right = number::<usize>(r[5])?;
            check(
                ancestor < state_count && left < state_count && right < state_count,
                "unknown split state",
            )?;
            check(
                seen.insert((node, ancestor, left, right)),
                "duplicate split scenario",
            )?;
            let event = crate::model_average::classify_split_event(
                node,
                state_space.get(ancestor).unwrap().bits(),
                state_space.get(left).unwrap().bits(),
                state_space.get(right).unwrap().bits(),
            )
            .map_err(invalid)?;
            check(r[6] == event.as_str(), "split event type mismatch")?;
            sums[node] += probability(r[7])?;
            Ok(())
        })?;
        for node in tree
            .postorder_internal_nodes()
            .iter()
            .filter(|n| !tree.is_direct_ancestor_node(**n))
        {
            check(
                (sums[*node] - 1.0).abs() <= TOLERANCE,
                "split probabilities do not sum to one",
            )?;
        }
    }
    Ok(())
}

fn probability(value: &str) -> io::Result<f64> {
    let probability = number::<f64>(value)?;
    check(
        probability.is_finite() && (0.0..=1.0 + TOLERANCE).contains(&probability),
        "invalid probability",
    )?;
    Ok(probability)
}
