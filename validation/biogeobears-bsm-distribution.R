args <- commandArgs(trailingOnly = TRUE)

sample_count <- if (length(args) >= 1) as.integer(args[[1]]) else 5000L
output_path <- if (length(args) >= 2) args[[2]] else
  "validation/golden/biogeobears-bsm-distribution-samples.tsv"
seed_base <- if (length(args) >= 3) as.numeric(args[[3]]) else 20260716
batch_size <- if (length(args) >= 4) as.integer(args[[4]]) else 100L

if (is.na(sample_count) || sample_count < 1) {
  stop("sample_count must be a positive integer", call. = FALSE)
}
if (!is.finite(seed_base) || seed_base < 0) {
  stop("seed_base must be a finite non-negative number", call. = FALSE)
}
if (is.na(batch_size) || batch_size < 1) {
  stop("batch_size must be a positive integer", call. = FALSE)
}

source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
env <- configure_project_r()

required_packages <- c("ape", "rexpokit", "cladoRcpp", "BioGeoBEARS")
missing_packages <- required_packages[!vapply(
  required_packages,
  requireNamespace,
  quietly = TRUE,
  FUN.VALUE = logical(1)
)]
if (length(missing_packages) > 0) {
  stop(
    "Missing project-local R packages: ",
    paste(missing_packages, collapse = ", "),
    ". Run: Rscript validation/setup-local-r-biogeobears.R",
    call. = FALSE
  )
}

suppressPackageStartupMessages({
  library(ape)
  library(rexpokit)
  library(cladoRcpp)
  library(BioGeoBEARS)
})

options(digits = 17)

fixture_id <- "bsm_3taxa_official_areas_allowed"
maxtries_per_branch <- 40000L
expected_branch_time <- 5
time_tolerance <- 1e-8

write_lagrange_geog <- function(ranges_path, destination) {
  ranges <- read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE)
  area_names <- names(ranges)[-1]
  bits <- apply(ranges[, -1, drop = FALSE], 1, paste0, collapse = "")
  lines <- c(
    paste(
      nrow(ranges),
      length(area_names),
      paste0("(", paste(area_names, collapse = " "), ")"),
      sep = "\t"
    ),
    paste(ranges[[1]], bits, sep = "\t")
  )
  writeLines(lines, destination, useBytes = TRUE)
}

