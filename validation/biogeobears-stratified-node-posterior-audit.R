args <- commandArgs(trailingOnly = TRUE)
output_path <- if (length(args) >= 1) args[[1]] else {
  "validation/golden/biogeobears-detection-full-stack-fixnode-posterior.tsv"
}
case_id <- if (length(args) >= 2) args[[2]] else {
  "psychotria_detection_constrained_full_stack"
}
node_filter <- integer(0)
if (length(args) >= 3 && tolower(args[[3]]) != "all") {
  node_filter <- as.integer(args[[3]])
  if (length(node_filter) != 1 || is.na(node_filter)) {
    stop("node filter must be an integer node number or 'all'", call. = FALSE)
  }
}
split_output_path <- if (length(args) >= 4) args[[4]] else NULL

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
  stop("Could not extract BioGeoBEARS log-likelihood", call. = FALSE)
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
  sort(unlist(lapply(children, function(child) {
    descendant_tip_labels(tree, child)
  }), use.names = FALSE))
}

node_children <- function(tree, node) {
  children <- tree$edge[tree$edge[, 1] == node, 2]
  if (length(children) != 2) {
    stop("Expected a bifurcating node: ", node, call. = FALSE)
  }
  children
}

node_ages_from_present <- function(tree) {
  depths <- node.depth.edgelength(tree)
  tree_height <- max(depths[seq_along(tree$tip.label)])
  pmax(0, tree_height - depths)
}

timeperiod_index_at_age <- function(age, oldest_ages) {
  tolerance <- max(1, max(oldest_ages)) * 1e-12
  matches <- which(age <= oldest_ages + tolerance)
  if (length(matches) == 0) {
    stop("Node age exceeds the oldest BioGeoBEARS time period: ", age, call. = FALSE)
  }
  matches[[1]]
}

run_fixed <- function(run_object) {
  if ("skip_optim" %in% names(formals(bears_optim_run))) {
    result <- bears_optim_run(
      BioGeoBEARS_run_object = run_object,
      skip_optim = TRUE,
      skip_optim_option = "return_loglike"
    )
  } else {
    result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  }
  extract_loglike(result)
}

repo_root <- env$repo_root
fixtures <- read.delim(
  file.path(repo_root, "validation", "detection_combination_fixtures.tsv"),
  check.names = FALSE,
  stringsAsFactors = FALSE
)
case <- fixtures[fixtures$case_id == case_id, , drop = FALSE]
if (nrow(case) != 1) {
  stop("Expected exactly one detection combination case named: ", case_id, call. = FALSE)
}

tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
detections_path <- normalizePath(
  file.path(repo_root, case$detections),
  winslash = "/",
  mustWork = TRUE
)
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
run_object$calc_ancprobs <- FALSE
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

direct_run <- run_object
direct_run$calc_ancprobs <- TRUE
direct_result <- bears_optim_run(BioGeoBEARS_run_object = direct_run)
direct_probabilities <- direct_result$ML_marginal_prob_each_state_at_branch_top_AT_node
states <- direct_result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
if (is.null(direct_probabilities) || is.null(states)) {
  stop("BioGeoBEARS direct uppass did not return posterior inputs", call. = FALSE)
}

tip_count <- length(tree$tip.label)
internal_nodes <- seq.int(tip_count + 1L, tip_count + tree$Nnode)
if (length(node_filter) == 1) {
  if (!(node_filter %in% internal_nodes)) {
    stop("Requested node is not an internal BioGeoBEARS node: ", node_filter, call. = FALSE)
  }
  internal_nodes <- node_filter
}
node_ages <- node_ages_from_present(tree)
oldest_ages <- as.numeric(run_object$timeperiods)

