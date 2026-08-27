args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/detection_optimization_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-detection-optim.tsv"

source("validation/biogeobears/r-env.R")
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

parse_bool <- function(value) {
  tolower(as.character(value)) %in% c("true", "t", "1", "yes")
}

free_parameter_names <- function(value) {
  names <- trimws(strsplit(as.character(value), ",", fixed = TRUE)[[1]])
  if (any(!(names %in% c("mf", "dp", "fdp")))) {
    stop("Unsupported detection free parameter: ", value, call. = FALSE)
  }
  unique(names)
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

set_free_param <- function(model_object, name, init, min_value, max_value) {
  table <- model_object@params_table
  table[name, "type"] <- "free"
  table[name, "init"] <- init
  table[name, "est"] <- init
  table[name, "min"] <- min_value
  table[name, "max"] <- max_value
  model_object@params_table <- table
  model_object
}

extract_loglike <- function(result) {
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
    if (is.finite(value)) {
      return(value)
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

run_case <- function(case, repo_root) {
  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  detections_path <- normalizePath(file.path(repo_root, case$detections), winslash = "/", mustWork = TRUE)
  controls_path <- normalizePath(file.path(repo_root, case$controls), winslash = "/", mustWork = TRUE)
  free <- free_parameter_names(case$free_parameters)

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

  model_object <- run_object$BioGeoBEARS_model_object
  model_object <- set_fixed_param(model_object, "d", as.numeric(case$d))
  model_object <- set_fixed_param(model_object, "e", as.numeric(case$e))
  model_object <- set_fixed_param(model_object, "j", 0)
  for (name in c("mf", "dp", "fdp")) {
    if (name %in% free) {
      model_object <- set_free_param(
        model_object,
        name,
        as.numeric(case[[paste0("init_", name)]]),
        as.numeric(case$min_probability),
        as.numeric(case$max_probability)
      )
    } else {
      model_object <- set_fixed_param(
        model_object,
        name,
        as.numeric(case[[paste0("fixed_", name)]])
      )
    }
  }
  run_object$BioGeoBEARS_model_object <- model_object
  check_BioGeoBEARS_run(run_object)

  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  estimates <- result$outputs@params_table
  optimizer_name <- if (is.data.frame(result$optim_result)) {
    rownames(result$optim_result)[[1]]
  } else {
    "optim"
  }
  data.frame(
    case_id = case$case_id,
    free_parameters = case$free_parameters,
    biogeobears_lnL = sprintf("%.15f", extract_loglike(result)),
    biogeobears_mf = sprintf("%.15g", estimates["mf", "est"]),
    biogeobears_dp = sprintf("%.15g", estimates["dp", "est"]),
    biogeobears_fdp = sprintf("%.15g", estimates["fdp", "est"]),
    convergence = extract_convergence(result),
    optimizer = optimizer_name,
    stringsAsFactors = FALSE
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
rows <- vector("list", nrow(fixtures))
for (index in seq_len(nrow(fixtures))) {
  cat("Running BioGeoBEARS detection optimization: ", fixtures$case_id[[index]], "\n", sep = "")
  rows[[index]] <- run_case(fixtures[index, , drop = FALSE], repo_root)
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
