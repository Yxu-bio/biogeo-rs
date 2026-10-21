// Version 3 adds event provenance without changing RNG or likelihood semantics.
pub const NODES: &str = "node\tlabel\tkind\tage\tis_direct_ancestor";
pub const EDGES: &str = "edge\tparent\tchild\tlength\tchild_order\tis_direct_ancestor_hook";
pub const COMPACT_SPLITS: &str = "sample\tnode\tancestor_state_index\tleft_state_index\tright_state_index\tscenario_weight\tevent_kind\tfounder_daughter_order\tsource_area_index\tsource_status";
pub const FULL_SPLITS: &str = "sample\tnode\tlabel\tkind\tclade\tleft_clade\tright_clade\tancestor_state_index\tancestor_range_bits\tancestor_range\tleft_state_index\tleft_range_bits\tleft_range\tright_state_index\tright_range_bits\tright_range\tscenario_weight\tevent_kind\tfounder_daughter_order\tsource_area_index\tsource_status";
pub const COMPACT_EVENTS: &str = "sample\tedge\tsegment\tq_index\ttime_from_parent\tevent_kind\tparameter\tarea_index\tfrom_state_index\tto_state_index\tsource_area_index\ttarget_area_index\tsource_status";
pub const FULL_EVENTS: &str = "sample\tedge\tparent\tparent_clade\tchild\tchild_clade\tsegment\tq_index\ttime_from_parent\tevent_kind\tparameter\tarea_index\tarea\tfrom_state_index\tfrom_range_bits\tfrom_range\tto_state_index\tto_range_bits\tto_range\tsource_area_index\ttarget_area_index\tsource_status";
pub const SEMANTICS: &[(&str, &str)] = &[
    ("split_event_kind_basis", "selected_cladogenetic_scenario"),
    ("dispersal_source_sampling", "not_sampled"),
    ("undetermined_source_area", "NA"),
    ("ordered_daughters", "input_tree_child_order_zero_based"),
    (
        "event_time_basis",
        "elapsed_from_parent_in_branch_length_units",
    ),
    (
        "node_age_basis",
        "max_root_to_tip_distance_minus_root_to_node_distance",
    ),
    ("event_age_rule", "parent_age_minus_time_from_parent"),
    (
        "node_id_scope",
        "parsed_input_tree_not_canonical_across_reordering",
    ),
    (
        "analysis_result_node_id_mapping",
        "identity_when_analysis_result_fingerprint_present",
    ),
    ("field_encoding", "percent_for_percent_tab_cr_lf"),
];
