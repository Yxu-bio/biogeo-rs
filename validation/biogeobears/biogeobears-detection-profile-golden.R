args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/detection_profile_fixtures.tsv"
profile_output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-detection-profile.tsv"
tips_output_path <- if (length(args) >= 3) args[[3]] else "validation/golden/biogeobears-detection-tip-likelihoods.tsv"

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

set_fixed_param <- function(model_object, name, value) {
  table <- model_object@params_table
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

state_label <- function(state, area_names) {
  if (length(state) == 0 || all(is.na(state)) || any(state %in% c("_", ""))) {
    return("null")
  }
  paste(area_names[as.integer(state) + 1L], collapse = "+")
}

run_case <- function(case, repo_root) {
  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  detections_path <- normalizePath(file.path(repo_root, case$detections), winslash = "/", mustWork = TRUE)
  controls_path <- normalizePath(file.path(repo_root, case$controls), winslash = "/", mustWork = TRUE)

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

  model_object <- run_object$BioGeoBEARS_model_object
  for (name in c("d", "e", "mf", "dp", "fdp")) {
    model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
  }
  model_object <- set_fixed_param(model_object, "j", 0)
  run_object$BioGeoBEARS_model_object <- model_object
  check_BioGeoBEARS_run(run_object)

  if ("skip_optim" %in% names(formals(bears_optim_run))) {
    result <- bears_optim_run(
      BioGeoBEARS_run_object = run_object,
      skip_optim = TRUE,
      skip_optim_option = "return_loglike"
    )
  } else {
    result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  }

  phy <- read.tree(tree_path)
  detections <- read_detections(detections_path, phy = phy)
  controls <- read_controls(controls_path, phy = phy)
  area_names <- colnames(detections)
  states <- rcpp_areas_list_to_states_list(
    areas = area_names,
    maxareas = as.integer(case$max_range_size),
    include_null_range = parse_bool(case$include_null_range)
  )
  tip_likelihoods <- tiplikes_wDetectionModel(
    states_list_0based_index = states,
    phy = phy,
    numareas = length(area_names),
    detects_df = detections,
    controls_df = controls,
    mean_frequency = as.numeric(case$mf),
    dp = as.numeric(case$dp),
    fdp = as.numeric(case$fdp),
    null_range_gets_0_like = TRUE,
    return_LnLs = TRUE,
    relative_LnLs = TRUE,
    exp_LnLs = TRUE,
    error_check = TRUE
  )

  list(
    log_likelihood = extract_loglike(result),
    phy = phy,
    states = states,
    area_names = area_names,
    tip_likelihoods = tip_likelihoods
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
profile_rows <- vector("list", nrow(fixtures))
tip_rows <- vector("list", nrow(fixtures))
for (index in seq_len(nrow(fixtures))) {
  case <- fixtures[index, , drop = FALSE]
  cat("Running BioGeoBEARS detection profile: ", case$case_id, "\n", sep = "")
  result <- run_case(case, repo_root)
  profile_rows[[index]] <- data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", result$log_likelihood),
    d = case$d,
    e = case$e,
    mf = case$mf,
    dp = case$dp,
    fdp = case$fdp,
    stringsAsFactors = FALSE
  )

  num_states <- length(result$states)
  tip_rows[[index]] <- data.frame(
    case_id = rep(case$case_id, each = num_states * length(result$phy$tip.label)),
    tip = rep(rep(result$phy$tip.label, each = num_states), times = 1),
    state_index = rep(seq_len(num_states) - 1L, times = length(result$phy$tip.label)),
    state = rep(
      vapply(result$states, state_label, character(1), area_names = result$area_names),
      times = length(result$phy$tip.label)
    ),
    likelihood = sprintf("%.17g", as.vector(t(result$tip_likelihoods))),
    stringsAsFactors = FALSE
  )
}

dir.create(dirname(profile_output_path), recursive = TRUE, showWarnings = FALSE)
write.table(
  do.call(rbind, profile_rows),
  profile_output_path,
  sep = "\t",
  quote = FALSE,
  row.names = FALSE
)
write.table(
  do.call(rbind, tip_rows),
  tips_output_path,
  sep = "\t",
  quote = FALSE,
  row.names = FALSE
)
cat("Wrote ", profile_output_path, " and ", tips_output_path, "\n", sep = "")
