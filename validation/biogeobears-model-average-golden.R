args <- commandArgs(trailingOnly = TRUE)
weights_path <- if (length(args) >= 1) {
  args[[1]]
} else {
  "validation/golden/biogeobears-model-average-weights.tsv"
}
probabilities_path <- if (length(args) >= 2) {
  args[[2]]
} else {
  "validation/golden/biogeobears-model-average-ancestral.tsv"
}

source("validation/r-env.R")
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

repo_root <- env$repo_root
tree_path <- normalizePath(
  file.path(repo_root, "validation/fixtures/three_area_tri_tip_null/tree.nwk"),
  winslash = "/",
  mustWork = TRUE
)
ranges_path <- normalizePath(
  file.path(repo_root, "validation/fixtures/three_area_tri_tip_null/ranges.tsv"),
  winslash = "/",
  mustWork = TRUE
)

write_lagrange_geog <- function(ranges_path, output_path) {
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
  writeLines(lines, output_path, useBytes = TRUE)
}

set_parameter <- function(model, name, type, init, min_value, max_value) {
  table <- model@params_table
  table[name, "type"] <- type
  for (column in c("init", "est")) {
    if (column %in% colnames(table)) {
      table[name, column] <- init
    }
  }
  if ("min" %in% colnames(table)) {
    table[name, "min"] <- min_value
  }
  if ("max" %in% colnames(table)) {
    table[name, "max"] <- max_value
  }
  model@params_table <- table
  model
}

extract_loglike <- function(result) {
  for (name in c("total_loglike", "loglike", "LnL")) {
    if (!is.null(result[[name]]) && length(result[[name]]) == 1) {
      return(as.numeric(result[[name]]))
    }
  }
  if (!is.null(result$optim_result) && !is.null(result$optim_result$value)) {
    return(as.numeric(result$optim_result$value))
  }
  stop("Could not extract BioGeoBEARS log-likelihood", call. = FALSE)
}

run_model <- function(model_id, founder_event) {
  tmp_dir <- tempfile("bgb-model-average-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
  run_object$max_range_size <- 2L
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

  model <- run_object$BioGeoBEARS_model_object
  model <- set_parameter(model, "d", "free", 0.01, 1e-12, 4.999999999999)
  model <- set_parameter(model, "e", "free", 0.01, 1e-12, 4.999999999999)
  if (founder_event) {
    model <- set_parameter(model, "j", "free", 0.0001, 0.00001, 2.99999)
  } else {
    model <- set_parameter(model, "j", "fixed", 0, 0.00001, 2.99999)
  }
  run_object$BioGeoBEARS_model_object <- model

  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  list(
    model_id = model_id,
    result = result,
    lnL = extract_loglike(result),
    numparams = if (founder_event) 3L else 2L
  )
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
  sort(unlist(
    lapply(children, function(child) descendant_tip_labels(tree, child)),
    use.names = FALSE
  ))
}

models <- list(
  run_model("DEC", FALSE),
  run_model("DEC+J", TRUE)
)
tree <- read.tree(tree_path)
tip_count <- length(tree$tip.label)
sample_size <- tip_count

comparison <- data.frame(
  model_id = vapply(models, `[[`, character(1), "model_id"),
  LnL = vapply(models, `[[`, numeric(1), "lnL"),
  numparams = vapply(models, `[[`, integer(1), "numparams"),
  stringsAsFactors = FALSE
)
comparison$AIC <- calc_AIC_vals(comparison$LnL, comparison$numparams)
comparison <- AkaikeWeights_on_summary_table(comparison, colname_to_use = "AIC")
if (any(sample_size <= comparison$numparams + 1L)) {
  message("AICc omitted because n <= k + 1 for at least one candidate model")
}

weights <- data.frame(
  criterion = "AIC",
  model_id = comparison$model_id,
  lnL = sprintf("%.17f", comparison$LnL),
  numparams = comparison$numparams,
  information_criterion = sprintf("%.17f", comparison$AIC),
  weight = sprintf("%.17f", comparison$AIC_wt),
  stringsAsFactors = FALSE
)

states <- models[[1]]$result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
area_names <- names(read.delim(ranges_path, check.names = FALSE))[-1]
internal_nodes <- seq.int(tip_count + 1, tip_count + tree$Nnode)
probability_rows <- list()
for (node in internal_nodes) {
  clade <- paste(descendant_tip_labels(tree, node), collapse = "+")
  averaged <- Reduce(
    `+`,
    lapply(seq_along(models), function(index) {
      comparison$AIC_wt[[index]] *
        models[[index]]$result$ML_marginal_prob_each_state_at_branch_top_AT_node[node, ]
    })
  )
  for (state_index in seq_along(states)) {
    state <- states[[state_index]]
    probability_rows[[length(probability_rows) + 1]] <- data.frame(
      criterion = "AIC",
      kind = if (node == tip_count + 1) "root" else "internal",
      clade = clade,
      state_index = state_index - 1,
      range_bits = state_to_bits(state),
      range = state_to_label(state, area_names),
      biogeobears_probability = sprintf("%.17f", averaged[[state_index]]),
      stringsAsFactors = FALSE
    )
  }
}

dir.create(dirname(weights_path), recursive = TRUE, showWarnings = FALSE)
dir.create(dirname(probabilities_path), recursive = TRUE, showWarnings = FALSE)
write.table(weights, weights_path, sep = "\t", quote = FALSE, row.names = FALSE)
write.table(
  do.call(rbind, probability_rows),
  probabilities_path,
  sep = "\t",
  quote = FALSE,
  row.names = FALSE
)
cat("Wrote", weights_path, "and", probabilities_path, "\n")
