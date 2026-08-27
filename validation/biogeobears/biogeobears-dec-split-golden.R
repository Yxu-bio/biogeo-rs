args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/dec_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-dec-split.tsv"
model_preset <- if (length(args) >= 3) toupper(args[[3]]) else "DEC"

if (!(model_preset %in% c("DEC", "DIVALIKE", "BAYAREALIKE"))) {
  stop("model_preset must be DEC, DIVALIKE, or BAYAREALIKE", call. = FALSE)
}

source("validation/biogeobears/r-env.R")
source("validation/biogeobears/biogeobears-fixture-modifiers.R")
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
      ". Run: Rscript validation/biogeobears/setup-local-r-biogeobears.R"
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

set_optional_range_size_params <- function(model_object, case) {
  for (name in c("mx01y", "mx01s", "mx01v", "mx01j")) {
    if (name %in% names(case) && !is.na(case[[name]]) && nzchar(as.character(case[[name]]))) {
      model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
    }
  }

  model_object
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

node_children <- function(tree, node) {
  children <- tree$edge[tree$edge[, 1] == node, 2]
  if (length(children) != 2) {
    stop("Expected binary node ", node, " but found ", length(children), " children", call. = FALSE)
  }
  children
}

node_ages_from_present <- function(tree) {
  depths <- node.depth.edgelength(tree)
  tip_depths <- depths[seq_along(tree$tip.label)]
  tree_height <- max(tip_depths)
  pmax(0, tree_height - depths)
}

timeperiod_index_at_age <- function(age, oldest_ages) {
  if (length(oldest_ages) == 0) {
    return(1L)
  }
  tolerance <- max(1, max(oldest_ages)) * 1e-12
  matches <- which(age <= oldest_ages + tolerance)
  if (length(matches) == 0) {
    stop(
      "Node age ", age, " exceeds the oldest BioGeoBEARS time period",
      call. = FALSE
    )
  }
  matches[[1]]
}

run_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-dec-split-")
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

  include_null_range <- parse_bool(case$include_null_range)

  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
  run_object$max_range_size <- as.integer(case$max_range_size)
  run_object$include_null_range <- include_null_range
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
  run_object$BioGeoBEARS_model_object <- set_optional_range_size_params(
    run_object$BioGeoBEARS_model_object,
    case
  )

  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  states_list <- result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
  node_posteriors <- result$ML_marginal_prob_each_state_at_branch_top_AT_node
  downpass_bottom <- result$relative_probs_of_each_state_at_branch_bottom_below_node_DOWNPASS
  if (is.null(states_list) || is.null(node_posteriors) || is.null(downpass_bottom)) {
    stop("BioGeoBEARS result did not contain split probability inputs", call. = FALSE)
  }

  tree <- read.tree(tree_path)
  area_names <- range_table_area_names(ranges_path)
  tip_count <- length(tree$tip.label)
  root_node <- tip_count + 1
  internal_nodes <- seq.int(root_node, tip_count + tree$Nnode)
  coo_offset <- if (include_null_range) 2 else 1
  oldest_ages <- if (is.null(run_object$timeperiods)) {
    numeric(0)
  } else {
    as.numeric(run_object$timeperiods)
  }
  node_ages <- if (length(oldest_ages) == 0) {
    rep(0, tip_count + tree$Nnode)
  } else {
    node_ages_from_present(tree)
  }
  timeperiod_count <- if (length(oldest_ages) == 0) 1L else length(oldest_ages)
  mats_by_timeperiod <- lapply(seq_len(timeperiod_count), function(timeperiod_i) {
    mats <- get_Qmat_COOmat_from_res(
      result,
      timeperiod_i = timeperiod_i,
      include_null_range = include_null_range
    )
    if (is.null(mats$COO_weights_columnar) || is.null(mats$Rsp_rowsums)) {
      stop(
        "BioGeoBEARS did not return COO cladogenesis weights for time period ",
        timeperiod_i,
        call. = FALSE
      )
    }
    mats
  })
  master_state_bits <- vapply(states_list, state_to_bits, numeric(1))

  rows <- list()
  for (node in internal_nodes) {
    child_edge_lengths <- tree$edge.length[tree$edge[, 1] == node]
    if (any(child_edge_lengths < min_branch_length)) {
      next
    }

    timeperiod_i <- timeperiod_index_at_age(node_ages[[node]], oldest_ages)
    mats <- mats_by_timeperiod[[timeperiod_i]]
    coo <- mats$COO_weights_columnar
    rowsums <- mats$Rsp_rowsums
    scenario_weights <- coo[[4]] / rowsums[coo[[1]] + 1]
    local_to_master <- match(
      vapply(mats$states_list, state_to_bits, numeric(1)),
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
    node_likelihoods <- numeric(length(states_list))
    for (scenario_index in seq_along(local_likelihoods)) {
      ancestor_index <- ancestor_indices[[scenario_index]]
      node_likelihoods[[ancestor_index]] <-
        node_likelihoods[[ancestor_index]] + local_likelihoods[[scenario_index]]
    }

    node_posterior <- node_posteriors[node, ]
    impossible <- node_likelihoods <= 0 & node_posterior > 1e-10
    if (any(impossible)) {
      stop(
        "Positive node posterior has zero local split likelihood at node ",
        node,
        call. = FALSE
      )
    }
    outside_likelihoods <- numeric(length(states_list))
    positive <- node_likelihoods > 0
    outside_likelihoods[positive] <- node_posterior[positive] / node_likelihoods[positive]
    probs <- outside_likelihoods[ancestor_indices] * local_likelihoods
    probs_sum <- sum(probs)
    if (!is.finite(probs_sum) || probs_sum <= 0) {
      stop("Corrected split posterior has no probability mass at node ", node, call. = FALSE)
    }
    probs <- probs / probs_sum

    kind <- if (node == root_node) "root" else "internal"
    clade <- node_clade(tree, node)
    left_clade <- node_clade(tree, left_node)
    right_clade <- node_clade(tree, right_node)

    for (scenario_index in seq_along(probs)) {
      ancestor_index_1based <- ancestor_indices[[scenario_index]]
      left_index_1based <- left_indices[[scenario_index]]
      right_index_1based <- right_indices[[scenario_index]]
      ancestor_state <- states_list[[ancestor_index_1based]]
      left_state <- states_list[[left_index_1based]]
      right_state <- states_list[[right_index_1based]]

      rows[[length(rows) + 1]] <- data.frame(
        case_id = case$case_id,
        bgb_node = node,
        kind = kind,
        clade = clade,
        left_clade = left_clade,
        right_clade = right_clade,
        ancestor_state_index = ancestor_index_1based - 1,
        ancestor_range_bits = state_to_bits(ancestor_state),
        ancestor_range = state_to_label(ancestor_state, area_names),
        left_state_index = left_index_1based - 1,
        left_range_bits = state_to_bits(left_state),
        left_range = state_to_label(left_state, area_names),
        right_state_index = right_index_1based - 1,
        right_range_bits = state_to_bits(right_state),
        right_range = state_to_label(right_state, area_names),
        biogeobears_scenario_weight = sprintf("%.15f", scenario_weights[[scenario_index]]),
        biogeobears_probability = sprintf("%.15f", probs[[scenario_index]]),
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
if ("biogeobears_split_ready" %in% names(fixtures)) {
  fixtures <- fixtures[
    tolower(fixtures$biogeobears_split_ready) == "true",
    ,
    drop = FALSE
  ]
}

rows <- list()
for (i in seq_len(nrow(fixtures))) {
  case <- fixtures[i, , drop = FALSE]
  cat("Running BioGeoBEARS ", model_preset, " split fixture: ", case$case_id, "\n", sep = "")
  rows[[length(rows) + 1]] <- run_case(case, repo_root)
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
