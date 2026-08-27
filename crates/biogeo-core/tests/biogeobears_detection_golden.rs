use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use biogeo_core::{
    BioGeoBearsPreset, DetectionModel, LikelihoodEngine, ModelConfig, RootPrior, StateSpace,
    parse_detection_data, parse_newick,
};

const MANIFEST: &str = "validation/detection_profile_fixtures.tsv";
const PROFILE_GOLDEN: &str = "validation/golden/biogeobears-detection-profile.tsv";
const TIP_GOLDEN: &str = "validation/golden/biogeobears-detection-tip-likelihoods.tsv";

#[derive(Clone, Debug)]
struct Fixture {
    case_id: String,
    tree: String,
    detections: String,
    controls: String,
    d: f64,
    e: f64,
    mf: f64,
    dp: f64,
    fdp: f64,
    max_range_size: u8,
    include_null_range: bool,
}

impl Fixture {
    fn from_row(row: &HashMap<String, String>) -> Self {
        assert_eq!(required(row, "root_prior"), "flat");
        Self {
            case_id: required(row, "case_id").to_owned(),
            tree: required(row, "tree").to_owned(),
            detections: required(row, "detections").to_owned(),
            controls: required(row, "controls").to_owned(),
            d: number(row, "d"),
            e: number(row, "e"),
            mf: number(row, "mf"),
            dp: number(row, "dp"),
            fdp: number(row, "fdp"),
            max_range_size: required(row, "max_range_size").parse().unwrap(),
            include_null_range: required(row, "include_null_range") == "true",
        }
    }
}

#[test]
fn official_psychotria_detection_tip_likelihoods_and_tree_likelihood_match_biogeobears() {
    let root = repo_root();
    let fixtures = read_tsv(&root.join(MANIFEST))
        .iter()
        .map(Fixture::from_row)
        .collect::<Vec<_>>();
    let profile = read_tsv(&root.join(PROFILE_GOLDEN))
        .into_iter()
        .map(|row| {
            (
                required(&row, "case_id").to_owned(),
                number(&row, "biogeobears_lnL"),
            )
        })
        .collect::<HashMap<_, _>>();
    let tip_golden = read_tsv(&root.join(TIP_GOLDEN))
        .into_iter()
        .map(|row| {
            (
                (
                    required(&row, "case_id").to_owned(),
                    required(&row, "tip").to_owned(),
                    required(&row, "state_index").parse::<usize>().unwrap(),
                ),
                (
                    required(&row, "state").to_owned(),
                    number(&row, "likelihood"),
                ),
            )
        })
        .collect::<HashMap<_, _>>();

    for fixture in fixtures {
        let tree = parse_newick(&fs::read_to_string(root.join(&fixture.tree)).unwrap()).unwrap();
        let data = parse_detection_data(
            &fs::read_to_string(root.join(&fixture.detections)).unwrap(),
            &fs::read_to_string(root.join(&fixture.controls)).unwrap(),
            &tree,
        )
        .unwrap();
        let states = StateSpace::new(
            data.area_names.len() as u8,
            fixture.max_range_size,
            fixture.include_null_range,
        )
        .unwrap();
        let detection = DetectionModel::new(fixture.mf, fixture.dp, fixture.fdp).unwrap();
        let tip_likelihoods = detection.tip_likelihoods(&data, &states).unwrap();

        for (tip, likelihood) in data.tips.iter().zip(&tip_likelihoods) {
            assert_eq!(tip.node, likelihood.node);
            for (state_index, actual) in likelihood.likelihoods.iter().enumerate() {
                let key = (fixture.case_id.clone(), tip.label.clone(), state_index);
                let (expected_state, expected) = tip_golden
                    .get(&key)
                    .unwrap_or_else(|| panic!("missing tip-likelihood golden for {key:?}"));
                assert_eq!(
                    expected_state,
                    &state_label(states.get(state_index).unwrap(), &data.area_names)
                );
                assert_close(*actual, *expected, 2e-13, &format!("{key:?}"));
            }
        }

        let table = BioGeoBearsPreset::Dec
            .parameter_table()
            .unwrap()
            .with_fixed("d", fixture.d)
            .unwrap()
            .with_fixed("e", fixture.e)
            .unwrap();
        let model =
            ModelConfig::from_biogeobears_core_parameters(&table.resolve_initial().unwrap())
                .unwrap();
        let pruning = LikelihoodEngine::new(&tree.tree, &states, RootPrior::Flat)
            .evaluate(&model, &tip_likelihoods)
            .unwrap();
        assert_close(
            pruning.log_likelihood,
            *profile.get(&fixture.case_id).unwrap(),
            5e-7,
            &fixture.case_id,
        );
    }
}

fn state_label(state: biogeo_core::AreaSet, area_names: &[String]) -> String {
    if state.is_empty() {
        return "null".to_owned();
    }
    area_names
        .iter()
        .enumerate()
        .filter_map(|(area, name)| state.contains(area as u8).then_some(name.as_str()))
        .collect::<Vec<_>>()
        .join("+")
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
