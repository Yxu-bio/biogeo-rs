args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 11) {
  stop(
    paste(
      "Usage: Rscript validation/benchmark-biogeobears-ponerinae-bsm.R",
      paste(
        "<tree> <ranges.data> <time_boundaries> <strata.tsv> <d> <e>",
        "<sample_count> <seed> <batch_size> <samples.tsv> <metadata.tsv>"
      )
    ),
    call. = FALSE
  )
}

tree_path <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
ranges_path <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
time_boundaries_path <- normalizePath(args[[3]], winslash = "/", mustWork = TRUE)
strata_path <- normalizePath(args[[4]], winslash = "/", mustWork = TRUE)
d_value <- as.numeric(args[[5]])
e_value <- as.numeric(args[[6]])
sample_count <- as.integer(args[[7]])
seed_base <- as.numeric(args[[8]])
batch_size <- as.integer(args[[9]])
output_path <- args[[10]]
metadata_path <- args[[11]]

source("validation/r-env.R")
env <- configure_project_r()

required_packages <- c("ape", "rexpokit", "cladoRcpp", "BioGeoBEARS")
missing_packages <- required_packages[
  !vapply(required_packages, requireNamespace, logical(1), quietly = TRUE)
]
if (length(missing_packages) > 0) {
  stop("Missing required R packages: ", paste(missing_packages, collapse = ", "), call. = FALSE)
}

suppressPackageStartupMessages({
  library(ape)
  library(rexpokit)
  library(cladoRcpp)
  library(BioGeoBEARS)
})

options(digits = 17)
maxtries_per_branch <- 40000L
time_tolerance <- 1e-7

set_fixed_param <- function(model_object, name, value) {
  if (!(name %in% rownames(model_object@params_table))) {
    return(model_object)
  }
  table <- model_object@params_table
  for (column in c("init", "est", "min", "max")) {
    if (column %in% colnames(table)) {
      table[name, column] <- value
    }
  }
  if ("type" %in% colnames(table)) {
    table[name, "type"] <- "fixed"
  }
  model_object@params_table <- table
  model_object
}

state_to_bits <- function(state) {
  if (length(state) == 1 && is.na(state)) {
    return(0)
  }
  sum(2 ^ as.numeric(state))
}

