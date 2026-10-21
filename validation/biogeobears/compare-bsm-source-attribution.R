# Validation only. Runtime users do not need R or BioGeoBEARS.
args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 3L) stop("Usage: Rscript compare-bsm-source-attribution.R BSM_DIR SOURCE_DIR OUTPUT_DIR")
source("validation/biogeobears/r-env.R")
configure_project_r(create = FALSE)
suppressPackageStartupMessages(library(BioGeoBEARS))
bsm_dir <- args[1]; source_dir <- args[2]; output_dir <- args[3]
dir.create(output_dir, recursive = TRUE, showWarnings = FALSE)
read_tsv <- function(path) read.delim(path, check.names = FALSE, stringsAsFactors = FALSE, quote = "", na.strings = "NA")
read_events <- function(name) {
  paths <- if (file.exists(file.path(bsm_dir, name))) file.path(bsm_dir, name) else file.path(sort(list.dirs(file.path(bsm_dir, "shards"), recursive = FALSE)), name)
  do.call(rbind, lapply(paths, read_tsv))
}
sources <- do.call(rbind, lapply(sort(list.files(file.path(source_dir, "shards"), full.names = TRUE, pattern = "[.]tsv$")), read_tsv))
states <- read_tsv(file.path(bsm_dir, "states.tsv"))
nodes <- read_tsv(file.path(bsm_dir, "nodes.tsv"))
edges <- read_tsv(file.path(bsm_dir, "edges.tsv"))
areas <- c("A", "B", "C")
stopifnot(identical(read_tsv(file.path(bsm_dir, "areas.tsv"))$area, areas))
young <- matrix(c(1,2,9,3,1,1,4,5,1), 3, byrow = TRUE)
old <- matrix(c(1,2,1,3,1,9,4,5,1), 3, byrow = TRUE)
dmats <- list(young, old)
times <- c(2, 8)
range_name <- function(index) {
  bits <- states$range_bits[match(index, states$state_index)]
  paste0(areas[bitwAnd(as.integer(bits), c(1L,2L,4L)) != 0L], collapse = "")
}
ana <- read_events("anagenetic_events.tsv")
ana$ordinal <- ave(seq_len(nrow(ana)), interaction(ana$sample, ana$edge), FUN = function(x) seq_along(x) - 1L)
ana$event_id <- paste0("s", ana$sample, ":a:e", ana$edge, ":i", ana$ordinal)
ana$age <- nodes$age[match(edges$parent[match(ana$edge, edges$edge)], nodes$node)] - ana$time_from_parent
clado <- read_events("cladogenetic_splits.tsv")
clado$event_id <- paste0("s", clado$sample, ":c:n", clado$node)
clado$age <- nodes$age[match(clado$node, nodes$node)]
selected <- sources[sources$event_kind %in% c("range_expansion", "founder_event"), ]
selected$from_text <- vapply(selected$source_state_index, range_name, "")
selected$to_text <- vapply(selected$target_state_index, range_name, "")
selected$target_text <- areas[selected$target_area_index + 1L]
selected$boundary <- abs(selected$age_before_present - 2) < 1e-12
selected$r_source <- NA_character_
stopifnot(!anyDuplicated(sources$event_id))

