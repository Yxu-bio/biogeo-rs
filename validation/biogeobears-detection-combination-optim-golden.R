args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else {
  "validation/detection_combination_optimization_fixtures.tsv"
}
output_path <- if (length(args) >= 2) args[[2]] else {
  "validation/golden/biogeobears-detection-combination-optim.tsv"
}

source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
env <- configure_project_r()

required_packages <- c("ape", "rexpokit", "cladoRcpp", "BioGeoBEARS", "optimx")
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

supported_parameters <- c(
  "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
  "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
)
parameter_bounds <- list(
  d = c(1e-12, 4.999999999999),
  e = c(1e-12, 4.999999999999),
  a = c(1e-12, 4.999999999999),
  b = c(1e-12, 0.999999999999),
  x = c(-2.5, 2.5),
  n = c(-10, 10),
  w = c(-10, 10),
  u = c(-10, 10),
  j = c(0.00001, 2.99999),
  y = c(0.00001, 1),
  s = c(0.00001, 1),
  v = c(0.00001, 1),
  mx01 = c(0.0001, 0.9999),
  mx01j = c(0.0001, 0.9999),
  mx01y = c(0.0001, 0.9999),
  mx01s = c(0.0001, 0.9999),
  mx01v = c(0.0001, 0.9999),
  mf = c(0.005, 0.995),
  dp = c(0.005, 0.995),
  fdp = c(0.005, 0.995)
)

parse_bool <- function(value) {
  tolower(as.character(value)) %in% c("true", "t", "1", "yes")
}

parse_free_parameters <- function(value) {
  parameters <- unique(trimws(strsplit(as.character(value), ",", fixed = TRUE)[[1]]))
  if (length(parameters) == 0 || any(!(parameters %in% supported_parameters))) {
    stop("Unsupported free parameter list: ", value, call. = FALSE)
  }
  parameters
}

