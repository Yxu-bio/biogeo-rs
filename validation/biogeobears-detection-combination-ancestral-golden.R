args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else {
  "validation/detection_combination_fixtures.tsv"
}
output_path <- if (length(args) >= 2) args[[2]] else {
  "validation/golden/biogeobears-detection-combination-ancestral.tsv"
}

source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
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

parse_bool <- function(value) {
  tolower(as.character(value)) %in% c("true", "t", "1", "yes")
}

set_fixed_param <- function(model_object, name, value) {
  table <- model_object@params_table
  table[name, "type"] <- "fixed"
  for (column in intersect(c("init", "est", "min", "max"), colnames(table))) {
    table[name, column] <- value
  }
  model_object@params_table <- table
  model_object
}

state_to_bits <- function(state) {
  if (length(state) == 0 || all(is.na(state))) {
    return(0)
  }
  sum(2 ^ as.integer(state))
}

state_to_label <- function(state, area_names) {
  if (length(state) == 0 || all(is.na(state))) {
    return("null")
  }
  paste(area_names[as.integer(state) + 1L], collapse = "+")
}

descendant_tip_labels <- function(tree, node) {
  if (node <= length(tree$tip.label)) {
    return(tree$tip.label[[node]])
  }
  children <- tree$edge[tree$edge[, 1] == node, 2]
  sort(unlist(lapply(children, function(child) descendant_tip_labels(tree, child)), use.names = FALSE))
}

node_clade <- function(tree, node) {
  paste(descendant_tip_labels(tree, node), collapse = "+")
}

run_case <- function(case, repo_root) {
  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  detections_path <- normalizePath(file.path(repo_root, case$detections), winslash = "/", mustWork = TRUE)
  controls_path <- normalizePath(file.path(repo_root, case$controls), winslash = "/", mustWork = TRUE)
  tree <- read.tree(tree_path)
  area_names <- colnames(read_detections(detections_path, phy = tree))

  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$detects_fn <- detections_path
  run_object$controls_fn <- controls_path
  run_object$use_detection_model <- TRUE
  run_object$max_range_size <- as.integer(case$max_range_size)
  run_object$include_null_range <- parse_bool(case$include_null_range)
  run_object$min_branchlength <- 0
  run_object$print_optim <- FALSE
  run_object$num_cores_to_use <- 1
  run_object$use_optimx <- FALSE
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- TRUE
  run_object$speedup <- FALSE
  run_object <- readfiles_BioGeoBEARS_run(run_object)
  run_object <- apply_fixture_dispersal_multipliers(
    run_object,
    case,
    repo_root,
    area_names = area_names
  )

  model_object <- run_object$BioGeoBEARS_model_object
  fixed_names <- c(
    "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
    "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
  )
  for (name in fixed_names) {
    model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
  }
  run_object$BioGeoBEARS_model_object <- calc_linked_params_BioGeoBEARS_model_object(model_object)
  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  probabilities <- result$ML_marginal_prob_each_state_at_branch_top_AT_node
  states <- result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
  if (is.null(probabilities) || is.null(states)) {
    stop("BioGeoBEARS result did not contain ancestral probabilities or states", call. = FALSE)
  }

  tip_count <- length(tree$tip.label)
  root_node <- tip_count + 1L
  internal_nodes <- seq.int(root_node, tip_count + tree$Nnode)
  rows <- list()
  for (node in internal_nodes) {
    for (state_index in seq_along(states)) {
      state <- states[[state_index]]
      rows[[length(rows) + 1L]] <- data.frame(
        case_id = case$case_id,
        bgb_node = node,
        kind = if (node == root_node) "root" else "internal",
        clade = node_clade(tree, node),
        state_index = state_index - 1L,
        range_bits = state_to_bits(state),
        range = state_to_label(state, area_names),
        biogeobears_probability = sprintf("%.15f", probabilities[node, state_index]),
        stringsAsFactors = FALSE
      )
    }
  }
  do.call(rbind, rows)
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
if ("posterior_ready" %in% names(fixtures)) {
  fixtures <- fixtures[tolower(fixtures$posterior_ready) == "true", , drop = FALSE]
}
rows <- vector("list", nrow(fixtures))
for (index in seq_len(nrow(fixtures))) {
  cat("Running BioGeoBEARS detection combination ancestral: ", fixtures$case_id[[index]], "\n", sep = "")
  rows[[index]] <- run_case(fixtures[index, , drop = FALSE], repo_root)
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