rows <- list()
fixnode_probabilities <- list()
for (node in internal_nodes) {
  message("fixnode posterior audit: node=", node)
  state_loglikes <- vapply(seq_along(states), function(state_index) {
    fixed_run <- run_object
    fixed_run$fixnode <- node
    fixed_run$fixlikes <- replace(rep(0, length(states)), state_index, 1)
    tryCatch(
      suppressWarnings(run_fixed(fixed_run)),
      error = function(error) {
        message(
          "  state ",
          state_index - 1L,
          " has zero/invalid conditional likelihood: ",
          conditionMessage(error)
        )
        -Inf
      }
    )
  }, numeric(1))
  if (!any(is.finite(state_loglikes))) {
    stop("All fixed-node states failed at node ", node, call. = FALSE)
  }
  maximum <- max(state_loglikes)
  weights <- exp(state_loglikes - maximum)
  probabilities <- weights / sum(weights)
  fixnode_probabilities[[as.character(node)]] <- probabilities
  age <- node_ages[[node]]
  period <- timeperiod_index_at_age(age, oldest_ages)
  clade <- paste(descendant_tip_labels(tree, node), collapse = "+")

  for (state_index in seq_along(states)) {
    state <- states[[state_index]]
    direct_probability <- direct_probabilities[node, state_index]
    rows[[length(rows) + 1L]] <- data.frame(
      biogeobears_version = as.character(packageVersion("BioGeoBEARS")),
      case_id = case_id,
      bgb_node = node,
      clade = clade,
      age = sprintf("%.15f", age),
      timeperiod = period,
      state_index = state_index - 1L,
      range_bits = state_to_bits(state),
      range = state_to_label(state, area_names),
      fixed_node_lnL = sprintf("%.15f", state_loglikes[[state_index]]),
      fixnode_probability = sprintf("%.15f", probabilities[[state_index]]),
      direct_uppass_probability = sprintf("%.15f", direct_probability),
      absolute_delta = sprintf(
        "%.15f",
        abs(probabilities[[state_index]] - direct_probability)
      ),
      stringsAsFactors = FALSE
    )
  }
}

resolve_output_path <- function(path) {
  if (!grepl("^(/|[A-Za-z]:[/\\\\])", path)) {
    path <- file.path(repo_root, path)
  }
  normalizePath(path, winslash = "/", mustWork = FALSE)
}
output_path <- resolve_output_path(output_path)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(
  do.call(rbind, rows),
  output_path,
  sep = "\t",
  quote = FALSE,
  row.names = FALSE
)
cat("Wrote BioGeoBEARS fixnode posterior audit to ", output_path, "\n", sep = "")

