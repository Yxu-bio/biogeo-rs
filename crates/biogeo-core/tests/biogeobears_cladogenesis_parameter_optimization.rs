use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use biogeo_core::{
    BioGeoBearsPreset, LikelihoodEngine, ModelConfig, ParameterBounds, ParameterOptimizationConfig,
    ParameterTable, RootPrior, StateSpace, optimize_parameter_table, parse_newick,
    parse_tip_ranges_table, tip_ranges_to_likelihoods,
};

const MANIFEST: &str = "validation/cladogenesis_parameter_optimization_fixtures.tsv";
const OPTIMIZATION_GOLDEN: &str = "validation/golden/biogeobears-cladogenesis-parameter-optim.tsv";
const PROFILE_GOLDEN: &str = "validation/golden/biogeobears-cladogenesis-parameter-profile.tsv";

#[derive(Clone, Debug)]
struct Fixture {
    case_id: String,
    tree: String,
    ranges: String,
    free_parameter: String,
    d: f64,
    e: f64,
    j: f64,
    y: f64,
    s: f64,
    v: f64,
    mx01: f64,
    mx01y: f64,
    mx01s: f64,
    mx01v: f64,
    mx01j: f64,
    initial: f64,
    min: f64,
    max: f64,
    starts: Vec<f64>,
    profile_values: Vec<f64>,
    max_range_size: u8,
    include_null_range: bool,
    likelihood_tolerance: f64,
    parameter_tolerance: f64,
}

impl Fixture {
    fn from_row(row: &HashMap<String, String>) -> Self {
        assert_eq!(required(row, "root_prior"), "flat");
        Self {
            case_id: required(row, "case_id").to_owned(),
            tree: required(row, "tree").to_owned(),
            ranges: required(row, "ranges").to_owned(),
            free_parameter: required(row, "free_parameter").to_owned(),
            d: number(row, "d"),
            e: number(row, "e"),
            j: number(row, "j"),
            y: number(row, "y"),
            s: number(row, "s"),
            v: number(row, "v"),
            mx01: number(row, "mx01"),
            mx01y: number(row, "mx01y"),
            mx01s: number(row, "mx01s"),
            mx01v: number(row, "mx01v"),
            mx01j: number(row, "mx01j"),
            initial: number(row, "init"),
            min: number(row, "min"),
            max: number(row, "max"),
            starts: number_list(row, "starts"),
            profile_values: number_list(row, "profile_values"),
            max_range_size: required(row, "max_range_size").parse().unwrap(),
            include_null_range: boolean(row, "include_null_range"),
            likelihood_tolerance: number(row, "lnL_tolerance"),
            parameter_tolerance: number(row, "parameter_tolerance"),
        }
    }
}

#[derive(Clone, Debug)]
struct OptimizationGolden {
    case_id: String,
    free_parameter: String,
    log_likelihood: f64,
    estimate: f64,
    candidate_source: String,
    convergence: i32,
}

