args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/dec_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-dec-ancestral.tsv"
model_preset <- if (length(args) >= 3) toupper(args[[3]]) else "DEC"

if (!(model_preset %in% c("DEC", "DIVALIKE", "BAYAREALIKE"))) {
  stop("model_preset must be DEC, DIVALIKE, or BAYAREALIKE", call. = FALSE)
}

source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
env <- configure_project_r()

required_packages <- c("ape", "rexpokit", "cladoRcpp", "BioGeoBEARS")
missing_packages <- required_packages[
  !vapply(required_packages, requireNamespace, logical(1), quietly = TRUE)
]

if (length(missing_packages) > 0) {
  stop(
    paste0(
      "Missing required R packages: ",
      paste(missing_packages, collapse = ", "),
      ". Run: Rscript validation/setup-local-r-biogeobears.R"
    ),
    call. = FALSE
  )
}

suppressPackageStartupMessages({
  library(ape)
  library(rexpokit)
  library(cladoRcpp)
  library(BioGeoBEARS)
})

parse_bool <- function(value) {
  value <- tolower(as.character(value))
  if (value %in% c("true", "t", "1", "yes")) {
    return(TRUE)
  }
  if (value %in% c("false", "f", "0", "no")) {
    return(FALSE)
  }
  stop("Invalid boolean value: ", value, call. = FALSE)
}

write_lagrange_geog <- function(ranges_path, output_path) {
  ranges <- read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE)
  if (ncol(ranges) < 2 || names(ranges)[[1]] != "tip") {
    stop("Range table must have first column named 'tip': ", ranges_path, call. = FALSE)
  }

  area_names <- names(ranges)[-1]
  bits <- apply(ranges[, -1, drop = FALSE], 1, paste0, collapse = "")
  lines <- c(
    paste(nrow(ranges), length(area_names), paste0("(", paste(area_names, collapse = " "), ")"), sep = "\t"),
    paste(ranges[[1]], bits, sep = "\t")
  )

  writeLines(lines, output_path, useBytes = TRUE)
}

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

set_param_type <- function(model_object, name, type) {
  if (!(name %in% rownames(model_object@params_table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }

  table <- model_object@params_table
  table[name, "type"] <- type
  model_object@params_table <- table
  model_object
}

apply_model_preset <- function(model_object, preset) {
  if (preset == "DEC") {
    return(model_object)
  }

  if (preset == "DIVALIKE") {
    model_object <- set_fixed_param(model_object, "s", 0)
    model_object <- set_param_type(model_object, "ysv", "2-j")
    model_object <- set_param_type(model_object, "ys", "ysv*1/2")
    model_object <- set_param_type(model_object, "y", "ysv*1/2")
    model_object <- set_param_type(model_object, "v", "ysv*1/2")
    return(set_fixed_param(model_object, "mx01v", 0.5))
  }

  model_object <- set_fixed_param(model_object, "s", 0)
  model_object <- set_fixed_param(model_object, "v", 0)
  model_object <- set_param_type(model_object, "ysv", "1-j")
  model_object <- set_param_type(model_object, "ys", "ysv*1/1")
  model_object <- set_param_type(model_object, "y", "1-j")
  set_fixed_param(model_object, "mx01y", 0.9999)
}

numeric_case_value <- function(case, name, default) {
  if (!(name %in% names(case))) {
    return(default)
  }

  value <- case[[name]]
  if (is.na(value) || !nzchar(as.character(value))) {
    return(default)
  }

  as.numeric(value)
}

range_table_area_names <- function(ranges_path) {
  ranges <- read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE)
  names(ranges)[-1]
}

state_to_bits <- function(state) {
  if (length(state) == 1 && is.na(state[[1]])) {
    return(0)
  }

  sum(2 ^ as.integer(state))
}

state_to_label <- function(state, area_names) {
  if (length(state) == 1 && is.na(state[[1]])) {
    return("null")
  }

  paste(area_names[as.integer(state) + 1], collapse = "+")
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
  tmp_dir <- tempfile("bgb-dec-ancestral-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  requested_min_branch_length <- numeric_case_value(case, "min_branch_length", 0)
  min_branch_length <- resolve_biogeobears_min_branch_length(
    tree_path,
    requested_min_branch_length
  )
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
  run_object$max_range_size <- as.integer(case$max_range_size)
  run_object$include_null_range <- parse_bool(case$include_null_range)
  run_object$min_branchlength <- min_branch_length
  run_object$print_optim <- FALSE
  run_object$num_cores_to_use <- 1
  run_object$use_optimx <- FALSE
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- TRUE
  run_object$speedup <- FALSE

  run_object <- readfiles_BioGeoBEARS_run(run_object)
  run_object$min_branchlength <- min_branch_length
  run_object <- apply_fixture_dispersal_multipliers(run_object, case, repo_root)
  run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "d", as.numeric(case$d))
  run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "e", as.numeric(case$e))
  run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "j", numeric_case_value(case, "j", 0))
  run_object$BioGeoBEARS_model_object <- apply_model_preset(
    run_object$BioGeoBEARS_model_object,
    model_preset
  )

  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  probabilities <- result$ML_marginal_prob_each_state_at_branch_top_AT_node
  states_list <- result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
  if (is.null(probabilities) || is.null(states_list)) {
    stop("BioGeoBEARS result did not contain ancestral probabilities or state list", call. = FALSE)
  }

  tree <- read.tree(tree_path)
  area_names <- range_table_area_names(ranges_path)
  tip_count <- length(tree$tip.label)
  root_node <- tip_count + 1
  internal_nodes <- seq.int(root_node, tip_count + tree$Nnode)

  rows <- list()
  for (node in internal_nodes) {
    kind <- if (node == root_node) "root" else "internal"
    clade <- node_clade(tree, node)
    for (state_index in seq_along(states_list)) {
      state <- states_list[[state_index]]
      rows[[length(rows) + 1]] <- data.frame(
        case_id = case$case_id,
        bgb_node = node,
        kind = kind,
        clade = clade,
        state_index = state_index - 1,
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
fixtures <- fixtures[tolower(fixtures$biogeobears_ready) == "true", , drop = FALSE]
if ("biogeobears_posterior_ready" %in% names(fixtures)) {
  fixtures <- fixtures[
    tolower(fixtures$biogeobears_posterior_ready) == "true",
    ,
    drop = FALSE
  ]
}

rows <- list()
for (i in seq_len(nrow(fixtures))) {
  case <- fixtures[i, , drop = FALSE]
  cat("Running BioGeoBEARS ", model_preset, " ancestral fixture: ", case$case_id, "\n", sep = "")
  rows[[length(rows) + 1]] <- run_case(case, repo_root)
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