ana_fun <- getFromNamespace("simulate_source_area_ana", "BioGeoBEARS")
clado_fun <- getFromNamespace("simulate_source_area_clado", "BioGeoBEARS")
make_table <- function(rows, kind) {
  if (kind == "range_expansion") {
    original <- ana[match(rows$event_id, ana$event_id), ]
    stopifnot(all(original$from_state_index == rows$source_state_index), all(original$to_state_index == rows$target_state_index), all(original$q_index == rows$q_index), max(abs(original$age - rows$age_before_present)) < 1e-12)
    data.frame(event_id = rows$event_id, event_txt = paste0(rows$from_text, "->", rows$to_text), abs_event_time = original$age, dispersal_to = rows$target_text, trailing = 0)
  } else {
    original <- clado[match(rows$event_id, clado$event_id), ]
    target <- ifelse(original$founder_daughter_order == 0, original$left_state_index, original$right_state_index)
    stopifnot(all(original$ancestor_state_index == rows$source_state_index), all(target == rows$target_state_index), all(original$founder_daughter_order == rows$founder_daughter_order), max(abs(original$age - rows$age_before_present)) < 1e-12)
    data.frame(event_id = rows$event_id, clado_event_txt = paste0(rows$from_text, "->", rows$from_text, ",", rows$to_text), time_bp = original$age, clado_dispersal_to = rows$target_text, trailing = 0)
  }
}
run_helper <- function(rows, dmat, times = NULL, instrument = FALSE) {
  kind <- unique(rows$event_kind); stopifnot(length(kind) == 1L)
  fun <- if (kind == "range_expansion") ana_fun else clado_fun
  probabilities <- NULL
  if (instrument) {
    # Copy only the environment to observe prob; the original function body is unchanged.
    probe <- new.env(parent = environment(fun))
    probe$sample <- function(x, size, replace, prob) {
      probabilities <<- setNames(prob, x)
      base::sample(x, size = size, replace = replace, prob = prob)
    }
    environment(fun) <- probe
  }
  result <- fun(make_table(rows, kind), areanames = areas, dmat = dmat, times = times)
  list(result = result, probabilities = probabilities)
}

set.seed(20260929)
for (kind in c("range_expansion", "founder_event")) {
  idx <- which(selected$event_kind == kind & !selected$boundary)
  if (length(idx)) {
    result <- run_helper(selected[idx, ], dmats, times)$result
    selected$r_source[idx] <- result[[if (kind == "range_expansion") "ana_dispersal_from" else "clado_dispersal_from"]]
  }
  # The real engine selects the younger matrix exactly at a split boundary.
  # Pass that already-selected matrix to R rather than altering the event age.
  for (q in 0:1) {
    idx <- which(selected$event_kind == kind & selected$boundary & selected$q_index == q)
    if (length(idx)) {
      result <- run_helper(selected[idx, ], dmats[[q + 1L]])$result
      selected$r_source[idx] <- result[[if (kind == "range_expansion") "ana_dispersal_from" else "clado_dispersal_from"]]
    }
  }
}

selected$case <- paste(selected$event_kind, selected$q_index, selected$from_text, selected$target_text, selected$boundary, sep = ":")
probability_rows <- list(); distribution_rows <- list()
for (case in unique(selected$case)) {
  rows <- selected[selected$case == case, ]
  first <- rows[1, , drop = FALSE]
  p <- run_helper(first, dmats[[first$q_index + 1L]], instrument = TRUE)$probabilities
  stopifnot(!is.null(p), abs(sum(p) - 1) < 1e-12)
  # Verify instrumentation does not change the helper's draw for the same R seed.
  set.seed(781); original <- run_helper(first, dmats[[first$q_index + 1L]])$result
  set.seed(781); probed <- run_helper(first, dmats[[first$q_index + 1L]], instrument = TRUE)$result
  stopifnot(identical(original, probed))
  expected <- unname(p[areas[rows$source_area_index + 1L]])
  error <- max(abs(expected - rows$conditional_probability))
  stopifnot(is.finite(error), error < 1e-12, all(rows$r_source %in% names(p)))
  for (source in names(p)) {
    probability_rows[[length(probability_rows) + 1L]] <- data.frame(case = case, source = source, r_probability = p[[source]], rust_observed_probability = if (any(areas[rows$source_area_index + 1L] == source)) unique(rows$conditional_probability[areas[rows$source_area_index + 1L] == source])[1] else NA_real_, max_abs_error = error)
    n <- nrow(rows); probability <- p[[source]]
    rust_count <- sum(areas[rows$source_area_index + 1L] == source)
    r_count <- sum(rows$r_source == source)
    tolerance <- if (probability %in% c(0,1)) 0 else 6 * sqrt(n * probability * (1 - probability)) + 2
    diff_tolerance <- if (probability %in% c(0,1)) 0 else 6 * sqrt(2 * n * probability * (1 - probability)) + 2
    passed <- abs(rust_count - n * probability) <= tolerance && abs(r_count - n * probability) <= tolerance && abs(rust_count - r_count) <= diff_tolerance
    distribution_rows[[length(distribution_rows) + 1L]] <- data.frame(case = case, source = source, n = n, probability = probability, rust_count = rust_count, r_count = r_count, rust_frequency = rust_count/n, r_frequency = r_count/n, passed = passed)
    stopifnot(passed)
  }
}
write.table(do.call(rbind, probability_rows), file.path(output_dir, "r-probabilities.tsv"), sep = "\t", row.names = FALSE, quote = FALSE)
write.table(do.call(rbind, distribution_rows), file.path(output_dir, "r-distributions.tsv"), sep = "\t", row.names = FALSE, quote = FALSE)