impl OptimizationGolden {
    fn from_row(row: &HashMap<String, String>) -> Self {
        Self {
            case_id: required(row, "case_id").to_owned(),
            free_parameter: required(row, "free_parameter").to_owned(),
            log_likelihood: number(row, "biogeobears_lnL"),
            estimate: number(row, "biogeobears_estimate"),
            candidate_source: required(row, "candidate_source").to_owned(),
            convergence: required(row, "convergence").parse().unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
struct ProfileGolden {
    case_id: String,
    free_parameter: String,
    value: f64,
    log_likelihood: f64,
}

impl ProfileGolden {
    fn from_row(row: &HashMap<String, String>) -> Self {
        Self {
            case_id: required(row, "case_id").to_owned(),
            free_parameter: required(row, "free_parameter").to_owned(),
            value: number(row, "value"),
            log_likelihood: number(row, "biogeobears_lnL"),
        }
    }
}

#[test]
fn cladogenesis_parameter_profiles_match_biogeobears() {
    let fixtures = load_fixtures();
    let profiles = read_tsv(&repo_path(PROFILE_GOLDEN))
        .iter()
        .map(ProfileGolden::from_row)
        .collect::<Vec<_>>();

    assert_eq!(fixtures.len(), 8);
    let expected_profiles = fixtures
        .values()
        .map(|fixture| profile_values(fixture).len())
        .sum::<usize>();
    assert_eq!(profiles.len(), expected_profiles);
    for fixture in fixtures.values() {
        let expected_values = profile_values(fixture);
        let observed = profiles
            .iter()
            .filter(|profile| profile.case_id == fixture.case_id)
            .collect::<Vec<_>>();
        assert_eq!(
            observed.len(),
            expected_values.len(),
            "{} profile row count",
            fixture.case_id
        );
        for expected_value in expected_values {
            assert!(
                observed
                    .iter()
                    .any(|profile| (profile.value - expected_value).abs() <= 1e-12),
                "{} missing profile value {}",
                fixture.case_id,
                expected_value
            );
        }
    }
    for profile in profiles {
        let fixture = fixtures.get(&profile.case_id).unwrap();
        assert_eq!(profile.free_parameter, fixture.free_parameter);
        let rust_log_likelihood = fixed_log_likelihood(fixture, profile.value);
        assert_close(
            rust_log_likelihood,
            profile.log_likelihood,
            fixture.likelihood_tolerance,
            &format!("{} profile {}", fixture.case_id, profile.value),
        );
    }
}

#[test]
fn generic_optimizer_matches_biogeobears_cladogenesis_parameters() {
    let fixtures = load_fixtures();
    let golden = read_tsv(&repo_path(OPTIMIZATION_GOLDEN))
        .iter()
        .map(OptimizationGolden::from_row)
        .collect::<Vec<_>>();

    assert_eq!(golden.len(), fixtures.len());
    for fixture in fixtures.values() {
        assert_eq!(
            golden
                .iter()
                .filter(|expected| expected.case_id == fixture.case_id)
                .count(),
            1,
            "{} optimization golden row count",
            fixture.case_id
        );
    }
    for expected in golden {
        let fixture = fixtures.get(&expected.case_id).unwrap();
        assert_eq!(expected.free_parameter, fixture.free_parameter);
        assert_eq!(
            expected.convergence, 0,
            "{} BGB convergence",
            fixture.case_id
        );

        let parsed_tree = parse_newick(&read_repo_file(&fixture.tree)).unwrap();
        let parsed_ranges =
            parse_tip_ranges_table(&read_repo_file(&fixture.ranges), &parsed_tree).unwrap();
        let states = StateSpace::new(
            parsed_ranges.area_names.len() as u8,
            fixture.max_range_size,
            fixture.include_null_range,
        )
        .unwrap();
        let table = configure_table(fixture, fixture.initial, true);
        let mut config = ParameterOptimizationConfig {
            initial_step: 0.5,
            tolerance: 1e-9,
            max_iterations: 600,
            additional_starts: Vec::new(),
        };
        let starts = optimization_starts(fixture);
        for start in &starts {
            if (*start - fixture.initial).abs() > 1e-12 {
                config.additional_starts.push(vec![*start]);
            }
        }

        let result = optimize_parameter_table(
            &parsed_tree.tree,
            &states,
            &parsed_ranges.tip_ranges,
            RootPrior::Flat,
            &table,
            &config,
            ModelConfig::from_biogeobears_core_parameters,
        )
        .unwrap();
        let estimate = &result.free_parameters[0];

        assert_eq!(result.free_parameters.len(), 1);
        assert_eq!(estimate.name, fixture.free_parameter);
        assert_eq!(result.starts, starts.len());
        let context = format!(
            "{} optimized lnL (Rust {}={}, BGB {}={} from {})",
            fixture.case_id,
            estimate.name,
            estimate.value,
            expected.free_parameter,
            expected.estimate,
            expected.candidate_source
        );
        if is_maxent_parameter(&fixture.free_parameter) {
            assert!(
                result.log_likelihood + fixture.likelihood_tolerance >= expected.log_likelihood,
                "{context}: Rust lnL={} is below screened BGB lnL={} by {}",
                result.log_likelihood,
                expected.log_likelihood,
                expected.log_likelihood - result.log_likelihood
            );
        } else {
            assert_close(
                result.log_likelihood,
                expected.log_likelihood,
                fixture.likelihood_tolerance,
                &context,
            );
            assert_close(
                estimate.value,
                expected.estimate,
                fixture.parameter_tolerance,
                &format!("{} optimized parameter", fixture.case_id),
            );
        }

        let rust_at_bgb = fixed_log_likelihood(fixture, expected.estimate);
        assert_close(
            rust_at_bgb,
            expected.log_likelihood,
            fixture.likelihood_tolerance,
            &format!("{} fixed at BGB estimate", fixture.case_id),
        );
    }
}

fn configure_table(fixture: &Fixture, target_value: f64, target_is_free: bool) -> ParameterTable {
    let mut table = BioGeoBearsPreset::Dec
        .parameter_table()
        .unwrap()
        .with_fixed("d", fixture.d)
        .unwrap()
        .with_fixed("e", fixture.e)
        .unwrap()
        .with_fixed("j", fixture.j)
        .unwrap()
        .with_fixed("y", fixture.y)
        .unwrap()
        .with_fixed("s", fixture.s)
        .unwrap()
        .with_fixed("v", fixture.v)
        .unwrap();

    if fixture.free_parameter == "mx01" {
        table = set_target(table, "mx01", target_value, target_is_free, fixture);
    } else {
        table = table
            .with_fixed("mx01", fixture.mx01)
            .unwrap()
            .with_fixed("mx01y", fixture.mx01y)
            .unwrap()
            .with_fixed("mx01s", fixture.mx01s)
            .unwrap()
            .with_fixed("mx01v", fixture.mx01v)
            .unwrap()
            .with_fixed("mx01j", fixture.mx01j)
            .unwrap();
    }

    if matches!(
        fixture.free_parameter.as_str(),
        "y" | "s" | "v" | "mx01y" | "mx01s" | "mx01v" | "mx01j"
    ) {
        table = set_target(
            table,
            &fixture.free_parameter,
            target_value,
            target_is_free,
            fixture,
        );
    }
    table
}

fn set_target(
    table: ParameterTable,
    parameter: &str,
    value: f64,
    target_is_free: bool,
    fixture: &Fixture,
) -> ParameterTable {
    if target_is_free {
        table
            .with_free(
                parameter,
                value,
                ParameterBounds::new(fixture.min, fixture.max).unwrap(),
            )
            .unwrap()
    } else {
        table.with_fixed(parameter, value).unwrap()
    }
}

fn fixed_log_likelihood(fixture: &Fixture, target_value: f64) -> f64 {
    let parsed_tree = parse_newick(&read_repo_file(&fixture.tree)).unwrap();
    let parsed_ranges =
        parse_tip_ranges_table(&read_repo_file(&fixture.ranges), &parsed_tree).unwrap();
    let states = StateSpace::new(
        parsed_ranges.area_names.len() as u8,
        fixture.max_range_size,
        fixture.include_null_range,
    )
    .unwrap();
    let table = configure_table(fixture, target_value, false);
    let parameters = table.resolve_initial().unwrap();
    let model = ModelConfig::from_biogeobears_core_parameters(&parameters).unwrap();
    let tips = tip_ranges_to_likelihoods(&states, &parsed_ranges.tip_ranges).unwrap();
    LikelihoodEngine::new(&parsed_tree.tree, &states, RootPrior::Flat)
        .evaluate(&model, &tips)
        .unwrap()
        .log_likelihood
}

fn is_maxent_parameter(name: &str) -> bool {
    matches!(name, "mx01" | "mx01y" | "mx01s" | "mx01v" | "mx01j")
}

fn optimization_starts(fixture: &Fixture) -> Vec<f64> {
    fixture.starts.clone()
}

fn profile_values(fixture: &Fixture) -> Vec<f64> {
    if is_maxent_parameter(&fixture.free_parameter) {
        expand_values(
            fixture,
            &fixture.profile_values,
            &[-0.0005, -0.00025, 0.0, 0.00025, 0.0005],
        )
    } else {
        fixture.profile_values.clone()
    }
}

fn expand_values(fixture: &Fixture, values: &[f64], offsets: &[f64]) -> Vec<f64> {
    let mut expanded = values
        .iter()
        .flat_map(|value| offsets.iter().map(move |offset| value + offset))
        .filter(|value| *value >= fixture.min && *value <= fixture.max)
        .collect::<Vec<_>>();
    expanded.sort_by(f64::total_cmp);
    expanded.dedup_by(|left, right| (*left - *right).abs() <= 1e-12);
    expanded
}

fn load_fixtures() -> HashMap<String, Fixture> {
    read_tsv(&repo_path(MANIFEST))
        .iter()
        .map(Fixture::from_row)
        .map(|fixture| (fixture.case_id.clone(), fixture))
        .collect()
}

fn read_tsv(path: &Path) -> Vec<HashMap<String, String>> {
    let contents = fs::read_to_string(path).unwrap();
    let mut lines = contents.lines().filter(|line| !line.trim().is_empty());
    let headers = lines
        .next()
        .expect("TSV must have a header")
        .split('\t')
        .map(str::to_owned)
        .collect::<Vec<_>>();

    lines
        .enumerate()
        .map(|(index, line)| {
            let values = line.split('\t').collect::<Vec<_>>();
            assert_eq!(
                values.len(),
                headers.len(),
                "{} row {} has wrong column count",
                path.display(),
                index + 2
            );
            headers
                .iter()
                .cloned()
                .zip(values.into_iter().map(str::to_owned))
                .collect()
        })
        .collect()
}

fn required<'a>(row: &'a HashMap<String, String>, name: &str) -> &'a str {
    row.get(name)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| panic!("missing TSV field {name}"))
}

fn number(row: &HashMap<String, String>, name: &str) -> f64 {
    required(row, name)
        .parse()
        .unwrap_or_else(|_| panic!("invalid number in TSV field {name}"))
}

fn number_list(row: &HashMap<String, String>, name: &str) -> Vec<f64> {
    required(row, name)
        .split(',')
        .map(|value| {
            value
                .parse()
                .unwrap_or_else(|_| panic!("invalid number list in TSV field {name}"))
        })
        .collect()
}

fn boolean(row: &HashMap<String, String>, name: &str) -> bool {
    match required(row, name).to_ascii_lowercase().as_str() {
        "true" | "t" | "1" | "yes" => true,
        "false" | "f" | "0" | "no" => false,
        _ => panic!("invalid boolean in TSV field {name}"),
    }
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn read_repo_file(relative: &str) -> String {
    fs::read_to_string(repo_path(relative)).unwrap()
}

fn assert_close(left: f64, right: f64, tolerance: f64, context: &str) {
    assert!(
        (left - right).abs() <= tolerance,
        "{context}: left={left}, right={right}, delta={}, tolerance={tolerance}",
        (left - right).abs()
    );
}