read_allowed_states <- function(path, expected_areas) {
  table <- read.delim(
    path,
    comment.char = "#",
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
  if (ncol(table) != length(expected_areas) + 1 || names(table)[[1]] != "range") {
    stop("Invalid allowed range table: ", path, call. = FALSE)
  }
  if (!identical(names(table)[-1], expected_areas)) {
    stop("Allowed range area order mismatch: ", path, call. = FALSE)
  }
  bits <- as.matrix(table[, -1, drop = FALSE])
  storage.mode(bits) <- "integer"
  if (any(is.na(bits)) || any(!(bits %in% c(0L, 1L)))) {
    stop("Allowed range table must contain only 0/1 values: ", path, call. = FALSE)
  }
  lapply(seq_len(nrow(bits)), function(index) {
    occupied <- which(bits[index, ] == 1L) - 1L
    if (length(occupied) == 0) as.integer(NA) else as.integer(occupied)
  })
}

read_stratified_states <- function(strata_path, time_boundaries_path, expected_areas) {
  strata <- read.delim(strata_path, check.names = FALSE, stringsAsFactors = FALSE)
  if (!all(c("oldest_age", "allowed_ranges") %in% names(strata))) {
    stop("Strata table must contain oldest_age and allowed_ranges", call. = FALSE)
  }
  boundaries <- scan(time_boundaries_path, what = numeric(), quiet = TRUE)
  if (nrow(strata) != length(boundaries) ||
      any(abs(as.numeric(strata$oldest_age) - boundaries) > 1e-10)) {
    stop("Strata ages do not match BioGeoBEARS time boundaries", call. = FALSE)
  }

  base_dir <- dirname(strata_path)
  states <- lapply(strata$allowed_ranges, function(raw_path) {
    if (is.na(raw_path) || raw_path %in% c("", "-", "none")) {
      stop("Every Ponerinae stratum must provide allowed_ranges", call. = FALSE)
    }
    is_absolute <- grepl("^[A-Za-z]:[/\\\\]", raw_path) ||
      startsWith(raw_path, "/") || startsWith(raw_path, "\\")
    path <- if (is_absolute) raw_path else file.path(base_dir, raw_path)
    read_allowed_states(normalizePath(path, winslash = "/", mustWork = TRUE), expected_areas)
  })
  list(boundaries = boundaries, states = states)
}

event_type_counts <- function(clado) {
  raw_types <- as.character(clado$clado_event_type)
  types <- raw_types[!is.na(raw_types) & nzchar(trimws(raw_types))]
  counts <- c(y = 0L, s = 0L, v = 0L, j = 0L)
  for (event_type in types) {
    matched <- names(counts)[vapply(
      names(counts),
      function(code) grepl(paste0("(", code, ")"), event_type, fixed = TRUE),
      logical(1)
    )]
    if (length(matched) != 1) {
      stop("Unsupported BioGeoBEARS cladogenetic event type: ", event_type, call. = FALSE)
    }
    counts[[matched]] <- counts[[matched]] + 1L
  }
  counts
}

summarize_bsm <- function(
  clado,
  ana,
  master_state_bits,
  local_state_bits,
  expected_branch_time,
  sample_index
) {
  required_clado <- c(
    "stratum", "piecenum", "piececlass", "SUBnode", "SUBedge.length", "reltimept",
    "sampled_states_AT_nodes", "sampled_states_AT_brbots", "clado_event_type"
  )
  if (!all(required_clado %in% names(clado))) {
    stop("BioGeoBEARS cladogenetic table is missing required columns", call. = FALSE)
  }
  has_ana <- is.data.frame(ana) && nrow(ana) > 0
  if (has_ana) {
    required_ana <- c(
      "SUBnode", "stratum", "piecenum", "trynum", "current_rangenum_1based",
      "new_rangenum_1based", "event_time", "event_type"
    )
    if (!all(required_ana %in% names(ana))) {
      stop("BioGeoBEARS anagenetic table is missing required columns", call. = FALSE)
    }
  }

  q_count <- length(local_state_bits)
  state_count <- length(master_state_bits)
  occupancy <- numeric(state_count)
  occupancy_by_q <- matrix(0, nrow = q_count, ncol = state_count)
  events_by_q <- integer(q_count)
  d_count <- 0L
  e_count <- 0L
  forbidden_state_transitions <- 0L
  forbidden_state_endpoints <- 0L
  forbidden_state_time <- 0

  master_index <- function(q_index, global_index) {
    if (global_index < 0 || global_index >= length(master_state_bits)) {
      stop("Global state index is out of bounds in sample ", sample_index, call. = FALSE)
    }
    global_index
  }
  state_is_allowed <- function(q_index, state_index) {
    master_state_bits[[state_index + 1L]] %in% local_state_bits[[q_index + 1L]]
  }

  branch_rows <- which(
    !is.na(clado$sampled_states_AT_brbots) &
      !is.na(clado$sampled_states_AT_nodes) &
      !is.na(clado$SUBnode)
  )
  for (row_index in branch_rows) {
    segment <- clado[row_index, , drop = FALSE]
    q_index <- as.integer(segment$stratum) - 1L
    if (is.na(q_index) || q_index < 0 || q_index >= q_count) {
      stop("Invalid BioGeoBEARS stratum in sample ", sample_index, call. = FALSE)
    }
    duration <- if (identical(as.character(segment$piececlass), "subbranch")) {
      as.numeric(segment$reltimept)
    } else {
      as.numeric(segment$SUBedge.length)
    }
    if (!is.finite(duration) || duration < 0) {
      stop("Invalid branch segment duration", call. = FALSE)
    }

    start_global <- as.integer(segment$sampled_states_AT_brbots) - 1L
    end_global <- as.integer(segment$sampled_states_AT_nodes) - 1L
    start_state <- master_index(q_index, start_global)
    end_state <- master_index(q_index, end_global)
    forbidden_state_endpoints <- forbidden_state_endpoints +
      as.integer(!state_is_allowed(q_index, start_state)) +
      as.integer(!state_is_allowed(q_index, end_state))

    segment_events <- NULL
    if (has_ana) {
      matching <- as.integer(ana$SUBnode) == as.integer(segment$SUBnode)
      matching <- matching & as.integer(ana$stratum) == as.integer(segment$stratum)
      matching <- matching & as.integer(ana$piecenum) == as.integer(segment$piecenum)
      segment_events <- ana[which(matching), , drop = FALSE]
      if (nrow(segment_events) > 0) {
        segment_events <- segment_events[
          order(as.numeric(segment_events$event_time)),
          ,
          drop = FALSE
        ]
      }
    }

    current_global <- start_global
    current_state <- start_state
    current_time <- 0
    if (!is.null(segment_events) && nrow(segment_events) > 0) {
      for (event_index in seq_len(nrow(segment_events))) {
        event <- segment_events[event_index, , drop = FALSE]
        event_time <- as.numeric(event$event_time)
        from_global <- as.integer(event$current_rangenum_1based) - 1L
        to_global <- as.integer(event$new_rangenum_1based) - 1L
        if (!is.finite(event_time) || event_time + time_tolerance < current_time ||
            event_time > duration + time_tolerance) {
          stop("Invalid event time in sample ", sample_index, call. = FALSE)
        }
        if (from_global != current_global) {
          stop("Broken global state chain in sample ", sample_index, call. = FALSE)
        }
        elapsed <- max(0, event_time - current_time)
        occupancy[[current_state + 1L]] <- occupancy[[current_state + 1L]] + elapsed
        occupancy_by_q[q_index + 1L, current_state + 1L] <-
          occupancy_by_q[q_index + 1L, current_state + 1L] + elapsed
        if (!state_is_allowed(q_index, current_state)) {
          forbidden_state_time <- forbidden_state_time + elapsed
        }
        events_by_q[[q_index + 1L]] <- events_by_q[[q_index + 1L]] + 1L

        kind <- as.character(event$event_type)
        if (identical(kind, "d")) {
          d_count <- d_count + 1L
        } else if (identical(kind, "e")) {
          e_count <- e_count + 1L
        } else {
          stop("Unsupported BioGeoBEARS anagenetic event type: ", kind, call. = FALSE)
        }
        current_global <- to_global
        current_state <- master_index(q_index, to_global)
        if (!state_is_allowed(q_index, current_state)) {
          forbidden_state_transitions <- forbidden_state_transitions + 1L
        }
        current_time <- event_time
      }
    }

    if (current_state != end_state) {
      stop("Branch endpoint mismatch in sample ", sample_index, call. = FALSE)
    }
    elapsed <- max(0, duration - current_time)
    occupancy[[current_state + 1L]] <- occupancy[[current_state + 1L]] + elapsed
    occupancy_by_q[q_index + 1L, current_state + 1L] <-
      occupancy_by_q[q_index + 1L, current_state + 1L] + elapsed
    if (!state_is_allowed(q_index, current_state)) {
      forbidden_state_time <- forbidden_state_time + elapsed
    }
  }

  anagenetic_total <- d_count + e_count
  if (sum(events_by_q) != anagenetic_total) {
    stop("Period event counts do not sum to the anagenetic total", call. = FALSE)
  }
  total_branch_time <- sum(occupancy)
  if (abs(total_branch_time - expected_branch_time) > time_tolerance) {
    stop(
      "Occupancy does not sum to tree branch time in sample ", sample_index,
      ": ", total_branch_time, " versus ", expected_branch_time,
      call. = FALSE
    )
  }

  clado_counts <- event_type_counts(clado)
  manual_fallback_branches <- 0L
  max_branch_tries <- 0L
  if (has_ana) {
    tries <- as.integer(ana$trynum)
    finite_tries <- tries[!is.na(tries)]
    if (length(finite_tries) > 0) {
      max_branch_tries <- max(finite_tries)
    }
    fallback_rows <- which(!is.na(tries) & tries > maxtries_per_branch)
    if (length(fallback_rows) > 0) {
      fallback_keys <- paste(
        ana$SUBnode[fallback_rows], ana$stratum[fallback_rows], ana$piecenum[fallback_rows],
        sep = ":"
      )
      manual_fallback_branches <- length(unique(fallback_keys))
    }
  }

  row <- data.frame(
    sample = sample_index,
    anagenetic_total = anagenetic_total,
    range_expansion = d_count,
    local_extirpation = e_count,
    cladogenetic_total = sum(clado_counts),
    range_copying = unname(clado_counts[["y"]]),
    subset_sympatry = unname(clado_counts[["s"]]),
    vicariance = unname(clado_counts[["v"]]),
    founder_event = unname(clado_counts[["j"]]),
    total_branch_time = total_branch_time,
    manual_fallback_branches = manual_fallback_branches,
    max_branch_tries = max_branch_tries,
    forbidden_state_transitions = forbidden_state_transitions,
    forbidden_state_endpoints = forbidden_state_endpoints,
    forbidden_state_time = forbidden_state_time,
    stringsAsFactors = FALSE
  )
  for (q_index in seq_len(q_count) - 1L) {
    row[[paste0("period_q", q_index, "_events")]] <- events_by_q[[q_index + 1L]]
  }
  for (state_index in seq_len(state_count) - 1L) {
    row[[paste0("occupancy_state_", state_index)]] <- occupancy[[state_index + 1L]]
  }
  for (q_index in seq_len(q_count) - 1L) {
    for (state_index in seq_len(state_count) - 1L) {
      row[[paste0("occupancy_q", q_index, "_state_", state_index)]] <-
        occupancy_by_q[q_index + 1L, state_index + 1L]
    }
  }
  row
}

if (!is.finite(d_value) || !is.finite(e_value) || d_value <= 0 || e_value <= 0) {
  stop("d/e must be finite positive rates", call. = FALSE)
}
if (is.na(sample_count) || sample_count < 1 || is.na(batch_size) || batch_size < 1) {
  stop("sample_count and batch_size must be positive", call. = FALSE)
}
if (!is.finite(seed_base) || seed_base < 0) {
  stop("seed must be finite and non-negative", call. = FALSE)
}

tipranges <- getranges_from_LagrangePHYLIP(lgdata_fn = ranges_path)
areas <- getareas_from_tipranges_object(tipranges)
stratified <- read_stratified_states(strata_path, time_boundaries_path, areas)
master_states <- rcpp_areas_list_to_states_list(
  areas = areas,
  maxareas = 5,
  include_null_range = TRUE
)
master_state_bits <- vapply(master_states, state_to_bits, numeric(1))
local_state_bits <- lapply(
  stratified$states,
  function(states) vapply(states, state_to_bits, numeric(1))
)
if (anyDuplicated(master_state_bits) ||
    any(vapply(local_state_bits, function(bits) anyDuplicated(bits) > 0, logical(1)))) {
  stop("State lists contain duplicate ranges", call. = FALSE)
}
if (any(vapply(local_state_bits, function(bits) any(!(bits %in% master_state_bits)), logical(1)))) {
  stop("A stratum contains a range outside the master state space", call. = FALSE)
}

phy <- read.tree(tree_path)
expected_branch_time <- sum(phy$edge.length)
work_parent <- file.path(env$repo_root, "validation", "benchmark-runs")
dir.create(work_parent, recursive = TRUE, showWarnings = FALSE)
work_dir <- tempfile("bgb-ponerinae-bsm-", tmpdir = work_parent)
dir.create(work_dir, recursive = TRUE, showWarnings = FALSE)
run_completed <- FALSE
on.exit({
  if (run_completed) {
    unlink(work_dir, recursive = TRUE, force = TRUE)
  } else {
    cat("Retained failed BioGeoBEARS BSM work directory: ", work_dir, "\n", sep = "")
  }
}, add = TRUE)

run_object <- define_BioGeoBEARS_run()
run_object$trfn <- tree_path
run_object$geogfn <- ranges_path
run_object$timesfn <- time_boundaries_path
run_object$max_range_size <- 5
run_object$include_null_range <- TRUE
run_object$min_branchlength <- 0
run_object$print_optim <- FALSE
run_object$num_cores_to_use <- 1
run_object$on_NaN_error <- -1e50
run_object$use_optimx <- FALSE
run_object$return_condlikes_table <- TRUE
run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
run_object$calc_ancprobs <- TRUE
run_object$speedup <- FALSE

setup_start <- proc.time()[["elapsed"]]
run_object <- readfiles_BioGeoBEARS_run(run_object)
run_object <- section_the_tree(
  inputs = run_object,
  make_master_table = TRUE,
  plot_pieces = FALSE,
  fossils_older_than = 0.001,
  cut_fossils = FALSE
)
run_object$lists_of_states_lists_0based <- stratified$states
model <- run_object$BioGeoBEARS_model_object
model <- set_fixed_param(model, "d", d_value)
model <- set_fixed_param(model, "e", e_value)
model <- set_fixed_param(model, "j", 0)
for (name in c("mx01y", "mx01s", "mx01v", "mx01j")) {
  model <- set_fixed_param(model, name, 0.0001)
}
run_object$BioGeoBEARS_model_object <- model
run_object <- fix_BioGeoBEARS_params_minmax(BioGeoBEARS_run_object = run_object)
check_BioGeoBEARS_run(run_object)

result <- NULL
invisible(capture.output({
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
}))
mapping_inputs <- NULL
invisible(capture.output({
  mapping_inputs <- get_inputs_for_stochastic_mapping(res = result)
}))
setup_seconds <- proc.time()[["elapsed"]] - setup_start

rows <- vector("list", sample_count)
completed <- 0L
batch_index <- 0L
history_start <- proc.time()[["elapsed"]]
while (completed < sample_count) {
  batch_index <- batch_index + 1L
  target <- min(batch_size, sample_count - completed)
  batch_dir <- file.path(work_dir, paste0("batch-", batch_index))
  dir.create(batch_dir, recursive = TRUE, showWarnings = FALSE)
  batch_seed <- seed_base + batch_index * 1000000
  messages <- capture.output(
    batch_output <- runBSM(
      result,
      stochastic_mapping_inputs_list = mapping_inputs,
      maxnum_maps_to_try = 2L * target,
      nummaps_goal = target,
      maxtries_per_branch = maxtries_per_branch,
      save_after_every_try = FALSE,
      savedir = batch_dir,
      seedval = batch_seed,
      wait_before_save = 0,
      master_nodenum_toPrint = 0
    )
  )
  generated <- length(batch_output$RES_clado_events_tables)
  if (generated != target || length(batch_output$RES_ana_events_tables) != target) {
    stop("BioGeoBEARS generated ", generated, " histories for target ", target, call. = FALSE)
  }
  saveRDS(batch_output, file.path(batch_dir, "batch-output.rds"))
  for (batch_sample in seq_len(target)) {
    sample_index <- completed + batch_sample - 1L
    rows[[sample_index + 1L]] <- summarize_bsm(
      batch_output$RES_clado_events_tables[[batch_sample]],
      batch_output$RES_ana_events_tables[[batch_sample]],
      master_state_bits,
      local_state_bits,
      expected_branch_time,
      sample_index
    )
  }
  completed <- completed + target
  elapsed <- proc.time()[["elapsed"]] - history_start
  cat(
    "BioGeoBEARS Ponerinae BSM progress: ", completed, "/", sample_count,
    " histories (", sprintf("%.1f", elapsed), " s)\n",
    sep = ""
  )
  rm(batch_output, messages)
  unlink(batch_dir, recursive = TRUE, force = TRUE)
  gc(verbose = FALSE)
}
history_seconds <- proc.time()[["elapsed"]] - history_start

samples <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(samples, output_path, sep = "\t", quote = FALSE, row.names = FALSE)

metadata <- data.frame(
  format = "biogeo-ponerinae-bgb-bsm-v1",
  sample_count = sample_count,
  seed_base = format(seed_base, scientific = FALSE, trim = TRUE),
  batch_size = batch_size,
  d = d_value,
  e = e_value,
  states = length(master_states),
  strata = length(stratified$states),
  allowed_range_counts = paste(vapply(stratified$states, length, integer(1)), collapse = ","),
  expected_branch_time = expected_branch_time,
  setup_seconds = setup_seconds,
  history_sampling_seconds = history_seconds,
  seconds_per_history = history_seconds / sample_count,
  maxtries_per_branch = maxtries_per_branch,
  biogeobears_version = as.character(packageVersion("BioGeoBEARS")),
  r_version = R.version.string,
  stringsAsFactors = FALSE
)
dir.create(dirname(metadata_path), recursive = TRUE, showWarnings = FALSE)
write.table(metadata, metadata_path, sep = "\t", quote = FALSE, row.names = FALSE)
run_completed <- TRUE
cat("Wrote ", output_path, "\n", sep = "")
cat("Wrote ", metadata_path, "\n", sep = "")