state_to_bits <- function(state) {
  if (length(state) == 1 && is.na(state)) {
    return(0)
  }
  sum(2 ^ as.numeric(state))
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

summarize_bsm <- function(clado, ana, state_count, q_count, sample_index) {
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

  occupancy <- numeric(state_count)
  occupancy_by_q <- matrix(0, nrow = q_count, ncol = state_count)
  events_by_q <- integer(q_count)
  d_count <- 0L
  e_count <- 0L

  branch_rows <- which(
    !is.na(clado$sampled_states_AT_brbots)
      & !is.na(clado$sampled_states_AT_nodes)
      & !is.na(clado$SUBnode)
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
      stop("Invalid segment duration in BioGeoBEARS sample ", sample_index, call. = FALSE)
    }

    start_state <- as.integer(segment$sampled_states_AT_brbots) - 1L
    end_state <- as.integer(segment$sampled_states_AT_nodes) - 1L
    if (any(c(start_state, end_state) < 0) || any(c(start_state, end_state) >= state_count)) {
      stop("BioGeoBEARS segment state is out of bounds in sample ", sample_index, call. = FALSE)
    }

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

    current_state <- start_state
    current_time <- 0
    if (!is.null(segment_events) && nrow(segment_events) > 0) {
      for (event_index in seq_len(nrow(segment_events))) {
        event <- segment_events[event_index, , drop = FALSE]
        event_time <- as.numeric(event$event_time)
        from_state <- as.integer(event$current_rangenum_1based) - 1L
        to_state <- as.integer(event$new_rangenum_1based) - 1L
        if (!is.finite(event_time)
            || event_time + time_tolerance < current_time
            || event_time > duration + time_tolerance) {
          stop("Invalid event time in BioGeoBEARS sample ", sample_index, call. = FALSE)
        }
        if (from_state != current_state) {
          stop("Broken event-state chain in BioGeoBEARS sample ", sample_index, call. = FALSE)
        }
        elapsed <- max(0, event_time - current_time)
        occupancy[[current_state + 1L]] <- occupancy[[current_state + 1L]] + elapsed
        occupancy_by_q[q_index + 1L, current_state + 1L] <-
          occupancy_by_q[q_index + 1L, current_state + 1L] + elapsed
        events_by_q[[q_index + 1L]] <- events_by_q[[q_index + 1L]] + 1L

        kind <- as.character(event$event_type)
        if (identical(kind, "d")) {
          d_count <- d_count + 1L
        } else if (identical(kind, "e")) {
          e_count <- e_count + 1L
        } else {
          stop("Unsupported BioGeoBEARS anagenetic event type: ", kind, call. = FALSE)
        }
        current_state <- to_state
        current_time <- event_time
      }
    }

    if (current_state != end_state) {
      stop("Segment endpoint mismatch in BioGeoBEARS sample ", sample_index, call. = FALSE)
    }
    elapsed <- max(0, duration - current_time)
    occupancy[[current_state + 1L]] <- occupancy[[current_state + 1L]] + elapsed
    occupancy_by_q[q_index + 1L, current_state + 1L] <-
      occupancy_by_q[q_index + 1L, current_state + 1L] + elapsed
  }

  anagenetic_total <- d_count + e_count
  if (sum(events_by_q) != anagenetic_total) {
    stop("Period event counts do not sum to the anagenetic total", call. = FALSE)
  }
  total_branch_time <- sum(occupancy)
  if (abs(total_branch_time - expected_branch_time) > time_tolerance) {
    stop(
      "BioGeoBEARS occupancy time does not equal tree branch time in sample ",
      sample_index,
      ": ",
      total_branch_time,
      call. = FALSE
    )
  }

  clado_counts <- event_type_counts(clado)
  manual_fallback_branches <- 0L
  max_branch_tries <- 0L
  if (has_ana) {
    tries <- as.integer(ana$trynum)
    max_branch_tries <- max(tries, na.rm = TRUE)
    fallback_rows <- which(!is.na(tries) & tries > maxtries_per_branch)
    if (length(fallback_rows) > 0) {
      fallback_keys <- paste(
        ana$SUBnode[fallback_rows],
        ana$stratum[fallback_rows],
        ana$piecenum[fallback_rows],
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

repo_root <- env$repo_root
fixtures <- read.delim(
  file.path(repo_root, "validation", "state_constraint_fixtures.tsv"),
  check.names = FALSE,
  stringsAsFactors = FALSE
)
case <- fixtures[fixtures$case_id == fixture_id, , drop = FALSE]
if (nrow(case) != 1) {
  stop("Could not find exactly one official BSM fixture row", call. = FALSE)
}

optimized <- read.delim(
  file.path(repo_root, "validation", "golden", "biogeobears-state-constraints-optim.tsv"),
  check.names = FALSE,
  stringsAsFactors = FALSE
)
optimized <- optimized[optimized$case_id == fixture_id, , drop = FALSE]
if (nrow(optimized) != 1 || as.integer(optimized$convergence) != 0) {
  stop("Official BSM fixture has no converged BioGeoBEARS ML parameters", call. = FALSE)
}
d_value <- as.numeric(optimized$biogeobears_d)
e_value <- as.numeric(optimized$biogeobears_e)

work_parent <- file.path(repo_root, "validation", "benchmark-runs")
dir.create(work_parent, recursive = TRUE, showWarnings = FALSE)
work_dir <- tempfile("bgb-bsm-distribution-", tmpdir = work_parent)
dir.create(work_dir, recursive = TRUE, showWarnings = FALSE)
if (!dir.exists(work_dir)) {
  stop("Could not create BioGeoBEARS BSM working directory", call. = FALSE)
}
on.exit(unlink(work_dir, recursive = TRUE, force = TRUE), add = TRUE)

tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
geog_path <- file.path(work_dir, "geog.data")
write_lagrange_geog(ranges_path, geog_path)

schedule_path <- normalizePath(
  file.path(repo_root, case$dispersal_strata),
  winslash = "/",
  mustWork = TRUE
)
schedule <- read.delim(schedule_path, check.names = FALSE, stringsAsFactors = FALSE)
times_path <- file.path(work_dir, "timeperiods.txt")
writeLines(format(as.numeric(schedule$oldest_age), scientific = FALSE, trim = TRUE), times_path)

cat("Preparing BioGeoBEARS official BSM fixture...\n")
run_object <- define_BioGeoBEARS_run()
run_object$trfn <- tree_path
run_object$geogfn <- geog_path
run_object$timesfn <- times_path
run_object$max_range_size <- as.integer(case$max_range_size)
run_object$include_null_range <- TRUE
run_object$min_branchlength <- 0
run_object$print_optim <- FALSE
run_object$num_cores_to_use <- 1
run_object$use_optimx <- FALSE
run_object$return_condlikes_table <- TRUE
run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
run_object$calc_ancprobs <- TRUE
run_object$speedup <- FALSE

run_object <- readfiles_BioGeoBEARS_run(run_object)
run_object <- apply_fixture_dispersal_multipliers(run_object, case, repo_root)
run_object <- set_fixture_fixed_param(run_object, "d", d_value)
run_object <- set_fixture_fixed_param(run_object, "e", e_value)
run_object <- set_fixture_fixed_param(run_object, "j", 0)
check_BioGeoBEARS_run(run_object)

result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
mapping_inputs <- get_inputs_for_stochastic_mapping(res = result)
states_list <- result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
state_bits <- vapply(states_list, state_to_bits, numeric(1))
expected_state_bits <- c(0, 1, 2, 4, 3, 5, 6, 7)
if (!identical(state_bits, expected_state_bits)) {
  stop(
    "Unexpected BioGeoBEARS state order: ",
    paste(state_bits, collapse = ","),
    call. = FALSE
  )
}
state_count <- length(states_list)
q_count <- length(as.numeric(schedule$oldest_age))

rows <- vector("list", sample_count)
completed <- 0L
batch_index <- 0L
start_time <- proc.time()[["elapsed"]]
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
    stop(
      "BioGeoBEARS generated ",
      generated,
      " maps for a batch target of ",
      target,
      call. = FALSE
    )
  }

  for (batch_sample in seq_len(target)) {
    sample_index <- completed + batch_sample - 1L
    rows[[sample_index + 1L]] <- summarize_bsm(
      batch_output$RES_clado_events_tables[[batch_sample]],
      batch_output$RES_ana_events_tables[[batch_sample]],
      state_count,
      q_count,
      sample_index
    )
  }
  completed <- completed + target
  elapsed <- proc.time()[["elapsed"]] - start_time
  cat(
    "BioGeoBEARS BSM progress: ",
    completed,
    "/",
    sample_count,
    " maps (",
    sprintf("%.1f", elapsed),
    " s)\n",
    sep = ""
  )
  rm(batch_output, messages)
  unlink(batch_dir, recursive = TRUE, force = TRUE)
  gc(verbose = FALSE)
}

samples <- do.call(rbind, rows)
if (any(samples$manual_fallback_branches != 0)) {
  stop("BioGeoBEARS used manual fallback histories; increase maxtries_per_branch", call. = FALSE)
}

output_path <- file.path(repo_root, output_path)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(samples, output_path, sep = "\t", quote = FALSE, row.names = FALSE)

elapsed <- proc.time()[["elapsed"]] - start_time
metadata_path <- sub("\\.tsv$", "-metadata.tsv", output_path)
if (identical(metadata_path, output_path)) {
  metadata_path <- paste0(output_path, "-metadata.tsv")
}
metadata <- data.frame(
  fixture_id = fixture_id,
  official_example = "BioGeoBEARS/examples/BSM_3taxa/M3areas_allowed",
  sample_count = sample_count,
  seed_base = format(seed_base, scientific = FALSE, trim = TRUE),
  batch_size = batch_size,
  d = d_value,
  e = e_value,
  j = 0,
  maxtries_per_branch = maxtries_per_branch,
  biogeobears_version = as.character(packageVersion("BioGeoBEARS")),
  r_version = R.version.string,
  elapsed_seconds = elapsed,
  stringsAsFactors = FALSE
)
write.table(metadata, metadata_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
cat("Wrote ", metadata_path, "\n", sep = "")
