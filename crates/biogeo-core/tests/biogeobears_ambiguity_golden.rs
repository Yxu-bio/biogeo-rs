use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use biogeo_core::{
    DecOptimizationConfig, LikelihoodEngine, ModelConfig, RootPrior, StateSpace,
    optimize_de_with_model_likelihoods, parse_newick, parse_tip_ranges_table_with_ambiguities,
};

const MANIFEST: &str = "validation/ambiguity_fixtures.tsv";
const PROFILE_GOLDEN: &str = "validation/golden/biogeobears-ambiguity-profile.tsv";
const TIP_GOLDEN: &str = "validation/golden/biogeobears-ambiguity-tip-likelihoods.tsv";
const ANCESTRAL_GOLDEN: &str = "validation/golden/biogeobears-ambiguity-ancestral.tsv";
const OPTIMIZATION_GOLDEN: &str = "validation/golden/biogeobears-ambiguity-optim.tsv";
const SOURCE_SEMANTICS_GOLDEN: &str =
    "validation/golden/biogeobears-ambiguity-source-semantics.tsv";

#[derive(Clone, Debug)]
struct Fixture {
    case_id: String,
    tree: String,
    ranges: String,
    d: f64,
    e: f64,
    max_range_size: u8,
    include_null_range: bool,
    likelihood_tolerance: f64,
    posterior_tolerance: f64,
    parameter_tolerance: f64,
}

impl Fixture {
    fn from_row(row: &HashMap<String, String>) -> Self {
        assert_eq!(required(row, "root_prior"), "flat");
        Self {
            case_id: required(row, "case_id").to_owned(),
            tree: required(row, "tree").to_owned(),
            ranges: required(row, "ranges").to_owned(),
            d: number(row, "d"),
            e: number(row, "e"),
            max_range_size: required(row, "max_range_size").parse().unwrap(),
            include_null_range: boolean(row, "include_null_range"),
            likelihood_tolerance: number(row, "lnL_tolerance"),
            posterior_tolerance: number(row, "posterior_tolerance"),
            parameter_tolerance: number(row, "parameter_tolerance"),
        }
    }
}

