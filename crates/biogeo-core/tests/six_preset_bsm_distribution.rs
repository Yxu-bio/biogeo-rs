use std::collections::HashMap;

use biogeo_core::{
    AnageneticEventKind, AreaSet, BsmSummaryError, CladogeneticEventKind, CladogeneticSplitSample,
    Edge, LikelihoodEngine, ModelConfig, RootPrior, StateSpace, TipLikelihood, Tree,
    classify_cladogenetic_event,
};

const SAMPLE_COUNT: usize = 20_000;

#[test]
fn six_preset_stochastic_histories_match_their_exact_posteriors() {
    let tree = Tree::new(
        6,
        7,
        vec![
            Edge {
                parent: 4,
                child: 0,
                length: 0.4,
            },
            Edge {
                parent: 4,
                child: 1,
                length: 0.6,
            },
            Edge {
                parent: 5,
                child: 4,
                length: 0.3,
            },
            Edge {
                parent: 5,
                child: 2,
                length: 0.5,
            },
            Edge {
                parent: 6,
                child: 5,
                length: 0.4,
            },
            Edge {
                parent: 6,
                child: 3,
                length: 0.7,
            },
        ],
    )
    .unwrap();
    let states = StateSpace::new(3, 3, false).unwrap();
    let tip_states = [0b001, 0b010, 0b100, 0b011].map(|bits| {
        states
            .index_of(AreaSet::from_bits(bits))
            .expect("fixture range must be present")
    });
    let tips = tip_states
        .into_iter()
        .enumerate()
        .map(|(node, state)| TipLikelihood {
            node,
            likelihoods: one_hot(states.len(), state),
        })
        .collect::<Vec<_>>();
    let cases = [
        (
            "DEC",
            ModelConfig::preset_dec(0.2, 0.1).unwrap(),
            [true, true, true, false],
        ),
        (
            "DEC+J",
            ModelConfig::preset_dec_j(0.2, 0.1, 0.5).unwrap(),
            [true, true, true, true],
        ),
        (
            "DIVALIKE",
            ModelConfig::preset_divalike(0.2, 0.1).unwrap(),
            [true, false, true, false],
        ),
        (
            "DIVALIKE+J",
            ModelConfig::preset_divalike_j(0.2, 0.1, 0.5).unwrap(),
            [true, false, true, true],
        ),
        (
            "BAYAREALIKE",
            ModelConfig::preset_bayarealike(0.2, 0.1).unwrap(),
            [true, false, false, false],
        ),
        (
            "BAYAREALIKE+J",
            ModelConfig::preset_bayarealike_j(0.2, 0.1, 0.5).unwrap(),
            [true, false, false, true],
        ),
    ];

    for (case_index, (name, model, supported_events)) in cases.into_iter().enumerate() {
        check_preset(
            name,
            &tree,
            &states,
            &tips,
            &model,
            supported_events,
            20260824 + case_index as u64,
        );
    }
}

