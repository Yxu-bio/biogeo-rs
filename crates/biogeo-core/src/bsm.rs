use std::error::Error;
use std::fmt;
use std::mem::size_of;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use rand::{Rng, RngExt, SeedableRng};
use rand_chacha::ChaCha12Rng;
use rayon::prelude::*;

use crate::branch_process::{BranchPropagator, OwnedBranchPropagator};
use crate::cladogenesis::{
    CladogenesisError, CladogeneticProcess, CladogeneticScenario, OwnedCladogeneticProcess,
};
use crate::constraints::StateMask;
use crate::ctmc_bridge::{
    AdaptiveCtmcBridgeOptions, CtmcBridgeError, sample_uniformized_bridge_adaptive_with_options,
};
use crate::engine::LikelihoodEngineError;
use crate::execution::ExecutionCancellationToken;
use crate::model::AnagenesisError;
use crate::propagation::{
    PropagationError, propagate_uniformized, propagate_uniformized_transpose,
};
use crate::pruning::{
    PruningError, PruningResult, RootPrior, propagated_branch_likelihoods, resolve_root_prior,
    validate_pruning_result_dimensions,
};
use crate::state::StateSpace;
use crate::tree::{Tree, TreeChild};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BranchEndpointSample {
    pub edge_index: usize,
    pub parent: usize,
    pub child: usize,
    pub start_state: usize,
    pub end_state: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CladogeneticSplitSample {
    pub node: usize,
    pub ancestor: usize,
    pub left: usize,
    pub right: usize,
    pub weight: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistorySkeleton {
    pub root_state: usize,
    pub node_states: Vec<usize>,
    pub branch_endpoints: Vec<BranchEndpointSample>,
    pub splits: Vec<CladogeneticSplitSample>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnageneticEventKind {
    RangeExpansion { area: u8 },
    LocalExtirpation { area: u8 },
    RangeSwitching { from_area: u8, to_area: u8 },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnageneticEventSample {
    pub edge_index: usize,
    pub segment_index: usize,
    pub q_index: usize,
    pub time_from_parent: f64,
    pub from_state: usize,
    pub to_state: usize,
    pub kind: AnageneticEventKind,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BranchSegmentHistory {
    pub segment_index: usize,
    pub q_index: usize,
    pub start_time_from_parent: f64,
    pub end_time_from_parent: f64,
    pub start_state: usize,
    pub end_state: usize,
    pub endpoint_probability: f64,
    pub virtual_jump_count: usize,
    pub events: Vec<AnageneticEventSample>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BranchHistory {
    pub edge_index: usize,
    pub parent: usize,
    pub child: usize,
    pub start_state: usize,
    pub end_state: usize,
    pub segments: Vec<BranchSegmentHistory>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BiogeographicStochasticMap {
    pub skeleton: HistorySkeleton,
    pub branches: Vec<BranchHistory>,
}

impl BiogeographicStochasticMap {
    pub fn anagenetic_event_count(&self) -> Result<usize, BsmError> {
        self.branches
            .iter()
            .flat_map(|branch| &branch.segments)
            .try_fold(0_usize, |total, segment| {
                total
                    .checked_add(segment.events.len())
                    .ok_or(BsmError::AnageneticEventCountOverflow)
            })
    }

    /// Logical heap bytes retained by this completed history's vector buffers.
    ///
    /// This excludes the inline map value, allocator overhead, worker-local
    /// sampling scratch, shared model caches, and downstream writer buffers.
    pub fn retained_heap_bytes(&self) -> Result<usize, BsmError> {
        let mut bytes = 0_usize;
        add_vector_capacity_bytes::<usize>(&mut bytes, self.skeleton.node_states.capacity())?;
        add_vector_capacity_bytes::<BranchEndpointSample>(
            &mut bytes,
            self.skeleton.branch_endpoints.capacity(),
        )?;
        add_vector_capacity_bytes::<CladogeneticSplitSample>(
            &mut bytes,
            self.skeleton.splits.capacity(),
        )?;
        add_vector_capacity_bytes::<BranchHistory>(&mut bytes, self.branches.capacity())?;
        for branch in &self.branches {
            add_vector_capacity_bytes::<BranchSegmentHistory>(
                &mut bytes,
                branch.segments.capacity(),
            )?;
            for segment in &branch.segments {
                add_vector_capacity_bytes::<AnageneticEventSample>(
                    &mut bytes,
                    segment.events.capacity(),
                )?;
            }
        }
        Ok(bytes)
    }
}

fn add_vector_capacity_bytes<T>(total: &mut usize, capacity: usize) -> Result<(), BsmError> {
    let bytes = capacity
        .checked_mul(size_of::<T>())
        .ok_or(BsmError::RetainedHistorySizeOverflow)?;
    *total = total
        .checked_add(bytes)
        .ok_or(BsmError::RetainedHistorySizeOverflow)?;
    Ok(())
}

pub const INDEXED_BSM_RNG_PROTOCOL: &str = "indexed-chacha12-v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StochasticMapLimits {
    pub max_anagenetic_events_per_map: Option<usize>,
}

impl StochasticMapLimits {
    pub const UNLIMITED: Self = Self {
        max_anagenetic_events_per_map: None,
    };

    pub const fn new(max_anagenetic_events_per_map: Option<usize>) -> Self {
        Self {
            max_anagenetic_events_per_map,
        }
    }
}

impl Default for StochasticMapLimits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StochasticMapTaskLimits {
    pub max_anagenetic_events: Option<usize>,
    pub completed_anagenetic_events: usize,
}

impl StochasticMapTaskLimits {
    pub const UNLIMITED: Self = Self {
        max_anagenetic_events: None,
        completed_anagenetic_events: 0,
    };

    pub const fn new(
        max_anagenetic_events: Option<usize>,
        completed_anagenetic_events: usize,
    ) -> Self {
        Self {
            max_anagenetic_events,
            completed_anagenetic_events,
        }
    }
}

impl Default for StochasticMapTaskLimits {
    fn default() -> Self {
        Self::UNLIMITED
    }
}

pub type StochasticMapCancellationToken = ExecutionCancellationToken;

#[derive(Debug, Default)]
struct StochasticMapPauseState {
    paused: AtomicBool,
    wait_lock: Mutex<()>,
    changed: Condvar,
}

#[derive(Clone, Debug, Default)]
pub struct StochasticMapPauseToken {
    state: Arc<StochasticMapPauseState>,
}

impl StochasticMapPauseToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pause(&self) -> bool {
        !self.state.paused.swap(true, Ordering::AcqRel)
    }

    pub fn resume(&self) -> bool {
        let was_paused = self.state.paused.swap(false, Ordering::AcqRel);
        if was_paused {
            self.state.changed.notify_all();
        }
        was_paused
    }

    pub fn is_paused(&self) -> bool {
        self.state.paused.load(Ordering::Acquire)
    }

    fn wait_while_paused(&self, timeout: Duration) {
        let guard = self
            .state
            .wait_lock
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !self.is_paused() {
            return;
        }
        match self
            .state
            .changed
            .wait_timeout_while(guard, timeout, |_| self.is_paused())
        {
            Ok((guard, _)) => drop(guard),
            Err(poisoned) => drop(poisoned.into_inner().0),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StochasticMapStopReason {
    Cancelled,
    DeadlineExceeded,
}

impl fmt::Display for StochasticMapStopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(f, "cancellation requested"),
            Self::DeadlineExceeded => write!(f, "execution deadline exceeded"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StochasticMapExecutionControl {
    cancellation: StochasticMapCancellationToken,
    deadline: Option<Instant>,
    pause: Option<StochasticMapPauseToken>,
}

impl StochasticMapExecutionControl {
    pub fn new(cancellation: StochasticMapCancellationToken, deadline: Option<Instant>) -> Self {
        Self {
            cancellation,
            deadline,
            pause: None,
        }
    }

    pub fn with_pause_token(mut self, pause: StochasticMapPauseToken) -> Self {
        self.pause = Some(pause);
        self
    }

    pub fn cancellation_token(&self) -> StochasticMapCancellationToken {
        self.cancellation.clone()
    }

    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    pub fn pause_token(&self) -> Option<StochasticMapPauseToken> {
        self.pause.clone()
    }

    pub fn stop_reason(&self) -> Option<StochasticMapStopReason> {
        if self.cancellation.is_cancelled() {
            Some(StochasticMapStopReason::Cancelled)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Some(StochasticMapStopReason::DeadlineExceeded)
        } else {
            None
        }
    }

    pub fn wait_until_runnable(&self) -> Option<StochasticMapStopReason> {
        const POLL_INTERVAL: Duration = Duration::from_millis(50);

        loop {
            if let Some(reason) = self.stop_reason() {
                return Some(reason);
            }
            let pause = self.pause.as_ref().filter(|pause| pause.is_paused())?;
            let timeout = self.deadline.map_or(POLL_INTERVAL, |deadline| {
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(POLL_INTERVAL)
            });
            pause.wait_while_paused(timeout);
        }
    }
}

#[derive(Clone, Debug)]
pub struct StochasticMapParallelOptions {
    pub threads: usize,
    pub max_in_flight: usize,
    pub limits: StochasticMapLimits,
    pub task_limits: StochasticMapTaskLimits,
    pub max_buffered_history_bytes: Option<usize>,
    pub execution_control: Option<StochasticMapExecutionControl>,
}

impl StochasticMapParallelOptions {
    pub const fn new(threads: usize, max_in_flight: usize) -> Self {
        Self {
            threads,
            max_in_flight,
            limits: StochasticMapLimits::UNLIMITED,
            task_limits: StochasticMapTaskLimits::UNLIMITED,
            max_buffered_history_bytes: None,
            execution_control: None,
        }
    }

    pub const fn with_limits(mut self, limits: StochasticMapLimits) -> Self {
        self.limits = limits;
        self
    }

    pub const fn with_task_limits(mut self, task_limits: StochasticMapTaskLimits) -> Self {
        self.task_limits = task_limits;
        self
    }

    pub const fn with_max_buffered_history_bytes(mut self, bytes: Option<usize>) -> Self {
        self.max_buffered_history_bytes = bytes;
        self
    }

    pub fn with_execution_control(mut self, control: StochasticMapExecutionControl) -> Self {
        self.execution_control = Some(control);
        self
    }
}

/// Effective bounded-window plan for ordered parallel history sampling.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StochasticMapParallelPlan {
    pub threads: usize,
    pub max_in_flight: usize,
    pub retained_bytes_per_sample_upper_bound: Option<usize>,
    pub buffered_history_bytes_upper_bound: Option<usize>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StochasticMapParallelPlanError {
    ZeroThreads,
    ZeroMaxInFlight,
    MaxInFlightBelowThreads {
        threads: usize,
        max_in_flight: usize,
    },
    MemoryBudgetRequiresPerMapEventLimit {
        budget_bytes: usize,
    },
    MemoryBudgetTooSmall {
        budget_bytes: usize,
        minimum_bytes: usize,
    },
    RetainedHistorySizeOverflow,
}

impl fmt::Display for StochasticMapParallelPlanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroThreads => write!(f, "stochastic-history thread count must be positive"),
            Self::ZeroMaxInFlight => {
                write!(f, "stochastic-history max-in-flight must be positive")
            }
            Self::MaxInFlightBelowThreads {
                threads,
                max_in_flight,
            } => write!(
                f,
                "stochastic-history max-in-flight {max_in_flight} is below worker count {threads}"
            ),
            Self::MemoryBudgetRequiresPerMapEventLimit { budget_bytes } => write!(
                f,
                "stochastic-history buffer budget of {budget_bytes} bytes requires a finite per-map anagenetic-event limit"
            ),
            Self::MemoryBudgetTooSmall {
                budget_bytes,
                minimum_bytes,
            } => write!(
                f,
                "stochastic-history buffer budget of {budget_bytes} bytes is below the {minimum_bytes}-byte upper bound required for one completed history"
            ),
            Self::RetainedHistorySizeOverflow => write!(
                f,
                "stochastic-history retained-memory upper-bound calculation overflowed usize"
            ),
        }
    }
}

impl Error for StochasticMapParallelPlanError {}

const SPLITMIX64_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
const BSM_RNG_DOMAIN: u64 = 0x4249_4F47_454F_4253;

type TransitionRowCache = OnceLock<Result<Vec<f64>, PruningError>>;

fn splitmix64(value: u64) -> u64 {
    let mut mixed = value.wrapping_add(SPLITMIX64_GAMMA);
    mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    mixed ^ (mixed >> 31)
}

fn indexed_bsm_rng(master_seed: u64, sample_index: u64) -> ChaCha12Rng {
    let mut seed = [0_u8; 32];
    for (lane, bytes) in seed.chunks_exact_mut(8).enumerate() {
        let lane_input = master_seed
            .wrapping_add(BSM_RNG_DOMAIN)
            .wrapping_add((lane as u64).wrapping_mul(SPLITMIX64_GAMMA));
        bytes.copy_from_slice(&splitmix64(lane_input).to_le_bytes());
    }
    let mut rng = ChaCha12Rng::from_seed(seed);
    rng.set_stream(sample_index);
    rng
}

pub struct HistorySkeletonSampler<'a> {
    tree: &'a Tree,
    states: &'a StateSpace,
    pruning: &'a PruningResult,
    propagator: OwnedBranchPropagator,
    cladogenesis: OwnedCladogeneticProcess,
    branch_likelihoods: Vec<Vec<f64>>,
    root_masses: Vec<f64>,
    transition_rows: Vec<Vec<TransitionRowCache>>,
}

pub type StochasticMapSampler<'a> = HistorySkeletonSampler<'a>;

impl<'a> HistorySkeletonSampler<'a> {
    pub(crate) fn new(
        tree: &'a Tree,
        states: &'a StateSpace,
        pruning: &'a PruningResult,
        propagator: OwnedBranchPropagator,
        cladogenesis: OwnedCladogeneticProcess,
        root_prior: RootPrior<'_>,
    ) -> Result<Self, BsmError> {
        let state_count = propagator.state_count();
        if state_count == 0 {
            return Err(PruningError::ZeroStates.into());
        }
        if cladogenesis.state_count() != state_count {
            return Err(PruningError::CladogenesisStateCountMismatch {
                q_states: state_count,
                cladogenesis_states: cladogenesis.state_count(),
            }
            .into());
        }
        validate_pruning_result_dimensions(tree, state_count, pruning)?;

        let branch_likelihoods = propagated_branch_likelihoods(tree, &propagator, pruning)?;
        let root = tree.root();
        let prior = resolve_root_prior(
            root_prior,
            state_count,
            cladogenesis.state_mask_for_node(root),
        )?;
        let root_masses = prior
            .iter()
            .zip(&pruning.scaled_likelihoods[root])
            .map(|(prior, likelihood)| prior * likelihood)
            .collect::<Vec<f64>>();

        validate_sampling_masses("root state", root, &root_masses)?;

        let transition_rows = (0..tree.edges().len())
            .map(|_| (0..state_count).map(|_| OnceLock::new()).collect())
            .collect();

        Ok(Self {
            tree,
            states,
            pruning,
            propagator,
            cladogenesis,
            branch_likelihoods,
            root_masses,
            transition_rows,
        })
    }

    pub fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Result<HistorySkeleton, BsmError> {
        let root = self.tree.root();
        let root_state = sample_weighted_index("root state", root, &self.root_masses, rng)?;
        let mut node_states = vec![None; self.tree.node_count()];
        let mut branch_endpoints = vec![None; self.tree.edges().len()];
        let mut splits = vec![None; self.tree.node_count()];
        let mut stack = vec![(root, root_state)];

        while let Some((node, ancestor)) = stack.pop() {
            node_states[node] = Some(ancestor);
            let children = self
                .tree
                .children(node)
                .expect("sampled node must belong to the tree");
            if children.is_empty() {
                continue;
            }
            if children.len() != 2 {
                return Err(PruningError::NonBinaryCladogenesisNode {
                    node,
                    child_count: children.len(),
                }
                .into());
            }

            let (left_start, right_start) = if self.tree.is_direct_ancestor_node(node) {
                (ancestor, ancestor)
            } else {
                let table = self.cladogenesis.table_for_node(node);
                let scenarios = table
                    .row(ancestor)
                    .ok_or(BsmError::MissingSplitScenarios { node, ancestor })?;
                if scenarios.is_empty() {
                    return Err(BsmError::MissingSplitScenarios { node, ancestor });
                }
                let scenario_masses: Vec<f64> = scenarios
                    .iter()
                    .map(|scenario| {
                        scenario.weight
                            * self.branch_likelihoods[children[0].edge_index][scenario.left]
                            * self.branch_likelihoods[children[1].edge_index][scenario.right]
                    })
                    .collect();
                let scenario_index =
                    sample_weighted_index("cladogenetic split", node, &scenario_masses, rng)?;
                let scenario = scenarios[scenario_index];
                splits[node] = Some(CladogeneticSplitSample::from_scenario(node, scenario));
                (scenario.left, scenario.right)
            };

            let left_end = self.sample_branch_endpoint(node, children[0], left_start, rng)?;
            let right_end = self.sample_branch_endpoint(node, children[1], right_start, rng)?;
            branch_endpoints[children[0].edge_index] = Some(left_end);
            branch_endpoints[children[1].edge_index] = Some(right_end);

            stack.push((children[1].node, right_end.end_state));
            stack.push((children[0].node, left_end.end_state));
        }

        let node_states = node_states
            .into_iter()
            .enumerate()
            .map(|(node, state)| state.ok_or(BsmError::IncompleteNodeState { node }))
            .collect::<Result<Vec<_>, _>>()?;
        let branch_endpoints = branch_endpoints
            .into_iter()
            .enumerate()
            .map(|(edge_index, sample)| {
                sample.ok_or(BsmError::IncompleteBranchEndpoint { edge_index })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let splits = self
            .tree
            .postorder_internal_nodes()
            .iter()
            .filter(|node| !self.tree.is_direct_ancestor_node(**node))
            .map(|node| splits[*node].ok_or(BsmError::IncompleteSplit { node: *node }))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(HistorySkeleton {
            root_state,
            node_states: into_exact_vec(node_states),
            branch_endpoints: into_exact_vec(branch_endpoints),
            splits: into_exact_vec(splits),
        })
    }

    pub fn sample_many<R: Rng + ?Sized>(
        &self,
        sample_count: usize,
        rng: &mut R,
    ) -> Result<Vec<HistorySkeleton>, BsmError> {
        (0..sample_count).map(|_| self.sample(rng)).collect()
    }

    pub fn sample_map<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
    ) -> Result<BiogeographicStochasticMap, BsmError> {
        self.sample_map_with_limits(rng, StochasticMapLimits::UNLIMITED)
    }

    pub fn sample_map_with_limits<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
        limits: StochasticMapLimits,
    ) -> Result<BiogeographicStochasticMap, BsmError> {
        self.sample_map_with_limits_and_control(rng, limits, None)
    }

    fn sample_map_with_limits_and_control<R: Rng + ?Sized>(
        &self,
        rng: &mut R,
        limits: StochasticMapLimits,
        control: Option<&StochasticMapExecutionControl>,
    ) -> Result<BiogeographicStochasticMap, BsmError> {
        check_execution_control(control)?;
        let skeleton = self.sample(rng)?;
        check_execution_control(control)?;
        let mut event_budget = AnageneticEventBudget::new(limits.max_anagenetic_events_per_map);
        let mut branches = Vec::with_capacity(skeleton.branch_endpoints.len());
        for endpoint in skeleton.branch_endpoints.iter().copied() {
            check_execution_control(control)?;
            branches.push(self.sample_branch_history_with_budget(
                endpoint,
                rng,
                &mut event_budget,
                control,
            )?);
        }

        Ok(BiogeographicStochasticMap {
            skeleton,
            branches: into_exact_vec(branches),
        })
    }

    pub fn sample_maps<R: Rng + ?Sized>(
        &self,
        sample_count: usize,
        rng: &mut R,
    ) -> Result<Vec<BiogeographicStochasticMap>, BsmError> {
        (0..sample_count).map(|_| self.sample_map(rng)).collect()
    }

    pub fn try_for_each_map<R, E, F>(
        &self,
        sample_count: usize,
        rng: &mut R,
        mut consumer: F,
    ) -> Result<(), StochasticMapStreamError<E>>
    where
        R: Rng + ?Sized,
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        for sample_index in 0..sample_count {
            let map = self
                .sample_map(rng)
                .map_err(StochasticMapStreamError::Sampling)?;
            consumer(sample_index, &map).map_err(StochasticMapStreamError::Consumer)?;
        }
        Ok(())
    }

    pub fn sample_indexed(
        &self,
        master_seed: u64,
        sample_index: u64,
    ) -> Result<HistorySkeleton, BsmError> {
        let mut rng = indexed_bsm_rng(master_seed, sample_index);
        self.sample(&mut rng)
    }

    pub fn sample_map_indexed(
        &self,
        master_seed: u64,
        sample_index: u64,
    ) -> Result<BiogeographicStochasticMap, BsmError> {
        self.sample_map_indexed_with_limits(
            master_seed,
            sample_index,
            StochasticMapLimits::UNLIMITED,
        )
    }

    pub fn sample_map_indexed_with_limits(
        &self,
        master_seed: u64,
        sample_index: u64,
        limits: StochasticMapLimits,
    ) -> Result<BiogeographicStochasticMap, BsmError> {
        self.sample_map_indexed_with_limits_and_control(master_seed, sample_index, limits, None)
    }

    fn sample_map_indexed_with_limits_and_control(
        &self,
        master_seed: u64,
        sample_index: u64,
        limits: StochasticMapLimits,
        control: Option<&StochasticMapExecutionControl>,
    ) -> Result<BiogeographicStochasticMap, BsmError> {
        let mut rng = indexed_bsm_rng(master_seed, sample_index);
        self.sample_map_with_limits_and_control(&mut rng, limits, control)
    }

    pub fn try_for_each_map_indexed_parallel<E, F>(
        &self,
        sample_count: usize,
        master_seed: u64,
        thread_count: usize,
        max_in_flight: usize,
        consumer: F,
    ) -> Result<(), StochasticMapParallelError<E>>
    where
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        self.try_for_each_map_indexed_parallel_with_options(
            sample_count,
            master_seed,
            StochasticMapParallelOptions::new(thread_count, max_in_flight),
            consumer,
        )
    }

    pub fn try_for_each_map_indexed_parallel_with_options<E, F>(
        &self,
        sample_count: usize,
        master_seed: u64,
        options: StochasticMapParallelOptions,
        consumer: F,
    ) -> Result<(), StochasticMapParallelError<E>>
    where
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        self.try_for_each_map_indexed_parallel_range_with_options(
            0..sample_count,
            master_seed,
            options,
            consumer,
        )
    }

    pub fn try_for_each_map_indexed_parallel_range_with_options<E, F>(
        &self,
        sample_range: Range<usize>,
        master_seed: u64,
        options: StochasticMapParallelOptions,
        mut consumer: F,
    ) -> Result<(), StochasticMapParallelError<E>>
    where
        F: FnMut(usize, &BiogeographicStochasticMap) -> Result<(), E>,
    {
        let sample_count = sample_range.end.checked_sub(sample_range.start).ok_or(
            StochasticMapParallelError::InvalidSampleRange {
                start: sample_range.start,
                end: sample_range.end,
            },
        )?;
        if sample_count == 0 {
            return Ok(());
        }

        let plan = self
            .plan_indexed_parallel(sample_count, &options)
            .map_err(StochasticMapParallelError::from_plan_error)?;
        let worker_count = plan.threads;
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(worker_count)
            .thread_name(|index| format!("biogeo-bsm-{index}"))
            .build()
            .map_err(StochasticMapParallelError::ThreadPoolBuild)?;
        let limits = options.limits;
        let task_limits = options.task_limits;
        let execution_control = options.execution_control;
        let mut completed_anagenetic_events = task_limits.completed_anagenetic_events;

        if let Some(limit) = task_limits.max_anagenetic_events
            && completed_anagenetic_events > limit
        {
            return Err(
                StochasticMapParallelError::TotalAnageneticEventLimitExceeded {
                    sample_index: sample_range.start,
                    limit,
                    completed: completed_anagenetic_events,
                    attempted: completed_anagenetic_events,
                },
            );
        }

        for window_start in (sample_range.start..sample_range.end).step_by(plan.max_in_flight) {
            if let Some(reason) = execution_control
                .as_ref()
                .and_then(StochasticMapExecutionControl::wait_until_runnable)
            {
                return Err(StochasticMapParallelError::Stopped {
                    sample_index: window_start,
                    reason,
                });
            }
            let window_end = window_start
                .saturating_add(plan.max_in_flight)
                .min(sample_range.end);
            let sampled = pool.install(|| {
                (window_start..window_end)
                    .into_par_iter()
                    .map(|sample_index| {
                        let indexed = u64::try_from(sample_index)
                            .map_err(|_| BsmError::SampleIndexOutOfRange { sample_index })?;
                        self.sample_map_indexed_with_limits_and_control(
                            master_seed,
                            indexed,
                            limits,
                            execution_control.as_ref(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice()
                    .into_vec()
            });

            for (offset, map) in sampled.into_iter().enumerate() {
                let sample_index = window_start + offset;
                if let Some(reason) = execution_control
                    .as_ref()
                    .and_then(StochasticMapExecutionControl::wait_until_runnable)
                {
                    return Err(StochasticMapParallelError::Stopped {
                        sample_index,
                        reason,
                    });
                }
                let map = match map {
                    Ok(map) => map,
                    Err(BsmError::ExecutionStopped { reason }) => {
                        return Err(StochasticMapParallelError::Stopped {
                            sample_index,
                            reason,
                        });
                    }
                    Err(source) => {
                        return Err(StochasticMapParallelError::Sampling {
                            sample_index,
                            source,
                        });
                    }
                };
                let map_event_count = map.anagenetic_event_count().map_err(|source| {
                    StochasticMapParallelError::Sampling {
                        sample_index,
                        source,
                    }
                })?;
                let attempted_anagenetic_events = completed_anagenetic_events
                    .checked_add(map_event_count)
                    .ok_or_else(|| StochasticMapParallelError::Sampling {
                        sample_index,
                        source: BsmError::AnageneticEventCountOverflow,
                    })?;
                if let Some(limit) = task_limits.max_anagenetic_events
                    && attempted_anagenetic_events > limit
                {
                    return Err(
                        StochasticMapParallelError::TotalAnageneticEventLimitExceeded {
                            sample_index,
                            limit,
                            completed: completed_anagenetic_events,
                            attempted: attempted_anagenetic_events,
                        },
                    );
                }
                consumer(sample_index, &map).map_err(|source| {
                    StochasticMapParallelError::Consumer {
                        sample_index,
                        source,
                    }
                })?;
                completed_anagenetic_events = attempted_anagenetic_events;
            }
        }
        Ok(())
    }

    /// Resolve the effective worker/window counts for a parallel sampling run.
    ///
    /// When a buffer budget is configured, this uses a conservative logical
    /// upper bound for completed history objects and reduces both values so the
    /// ordered result window fits. A finite per-map event limit is required
    /// because otherwise the retained event vector has no finite upper bound.
    pub fn plan_indexed_parallel(
        &self,
        sample_count: usize,
        options: &StochasticMapParallelOptions,
    ) -> Result<StochasticMapParallelPlan, StochasticMapParallelPlanError> {
        if options.threads == 0 {
            return Err(StochasticMapParallelPlanError::ZeroThreads);
        }
        if options.max_in_flight == 0 {
            return Err(StochasticMapParallelPlanError::ZeroMaxInFlight);
        }
        if sample_count == 0 {
            return Ok(StochasticMapParallelPlan {
                threads: 0,
                max_in_flight: 0,
                retained_bytes_per_sample_upper_bound: None,
                buffered_history_bytes_upper_bound: None,
            });
        }

        let requested_threads = options.threads.min(sample_count);
        let requested_max_in_flight = options.max_in_flight.min(sample_count);
        if requested_max_in_flight < requested_threads {
            return Err(StochasticMapParallelPlanError::MaxInFlightBelowThreads {
                threads: requested_threads,
                max_in_flight: requested_max_in_flight,
            });
        }

        let Some(budget_bytes) = options.max_buffered_history_bytes else {
            return Ok(StochasticMapParallelPlan {
                threads: requested_threads,
                max_in_flight: requested_max_in_flight,
                retained_bytes_per_sample_upper_bound: None,
                buffered_history_bytes_upper_bound: None,
            });
        };
        let Some(event_limit) = options.limits.max_anagenetic_events_per_map else {
            return Err(
                StochasticMapParallelPlanError::MemoryBudgetRequiresPerMapEventLimit {
                    budget_bytes,
                },
            );
        };
        let bytes_per_sample = self
            .retained_bytes_per_sample_upper_bound(event_limit)
            .map_err(|_| StochasticMapParallelPlanError::RetainedHistorySizeOverflow)?;
        let affordable_samples = budget_bytes / bytes_per_sample;
        if affordable_samples == 0 {
            return Err(StochasticMapParallelPlanError::MemoryBudgetTooSmall {
                budget_bytes,
                minimum_bytes: bytes_per_sample,
            });
        }
        let max_in_flight = requested_max_in_flight.min(affordable_samples);
        let threads = requested_threads.min(max_in_flight);
        let buffered_history_bytes_upper_bound = bytes_per_sample
            .checked_mul(max_in_flight)
            .ok_or(StochasticMapParallelPlanError::RetainedHistorySizeOverflow)?;

        Ok(StochasticMapParallelPlan {
            threads,
            max_in_flight,
            retained_bytes_per_sample_upper_bound: Some(bytes_per_sample),
            buffered_history_bytes_upper_bound: Some(buffered_history_bytes_upper_bound),
        })
    }

    fn retained_bytes_per_sample_upper_bound(
        &self,
        max_anagenetic_events: usize,
    ) -> Result<usize, BsmError> {
        let edge_count = self.tree.edges().len();
        let segment_count = (0..edge_count).try_fold(0_usize, |total, edge_index| {
            total
                .checked_add(self.propagator.history_segment_count(edge_index))
                .ok_or(BsmError::RetainedHistorySizeOverflow)
        })?;
        let internal_count = self.tree.cladogenesis_node_count();
        let mut bytes = size_of::<Result<BiogeographicStochasticMap, BsmError>>();
        add_vector_capacity_bytes::<usize>(&mut bytes, self.tree.node_count())?;
        add_vector_capacity_bytes::<BranchEndpointSample>(&mut bytes, edge_count)?;
        add_vector_capacity_bytes::<CladogeneticSplitSample>(&mut bytes, internal_count)?;
        add_vector_capacity_bytes::<BranchHistory>(&mut bytes, edge_count)?;
        add_vector_capacity_bytes::<BranchSegmentHistory>(&mut bytes, segment_count)?;
        add_vector_capacity_bytes::<AnageneticEventSample>(&mut bytes, max_anagenetic_events)?;
        Ok(bytes)
    }

    #[cfg(test)]
    fn sample_branch_history<R: Rng + ?Sized>(
        &self,
        endpoint: BranchEndpointSample,
        rng: &mut R,
    ) -> Result<BranchHistory, BsmError> {
        let mut event_budget = AnageneticEventBudget::new(None);
        self.sample_branch_history_with_budget(endpoint, rng, &mut event_budget, None)
    }

    fn sample_branch_history_with_budget<R: Rng + ?Sized>(
        &self,
        endpoint: BranchEndpointSample,
        rng: &mut R,
        event_budget: &mut AnageneticEventBudget,
        control: Option<&StochasticMapExecutionControl>,
    ) -> Result<BranchHistory, BsmError> {
        check_execution_control(control)?;
        let edge = self.tree.edges()[endpoint.edge_index];
        let process_segments = self
            .propagator
            .history_segments(endpoint.edge_index, edge.length);
        if process_segments.is_empty() {
            if endpoint.start_state != endpoint.end_state {
                return Err(BsmError::MissingBranchSegments {
                    edge_index: endpoint.edge_index,
                    start_state: endpoint.start_state,
                    end_state: endpoint.end_state,
                });
            }
            return Ok(BranchHistory {
                edge_index: endpoint.edge_index,
                parent: endpoint.parent,
                child: endpoint.child,
                start_state: endpoint.start_state,
                end_state: endpoint.end_state,
                segments: Vec::new(),
            });
        }

        let total_duration: f64 = process_segments
            .iter()
            .map(|segment| segment.duration)
            .sum();
        let effective_branch_length = self
            .propagator
            .effective_branch_length(endpoint.edge_index, edge.length);
        let duration_tolerance = 1e-10 * effective_branch_length.abs().max(1.0);
        if (total_duration - effective_branch_length).abs() > duration_tolerance {
            return Err(BsmError::BranchSegmentDurationMismatch {
                edge_index: endpoint.edge_index,
                branch_length: effective_branch_length,
                segment_duration: total_duration,
            });
        }

        let state_count = self.states.len();
        let segment_count = process_segments.len();
        let mut suffix_likelihoods = vec![vec![0.0; state_count]; segment_count + 1];
        suffix_likelihoods[segment_count][endpoint.end_state] = 1.0;
        for segment_index in (0..segment_count).rev() {
            check_execution_control(control)?;
            let segment = process_segments[segment_index];
            let mut after = suffix_likelihoods[segment_index + 1].clone();
            if let Some(mask) = segment.state_mask {
                project_state_mask(&mut after, mask);
            }
            let mut before = propagate_uniformized(segment.q, segment.duration, &after)?;
            if let Some(mask) = segment.state_mask {
                project_state_mask(&mut before, mask);
            }
            suffix_likelihoods[segment_index] = before;
        }
        if suffix_likelihoods[0][endpoint.start_state] <= 0.0 {
            return Err(BsmError::ImpossiblePiecewiseEndpoint {
                edge_index: endpoint.edge_index,
                start_state: endpoint.start_state,
                end_state: endpoint.end_state,
            });
        }

        let mut boundary_states = Vec::with_capacity(segment_count + 1);
        boundary_states.push(endpoint.start_state);
        let mut current_state = endpoint.start_state;
        for (segment_index, segment) in process_segments.iter().copied().enumerate() {
            check_execution_control(control)?;
            let mut one_hot = vec![0.0; state_count];
            one_hot[current_state] = 1.0;
            let transition_row =
                propagate_uniformized_transpose(segment.q, segment.duration, &one_hot)?;
            let mut masses: Vec<f64> = transition_row
                .iter()
                .zip(&suffix_likelihoods[segment_index + 1])
                .map(|(transition, suffix)| transition * suffix)
                .collect();
            if let Some(mask) = segment.state_mask {
                project_state_mask(&mut masses, mask);
            }
            current_state =
                sample_weighted_index("piecewise branch boundary", endpoint.child, &masses, rng)?;
            boundary_states.push(current_state);
        }
        if current_state != endpoint.end_state {
            return Err(BsmError::PiecewisePathEndpointMismatch {
                edge_index: endpoint.edge_index,
                expected: endpoint.end_state,
                actual: current_state,
            });
        }

        let mut elapsed = 0.0;
        let mut segments = Vec::with_capacity(segment_count);
        for (segment_index, segment) in process_segments.iter().copied().enumerate() {
            check_execution_control(control)?;
            let bridge = sample_uniformized_bridge_adaptive_with_options(
                segment.q,
                segment.duration,
                boundary_states[segment_index],
                boundary_states[segment_index + 1],
                AdaptiveCtmcBridgeOptions {
                    max_real_events: event_budget.remaining(),
                    ..AdaptiveCtmcBridgeOptions::default()
                },
                rng,
            )
            .map_err(|error| event_budget.map_bridge_error(error))?;
            check_execution_control(control)?;
            event_budget.consume(bridge.events.len())?;
            let events = bridge
                .events
                .iter()
                .map(|event| {
                    Ok(AnageneticEventSample {
                        edge_index: endpoint.edge_index,
                        segment_index,
                        q_index: segment.q_index,
                        time_from_parent: elapsed + event.time,
                        from_state: event.from_state,
                        to_state: event.to_state,
                        kind: classify_anagenetic_event(
                            self.states,
                            endpoint.edge_index,
                            event.from_state,
                            event.to_state,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, BsmError>>()?;
            segments.push(BranchSegmentHistory {
                segment_index,
                q_index: segment.q_index,
                start_time_from_parent: elapsed,
                end_time_from_parent: elapsed + segment.duration,
                start_state: bridge.start_state,
                end_state: bridge.end_state,
                endpoint_probability: bridge.endpoint_probability,
                virtual_jump_count: bridge.virtual_jump_count,
                events: into_exact_vec(events),
            });
            elapsed += segment.duration;
        }

        Ok(BranchHistory {
            edge_index: endpoint.edge_index,
            parent: endpoint.parent,
            child: endpoint.child,
            start_state: endpoint.start_state,
            end_state: endpoint.end_state,
            segments: into_exact_vec(segments),
        })
    }

    fn sample_branch_endpoint<R: Rng + ?Sized>(
        &self,
        parent: usize,
        child: TreeChild,
        start_state: usize,
        rng: &mut R,
    ) -> Result<BranchEndpointSample, BsmError> {
        let transition_row = self.transition_row(child, start_state)?;
        let endpoint_masses: Vec<f64> = transition_row
            .iter()
            .zip(&self.pruning.scaled_likelihoods[child.node])
            .map(|(transition, likelihood)| transition * likelihood)
            .collect();
        let end_state =
            sample_weighted_index("branch endpoint", child.node, &endpoint_masses, rng)?;

        Ok(BranchEndpointSample {
            edge_index: child.edge_index,
            parent,
            child: child.node,
            start_state,
            end_state,
        })
    }

    fn transition_row(&self, child: TreeChild, start_state: usize) -> Result<&[f64], BsmError> {
        let cached = self.transition_rows[child.edge_index][start_state].get_or_init(|| {
            let mut one_hot = vec![0.0; self.propagator.state_count()];
            one_hot[start_state] = 1.0;
            self.propagator
                .propagate_transpose(child.edge_index, child.length, &one_hot)
                .map_err(PruningError::from)
        });
        cached
            .as_deref()
            .map_err(|error| BsmError::Pruning(error.clone()))
    }
}

struct AnageneticEventBudget {
    limit: Option<usize>,
    used: usize,
}

impl AnageneticEventBudget {
    fn new(limit: Option<usize>) -> Self {
        Self { limit, used: 0 }
    }

    fn remaining(&self) -> Option<usize> {
        self.limit.map(|limit| limit - self.used)
    }

    fn consume(&mut self, count: usize) -> Result<(), BsmError> {
        let attempted = self
            .used
            .checked_add(count)
            .ok_or(BsmError::AnageneticEventCountOverflow)?;
        if let Some(limit) = self.limit
            && attempted > limit
        {
            return Err(BsmError::AnageneticEventLimitExceeded { limit, attempted });
        }
        self.used = attempted;
        Ok(())
    }

    fn map_bridge_error(&self, error: CtmcBridgeError) -> BsmError {
        match error {
            CtmcBridgeError::RealEventLimitExceeded {
                attempted: additional,
                ..
            } => {
                let attempted = self.used.saturating_add(additional);
                BsmError::AnageneticEventLimitExceeded {
                    limit: self
                        .limit
                        .expect("bridge event limit is only configured for a finite map budget"),
                    attempted,
                }
            }
            other => BsmError::CtmcBridge(other),
        }
    }
}

fn project_state_mask(values: &mut [f64], mask: &StateMask) {
    debug_assert_eq!(values.len(), mask.len());
    for (value, allowed) in values.iter_mut().zip(mask.values()) {
        if !allowed {
            *value = 0.0;
        }
    }
}

fn classify_anagenetic_event(
    states: &StateSpace,
    edge_index: usize,
    from_state: usize,
    to_state: usize,
) -> Result<AnageneticEventKind, BsmError> {
    let from = states
        .get(from_state)
        .expect("bridge event source state must exist in the state space");
    let to = states
        .get(to_state)
        .expect("bridge event target state must exist in the state space");
    let changed_bits = from.bits() ^ to.bits();
    if changed_bits.count_ones() == 2 && from.size() == 1 && to.size() == 1 {
        return Ok(AnageneticEventKind::RangeSwitching {
            from_area: from.bits().trailing_zeros() as u8,
            to_area: to.bits().trailing_zeros() as u8,
        });
    }
    if changed_bits.count_ones() != 1 {
        return Err(BsmError::InvalidAnageneticTransition {
            edge_index,
            from_state,
            to_state,
        });
    }
    let area = changed_bits.trailing_zeros() as u8;
    match (from.contains(area), to.contains(area)) {
        (false, true) => Ok(AnageneticEventKind::RangeExpansion { area }),
        (true, false) => Ok(AnageneticEventKind::LocalExtirpation { area }),
        _ => Err(BsmError::InvalidAnageneticTransition {
            edge_index,
            from_state,
            to_state,
        }),
    }
}

fn check_execution_control(
    control: Option<&StochasticMapExecutionControl>,
) -> Result<(), BsmError> {
    match control.and_then(StochasticMapExecutionControl::wait_until_runnable) {
        Some(reason) => Err(BsmError::ExecutionStopped { reason }),
        None => Ok(()),
    }
}

fn into_exact_vec<T>(values: Vec<T>) -> Vec<T> {
    values.into_boxed_slice().into_vec()
}

impl CladogeneticSplitSample {
    fn from_scenario(node: usize, scenario: CladogeneticScenario) -> Self {
        Self {
            node,
            ancestor: scenario.ancestor,
            left: scenario.left,
            right: scenario.right,
            weight: scenario.weight,
        }
    }
}

fn sample_weighted_index<R: Rng + ?Sized>(
    stage: &'static str,
    node: usize,
    masses: &[f64],
    rng: &mut R,
) -> Result<usize, BsmError> {
    sample_weighted_index_with_draw(stage, node, masses, rng.random::<f64>())
}

fn sample_weighted_index_with_draw(
    stage: &'static str,
    node: usize,
    masses: &[f64],
    draw: f64,
) -> Result<usize, BsmError> {
    let total = validate_sampling_masses(stage, node, masses)?;
    if !draw.is_finite() || !(0.0..1.0).contains(&draw) {
        return Err(BsmError::InvalidRandomDraw { value: draw });
    }

    let threshold = draw * total;
    let mut cumulative = 0.0;
    let mut last_positive = None;
    for (index, mass) in masses.iter().copied().enumerate() {
        if mass <= 0.0 {
            continue;
        }
        last_positive = Some(index);
        cumulative += mass;
        if threshold < cumulative {
            return Ok(index);
        }
    }

    Ok(last_positive.expect("positive total mass must have a positive entry"))
}

fn validate_sampling_masses(
    stage: &'static str,
    node: usize,
    masses: &[f64],
) -> Result<f64, BsmError> {
    let mut total = 0.0;
    for (item, mass) in masses.iter().copied().enumerate() {
        if !mass.is_finite() || mass < 0.0 {
            return Err(BsmError::InvalidSamplingMass {
                stage,
                node,
                item,
                value: mass,
            });
        }
        total += mass;
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(BsmError::NonPositiveSamplingMass {
            stage,
            node,
            value: total,
        });
    }
    Ok(total)
}

#[derive(Clone, Debug, PartialEq)]
pub enum BsmError {
    Anagenesis(AnagenesisError),
    Cladogenesis(CladogenesisError),
    Pruning(PruningError),
    Propagation(PropagationError),
    CtmcBridge(CtmcBridgeError),
    MissingSplitScenarios {
        node: usize,
        ancestor: usize,
    },
    InvalidSamplingMass {
        stage: &'static str,
        node: usize,
        item: usize,
        value: f64,
    },
    NonPositiveSamplingMass {
        stage: &'static str,
        node: usize,
        value: f64,
    },
    InvalidRandomDraw {
        value: f64,
    },
    SampleIndexOutOfRange {
        sample_index: usize,
    },
    ExecutionStopped {
        reason: StochasticMapStopReason,
    },
    IncompleteNodeState {
        node: usize,
    },
    IncompleteBranchEndpoint {
        edge_index: usize,
    },
    IncompleteSplit {
        node: usize,
    },
    MissingBranchSegments {
        edge_index: usize,
        start_state: usize,
        end_state: usize,
    },
    BranchSegmentDurationMismatch {
        edge_index: usize,
        branch_length: f64,
        segment_duration: f64,
    },
    ImpossiblePiecewiseEndpoint {
        edge_index: usize,
        start_state: usize,
        end_state: usize,
    },
    PiecewisePathEndpointMismatch {
        edge_index: usize,
        expected: usize,
        actual: usize,
    },
    InvalidAnageneticTransition {
        edge_index: usize,
        from_state: usize,
        to_state: usize,
    },
    AnageneticEventLimitExceeded {
        limit: usize,
        attempted: usize,
    },
    AnageneticEventCountOverflow,
    RetainedHistorySizeOverflow,
}

impl fmt::Display for BsmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anagenesis(error) => write!(f, "anagenesis setup failed: {error}"),
            Self::Cladogenesis(error) => write!(f, "cladogenesis setup failed: {error}"),
            Self::Pruning(error) => write!(f, "stochastic traceback setup failed: {error}"),
            Self::Propagation(error) => write!(f, "branch-history propagation failed: {error}"),
            Self::CtmcBridge(error) => write!(f, "conditional CTMC bridge failed: {error}"),
            Self::MissingSplitScenarios { node, ancestor } => write!(
                f,
                "node {node} has no cladogenetic scenarios for sampled ancestor state {ancestor}"
            ),
            Self::InvalidSamplingMass {
                stage,
                node,
                item,
                value,
            } => write!(
                f,
                "{stage} sampling mass at node {node}, item {item} must be finite and non-negative, got {value}"
            ),
            Self::NonPositiveSamplingMass { stage, node, value } => write!(
                f,
                "{stage} sampling mass at node {node} must have a positive finite sum, got {value}"
            ),
            Self::InvalidRandomDraw { value } => write!(
                f,
                "weighted sampling draw must be finite and in [0, 1), got {value}"
            ),
            Self::SampleIndexOutOfRange { sample_index } => write!(
                f,
                "sample index {sample_index} cannot be represented by the indexed RNG protocol"
            ),
            Self::ExecutionStopped { reason } => {
                write!(f, "stochastic-history execution stopped: {reason}")
            }
            Self::IncompleteNodeState { node } => {
                write!(f, "stochastic traceback did not assign node {node}")
            }
            Self::IncompleteBranchEndpoint { edge_index } => write!(
                f,
                "stochastic traceback did not assign branch endpoint for edge {edge_index}"
            ),
            Self::IncompleteSplit { node } => {
                write!(
                    f,
                    "stochastic traceback did not assign a split at node {node}"
                )
            }
            Self::MissingBranchSegments {
                edge_index,
                start_state,
                end_state,
            } => write!(
                f,
                "edge {edge_index} has no process segments but must connect state {start_state} to {end_state}"
            ),
            Self::BranchSegmentDurationMismatch {
                edge_index,
                branch_length,
                segment_duration,
            } => write!(
                f,
                "edge {edge_index} has branch length {branch_length}, but its process segments sum to {segment_duration}"
            ),
            Self::ImpossiblePiecewiseEndpoint {
                edge_index,
                start_state,
                end_state,
            } => write!(
                f,
                "edge {edge_index} cannot connect state {start_state} to {end_state} through its piecewise process"
            ),
            Self::PiecewisePathEndpointMismatch {
                edge_index,
                expected,
                actual,
            } => write!(
                f,
                "edge {edge_index} sampled piecewise path ended in state {actual}, expected {expected}"
            ),
            Self::InvalidAnageneticTransition {
                edge_index,
                from_state,
                to_state,
            } => write!(
                f,
                "edge {edge_index} sampled unsupported anagenetic transition {from_state}->{to_state}; expected one-area gain or loss"
            ),
            Self::AnageneticEventLimitExceeded { limit, attempted } => write!(
                f,
                "stochastic history sampled at least {attempted} anagenetic events, exceeding the per-map limit of {limit}"
            ),
            Self::AnageneticEventCountOverflow => {
                write!(
                    f,
                    "stochastic-history anagenetic event count overflowed usize"
                )
            }
            Self::RetainedHistorySizeOverflow => write!(
                f,
                "stochastic-history retained-memory size calculation overflowed usize"
            ),
        }
    }
}

impl Error for BsmError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Anagenesis(error) => Some(error),
            Self::Cladogenesis(error) => Some(error),
            Self::Pruning(error) => Some(error),
            Self::Propagation(error) => Some(error),
            Self::CtmcBridge(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub enum StochasticMapStreamError<E> {
    Sampling(BsmError),
    Consumer(E),
}

impl<E: fmt::Display> fmt::Display for StochasticMapStreamError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Sampling(error) => write!(f, "stochastic-history sampling failed: {error}"),
            Self::Consumer(error) => write!(f, "stochastic-history consumer failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for StochasticMapStreamError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Sampling(error) => Some(error),
            Self::Consumer(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub enum StochasticMapParallelError<E> {
    ZeroThreads,
    ZeroMaxInFlight,
    InvalidSampleRange {
        start: usize,
        end: usize,
    },
    MaxInFlightBelowThreads {
        threads: usize,
        max_in_flight: usize,
    },
    Planning(StochasticMapParallelPlanError),
    ThreadPoolBuild(rayon::ThreadPoolBuildError),
    Preparation(BsmError),
    Sampling {
        sample_index: usize,
        source: BsmError,
    },
    TotalAnageneticEventLimitExceeded {
        sample_index: usize,
        limit: usize,
        completed: usize,
        attempted: usize,
    },
    Stopped {
        sample_index: usize,
        reason: StochasticMapStopReason,
    },
    Consumer {
        sample_index: usize,
        source: E,
    },
}

impl<E: fmt::Display> fmt::Display for StochasticMapParallelError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroThreads => write!(f, "stochastic-history thread count must be positive"),
            Self::ZeroMaxInFlight => {
                write!(f, "stochastic-history max-in-flight must be positive")
            }
            Self::InvalidSampleRange { start, end } => write!(
                f,
                "stochastic-history sample range start {start} exceeds end {end}"
            ),
            Self::MaxInFlightBelowThreads {
                threads,
                max_in_flight,
            } => write!(
                f,
                "stochastic-history max-in-flight {max_in_flight} is below worker count {threads}"
            ),
            Self::Planning(error) => write!(f, "stochastic-history planning failed: {error}"),
            Self::ThreadPoolBuild(error) => {
                write!(
                    f,
                    "failed to create stochastic-history thread pool: {error}"
                )
            }
            Self::Preparation(error) => {
                write!(f, "failed to prepare stochastic-history sampler: {error}")
            }
            Self::Sampling {
                sample_index,
                source,
            } => write!(
                f,
                "stochastic-history sample {sample_index} failed: {source}"
            ),
            Self::TotalAnageneticEventLimitExceeded {
                sample_index,
                limit,
                completed,
                attempted,
            } => write!(
                f,
                "stochastic-history sample {sample_index} would raise the task anagenetic-event count from {completed} to {attempted}, exceeding the limit of {limit}"
            ),
            Self::Stopped {
                sample_index,
                reason,
            } => write!(
                f,
                "stochastic-history execution stopped before sample {sample_index}: {reason}"
            ),
            Self::Consumer {
                sample_index,
                source,
            } => write!(
                f,
                "stochastic-history consumer failed for sample {sample_index}: {source}"
            ),
        }
    }
}

impl<E: Error + 'static> Error for StochasticMapParallelError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Planning(error) => Some(error),
            Self::ThreadPoolBuild(error) => Some(error),
            Self::Preparation(error) => Some(error),
            Self::Sampling { source, .. } => Some(source),
            Self::Consumer { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl<E> StochasticMapParallelError<E> {
    fn from_plan_error(error: StochasticMapParallelPlanError) -> Self {
        match error {
            StochasticMapParallelPlanError::ZeroThreads => Self::ZeroThreads,
            StochasticMapParallelPlanError::ZeroMaxInFlight => Self::ZeroMaxInFlight,
            StochasticMapParallelPlanError::MaxInFlightBelowThreads {
                threads,
                max_in_flight,
            } => Self::MaxInFlightBelowThreads {
                threads,
                max_in_flight,
            },
            other => Self::Planning(other),
        }
    }
}

impl From<AnagenesisError> for BsmError {
    fn from(value: AnagenesisError) -> Self {
        Self::Anagenesis(value)
    }
}

impl From<CladogenesisError> for BsmError {
    fn from(value: CladogenesisError) -> Self {
        Self::Cladogenesis(value)
    }
}

impl From<PruningError> for BsmError {
    fn from(value: PruningError) -> Self {
        Self::Pruning(value)
    }
}

impl From<PropagationError> for BsmError {
    fn from(value: PropagationError) -> Self {
        Self::Propagation(value)
    }
}

impl From<CtmcBridgeError> for BsmError {
    fn from(value: CtmcBridgeError) -> Self {
        Self::CtmcBridge(value)
    }
}

impl From<LikelihoodEngineError> for BsmError {
    fn from(value: LikelihoodEngineError) -> Self {
        match value {
            LikelihoodEngineError::Anagenesis(error) => Self::Anagenesis(error),
            LikelihoodEngineError::Cladogenesis(error) => Self::Cladogenesis(error),
            LikelihoodEngineError::Pruning(error) => Self::Pruning(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rand::rngs::StdRng;
    use rand::{RngExt, SeedableRng};

    use super::*;
    use crate::constraints::{BinaryAreaMatrix, RangeStateConstraint};
    use crate::dispersal::{
        AnageneticTimeStratum, DispersalMultiplierMatrix, ExtirpationMultiplierVector,
        TimeStratifiedAnagenesis,
    };
    use crate::engine::LikelihoodEngine;
    use crate::model::ModelConfig;
    use crate::pruning::{RootPrior, TipLikelihood};
    use crate::state::{AreaSet, StateSpace};
    use crate::tree::Edge;

    #[test]
    fn weighted_sampling_obeys_boundaries() {
        let masses = [1.0, 0.0, 3.0];
        assert_eq!(
            sample_weighted_index_with_draw("test", 0, &masses, 0.0).unwrap(),
            0
        );
        assert_eq!(
            sample_weighted_index_with_draw("test", 0, &masses, 0.249999).unwrap(),
            0
        );
        assert_eq!(
            sample_weighted_index_with_draw("test", 0, &masses, 0.25).unwrap(),
            2
        );
        assert_eq!(
            sample_weighted_index_with_draw("test", 0, &masses, 0.999999).unwrap(),
            2
        );
    }

    #[test]
    fn classifies_singleton_to_singleton_transition_as_range_switching() {
        let states = StateSpace::new(2, 1, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();

        assert_eq!(
            classify_anagenetic_event(&states, 0, a, b).unwrap(),
            AnageneticEventKind::RangeSwitching {
                from_area: 0,
                to_area: 1,
            }
        );
    }

    #[test]
    fn weighted_sampling_rejects_invalid_inputs() {
        assert!(matches!(
            sample_weighted_index_with_draw("test", 4, &[0.0, 0.0], 0.5),
            Err(BsmError::NonPositiveSamplingMass { node: 4, .. })
        ));
        assert!(matches!(
            sample_weighted_index_with_draw("test", 0, &[1.0, f64::NAN], 0.5),
            Err(BsmError::InvalidSamplingMass { item: 1, .. })
        ));
        assert!(matches!(
            sample_weighted_index_with_draw("test", 0, &[1.0], 1.0),
            Err(BsmError::InvalidRandomDraw { value: 1.0 })
        ));
    }

    #[test]
    fn indexed_rng_protocol_separates_samples() {
        let mut sample_zero = indexed_bsm_rng(42, 0);
        let mut sample_one = indexed_bsm_rng(42, 1);
        let zero_draws = [sample_zero.random::<u64>(), sample_zero.random::<u64>()];
        let one_draws = [sample_one.random::<u64>(), sample_one.random::<u64>()];

        assert_eq!(
            zero_draws,
            [10_556_617_607_173_427_324, 12_275_386_645_274_581_333]
        );
        assert_eq!(
            one_draws,
            [8_139_808_042_003_824_895, 5_235_440_105_153_271_624]
        );
        assert_eq!(INDEXED_BSM_RNG_PROTOCOL, "indexed-chacha12-v1");
    }

    #[test]
    fn zero_length_dec_traceback_is_coherent_and_deterministic() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 0.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 0.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let ab = states.index_of(AreaSet::from_bits(0b11)).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), b),
            },
        ];
        let model = ModelConfig::preset_dec(0.1, 0.2).unwrap();
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();

        let samples = engine
            .sample_history_skeletons_seeded(&model, &pruning, 32, 7)
            .unwrap();

        for sample in samples {
            assert_eq!(sample.root_state, ab);
            assert_eq!(sample.node_states, vec![a, b, ab]);
            assert_eq!(
                sample.branch_endpoints,
                vec![
                    BranchEndpointSample {
                        edge_index: 0,
                        parent: 2,
                        child: 0,
                        start_state: a,
                        end_state: a,
                    },
                    BranchEndpointSample {
                        edge_index: 1,
                        parent: 2,
                        child: 1,
                        start_state: b,
                        end_state: b,
                    },
                ]
            );
            assert_eq!(sample.splits.len(), 1);
            assert_eq!(sample.splits[0].node, 2);
            assert_eq!(sample.splits[0].ancestor, ab);
            assert_eq!(sample.splits[0].left, a);
            assert_eq!(sample.splits[0].right, b);
        }

        let maps = engine
            .sample_stochastic_maps_seeded(&model, &pruning, 8, 7)
            .unwrap();
        for map in maps {
            assert_history_is_coherent(&tree, &map.skeleton);
            assert_map_is_coherent(&tree, &states, &map);
            assert!(
                map.branches
                    .iter()
                    .flat_map(|branch| &branch.segments)
                    .all(|segment| segment.events.is_empty())
            );
        }
    }

    #[test]
    fn direct_ancestor_history_copies_state_without_recording_a_split() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1e-7,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap()
        .with_direct_ancestor_hooks_below(1e-6)
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), a),
            },
        ];
        let model = ModelConfig::preset_dec(0.0, 0.0).unwrap();
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();

        let samples = engine
            .sample_history_skeletons_seeded(&model, &pruning, 8, 11)
            .unwrap();
        for sample in samples {
            assert_eq!(sample.root_state, a);
            assert_eq!(sample.node_states, vec![a, a, a]);
            assert!(sample.splits.is_empty());
            assert!(
                sample
                    .branch_endpoints
                    .iter()
                    .all(|endpoint| endpoint.start_state == a && endpoint.end_state == a)
            );
        }
    }

    #[test]
    fn range_switching_q_samples_atomic_a_events_end_to_end() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 1, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), b),
            },
        ];
        let mut model = ModelConfig::preset_dec(0.0, 0.0).unwrap();
        model.anagenesis = model.anagenesis.with_range_switching_rate(0.8).unwrap();
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();
        let maps = engine
            .sample_stochastic_maps_seeded(&model, &pruning, 32, 20260717)
            .unwrap();

        for map in maps {
            assert_history_is_coherent(&tree, &map.skeleton);
            assert_map_is_coherent(&tree, &states, &map);
            let events = map
                .branches
                .iter()
                .flat_map(|branch| &branch.segments)
                .flat_map(|segment| &segment.events)
                .collect::<Vec<_>>();
            assert!(!events.is_empty());
            assert!(
                events
                    .iter()
                    .all(|event| matches!(event.kind, AnageneticEventKind::RangeSwitching { .. }))
            );
        }
    }

    #[test]
    fn streaming_maps_match_batch_sequence_and_stop_on_consumer_error() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(1, 1, true).unwrap();
        let null = states.index_of(AreaSet::EMPTY).unwrap();
        let a = states.index_of(AreaSet::from_bits(1)).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), null),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), a),
            },
        ];
        let model = ModelConfig::preset_dec(0.4, 0.7).unwrap();
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();

        let batch = engine
            .sample_stochastic_maps_seeded(&model, &pruning, 16, 1234)
            .unwrap();
        let mut streamed = Vec::new();
        engine
            .try_for_each_stochastic_map_seeded(&model, &pruning, 16, 1234, |sample_index, map| {
                assert_eq!(sample_index, streamed.len());
                streamed.push(map.clone());
                Ok::<(), std::convert::Infallible>(())
            })
            .unwrap();
        assert_eq!(streamed, batch);

        for thread_count in [1, 2, 4, 8, 16] {
            let mut parallel = Vec::new();
            engine
                .try_for_each_stochastic_map_parallel_seeded(
                    &model,
                    &pruning,
                    16,
                    1234,
                    StochasticMapParallelOptions::new(thread_count, thread_count),
                    |sample_index, map| {
                        assert_eq!(sample_index, parallel.len());
                        parallel.push(map.clone());
                        Ok::<(), std::convert::Infallible>(())
                    },
                )
                .unwrap();
            assert_eq!(parallel, batch, "thread count {thread_count}");
        }

        let pause = StochasticMapPauseToken::new();
        assert!(pause.pause());
        let paused_control =
            StochasticMapExecutionControl::new(StochasticMapCancellationToken::new(), None)
                .with_pause_token(pause.clone());
        std::thread::scope(|scope| {
            let handle = scope.spawn(|| {
                let mut paused = Vec::new();
                engine
                    .try_for_each_stochastic_map_parallel_seeded(
                        &model,
                        &pruning,
                        16,
                        1234,
                        StochasticMapParallelOptions::new(4, 8)
                            .with_execution_control(paused_control),
                        |sample_index, map| {
                            assert_eq!(sample_index, paused.len());
                            paused.push(map.clone());
                            Ok::<(), std::convert::Infallible>(())
                        },
                    )
                    .unwrap();
                paused
            });
            std::thread::sleep(Duration::from_millis(100));
            assert!(!handle.is_finished());
            assert!(pause.resume());
            assert_eq!(handle.join().unwrap(), batch);
        });

        let paused_cancellation = StochasticMapCancellationToken::new();
        let paused_for_cancel = StochasticMapPauseToken::new();
        paused_for_cancel.pause();
        let cancelled_while_paused =
            StochasticMapExecutionControl::new(paused_cancellation.clone(), None)
                .with_pause_token(paused_for_cancel);
        paused_cancellation.cancel();
        assert_eq!(
            cancelled_while_paused.wait_until_runnable(),
            Some(StochasticMapStopReason::Cancelled)
        );
        let paused_past_deadline = StochasticMapPauseToken::new();
        paused_past_deadline.pause();
        assert_eq!(
            StochasticMapExecutionControl::new(
                StochasticMapCancellationToken::new(),
                Some(Instant::now()),
            )
            .with_pause_token(paused_past_deadline)
            .wait_until_runnable(),
            Some(StochasticMapStopReason::DeadlineExceeded)
        );

        let mut resumed_suffix = Vec::new();
        engine
            .try_for_each_stochastic_map_parallel_seeded_range(
                &model,
                &pruning,
                5..16,
                1234,
                StochasticMapParallelOptions::new(4, 8),
                |sample_index, map| {
                    assert_eq!(sample_index, resumed_suffix.len() + 5);
                    resumed_suffix.push(map.clone());
                    Ok::<(), std::convert::Infallible>(())
                },
            )
            .unwrap();
        assert_eq!(resumed_suffix, batch[5..]);
        assert!(matches!(
            engine.try_for_each_stochastic_map_parallel_seeded_range(
                &model,
                &pruning,
                Range { start: 5, end: 4 },
                1234,
                StochasticMapParallelOptions::new(2, 4),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            ),
            Err(StochasticMapParallelError::InvalidSampleRange { start: 5, end: 4 })
        ));

        assert!(matches!(
            engine.try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                4,
                1234,
                StochasticMapParallelOptions::new(0, 1),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            ),
            Err(StochasticMapParallelError::ZeroThreads)
        ));
        assert!(matches!(
            engine.try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                4,
                1234,
                StochasticMapParallelOptions::new(4, 0),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            ),
            Err(StochasticMapParallelError::ZeroMaxInFlight)
        ));
        assert!(matches!(
            engine.try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                4,
                1234,
                StochasticMapParallelOptions::new(4, 3),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            ),
            Err(StochasticMapParallelError::MaxInFlightBelowThreads {
                threads: 4,
                max_in_flight: 3
            })
        ));

        let mut parallel_visited = Vec::new();
        let parallel_error = engine
            .try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                16,
                1234,
                StochasticMapParallelOptions::new(4, 8),
                |sample_index, _| {
                    parallel_visited.push(sample_index);
                    if sample_index == 2 {
                        Err("stop")
                    } else {
                        Ok(())
                    }
                },
            )
            .unwrap_err();
        assert_eq!(parallel_visited, vec![0, 1, 2]);
        assert!(matches!(
            parallel_error,
            StochasticMapParallelError::Consumer {
                sample_index: 2,
                source: "stop"
            }
        ));

        let event_counts = batch
            .iter()
            .map(BiogeographicStochasticMap::anagenetic_event_count)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let max_events_per_map = event_counts.iter().copied().max().unwrap();
        let sampler = engine
            .prepare_stochastic_map_sampler(&model, &pruning)
            .unwrap();
        let sizing_options = StochasticMapParallelOptions::new(8, 8)
            .with_limits(StochasticMapLimits::new(Some(max_events_per_map)))
            .with_max_buffered_history_bytes(Some(usize::MAX));
        let sizing_plan = sampler.plan_indexed_parallel(16, &sizing_options).unwrap();
        let bytes_per_sample = sizing_plan.retained_bytes_per_sample_upper_bound.unwrap();
        for map in &batch {
            let retained_with_result_slot = map.retained_heap_bytes().unwrap()
                + std::mem::size_of::<Result<BiogeographicStochasticMap, BsmError>>();
            assert!(retained_with_result_slot <= bytes_per_sample);
        }

        let three_sample_budget = bytes_per_sample.checked_mul(3).unwrap();
        let budgeted_options = StochasticMapParallelOptions::new(8, 8)
            .with_limits(StochasticMapLimits::new(Some(max_events_per_map)))
            .with_max_buffered_history_bytes(Some(three_sample_budget));
        let budgeted_plan = sampler
            .plan_indexed_parallel(16, &budgeted_options)
            .unwrap();
        assert_eq!(budgeted_plan.threads, 3);
        assert_eq!(budgeted_plan.max_in_flight, 3);
        assert_eq!(
            budgeted_plan.buffered_history_bytes_upper_bound,
            Some(three_sample_budget)
        );
        let mut budgeted = Vec::new();
        engine
            .try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                16,
                1234,
                budgeted_options,
                |sample_index, map| {
                    assert_eq!(sample_index, budgeted.len());
                    budgeted.push(map.clone());
                    Ok::<(), std::convert::Infallible>(())
                },
            )
            .unwrap();
        assert_eq!(budgeted, batch);

        assert!(matches!(
            sampler.plan_indexed_parallel(
                16,
                &StochasticMapParallelOptions::new(8, 8)
                    .with_max_buffered_history_bytes(Some(three_sample_budget))
            ),
            Err(
                StochasticMapParallelPlanError::MemoryBudgetRequiresPerMapEventLimit {
                    budget_bytes
                }
            ) if budget_bytes == three_sample_budget
        ));
        assert!(matches!(
            sampler.plan_indexed_parallel(
                16,
                &StochasticMapParallelOptions::new(8, 8)
                    .with_limits(StochasticMapLimits::new(Some(max_events_per_map)))
                    .with_max_buffered_history_bytes(Some(bytes_per_sample - 1))
            ),
            Err(StochasticMapParallelPlanError::MemoryBudgetTooSmall {
                budget_bytes,
                minimum_bytes,
            }) if budget_bytes == bytes_per_sample - 1 && minimum_bytes == bytes_per_sample
        ));
        let positive_samples = event_counts
            .iter()
            .enumerate()
            .filter_map(|(sample_index, count)| (*count > 0).then_some(sample_index))
            .collect::<Vec<_>>();
        assert!(positive_samples.len() >= 2);
        let event_limit_stop = positive_samples[1];
        let event_limit = event_counts[..event_limit_stop].iter().sum::<usize>();
        let attempted = event_limit + event_counts[event_limit_stop];
        for thread_count in [1, 4, 16] {
            let mut limited_prefix = Vec::new();
            let error = engine
                .try_for_each_stochastic_map_parallel_seeded(
                    &model,
                    &pruning,
                    16,
                    1234,
                    StochasticMapParallelOptions::new(thread_count, thread_count)
                        .with_task_limits(StochasticMapTaskLimits::new(Some(event_limit), 0)),
                    |sample_index, map| {
                        assert_eq!(sample_index, limited_prefix.len());
                        limited_prefix.push(map.clone());
                        Ok::<(), std::convert::Infallible>(())
                    },
                )
                .unwrap_err();
            assert_eq!(limited_prefix, batch[..event_limit_stop]);
            assert!(matches!(
                error,
                StochasticMapParallelError::TotalAnageneticEventLimitExceeded {
                    sample_index,
                    limit,
                    completed,
                    attempted: found_attempted,
                } if sample_index == event_limit_stop
                    && limit == event_limit
                    && completed == event_limit
                    && found_attempted == attempted
            ));
        }

        let resumed_limit_error = engine
            .try_for_each_stochastic_map_parallel_seeded_range(
                &model,
                &pruning,
                event_limit_stop..16,
                1234,
                StochasticMapParallelOptions::new(4, 8)
                    .with_task_limits(StochasticMapTaskLimits::new(Some(event_limit), event_limit)),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            )
            .unwrap_err();
        assert!(matches!(
            resumed_limit_error,
            StochasticMapParallelError::TotalAnageneticEventLimitExceeded {
                sample_index,
                limit,
                completed,
                attempted: found_attempted,
            } if sample_index == event_limit_stop
                && limit == event_limit
                && completed == event_limit
                && found_attempted == attempted
        ));

        let cancellation = StochasticMapCancellationToken::new();
        let control = StochasticMapExecutionControl::new(cancellation.clone(), None);
        let mut cancelled_prefix = Vec::new();
        let cancellation_error = engine
            .try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                16,
                1234,
                StochasticMapParallelOptions::new(4, 8).with_execution_control(control),
                |sample_index, map| {
                    cancelled_prefix.push(map.clone());
                    if sample_index == 2 {
                        cancellation.cancel();
                    }
                    Ok::<(), std::convert::Infallible>(())
                },
            )
            .unwrap_err();
        assert_eq!(cancelled_prefix, batch[..3]);
        assert!(matches!(
            cancellation_error,
            StochasticMapParallelError::Stopped {
                sample_index: 3,
                reason: StochasticMapStopReason::Cancelled,
            }
        ));

        let deadline_error = engine
            .try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                4,
                1234,
                StochasticMapParallelOptions::new(2, 4).with_execution_control(
                    StochasticMapExecutionControl::new(
                        StochasticMapCancellationToken::new(),
                        Some(Instant::now()),
                    ),
                ),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            )
            .unwrap_err();
        assert!(matches!(
            deadline_error,
            StochasticMapParallelError::Stopped {
                sample_index: 0,
                reason: StochasticMapStopReason::DeadlineExceeded,
            }
        ));

        let mut visited = Vec::new();
        let error = engine
            .try_for_each_stochastic_map_seeded(&model, &pruning, 16, 1234, |sample_index, _| {
                visited.push(sample_index);
                if sample_index == 2 {
                    Err("stop")
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(visited, vec![0, 1, 2]);
        assert!(matches!(error, StochasticMapStreamError::Consumer("stop")));
    }

    #[test]
    fn per_map_event_limit_accumulates_across_branches_without_changing_valid_samples() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(1, 1, true).unwrap();
        let null = states.index_of(AreaSet::EMPTY).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b1)).unwrap();
        let model = ModelConfig::preset_dec(0.0, 1.0).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), null),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), null),
            },
        ];
        let mut root_prior = vec![0.0; states.len()];
        root_prior[a] = 1.0;
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Given(&root_prior));
        let pruning = engine.evaluate(&model, &tips).unwrap();
        let sampler = engine
            .prepare_stochastic_map_sampler(&model, &pruning)
            .unwrap();

        let unlimited = sampler.sample_map_indexed(19, 0).unwrap();
        let exact_limit = sampler
            .sample_map_indexed_with_limits(19, 0, StochasticMapLimits::new(Some(2)))
            .unwrap();
        assert_eq!(unlimited, exact_limit);
        assert_eq!(
            unlimited
                .branches
                .iter()
                .flat_map(|branch| &branch.segments)
                .map(|segment| segment.events.len())
                .sum::<usize>(),
            2
        );

        assert_eq!(
            sampler.sample_map_indexed_with_limits(19, 0, StochasticMapLimits::new(Some(1))),
            Err(BsmError::AnageneticEventLimitExceeded {
                limit: 1,
                attempted: 2,
            })
        );

        let parallel_error = engine
            .try_for_each_stochastic_map_parallel_seeded(
                &model,
                &pruning,
                4,
                19,
                StochasticMapParallelOptions::new(2, 4)
                    .with_limits(StochasticMapLimits::new(Some(1))),
                |_, _| Ok::<(), std::convert::Infallible>(()),
            )
            .unwrap_err();
        assert!(matches!(
            parallel_error,
            StochasticMapParallelError::Sampling {
                sample_index: 0,
                source: BsmError::AnageneticEventLimitExceeded {
                    limit: 1,
                    attempted: 2,
                }
            }
        ));
    }

    #[test]
    fn piecewise_bridge_event_period_matches_one_way_analytic_probability() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(1, 1, true).unwrap();
        let null = states.index_of(AreaSet::EMPTY).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b1)).unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![
            AnageneticTimeStratum::new(
                0.6,
                None,
                Some(ExtirpationMultiplierVector::new(vec![0.2]).unwrap()),
            )
            .unwrap(),
            AnageneticTimeStratum::new(
                1.0,
                None,
                Some(ExtirpationMultiplierVector::new(vec![3.0]).unwrap()),
            )
            .unwrap(),
        ])
        .unwrap();
        let model = ModelConfig::preset_dec(0.0, 1.0)
            .unwrap()
            .with_time_stratified_anagenesis(schedule);
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), a),
            },
        ];
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();
        let sampler = engine
            .prepare_stochastic_map_sampler(&model, &pruning)
            .unwrap();
        let endpoint = BranchEndpointSample {
            edge_index: 0,
            parent: 2,
            child: 0,
            start_state: a,
            end_state: null,
        };
        let old_hazard = 3.0 * 0.4;
        let young_hazard = 0.2 * 0.6;
        let expected_old_probability =
            (1.0 - f64::exp(-old_hazard)) / (1.0 - f64::exp(-(old_hazard + young_hazard)));
        let sample_count = 20_000;
        let mut rng = StdRng::seed_from_u64(271828);
        let mut old_events = 0;

        for _ in 0..sample_count {
            let branch = sampler.sample_branch_history(endpoint, &mut rng).unwrap();
            assert_eq!(branch.segments.len(), 2);
            assert_eq!(branch.segments[0].q_index, 1);
            assert_eq!(branch.segments[1].q_index, 0);
            assert_eq!(branch.segments[0].start_time_from_parent, 0.0);
            assert!((branch.segments[0].end_time_from_parent - 0.4).abs() < 1e-12);
            assert!((branch.segments[1].start_time_from_parent - 0.4).abs() < 1e-12);
            assert!((branch.segments[1].end_time_from_parent - 1.0).abs() < 1e-12);
            let events: Vec<&AnageneticEventSample> = branch
                .segments
                .iter()
                .flat_map(|segment| &segment.events)
                .collect();
            assert_eq!(events.len(), 1);
            assert_eq!(events[0].from_state, a);
            assert_eq!(events[0].to_state, null);
            assert_eq!(
                events[0].kind,
                AnageneticEventKind::LocalExtirpation { area: 0 }
            );
            if events[0].q_index == 1 {
                old_events += 1;
                assert!(events[0].time_from_parent < 0.4);
            } else {
                assert_eq!(events[0].q_index, 0);
                assert!(events[0].time_from_parent >= 0.4);
            }
        }

        let empirical_old_probability = old_events as f64 / sample_count as f64;
        assert!(
            (empirical_old_probability - expected_old_probability).abs() < 0.015,
            "old-period event probability: empirical {empirical_old_probability}, expected {expected_old_probability}"
        );
    }

    #[test]
    fn high_rate_bridge_keeps_numerical_subdivisions_inside_one_biological_segment() {
        let tree = Tree::new(
            2,
            3,
            vec![
                Edge {
                    parent: 2,
                    child: 0,
                    length: 1.0,
                },
                Edge {
                    parent: 2,
                    child: 1,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(2, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b01)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b10)).unwrap();
        let model = ModelConfig::preset_dec(10_000.0, 10_000.0).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), b),
            },
        ];
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();
        let sampler = engine
            .prepare_stochastic_map_sampler(&model, &pruning)
            .unwrap();
        let endpoint = BranchEndpointSample {
            edge_index: 0,
            parent: 2,
            child: 0,
            start_state: a,
            end_state: b,
        };
        let mut rng = StdRng::seed_from_u64(20260716);

        let branch = sampler.sample_branch_history(endpoint, &mut rng).unwrap();

        assert_eq!(branch.segments.len(), 1);
        let segment = &branch.segments[0];
        assert_eq!(segment.segment_index, 0);
        assert_eq!(segment.q_index, 0);
        assert_eq!(segment.start_time_from_parent, 0.0);
        assert_eq!(segment.end_time_from_parent, 1.0);
        assert_eq!(segment.start_state, a);
        assert_eq!(segment.end_state, b);
        assert!(segment.endpoint_probability.is_finite());
        assert!(segment.endpoint_probability > 0.0);
        assert!(segment.virtual_jump_count > 10_000);
        assert!(!segment.events.is_empty());
        assert_eq!(segment.events.last().unwrap().to_state, b);
        assert!(
            segment
                .events
                .windows(2)
                .all(|pair| pair[0].time_from_parent <= pair[1].time_from_parent)
        );
        assert!(segment.events.iter().all(|event| {
            event.segment_index == 0
                && event.q_index == 0
                && event.time_from_parent >= 0.0
                && event.time_from_parent < 1.0
        }));
    }

    #[test]
    fn traceback_frequencies_match_exact_node_and_split_posteriors() {
        let tree = Tree::new(
            4,
            5,
            vec![
                Edge {
                    parent: 3,
                    child: 0,
                    length: 0.4,
                },
                Edge {
                    parent: 3,
                    child: 1,
                    length: 0.5,
                },
                Edge {
                    parent: 4,
                    child: 3,
                    length: 0.3,
                },
                Edge {
                    parent: 4,
                    child: 2,
                    length: 0.8,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(3, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b001)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b010)).unwrap();
        let c = states.index_of(AreaSet::from_bits(0b100)).unwrap();
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), b),
            },
            TipLikelihood {
                node: 2,
                likelihoods: one_hot(states.len(), c),
            },
        ];
        let model = ModelConfig::preset_dec_j(0.23, 0.11, 0.4).unwrap();
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Flat);
        let pruning = engine.evaluate(&model, &tips).unwrap();
        let exact_nodes = engine.node_state_posteriors(&model, &pruning).unwrap();
        let exact_splits = engine.split_scenario_posteriors(&model, &pruning).unwrap();

        let sample_count = 20_000;
        let samples = engine
            .sample_history_skeletons_seeded(&model, &pruning, sample_count, 20260715)
            .unwrap();
        let mut node_counts = vec![vec![0_usize; states.len()]; tree.node_count()];
        let mut split_counts = HashMap::new();

        for sample in &samples {
            assert_history_is_coherent(&tree, sample);
            for (node, state) in sample.node_states.iter().copied().enumerate() {
                node_counts[node][state] += 1;
            }
            for split in &sample.splits {
                *split_counts
                    .entry((split.node, split.ancestor, split.left, split.right))
                    .or_insert(0_usize) += 1;
            }
        }

        for posterior in exact_nodes {
            for (state, exact) in posterior.probabilities.iter().copied().enumerate() {
                let empirical = node_counts[posterior.node][state] as f64 / sample_count as f64;
                assert!(
                    (empirical - exact).abs() < 0.02,
                    "node {}, state {state}: empirical {empirical}, exact {exact}",
                    posterior.node
                );
            }
        }

        let mut exact_split_probabilities = HashMap::new();
        for split in exact_splits {
            *exact_split_probabilities
                .entry((split.node, split.ancestor, split.left, split.right))
                .or_insert(0.0) += split.probability;
        }
        for (key, exact) in exact_split_probabilities {
            let empirical =
                split_counts.get(&key).copied().unwrap_or(0) as f64 / sample_count as f64;
            assert!(
                (empirical - exact).abs() < 0.02,
                "split {key:?}: empirical {empirical}, exact {exact}"
            );
        }

        let repeated = engine
            .sample_history_skeletons_seeded(&model, &pruning, 20, 20260715)
            .unwrap();
        assert_eq!(&samples[..20], repeated);
    }

    #[test]
    fn stratified_constrained_traceback_matches_exact_posteriors() {
        let tree = Tree::new(
            4,
            5,
            vec![
                Edge {
                    parent: 3,
                    child: 0,
                    length: 0.2,
                },
                Edge {
                    parent: 3,
                    child: 1,
                    length: 0.2,
                },
                Edge {
                    parent: 4,
                    child: 3,
                    length: 0.8,
                },
                Edge {
                    parent: 4,
                    child: 2,
                    length: 1.0,
                },
            ],
        )
        .unwrap();
        let states = StateSpace::new(3, 2, false).unwrap();
        let a = states.index_of(AreaSet::from_bits(0b001)).unwrap();
        let b = states.index_of(AreaSet::from_bits(0b010)).unwrap();
        let c = states.index_of(AreaSet::from_bits(0b100)).unwrap();
        let adjacency = BinaryAreaMatrix::new(
            3,
            vec![true, true, false, true, true, false, false, false, true],
        )
        .unwrap();
        let young_constraint = RangeStateConstraint::new(None, Some(adjacency)).unwrap();
        let young_mask = young_constraint.state_mask(&states).unwrap();
        let young_dispersal =
            DispersalMultiplierMatrix::new(3, vec![1.0, 0.4, 0.1, 1.7, 1.0, 0.2, 0.3, 2.0, 1.0])
                .unwrap();
        let old_dispersal =
            DispersalMultiplierMatrix::new(3, vec![1.0, 3.0, 0.7, 0.2, 1.0, 2.5, 1.8, 0.4, 1.0])
                .unwrap();
        let schedule = TimeStratifiedAnagenesis::new(vec![
            AnageneticTimeStratum::new(
                0.5,
                Some(young_dispersal),
                Some(ExtirpationMultiplierVector::new(vec![1.0, 1.8, 0.6]).unwrap()),
            )
            .unwrap()
            .with_state_constraint(young_constraint)
            .unwrap(),
            AnageneticTimeStratum::new(
                1.1,
                Some(old_dispersal),
                Some(ExtirpationMultiplierVector::new(vec![0.5, 1.0, 2.2]).unwrap()),
            )
            .unwrap(),
        ])
        .unwrap();
        let model = ModelConfig::preset_dec_j(0.18, 0.07, 0.5)
            .unwrap()
            .with_time_stratified_anagenesis(schedule);
        let tips = vec![
            TipLikelihood {
                node: 0,
                likelihoods: one_hot(states.len(), a),
            },
            TipLikelihood {
                node: 1,
                likelihoods: one_hot(states.len(), b),
            },
            TipLikelihood {
                node: 2,
                likelihoods: one_hot(states.len(), c),
            },
        ];
        let engine = LikelihoodEngine::new(&tree, &states, RootPrior::Equal);
        let pruning = engine.evaluate(&model, &tips).unwrap();
        let exact_nodes = engine.node_state_posteriors(&model, &pruning).unwrap();
        let exact_splits = engine.split_scenario_posteriors(&model, &pruning).unwrap();
        let sample_count = 20_000;
        let samples = engine
            .sample_history_skeletons_seeded(&model, &pruning, sample_count, 314159)
            .unwrap();
        let mut node_counts = vec![vec![0_usize; states.len()]; tree.node_count()];
        let mut split_counts = HashMap::new();

        for sample in &samples {
            assert_history_is_coherent(&tree, sample);
            for node in [0, 1, 2, 3] {
                assert!(young_mask.is_allowed(sample.node_states[node]));
            }
            for split in sample.splits.iter().filter(|split| split.node == 3) {
                assert!(young_mask.is_allowed(split.ancestor));
                assert!(young_mask.is_allowed(split.left));
                assert!(young_mask.is_allowed(split.right));
            }
            for (node, state) in sample.node_states.iter().copied().enumerate() {
                node_counts[node][state] += 1;
            }
            for split in &sample.splits {
                *split_counts
                    .entry((split.node, split.ancestor, split.left, split.right))
                    .or_insert(0_usize) += 1;
            }
        }

        for posterior in exact_nodes {
            for (state, exact) in posterior.probabilities.iter().copied().enumerate() {
                let empirical = node_counts[posterior.node][state] as f64 / sample_count as f64;
                assert!(
                    (empirical - exact).abs() < 0.02,
                    "stratified node {}, state {state}: empirical {empirical}, exact {exact}",
                    posterior.node
                );
            }
        }
        let mut exact_split_probabilities = HashMap::new();
        for split in exact_splits {
            *exact_split_probabilities
                .entry((split.node, split.ancestor, split.left, split.right))
                .or_insert(0.0) += split.probability;
        }
        for (key, exact) in exact_split_probabilities {
            let empirical =
                split_counts.get(&key).copied().unwrap_or(0) as f64 / sample_count as f64;
            assert!(
                (empirical - exact).abs() < 0.02,
                "stratified split {key:?}: empirical {empirical}, exact {exact}"
            );
        }

        let maps = engine
            .sample_stochastic_maps_seeded(&model, &pruning, 2_000, 1618033)
            .unwrap();
        for map in &maps {
            assert_map_is_coherent(&tree, &states, map);
            for branch in &map.branches {
                for segment in &branch.segments {
                    if segment.q_index == 0 {
                        assert!(young_mask.is_allowed(segment.start_state));
                        assert!(young_mask.is_allowed(segment.end_state));
                        for event in &segment.events {
                            assert!(young_mask.is_allowed(event.from_state));
                            assert!(young_mask.is_allowed(event.to_state));
                        }
                    }
                }
            }
        }
    }

    fn one_hot(state_count: usize, state: usize) -> Vec<f64> {
        let mut likelihoods = vec![0.0; state_count];
        likelihoods[state] = 1.0;
        likelihoods
    }

    fn assert_history_is_coherent(tree: &Tree, sample: &HistorySkeleton) {
        assert_eq!(sample.root_state, sample.node_states[tree.root()]);
        for split in &sample.splits {
            assert_eq!(split.ancestor, sample.node_states[split.node]);
            let children = tree.children(split.node).unwrap();
            let left = sample.branch_endpoints[children[0].edge_index];
            let right = sample.branch_endpoints[children[1].edge_index];
            assert_eq!(left.start_state, split.left);
            assert_eq!(right.start_state, split.right);
        }
        for endpoint in &sample.branch_endpoints {
            assert_eq!(endpoint.end_state, sample.node_states[endpoint.child]);
        }
    }

    fn assert_map_is_coherent(tree: &Tree, states: &StateSpace, map: &BiogeographicStochasticMap) {
        assert_eq!(map.branches.len(), tree.edges().len());
        for branch in &map.branches {
            let endpoint = map.skeleton.branch_endpoints[branch.edge_index];
            assert_eq!(branch.start_state, endpoint.start_state);
            assert_eq!(branch.end_state, endpoint.end_state);
            if branch.segments.is_empty() {
                assert_eq!(branch.start_state, branch.end_state);
                continue;
            }

            assert_eq!(branch.segments[0].start_state, branch.start_state);
            assert_eq!(branch.segments[0].start_time_from_parent, 0.0);
            let mut previous_state = branch.start_state;
            let mut previous_time = 0.0;
            for segment in &branch.segments {
                assert_eq!(segment.start_state, previous_state);
                assert!((segment.start_time_from_parent - previous_time).abs() < 1e-10);
                let mut event_state = segment.start_state;
                let mut event_time = segment.start_time_from_parent;
                for event in &segment.events {
                    assert_eq!(event.from_state, event_state);
                    assert!(event.time_from_parent >= event_time);
                    assert!(event.time_from_parent < segment.end_time_from_parent);
                    let expected_kind = classify_anagenetic_event(
                        states,
                        branch.edge_index,
                        event.from_state,
                        event.to_state,
                    )
                    .unwrap();
                    assert_eq!(event.kind, expected_kind);
                    event_state = event.to_state;
                    event_time = event.time_from_parent;
                }
                assert_eq!(event_state, segment.end_state);
                previous_state = segment.end_state;
                previous_time = segment.end_time_from_parent;
            }
            assert_eq!(previous_state, branch.end_state);
            assert!((previous_time - tree.edges()[branch.edge_index].length).abs() < 1e-10);
        }
    }
}