#[test]
fn source_level_all_unknown_absence_only_and_mixed_semantics_match_biogeobears() {
    let tree = parse_newick("(all_unknown:1,(absence_only:0.5,mixed:0.5):0.5);").unwrap();
    let ranges = parse_tip_ranges_table_with_ambiguities(
        "tip\tA\tB\tC\nall_unknown\t?\t?\t?\nabsence_only\t0\t?\t0\nmixed\t1\t?\t0\n",
        &tree,
    )
    .unwrap();
    let states = StateSpace::new(3, 2, true).unwrap();
    let likelihoods = ranges.tip_likelihoods(&states).unwrap();
    let golden = read_tsv(&repo_path(SOURCE_SEMANTICS_GOLDEN))
        .into_iter()
        .map(|row| {
            (
                (
                    required(&row, "tip").to_owned(),
                    required(&row, "range_bits").parse::<u64>().unwrap(),
                ),
                number(&row, "likelihood"),
            )
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(golden.len(), 3 * states.len());

    let labels_by_node = tree
        .tip_labels
        .iter()
        .map(|tip| (tip.node, tip.label.as_str()))
        .collect::<HashMap<_, _>>();
    for tip in &likelihoods {
        let label = labels_by_node[&tip.node];
        for (state_index, actual) in tip.likelihoods.iter().copied().enumerate() {
            let bits = states.get(state_index).unwrap().bits();
            let expected = golden.get(&(label.to_owned(), bits)).unwrap_or_else(|| {
                panic!("missing source-semantics golden for {label} bits={bits}")
            });
            assert_close(actual, *expected, 0.0, &format!("{label} bits={bits}"));
        }
    }
}

#[test]
fn official_derived_psychotria_tip_likelihoods_lnl_and_posteriors_match_biogeobears() {
    let fixture = load_fixture();
    let root = repo_root();
    let parsed_tree = parse_newick(&fs::read_to_string(root.join(&fixture.tree)).unwrap()).unwrap();
    let parsed_ranges = parse_tip_ranges_table_with_ambiguities(
        &fs::read_to_string(root.join(&fixture.ranges)).unwrap(),
        &parsed_tree,
    )
    .unwrap();
    let states = StateSpace::new(
        parsed_ranges.area_names.len() as u8,
        fixture.max_range_size,
        fixture.include_null_range,
    )
    .unwrap();
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states).unwrap();

    assert_eq!(parsed_ranges.ambiguous_tip_count(), 15);
    assert_eq!(parsed_ranges.unknown_cell_count(), 31);
    assert_eq!(parsed_ranges.all_unknown_tip_count(), 0);
    compare_tip_likelihoods(&fixture, &parsed_tree, &states, &tip_likelihoods);

    let model = ModelConfig::preset_dec(fixture.d, fixture.e).unwrap();
    let engine = LikelihoodEngine::new(&parsed_tree.tree, &states, RootPrior::Flat);
    let pruning = engine.evaluate(&model, &tip_likelihoods).unwrap();
    let profile = only_row(PROFILE_GOLDEN);
    assert_eq!(required(&profile, "case_id"), fixture.case_id);
    assert_eq!(required(&profile, "biogeobears_version"), "1.1.3");
    assert_eq!(required(&profile, "useAmbiguities"), "TRUE");
    assert_close(
        pruning.log_likelihood,
        number(&profile, "biogeobears_lnL"),
        fixture.likelihood_tolerance,
        "Psychotria ambiguous fixed lnL",
    );

    let posteriors = engine.node_state_posteriors(&model, &pruning).unwrap();
    let posterior_by_node = posteriors
        .iter()
        .map(|posterior| (posterior.node, posterior))
        .collect::<HashMap<_, _>>();
    let golden = read_tsv(&repo_path(ANCESTRAL_GOLDEN))
        .into_iter()
        .map(|row| {
            assert_eq!(required(&row, "case_id"), fixture.case_id);
            (
                (
                    required(&row, "clade").to_owned(),
                    required(&row, "range_bits").parse::<u64>().unwrap(),
                ),
                number(&row, "biogeobears_probability"),
            )
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(
        golden.len(),
        parsed_tree.tree.postorder_internal_nodes().len() * states.len()
    );

    for node in parsed_tree.tree.postorder_internal_nodes() {
        let clade = clade_label(&parsed_tree, *node);
        let posterior = posterior_by_node[node];
        for (state_index, actual) in posterior.probabilities.iter().copied().enumerate() {
            let bits = states.get(state_index).unwrap().bits();
            let expected = golden
                .get(&(clade.clone(), bits))
                .unwrap_or_else(|| panic!("missing ancestral golden for {clade} bits={bits}"));
            assert_close(
                actual,
                *expected,
                fixture.posterior_tolerance,
                &format!("ancestral {clade} bits={bits}"),
            );
        }
    }
}

#[test]
fn ambiguous_psychotria_de_optimization_matches_and_cross_evaluates_biogeobears() {
    let fixture = load_fixture();
    let root = repo_root();
    let parsed_tree = parse_newick(&fs::read_to_string(root.join(&fixture.tree)).unwrap()).unwrap();
    let parsed_ranges = parse_tip_ranges_table_with_ambiguities(
        &fs::read_to_string(root.join(&fixture.ranges)).unwrap(),
        &parsed_tree,
    )
    .unwrap();
    let states = StateSpace::new(
        parsed_ranges.area_names.len() as u8,
        fixture.max_range_size,
        fixture.include_null_range,
    )
    .unwrap();
    let tip_likelihoods = parsed_ranges.tip_likelihoods(&states).unwrap();
    let golden = only_row(OPTIMIZATION_GOLDEN);
    assert_eq!(required(&golden, "case_id"), fixture.case_id);
    assert_eq!(required(&golden, "convergence"), "0");
    let bgb_lnl = number(&golden, "biogeobears_lnL");
    let bgb_d = number(&golden, "biogeobears_d");
    let bgb_e = number(&golden, "biogeobears_e");

    let at_bgb_model = ModelConfig::preset_dec(bgb_d, bgb_e).unwrap();
    let at_bgb = LikelihoodEngine::new(&parsed_tree.tree, &states, RootPrior::Flat)
        .evaluate(&at_bgb_model, &tip_likelihoods)
        .unwrap();
    assert_close(
        at_bgb.log_likelihood,
        bgb_lnl,
        fixture.likelihood_tolerance,
        "Rust objective at BioGeoBEARS optimum",
    );

    let config = DecOptimizationConfig {
        tolerance: 1e-9,
        max_iterations: 400,
        ..DecOptimizationConfig::default()
    };
    let optimized = optimize_de_with_model_likelihoods(
        &parsed_tree.tree,
        &states,
        &tip_likelihoods,
        RootPrior::Flat,
        config,
        |d, e| Ok(ModelConfig::preset_dec(d, e)?),
    )
    .unwrap();

    assert!(optimized.converged);
    assert_close(
        optimized.log_likelihood,
        bgb_lnl,
        fixture.likelihood_tolerance,
        "optimized lnL",
    );
    assert_close(
        optimized.d,
        bgb_d,
        fixture.parameter_tolerance,
        "optimized d",
    );
    assert_close(
        optimized.e,
        bgb_e,
        fixture.parameter_tolerance,
        "optimized e",
    );
}

fn compare_tip_likelihoods(
    fixture: &Fixture,
    parsed_tree: &biogeo_core::ParsedNewickTree,
    states: &StateSpace,
    likelihoods: &[biogeo_core::TipLikelihood],
) {
    let golden = read_tsv(&repo_path(TIP_GOLDEN))
        .into_iter()
        .map(|row| {
            assert_eq!(required(&row, "case_id"), fixture.case_id);
            (
                (
                    required(&row, "tip").to_owned(),
                    required(&row, "range_bits").parse::<u64>().unwrap(),
                ),
                number(&row, "likelihood"),
            )
        })
        .collect::<HashMap<_, _>>();
    assert_eq!(golden.len(), parsed_tree.tip_labels.len() * states.len());
    let labels_by_node = parsed_tree
        .tip_labels
        .iter()
        .map(|tip| (tip.node, tip.label.as_str()))
        .collect::<HashMap<_, _>>();

    for tip in likelihoods {
        let label = labels_by_node[&tip.node];
        for (state_index, actual) in tip.likelihoods.iter().copied().enumerate() {
            let bits = states.get(state_index).unwrap().bits();
            let expected = golden
                .get(&(label.to_owned(), bits))
                .unwrap_or_else(|| panic!("missing tip golden for {label} bits={bits}"));
            assert_close(actual, *expected, 0.0, &format!("tip {label} bits={bits}"));
        }
    }
}

fn clade_label(parsed_tree: &biogeo_core::ParsedNewickTree, node: usize) -> String {
    let labels_by_node = parsed_tree
        .tip_labels
        .iter()
        .map(|tip| (tip.node, tip.label.as_str()))
        .collect::<HashMap<_, _>>();
    let mut labels = Vec::new();
    collect_descendant_labels(parsed_tree, node, &labels_by_node, &mut labels);
    labels.sort_unstable();
    labels.join("+")
}

fn collect_descendant_labels<'a>(
    parsed_tree: &'a biogeo_core::ParsedNewickTree,
    node: usize,
    labels_by_node: &HashMap<usize, &'a str>,
    labels: &mut Vec<&'a str>,
) {
    if let Some(label) = labels_by_node.get(&node) {
        labels.push(label);
        return;
    }
    for child in parsed_tree.tree.children(node).unwrap() {
        collect_descendant_labels(parsed_tree, child.node, labels_by_node, labels);
    }
}

fn load_fixture() -> Fixture {
    Fixture::from_row(&only_row(MANIFEST))
}

fn only_row(path: &str) -> HashMap<String, String> {
    let mut rows = read_tsv(&repo_path(path));
    assert_eq!(rows.len(), 1, "expected one row in {path}");
    rows.pop().unwrap()
}

fn assert_close(actual: f64, expected: f64, tolerance: f64, context: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{context}: actual={actual:.17}, expected={expected:.17}, delta={:.3e}, tolerance={tolerance:.3e}",
        (actual - expected).abs()
    );
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn repo_path(path: &str) -> PathBuf {
    repo_root().join(path)
}

fn read_tsv(path: &Path) -> Vec<HashMap<String, String>> {
    let input = fs::read_to_string(path).unwrap();
    let mut lines = input.lines().filter(|line| !line.trim().is_empty());
    let header = lines
        .next()
        .unwrap_or_else(|| panic!("empty TSV: {}", path.display()))
        .split('\t')
        .map(str::to_owned)
        .collect::<Vec<_>>();
    lines
        .map(|line| {
            let fields = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                fields.len(),
                header.len(),
                "invalid TSV row in {}",
                path.display()
            );
            header
                .iter()
                .cloned()
                .zip(fields.into_iter().map(str::to_owned))
                .collect()
        })
        .collect()
}

fn required<'a>(row: &'a HashMap<String, String>, name: &str) -> &'a str {
    row.get(name)
        .unwrap_or_else(|| panic!("missing TSV column {name:?}"))
}

fn number(row: &HashMap<String, String>, name: &str) -> f64 {
    required(row, name)
        .parse()
        .unwrap_or_else(|_| panic!("invalid numeric TSV field {name:?}"))
}

fn boolean(row: &HashMap<String, String>, name: &str) -> bool {
    match required(row, name) {
        "true" => true,
        "false" => false,
        value => panic!("invalid boolean TSV field {name:?}: {value:?}"),
    }
}