fn check_preset(
    name: &str,
    tree: &Tree,
    states: &StateSpace,
    tips: &[TipLikelihood],
    model: &ModelConfig,
    supported_events: [bool; 4],
    seed: u64,
) {
    let engine = LikelihoodEngine::new(tree, states, RootPrior::Flat);
    let pruning = engine.evaluate(model, tips).unwrap();
    let exact_nodes = engine.node_state_posteriors(model, &pruning).unwrap();
    let exact_splits = engine.split_scenario_posteriors(model, &pruning).unwrap();
    let mut exact_split_probabilities = HashMap::new();
    let mut exact_events_by_node = vec![[0.0_f64; 4]; tree.node_count()];
    for split in exact_splits {
        let key = (split.node, split.ancestor, split.left, split.right);
        *exact_split_probabilities.entry(key).or_insert(0.0) += split.probability;
        let kind = classify_cladogenetic_event(
            states,
            CladogeneticSplitSample {
                node: split.node,
                ancestor: split.ancestor,
                left: split.left,
                right: split.right,
                weight: split.weight,
            },
        )
        .unwrap();
        exact_events_by_node[split.node][event_index(kind)] += split.probability;
    }

    let mut node_counts = vec![vec![0_usize; states.len()]; tree.node_count()];
    let mut split_counts = HashMap::new();
    let mut sampled_events_by_node = vec![[0_usize; 4]; tree.node_count()];
    let expected_branch_time: f64 = tree.edges().iter().map(|edge| edge.length).sum();
    engine
        .try_for_each_stochastic_map_seeded::<BsmSummaryError, _>(
            model,
            &pruning,
            SAMPLE_COUNT,
            seed,
            |_, map| {
                for (node, state) in map.skeleton.node_states.iter().copied().enumerate() {
                    node_counts[node][state] += 1;
                }
                for split in &map.skeleton.splits {
                    *split_counts
                        .entry((split.node, split.ancestor, split.left, split.right))
                        .or_insert(0_usize) += 1;
                    let kind = classify_cladogenetic_event(states, *split)?;
                    sampled_events_by_node[split.node][event_index(kind)] += 1;
                }

                let summary = map.summarize(states)?;
                assert_eq!(
                    summary.anagenetic_event_count,
                    summary.range_expansion_count
                        + summary.local_extirpation_count
                        + summary.range_switching_count,
                    "{name}: inconsistent anagenetic event totals"
                );
                assert!((summary.total_branch_time - expected_branch_time).abs() < 1e-10);
                assert!(
                    (summary.occupancy_time_by_state.iter().sum::<f64>() - expected_branch_time)
                        .abs()
                        < 1e-10,
                    "{name}: state occupancy time does not sum to tree length"
                );
                for branch in &map.branches {
                    for segment in &branch.segments {
                        for event in &segment.events {
                            assert!(matches!(
                                event.kind,
                                AnageneticEventKind::RangeExpansion { .. }
                                    | AnageneticEventKind::LocalExtirpation { .. }
                                    | AnageneticEventKind::RangeSwitching { .. }
                            ));
                        }
                    }
                }
                Ok(())
            },
        )
        .unwrap();

    for posterior in exact_nodes {
        for (state, exact) in posterior.probabilities.iter().copied().enumerate() {
            assert_binomial_frequency(
                &format!("{name} node {} state {state}", posterior.node),
                node_counts[posterior.node][state],
                exact,
            );
        }
    }
    for (key, exact) in &exact_split_probabilities {
        assert_binomial_frequency(
            &format!("{name} split {key:?}"),
            split_counts.get(key).copied().unwrap_or(0),
            *exact,
        );
    }
    for key in split_counts.keys() {
        assert!(
            exact_split_probabilities.contains_key(key),
            "{name}: sampled split {key:?} is absent from the exact posterior"
        );
    }

    let mut exact_event_totals = [0.0_f64; 4];
    let mut sampled_event_totals = [0_usize; 4];
    for &node in tree.postorder_internal_nodes() {
        for kind in 0..4 {
            let exact = exact_events_by_node[node][kind];
            let sampled = sampled_events_by_node[node][kind];
            assert_binomial_frequency(
                &format!("{name} node {node} event {}", EVENT_NAMES[kind]),
                sampled,
                exact,
            );
            exact_event_totals[kind] += exact;
            sampled_event_totals[kind] += sampled;
        }
    }
    for kind in 0..4 {
        if supported_events[kind] {
            assert!(
                exact_event_totals[kind] > 0.0,
                "{name}: fixture has no exact support for expected event {}",
                EVENT_NAMES[kind]
            );
            assert!(
                sampled_event_totals[kind] > 0,
                "{name}: no sampled {} events",
                EVENT_NAMES[kind]
            );
        } else {
            assert_eq!(
                exact_event_totals[kind], 0.0,
                "{name}: unsupported event {} has exact posterior mass",
                EVENT_NAMES[kind]
            );
            assert_eq!(
                sampled_event_totals[kind], 0,
                "{name}: unsupported event {} was sampled",
                EVENT_NAMES[kind]
            );
        }
    }

    println!(
        "preset={name} samples={SAMPLE_COUNT} event_means=y:{:.6}/{:.6},s:{:.6}/{:.6},v:{:.6}/{:.6},j:{:.6}/{:.6}",
        sampled_event_totals[0] as f64 / SAMPLE_COUNT as f64,
        exact_event_totals[0],
        sampled_event_totals[1] as f64 / SAMPLE_COUNT as f64,
        exact_event_totals[1],
        sampled_event_totals[2] as f64 / SAMPLE_COUNT as f64,
        exact_event_totals[2],
        sampled_event_totals[3] as f64 / SAMPLE_COUNT as f64,
        exact_event_totals[3],
    );
}

const EVENT_NAMES: [&str; 4] = ["y", "s", "v", "j"];

fn event_index(kind: CladogeneticEventKind) -> usize {
    match kind {
        CladogeneticEventKind::RangeCopying => 0,
        CladogeneticEventKind::SubsetSympatry => 1,
        CladogeneticEventKind::Vicariance => 2,
        CladogeneticEventKind::FounderEvent => 3,
    }
}

fn assert_binomial_frequency(label: &str, count: usize, expected: f64) {
    let empirical = count as f64 / SAMPLE_COUNT as f64;
    if expected == 0.0 {
        assert_eq!(count, 0, "{label}: expected zero, observed {empirical}");
        return;
    }
    if expected == 1.0 {
        assert_eq!(
            count, SAMPLE_COUNT,
            "{label}: expected one, observed {empirical}"
        );
        return;
    }
    let standard_error = (expected * (1.0 - expected) / SAMPLE_COUNT as f64).sqrt();
    let tolerance = (6.0 * standard_error).max(3.0 / SAMPLE_COUNT as f64);
    assert!(
        (empirical - expected).abs() <= tolerance,
        "{label}: empirical {empirical}, exact {expected}, tolerance {tolerance}"
    );
}

fn one_hot(state_count: usize, state: usize) -> Vec<f64> {
    let mut likelihoods = vec![0.0; state_count];
    likelihoods[state] = 1.0;
    likelihoods
}