# Capture the intentional edge-case differences using the actual R helpers.
edge_cases <- list()
for (kind in c("range_expansion", "founder_event")) {
  one <- selected[selected$event_kind == kind & selected$from_text == "AB" & selected$target_text == "C", ][1, , drop = FALSE]
  for (weights in list(c(9,1), c(1,9), c(0,1), c(1,0), c(1,1))) {
    matrix <- young; matrix[1:2, 3] <- weights
    observed <- run_helper(one, matrix, instrument = TRUE)$probabilities
    expected <- weights / sum(weights)
    stopifnot(max(abs(unname(observed[c("A","B")]) - expected)) < 1e-12)
    edge_cases[[length(edge_cases) + 1L]] <- data.frame(event_kind = kind, weights = paste(weights, collapse=":"), p_A = observed[["A"]], p_B = observed[["B"]], passed = TRUE)
  }
}
write.table(do.call(rbind, edge_cases), file.path(output_dir, "r-edge-probabilities.tsv"), sep="\t", row.names=FALSE, quote=FALSE)
toy_ana <- data.frame(event_id = "zero", event_txt = "AB->ABC", abs_event_time = 1, dispersal_to = "C", trailing = 0)
toy_clado <- data.frame(event_id = "zero", clado_event_txt = "AB->AB,C", time_bp = 1, clado_dispersal_to = "C", trailing = 0)
warnings <- character()
invisible(capture.output(zero_ana <- withCallingHandlers(ana_fun(toy_ana, areas, matrix(0,3,3)), warning = function(w) { warnings <<- c(warnings, conditionMessage(w)); invokeRestart("muffleWarning") })))
zero_clado <- tryCatch({ clado_fun(toy_clado, areas, matrix(0,3,3)); FALSE }, error = function(e) TRUE)
stopifnot(length(warnings) > 0, zero_ana$ana_dispersal_from %in% c("A", "B"), zero_clado)
boundary <- selected[selected$boundary & selected$from_text == "AB" & selected$target_text == "C", ][1, , drop = FALSE]
stopifnot(nrow(boundary) == 1L, !is.na(boundary$case))
r_boundary_p <- run_helper(boundary, dmats, times, instrument = TRUE)$probabilities
stopifnot(abs(r_boundary_p[["A"]] - 0.1) < 1e-12, boundary$q_index == 0)
summary <- data.frame(key = c("status", "BioGeoBEARS_version", "R_version", "ana_helper_body_sha256", "clado_helper_body_sha256", "events_compared", "conditional_cases", "max_probability_error", "distribution_gate", "exact_boundary_difference", "zero_ana_behavior", "zero_clado_behavior", "rust_zero_policy"), value = c("passed", as.character(packageVersion("BioGeoBEARS")), R.version.string, digest::digest(body(ana_fun), algo="sha256"), digest::digest(body(clado_fun), algo="sha256"), nrow(selected), length(unique(selected$case)), max(vapply(probability_rows, function(x) x$max_abs_error, 0.0)), "each_language_6sigma_plus2_and_between_languages_6sigma_plus2", "R_older_pA_0.1_Rust_actual_younger_pA_0.9", "warning_then_uniform_fallback", "error", "undetermined_zero_total_weight_no_fallback"))
write.table(summary, file.path(output_dir, "r-comparison-summary.tsv"), sep="\t", row.names=FALSE, quote=FALSE)
print(summary, row.names=FALSE)
