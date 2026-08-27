args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/ambiguity_fixtures.tsv"
profile_output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-ambiguity-profile.tsv"
tips_output_path <- if (length(args) >= 3) args[[3]] else "validation/golden/biogeobears-ambiguity-tip-likelihoods.tsv"
ancestral_output_path <- if (length(args) >= 4) args[[4]] else "validation/golden/biogeobears-ambiguity-ancestral.tsv"
optimization_output_path <- if (length(args) >= 5) args[[5]] else "validation/golden/biogeobears-ambiguity-optim.tsv"
source_semantics_output_path <- if (length(args) >= 6) args[[6]] else "validation/golden/biogeobears-ambiguity-source-semantics.tsv"

source("validation/biogeobears/r-env.R")
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

write_lagrange_geog <- function(ranges_path, output_path) {
  ranges <- read.delim(
    ranges_path,
    check.names = FALSE,
    stringsAsFactors = FALSE,
    colClasses = "character"
  )
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
  table <- model_object@params_table
  if (!(name %in% rownames(table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
  table[name, "type"] <- "fixed"
  for (column in intersect(c("init", "est", "min", "max"), colnames(table))) {
    table[name, column] <- value
  }
  model_object@params_table <- table
  model_object
}

set_free_param <- function(model_object, name, init, min_value, max_value) {
  table <- model_object@params_table
  if (!(name %in% rownames(table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
  table[name, "type"] <- "free"
  for (column in intersect(c("init", "est"), colnames(table))) {
    table[name, column] <- init
  }
  table[name, "min"] <- min_value
  table[name, "max"] <- max_value
  model_object@params_table <- table
  model_object
}

extract_loglike <- function(result) {
  if (is.numeric(result) && length(result) == 1 && is.finite(result)) {
    return(as.numeric(result))
  }
  for (name in c("total_loglikelihood", "total_loglike", "lnL", "loglike", "LnL")) {
    value <- result[[name]]
    if (!is.null(value) && is.numeric(value) && length(value) == 1 && is.finite(value)) {
      return(as.numeric(value))
    }
  }
  if (!is.null(result$outputs)) {
    for (name in c("total_loglikelihood", "total_loglike", "lnL", "loglike", "LnL")) {
      value <- tryCatch(slot(result$outputs, name), error = function(e) NULL)
      if (!is.null(value) && is.numeric(value) && length(value) == 1 && is.finite(value)) {
        return(as.numeric(value))
      }
    }
  }
  if (!is.null(result$optim_result) && !is.null(result$optim_result$value)) {
    value <- as.numeric(result$optim_result$value[[1]])
    if (length(value) == 1 && is.finite(value)) {
      return(value)
    }
  }
  stop("Could not extract BioGeoBEARS log-likelihood", call. = FALSE)
}

extract_param <- function(result, name) {
  if (!is.null(result$outputs)) {
    table <- result$outputs@params_table
    if (name %in% rownames(table)) {
      return(as.numeric(table[name, "est"]))
    }
  }
  if (!is.null(result$optim_result) && !is.null(result$optim_result$par)) {
    parameters <- result$optim_result$par
    if (!is.null(names(parameters)) && name %in% names(parameters)) {
      return(as.numeric(parameters[[name]]))
    }
    index <- match(name, c("d", "e"))
    if (!is.na(index) && length(parameters) >= index) {
      return(as.numeric(parameters[[index]]))
    }
  }
  stop("Could not extract BioGeoBEARS parameter: ", name, call. = FALSE)
}

extract_convergence <- function(result) {
  optim_result <- result$optim_result
  if (is.data.frame(optim_result) && "convcode" %in% names(optim_result)) {
    return(as.integer(optim_result$convcode[[1]]))
  }
  if (!is.null(optim_result$convergence)) {
    return(as.integer(optim_result$convergence[[1]]))
  }
  NA_integer_
}

state_to_bits <- function(state) {
  if (length(state) == 0 || all(is.na(state)) || any(state %in% c("_", ""))) {
    return(0)
  }
  sum(2 ^ as.integer(state))
}

state_to_label <- function(state, area_names) {
  if (length(state) == 0 || all(is.na(state)) || any(state %in% c("_", ""))) {
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

new_run_object <- function(case, tree_path, geog_path, calc_ancprobs) {
  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
  run_object$max_range_size <- as.integer(case$max_range_size)
  run_object$include_null_range <- parse_bool(case$include_null_range)
  run_object$useAmbiguities <- TRUE
  run_object$min_branchlength <- 0
  run_object$print_optim <- FALSE
  run_object$num_cores_to_use <- 1
  run_object$use_optimx <- FALSE
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- calc_ancprobs
  run_object$speedup <- FALSE
  readfiles_BioGeoBEARS_run(run_object)
}

run_fixed_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-ambiguity-fixed-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  run_object <- new_run_object(case, tree_path, geog_path, TRUE)
  model_object <- run_object$BioGeoBEARS_model_object
  model_object <- set_fixed_param(model_object, "d", as.numeric(case$d))
  model_object <- set_fixed_param(model_object, "e", as.numeric(case$e))
  model_object <- set_fixed_param(model_object, "j", 0)
  run_object$BioGeoBEARS_model_object <- model_object
  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  phy <- read.tree(tree_path)
  tipranges <- getranges_from_LagrangePHYLIP(lgdata_fn = geog_path)
  states <- result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
  area_names <- colnames(tipranges@df)
  tip_likelihoods <- tipranges_to_tip_condlikes_of_data_on_each_state(
    tipranges = tipranges,
    phy = phy,
    states_list = states,
    maxareas = as.integer(case$max_range_size),
    include_null_range = parse_bool(case$include_null_range),
    useAmbiguities = TRUE
  )

  list(
    result = result,
    phy = phy,
    states = states,
    area_names = area_names,
    tip_likelihoods = tip_likelihoods,
    log_likelihood = extract_loglike(result)
  )
}

run_optimization_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-ambiguity-optim-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  run_object <- new_run_object(case, tree_path, geog_path, FALSE)
  model_object <- run_object$BioGeoBEARS_model_object
  model_object <- set_free_param(model_object, "d", 0.01, 1e-12, 10)
  model_object <- set_free_param(model_object, "e", 0.01, 1e-12, 10)
  model_object <- set_fixed_param(model_object, "j", 0)
  run_object$BioGeoBEARS_model_object <- model_object
  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", extract_loglike(result)),
    biogeobears_d = sprintf("%.15f", extract_param(result, "d")),
    biogeobears_e = sprintf("%.15f", extract_param(result, "e")),
    convergence = extract_convergence(result),
    optimizer = "L-BFGS-B",
    stringsAsFactors = FALSE
  )
}

source_semantics_rows <- function() {
  tmp_dir <- tempfile("bgb-ambiguity-source-semantics-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  geog_path <- file.path(tmp_dir, "geog.data")
  writeLines(
    c(
      "3\t3\t(A B C)",
      "all_unknown\t100",
      "absence_only\t100",
      "mixed\t100"
    ),
    geog_path,
    useBytes = TRUE
  )
  phy <- read.tree(text = "(all_unknown:1,(absence_only:0.5,mixed:0.5):0.5);")
  tipranges <- getranges_from_LagrangePHYLIP(lgdata_fn = geog_path)
  tipranges@df["all_unknown", ] <- c("?", "?", "?")
  tipranges@df["absence_only", ] <- c("0", "?", "0")
  tipranges@df["mixed", ] <- c("1", "?", "0")
  states <- areas_list_to_states_list_new(
    areas = 0:2,
    include_null_range = TRUE,
    maxareas = 2
  )
  likelihoods <- tipranges_to_tip_condlikes_of_data_on_each_state(
    tipranges = tipranges,
    phy = phy,
    states_list = states,
    maxareas = 2,
    include_null_range = TRUE,
    useAmbiguities = TRUE
  )
  area_names <- colnames(tipranges@df)
  num_states <- length(states)
  data.frame(
    case_id = "biogeobears_1.1.3_source_semantics",
    tip = rep(phy$tip.label, each = num_states),
    range_bits = rep(vapply(states, state_to_bits, numeric(1)), times = length(phy$tip.label)),
    range = rep(
      vapply(states, state_to_label, character(1), area_names = area_names),
      times = length(phy$tip.label)
    ),
    likelihood = sprintf("%.17g", as.vector(t(likelihoods))),
    stringsAsFactors = FALSE
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
profile_rows <- vector("list", nrow(fixtures))
tip_rows <- vector("list", nrow(fixtures))
ancestral_rows <- vector("list", nrow(fixtures))
optimization_rows <- vector("list", nrow(fixtures))

for (case_index in seq_len(nrow(fixtures))) {
  case <- fixtures[case_index, , drop = FALSE]
  cat("Running BioGeoBEARS ambiguity fixture: ", case$case_id, "\n", sep = "")
  fixed <- run_fixed_case(case, repo_root)
  package_version <- as.character(packageVersion("BioGeoBEARS"))
  profile_rows[[case_index]] <- data.frame(
    case_id = case$case_id,
    biogeobears_version = package_version,
    useAmbiguities = TRUE,
    biogeobears_lnL = sprintf("%.15f", fixed$log_likelihood),
    d = case$d,
    e = case$e,
    stringsAsFactors = FALSE
  )

  num_states <- length(fixed$states)
  state_bits <- vapply(fixed$states, state_to_bits, numeric(1))
  state_labels <- vapply(
    fixed$states,
    state_to_label,
    character(1),
    area_names = fixed$area_names
  )
  tip_rows[[case_index]] <- data.frame(
    case_id = rep(case$case_id, length(fixed$phy$tip.label) * num_states),
    tip = rep(fixed$phy$tip.label, each = num_states),
    range_bits = rep(state_bits, times = length(fixed$phy$tip.label)),
    range = rep(state_labels, times = length(fixed$phy$tip.label)),
    likelihood = sprintf("%.17g", as.vector(t(fixed$tip_likelihoods))),
    stringsAsFactors = FALSE
  )

  probabilities <- fixed$result$ML_marginal_prob_each_state_at_branch_top_AT_node
  if (is.null(probabilities)) {
    stop("BioGeoBEARS result did not contain ancestral probabilities", call. = FALSE)
  }
  tip_count <- length(fixed$phy$tip.label)
  root_node <- tip_count + 1L
  internal_nodes <- seq.int(root_node, tip_count + fixed$phy$Nnode)
  case_ancestral <- vector("list", length(internal_nodes) * num_states)
  row_index <- 1L
  for (node in internal_nodes) {
    for (state_index in seq_len(num_states)) {
      case_ancestral[[row_index]] <- data.frame(
        case_id = case$case_id,
        bgb_node = node,
        kind = if (node == root_node) "root" else "internal",
        clade = node_clade(fixed$phy, node),
        range_bits = state_bits[[state_index]],
        range = state_labels[[state_index]],
        biogeobears_probability = sprintf("%.15f", probabilities[node, state_index]),
        stringsAsFactors = FALSE
      )
      row_index <- row_index + 1L
    }
  }
  ancestral_rows[[case_index]] <- do.call(rbind, case_ancestral)

  cat("Optimizing BioGeoBEARS ambiguity fixture: ", case$case_id, "\n", sep = "")
  optimization_rows[[case_index]] <- run_optimization_case(case, repo_root)
}

for (path in c(
  profile_output_path,
  tips_output_path,
  ancestral_output_path,
  optimization_output_path,
  source_semantics_output_path
)) {
  dir.create(dirname(path), recursive = TRUE, showWarnings = FALSE)
}
write.table(do.call(rbind, profile_rows), profile_output_path, sep = "\t", quote = FALSE, row.names = FALSE)
write.table(do.call(rbind, tip_rows), tips_output_path, sep = "\t", quote = FALSE, row.names = FALSE)
write.table(do.call(rbind, ancestral_rows), ancestral_output_path, sep = "\t", quote = FALSE, row.names = FALSE)
write.table(do.call(rbind, optimization_rows), optimization_output_path, sep = "\t", quote = FALSE, row.names = FALSE)
write.table(source_semantics_rows(), source_semantics_output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat(
  "Wrote ", profile_output_path, ", ", tips_output_path, ", ",
  ancestral_output_path, ", ", optimization_output_path, ", and ",
  source_semantics_output_path, "\n",
  sep = ""
)
