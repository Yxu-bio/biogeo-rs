mod branch_process;
pub mod bsm;
pub mod bsm_summary;
pub mod cladogenesis;
pub mod constraints;
pub mod ctmc_bridge;
pub mod dec;
pub mod detection;
pub mod dispersal;
pub mod engine;
pub mod execution;
pub mod fossil_placement;
pub mod model;
pub mod newick;
pub mod parameter_optimization;
pub mod parameters;
pub mod propagation;
pub mod pruning;
pub mod q;
pub mod ranges;
pub mod state;
pub mod tree;
pub mod tree_input;

pub use bsm::{
    AnageneticEventKind, AnageneticEventSample, BiogeographicStochasticMap, BranchEndpointSample,
    BranchHistory, BranchSegmentHistory, BsmError, CladogeneticSplitSample, HistorySkeleton,
    HistorySkeletonSampler, INDEXED_BSM_RNG_PROTOCOL, StochasticMapCancellationToken,
    StochasticMapExecutionControl, StochasticMapLimits, StochasticMapParallelError,
    StochasticMapParallelOptions, StochasticMapParallelPlan, StochasticMapParallelPlanError,
    StochasticMapPauseToken, StochasticMapSampler, StochasticMapStopReason,
    StochasticMapStreamError, StochasticMapTaskLimits,
};
pub use bsm_summary::{
    BsmSampleDiagnostics, BsmSampleSummary, BsmSummaryError, CladogeneticEventCounts,
    CladogeneticEventKind, classify_cladogenetic_event, summarize_stochastic_map,
    summarize_stochastic_map_with_state_masks,
};
pub use cladogenesis::{
    CladogenesisConfig, CladogenesisError, CladogenesisRangeSizeConfig, CladogeneticEventWeights,
    CladogeneticScenario, CladogeneticTable, DEFAULT_MAXENT_CONSTRAINT, DecCladogeneticModel,
    MAX_MAXENT_CONSTRAINT, MIN_MAXENT_CONSTRAINT,
};
pub use constraints::{
    AllowedRangeStates, BinaryAreaMatrix, RangeStateConstraint, StateConstraintError,
    StateConstraintParseError, StateMask, parse_allowed_range_states, parse_binary_area_matrix,
};
pub use ctmc_bridge::{
    AdaptiveCtmcBridgeOptions, CtmcBridge, CtmcBridgeError, CtmcBridgeEvent, CtmcBridgeOptions,
    sample_uniformized_bridge, sample_uniformized_bridge_adaptive_with_options,
    sample_uniformized_bridge_with_options,
};
pub use dec::{
    DecAnalysisError, DecExponentOptimizationConfig, DecExponentOptimizationResult,
    DecJOptimizationConfig, DecJOptimizationResult, DecOptimizationConfig, DecOptimizationError,
    DecOptimizationResult, DecPairProfileConfig, DecPairProfilePoint, DecPairProfileResult,
    DecProfileAxis, DecProfileSupportSpan, DecXnuOptimizationConfig, DecXnuOptimizationResult,
    OptimizationBound, PROFILE_95_SUPPORT_DELTA, TipRange, dec_j_node_state_posteriors,
    dec_j_split_scenario_posteriors, dec_node_state_posteriors, dec_split_scenario_posteriors,
    model_node_state_posteriors, model_split_scenario_posteriors, optimize_bayarealike_de,
    optimize_bayarealikej_dej, optimize_de_exponent_with_model,
    optimize_de_exponent_with_model_likelihoods, optimize_de_with_model,
    optimize_de_with_model_likelihoods, optimize_de_xnu_with_model,
    optimize_de_xnu_with_model_likelihoods, optimize_dec_de, optimize_decj_dej,
    optimize_decj_dej_with_model, optimize_decj_dej_with_model_likelihoods, optimize_divalike_de,
    optimize_divalikej_dej, profile_de_pair_with_model, profile_de_pair_with_model_likelihoods,
    run_fixed_bayarealike_j, run_fixed_dec, run_fixed_dec_j, run_fixed_divalike_j, run_fixed_model,
    run_fixed_model_likelihoods, tip_ranges_to_likelihoods,
};
pub use detection::{
    DetectionData, DetectionDataParseError, DetectionModel, DetectionModelError,
    TipDetectionCounts, parse_detection_data,
};
pub use dispersal::{
    AnageneticStrataParseError, AnageneticStratumSpec, AnageneticTimeStratum, AreaSizeError,
    AreaSizeParseError, AreaSizeVector, DispersalMatrixError, DispersalMatrixParseError,
    DispersalMultiplierMatrix, DispersalScheduleError, DispersalStrataParseError,
    DispersalStratumSpec, DispersalTimeStratum, ExtirpationMultiplierError,
    ExtirpationMultiplierParseError, ExtirpationMultiplierVector, TimeStratifiedAnagenesis,
    TimeStratifiedDispersal, parse_anagenetic_strata_table, parse_area_sizes_table,
    parse_dispersal_multipliers_table, parse_dispersal_strata_table,
    parse_extirpation_multipliers_table,
};
pub use engine::{
    LikelihoodEngine, LikelihoodEngineError, TipStateConstraintViolation,
    tip_likelihood_state_constraint_violations,
};
pub use execution::ExecutionCancellationToken;
pub use fossil_placement::{
    CladePlacementScope, FOSSIL_PLACEMENT_RNG_PROTOCOL, FossilAttachment, FossilPlacementError,
    FossilPlacementRecord, FossilPlacementResult, FossilPlacementSpec, place_fossils_randomly,
    place_fossils_with_rng,
};
pub use model::{
    AnagenesisError, BioGeoBearsModelError, BioGeoModelConfig, DecAnageneticModel, DecModelError,
    MODEL_IDENTITY_FORMAT_VERSION, ModelConfig,
};
pub use newick::{
    InternalNodeLabel, MissingBranchLengthPolicy, NewickError, NewickParseOptions,
    ParsedNewickTree, TipLabel, format_newick, parse_newick, parse_newick_with_options,
};
pub use parameter_optimization::{
    ParameterEstimate, ParameterOptimizationConfig, ParameterOptimizationError,
    ParameterOptimizationExecution, ParameterOptimizationProgress,
    ParameterOptimizationProgressPhase, ParameterOptimizationResult, optimize_parameter_table,
    optimize_parameter_table_dynamic_likelihoods,
    optimize_parameter_table_dynamic_likelihoods_with_control,
    optimize_parameter_table_likelihoods, optimize_parameter_table_likelihoods_with_control,
};
pub use parameters::{
    BIOGEOBEARS_PARAMETER_NAMES, BioGeoBearsPreset, PARAMETER_TABLE_FORMAT_VERSION,
    ParameterBounds, ParameterError, ParameterExpression, ParameterExpressionParseError,
    ParameterMode, ParameterSpec, ParameterTable, ParameterTableParseError, ParameterTransform,
    ResolvedParameters, biogeobears_default_parameter_table, parse_parameter_table,
};
pub use propagation::{PropagationError, propagate_uniformized};
pub use pruning::{
    NodeStatePosterior, PruningError, PruningResult, RootPrior, SplitScenarioPosterior,
    TipLikelihood, cladogenetic_node_state_posteriors, cladogenetic_split_scenario_posteriors,
    prune_fixed_q, prune_with_cladogenesis,
};
pub use q::{RateTransition, SparseQ};
pub use ranges::{
    ParsedTipRanges, RangeLikelihoodError, RangeParseError, TipRangeConstraint,
    parse_tip_ranges_table, parse_tip_ranges_table_with_ambiguities,
};
pub use state::{AreaSet, StateSpace, StateSpaceError};
pub use tree::{Edge, NodeEvent, Tree, TreeChild, TreeError, default_tip_age_tolerance};
pub use tree_input::{
    NexusError, ParsedTreeInput, TreeInputError, TreeInputFormat, parse_tree_input,
    parse_tree_input_named, parse_tree_input_named_with_options, parse_tree_input_with_options,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_api_builds_dec_q() {
        let states = StateSpace::new(2, 2, false).unwrap();
        let model = DecAnageneticModel::new(0.1, 0.2).unwrap();
        let q = model.build_q(&states).unwrap();

        assert_eq!(q.size(), 3);
        assert_eq!(q.off_diagonal_count(), 4);
    }
}