if (!is.null(split_output_path)) {
  uppass_top <- direct_result$relative_probs_of_each_state_at_branch_top_AT_node_UPPASS
  downpass_bottom <- direct_result$relative_probs_of_each_state_at_branch_bottom_below_node_DOWNPASS
  if (is.null(uppass_top) || is.null(downpass_bottom)) {
    stop("BioGeoBEARS result did not contain split-posterior inputs", call. = FALSE)
  }

  period_count <- length(oldest_ages)
  matrices_by_period <- lapply(seq_len(period_count), function(timeperiod_i) {
    matrices <- get_Qmat_COOmat_from_res(
      direct_result,
      timeperiod_i = timeperiod_i,
      include_null_range = run_object$include_null_range
    )
    if (is.null(matrices$COO_weights_columnar) || is.null(matrices$Rsp_rowsums)) {
      stop("Missing cladogenesis COO weights for period ", timeperiod_i, call. = FALSE)
    }
    matrices
  })
  coo_offset <- if (run_object$include_null_range) 2L else 1L
  master_state_bits <- vapply(states, state_to_bits, numeric(1))
  split_rows <- list()

  for (node in internal_nodes) {
    age <- node_ages[[node]]
    period <- timeperiod_index_at_age(age, oldest_ages)
    matrices <- matrices_by_period[[period]]
    coo <- matrices$COO_weights_columnar
    rowsums <- matrices$Rsp_rowsums
    scenario_weights <- coo[[4]] / rowsums[coo[[1]] + 1L]
    local_to_master <- match(
      vapply(matrices$states_list, state_to_bits, numeric(1)),
      master_state_bits
    )
    if (any(is.na(local_to_master))) {
      stop("A period state could not be mapped to the master state space", call. = FALSE)
    }
    ancestor_indices <- local_to_master[coo[[1]] + coo_offset]
    left_indices <- local_to_master[coo[[2]] + coo_offset]
    right_indices <- local_to_master[coo[[3]] + coo_offset]
    children <- node_children(tree, node)
    left_node <- children[[1]]
    right_node <- children[[2]]

    local_likelihoods <- scenario_weights *
      downpass_bottom[left_node, left_indices] *
      downpass_bottom[right_node, right_indices]
    local_likelihoods[!is.finite(local_likelihoods)] <- 0
    node_likelihoods <- numeric(length(states))
    for (scenario_index in seq_along(local_likelihoods)) {
      ancestor_index <- ancestor_indices[[scenario_index]]
      node_likelihoods[[ancestor_index]] <-
        node_likelihoods[[ancestor_index]] + local_likelihoods[[scenario_index]]
    }

    node_posterior <- fixnode_probabilities[[as.character(node)]]
    impossible <- node_likelihoods <= 0 & node_posterior > 1e-10
    if (any(impossible)) {
      stop("Positive fixnode posterior has zero local likelihood at node ", node, call. = FALSE)
    }
    outside_likelihoods <- numeric(length(states))
    positive <- node_likelihoods > 0
    outside_likelihoods[positive] <- node_posterior[positive] / node_likelihoods[positive]
    corrected <- outside_likelihoods[ancestor_indices] * local_likelihoods
    corrected_sum <- sum(corrected)
    if (!is.finite(corrected_sum) || corrected_sum <= 0) {
      stop("Corrected split posterior has no mass at node ", node, call. = FALSE)
    }
    corrected <- corrected / corrected_sum

    corrected_ancestor_marginal <- numeric(length(states))
    for (scenario_index in seq_along(corrected)) {
      ancestor_index <- ancestor_indices[[scenario_index]]
      corrected_ancestor_marginal[[ancestor_index]] <-
        corrected_ancestor_marginal[[ancestor_index]] + corrected[[scenario_index]]
    }
    marginal_delta <- max(abs(corrected_ancestor_marginal - node_posterior))
    if (marginal_delta > 1e-10) {
      stop("Corrected split marginal mismatch at node ", node, ": ", marginal_delta, call. = FALSE)
    }

    direct <- uppass_top[node, ancestor_indices] * local_likelihoods
    direct_sum <- sum(direct)
    if (!is.finite(direct_sum) || direct_sum <= 0) {
      stop("Direct uppass split posterior has no mass at node ", node, call. = FALSE)
    }
    direct <- direct / direct_sum
    clade <- paste(descendant_tip_labels(tree, node), collapse = "+")
    left_clade <- paste(descendant_tip_labels(tree, left_node), collapse = "+")
    right_clade <- paste(descendant_tip_labels(tree, right_node), collapse = "+")

    for (scenario_index in seq_along(corrected)) {
      ancestor_index <- ancestor_indices[[scenario_index]]
      left_index <- left_indices[[scenario_index]]
      right_index <- right_indices[[scenario_index]]
      ancestor_state <- states[[ancestor_index]]
      left_state <- states[[left_index]]
      right_state <- states[[right_index]]
      split_rows[[length(split_rows) + 1L]] <- data.frame(
        biogeobears_version = as.character(packageVersion("BioGeoBEARS")),
        case_id = case_id,
        bgb_node = node,
        kind = if (node == tip_count + 1L) "root" else "internal",
        clade = clade,
        left_clade = left_clade,
        right_clade = right_clade,
        age = sprintf("%.15f", age),
        timeperiod = period,
        ancestor_state_index = ancestor_index - 1L,
        ancestor_range_bits = state_to_bits(ancestor_state),
        ancestor_range = state_to_label(ancestor_state, area_names),
        left_state_index = left_index - 1L,
        left_range_bits = state_to_bits(left_state),
        left_range = state_to_label(left_state, area_names),
        right_state_index = right_index - 1L,
        right_range_bits = state_to_bits(right_state),
        right_range = state_to_label(right_state, area_names),
        biogeobears_scenario_weight = sprintf("%.15f", scenario_weights[[scenario_index]]),
        fixnode_probability = sprintf("%.15f", corrected[[scenario_index]]),
        direct_uppass_probability = sprintf("%.15f", direct[[scenario_index]]),
        absolute_delta = sprintf("%.15f", abs(corrected[[scenario_index]] - direct[[scenario_index]])),
        stringsAsFactors = FALSE
      )
    }
  }

  split_output_path <- resolve_output_path(split_output_path)
  dir.create(dirname(split_output_path), recursive = TRUE, showWarnings = FALSE)
  write.table(
    do.call(rbind, split_rows),
    split_output_path,
    sep = "\t",
    quote = FALSE,
    row.names = FALSE
  )
  cat("Wrote BioGeoBEARS corrected split audit to ", split_output_path, "\n", sep = "")
}