parse_additional_starts <- function(value, free_parameters) {
  raw_value <- as.character(value)
  if (length(raw_value) == 0 || is.na(raw_value) || !nzchar(raw_value) || raw_value == "-") {
    return(list())
  }
  raw_starts <- strsplit(raw_value, ";", fixed = TRUE)[[1]]
  lapply(seq_along(raw_starts), function(index) {
    values <- suppressWarnings(as.numeric(
      strsplit(raw_starts[[index]], ",", fixed = TRUE)[[1]]
    ))
    if (length(values) != length(free_parameters) || any(!is.finite(values))) {
      stop(
        "Additional start ", index, " must contain one finite value per free parameter",
        call. = FALSE
      )
    }
    for (parameter_index in seq_along(free_parameters)) {
      name <- free_parameters[[parameter_index]]
      bounds <- parameter_bounds[[name]]
      if (values[[parameter_index]] < bounds[[1]] || values[[parameter_index]] > bounds[[2]]) {
        stop("Additional start is outside the bounds for ", name, call. = FALSE)
      }
    }
    setNames(values, free_parameters)
  })
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

set_free_param <- function(model_object, name, init) {
  bounds <- parameter_bounds[[name]]
  table <- model_object@params_table
  table[name, "type"] <- "free"
  table[name, "init"] <- init
  table[name, "est"] <- init
  table[name, "min"] <- bounds[[1]]
  table[name, "max"] <- bounds[[2]]
  model_object@params_table <- table
  model_object
}

extract_loglike <- function(result) {
  if (is.numeric(result) && length(result) == 1 && is.finite(result)) {
    return(as.numeric(result))
  }
  if (is.data.frame(result$optim_result) && "value" %in% names(result$optim_result)) {
    value <- as.numeric(result$optim_result$value[[1]])
    if (length(value) == 1 && is.finite(value)) {
      return(value)
    }
  }
  if (!is.null(result$optim_result) && !is.null(result$optim_result$value)) {
    value <- as.numeric(result$optim_result$value[[1]])
    if (length(value) == 1 && is.finite(value)) {
      return(value)
    }
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

extract_convergence <- function(result) {
  if (is.data.frame(result$optim_result) && "convcode" %in% names(result$optim_result)) {
    return(as.integer(result$optim_result$convcode[[1]]))
  }
  if (!is.null(result$optim_result$convergence)) {
    return(as.integer(result$optim_result$convergence[[1]]))
  }
  NA_integer_
}

evaluate_fixed_point <- function(run_object, free, values) {
  fixed_run <- run_object
  fixed_model <- fixed_run$BioGeoBEARS_model_object
  for (name in free) {
    fixed_model <- set_fixed_param(fixed_model, name, values[[name]])
  }
  fixed_run$BioGeoBEARS_model_object <- calc_linked_params_BioGeoBEARS_model_object(
    fixed_model
  )
  check_BioGeoBEARS_run(fixed_run)
  if (!("skip_optim" %in% names(formals(bears_optim_run)))) {
    stop("Installed BioGeoBEARS lacks skip_optim support", call. = FALSE)
  }
  result <- bears_optim_run(
    BioGeoBEARS_run_object = fixed_run,
    skip_optim = TRUE,
    skip_optim_option = "return_loglike"
  )
  extract_loglike(result)
}

extract_free_estimates <- function(result, free) {
  if (is.null(result$outputs)) {
    stop("BioGeoBEARS optimization result has no outputs object", call. = FALSE)
  }
  table <- result$outputs@params_table
  if (any(!(free %in% rownames(table)))) {
    stop("BioGeoBEARS optimization result is missing free parameters", call. = FALSE)
  }
  values <- as.numeric(table[free, "est"])
  names(values) <- free
  if (any(!is.finite(values))) {
    stop("BioGeoBEARS optimization returned non-finite parameters", call. = FALSE)
  }
  values
}

run_case <- function(case, repo_root) {
  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  detections_path <- normalizePath(file.path(repo_root, case$detections), winslash = "/", mustWork = TRUE)
  controls_path <- normalizePath(file.path(repo_root, case$controls), winslash = "/", mustWork = TRUE)
  phy <- read.tree(tree_path)
  area_names <- colnames(read_detections(detections_path, phy = phy))
  free <- parse_free_parameters(case$free_parameters)

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
  run_object$use_optimx <- TRUE
  run_object$rescale_params <- FALSE
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- FALSE
  run_object$speedup <- FALSE
  run_object <- readfiles_BioGeoBEARS_run(run_object)
  run_object <- apply_fixture_dispersal_multipliers(
    run_object,
    case,
    repo_root,
    free_parameter = intersect(free, c("x", "n", "u")),
    area_names = area_names
  )

  model_object <- run_object$BioGeoBEARS_model_object
  for (name in supported_parameters) {
    value <- as.numeric(case[[name]])
    if (name %in% free) {
      model_object <- set_free_param(model_object, name, value)
    } else {
      model_object <- set_fixed_param(model_object, name, value)
    }
  }
  run_object$BioGeoBEARS_model_object <- calc_linked_params_BioGeoBEARS_model_object(model_object)

  starts <- c(
    list(setNames(vapply(free, function(name) as.numeric(case[[name]]), numeric(1)), free)),
    parse_additional_starts(case$additional_starts, free)
  )
  best_candidate <- NULL
  best_loglike <- -Inf
  converged_starts <- 0L
  nonworsening_starts <- 0L
  failed_starts <- 0L
  for (start_index in seq_along(starts)) {
    candidate_run_object <- run_object
    candidate_model <- candidate_run_object$BioGeoBEARS_model_object
    for (name in free) {
      candidate_model <- set_free_param(candidate_model, name, starts[[start_index]][[name]])
    }
    candidate_run_object$BioGeoBEARS_model_object <- calc_linked_params_BioGeoBEARS_model_object(
      candidate_model
    )
    check_BioGeoBEARS_run(candidate_run_object)
    start_loglike <- evaluate_fixed_point(run_object, free, starts[[start_index]])
    candidate_error <- NULL
    candidate_result <- tryCatch(
      bears_optim_run(BioGeoBEARS_run_object = candidate_run_object),
      error = function(error) {
        candidate_error <<- conditionMessage(error)
        NULL
      }
    )
    if (is.null(candidate_result)) {
      failed_starts <- failed_starts + 1L
      cat(
        "Candidate start ", start_index, "/", length(starts),
        ": start_lnL=", format(start_loglike, digits = 15),
        ", failed=", gsub("[\t\r\n]+", " ", candidate_error), "\n",
        sep = ""
      )
      next
    }
    candidate_reported_loglike <- tryCatch(
      extract_loglike(candidate_result),
      error = function(error) NA_real_
    )
    candidate_convergence <- extract_convergence(candidate_result)
    candidate_estimates <- tryCatch(
      extract_free_estimates(candidate_result, free),
      error = function(error) NULL
    )
    candidate_replayed_loglike <- if (is.null(candidate_estimates)) {
      NA_real_
    } else {
      tryCatch(
        evaluate_fixed_point(run_object, free, candidate_estimates),
        error = function(error) NA_real_
      )
    }
    cat(
      "Candidate start ", start_index, "/", length(starts),
      ": start_lnL=", format(start_loglike, digits = 15),
      ", reported_lnL=", format(candidate_reported_loglike, digits = 15),
      ", replayed_lnL=", format(candidate_replayed_loglike, digits = 15),
      ", convergence=", candidate_convergence, "\n",
      sep = ""
    )
    if (!is.na(candidate_convergence)
        && candidate_convergence == 0L
        && is.finite(candidate_reported_loglike)
        && is.finite(candidate_replayed_loglike)) {
      converged_starts <- converged_starts + 1L
      if (candidate_replayed_loglike + 1e-8 < start_loglike) {
        cat("Rejecting converged candidate because it worsened its fixed start point.\n")
        next
      }
      nonworsening_starts <- nonworsening_starts + 1L
      if (is.null(best_candidate) || candidate_replayed_loglike > best_loglike) {
        best_candidate <- list(
          result = candidate_result,
          estimates = candidate_estimates,
          replayed_loglike = candidate_replayed_loglike,
          reported_loglike = candidate_reported_loglike,
          start_loglike = start_loglike,
          start_index = start_index
        )
        best_loglike <- candidate_replayed_loglike
      }
    }
  }
  if (is.null(best_candidate)) {
    stop(
      "No non-worsening BioGeoBEARS optimization start converged for ",
      case$case_id,
      call. = FALSE
    )
  }

  result <- best_candidate$result
  estimates <- best_candidate$estimates
  optimizer_name <- if (is.data.frame(result$optim_result)) {
    rownames(result$optim_result)[[1]]
  } else {
    "optim"
  }
  row <- data.frame(
    case_id = case$case_id,
    free_parameters = case$free_parameters,
    biogeobears_lnL = sprintf("%.15f", best_candidate$replayed_loglike),
    convergence = extract_convergence(result),
    optimizer = optimizer_name,
    starts_evaluated = length(starts),
    converged_starts = converged_starts,
    nonworsening_starts = nonworsening_starts,
    failed_starts = failed_starts,
    selected_start = best_candidate$start_index,
    selected_start_lnL = sprintf("%.15f", best_candidate$start_loglike),
    optimizer_reported_lnL = sprintf("%.15f", best_candidate$reported_loglike),
    optimizer_replay_delta = sprintf(
      "%.15g",
      best_candidate$replayed_loglike - best_candidate$reported_loglike
    ),
    optimizer_improvement = sprintf(
      "%.15g",
      best_candidate$replayed_loglike - best_candidate$start_loglike
    ),
    stringsAsFactors = FALSE
  )
  for (name in free) {
    row[[paste0("biogeobears_", name)]] <- sprintf("%.15g", estimates[[name]])
  }
  row
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
rows <- vector("list", nrow(fixtures))
for (index in seq_len(nrow(fixtures))) {
  cat("Running BioGeoBEARS joint-module detection optimization: ", fixtures$case_id[[index]], "\n", sep = "")
  rows[[index]] <- run_case(fixtures[index, , drop = FALSE], repo_root)
}

bind_rows_fill <- function(rows) {
  columns <- unique(unlist(lapply(rows, names), use.names = FALSE))
  normalized <- lapply(rows, function(row) {
    for (column in setdiff(columns, names(row))) {
      row[[column]] <- NA
    }
    row[columns]
  })
  do.call(rbind, normalized)
}

output <- bind_rows_fill(rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, output_path, sep = "\t", quote = FALSE, row.names = FALSE, na = "")
cat("Wrote ", output_path, "\n", sep = "")
