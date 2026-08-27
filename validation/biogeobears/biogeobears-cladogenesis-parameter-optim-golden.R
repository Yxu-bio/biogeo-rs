args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else {
  "validation/cladogenesis_parameter_optimization_fixtures.tsv"
}
output_path <- if (length(args) >= 2) args[[2]] else {
  "validation/golden/biogeobears-cladogenesis-parameter-optim.tsv"
}
profile_output_path <- if (length(args) >= 3) args[[3]] else {
  "validation/golden/biogeobears-cladogenesis-parameter-profile.tsv"
}

source("validation/biogeobears/r-env.R")
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

supported_parameters <- c("y", "s", "v", "mx01", "mx01y", "mx01s", "mx01v", "mx01j")
event_weights <- c("y", "s", "v")
range_size_parameters <- c("mx01y", "mx01s", "mx01v", "mx01j")
maxent_parameters <- c("mx01", range_size_parameters)

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

parse_number_list <- function(value, field, case_id) {
  values <- as.numeric(strsplit(as.character(value), ",", fixed = TRUE)[[1]])
  if (length(values) == 0 || any(!is.finite(values))) {
    stop("Invalid ", field, " for ", case_id, call. = FALSE)
  }
  values
}

expand_values <- function(values, offsets, min_value, max_value) {
  expanded <- as.vector(outer(values, offsets, "+"))
  expanded <- expanded[expanded >= min_value & expanded <= max_value]
  sort(unique(expanded))
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

require_parameter <- function(model_object, name) {
  if (!(name %in% rownames(model_object@params_table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
}

set_fixed_param <- function(model_object, name, value) {
  require_parameter(model_object, name)
  table <- model_object@params_table
  table[name, "type"] <- "fixed"
  for (column in c("init", "est", "min", "max")) {
    if (column %in% colnames(table)) {
      table[name, column] <- value
    }
  }
  model_object@params_table <- table
  model_object
}

set_free_param <- function(model_object, name, init, min_value, max_value) {
  require_parameter(model_object, name)
  table <- model_object@params_table
  table[name, "type"] <- "free"
  for (column in c("init", "est")) {
    if (column %in% colnames(table)) {
      table[name, column] <- init
    }
  }
  table[name, "min"] <- min_value
  table[name, "max"] <- max_value
  model_object@params_table <- table
  model_object
}

set_linked_mx01_param <- function(model_object, name, value) {
  require_parameter(model_object, name)
  table <- model_object@params_table
  table[name, "type"] <- "mx01"
  for (column in c("init", "est")) {
    if (column %in% colnames(table)) {
      table[name, column] <- value
    }
  }
  model_object@params_table <- table
  model_object
}

configure_model <- function(model_object, case, target_value, target_is_free) {
  free_parameter <- as.character(case$free_parameter)
  for (name in c("d", "e", "j", event_weights)) {
    model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
  }

  if (free_parameter == "mx01") {
    if (target_is_free) {
      model_object <- set_free_param(
        model_object,
        "mx01",
        target_value,
        as.numeric(case$min),
        as.numeric(case$max)
      )
    } else {
      model_object <- set_fixed_param(model_object, "mx01", target_value)
    }
    for (name in range_size_parameters) {
      model_object <- set_linked_mx01_param(model_object, name, target_value)
    }
  } else {
    model_object <- set_fixed_param(model_object, "mx01", as.numeric(case$mx01))
    for (name in range_size_parameters) {
      model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
    }
  }

  if (free_parameter %in% c(event_weights, range_size_parameters)) {
    if (target_is_free) {
      model_object <- set_free_param(
        model_object,
        free_parameter,
        target_value,
        as.numeric(case$min),
        as.numeric(case$max)
      )
    } else {
      model_object <- set_fixed_param(model_object, free_parameter, target_value)
    }
  }

  calc_linked_params_BioGeoBEARS_model_object(model_object)
}

build_run <- function(case, repo_root, geog_path, target_value, target_is_free) {
  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
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
  run_object$BioGeoBEARS_model_object <- configure_model(
    run_object$BioGeoBEARS_model_object,
    case,
    target_value,
    target_is_free
  )
  check_BioGeoBEARS_run(run_object)
  run_object
}

extract_loglike <- function(result) {
  if (is.numeric(result) && length(result) == 1) {
    return(as.numeric(result))
  }
  for (name in c("total_loglike", "loglike", "LnL")) {
    if (!is.null(result[[name]]) && is.numeric(result[[name]]) && length(result[[name]]) == 1) {
      return(as.numeric(result[[name]]))
    }
  }
  if (!is.null(result$outputs)) {
    for (name in c("total_loglike", "loglike", "LnL")) {
      value <- tryCatch(slot(result$outputs, name), error = function(e) NULL)
      if (!is.null(value) && is.numeric(value) && length(value) == 1) {
        return(as.numeric(value))
      }
    }
  }
  if (!is.null(result$optim_result) && !is.null(result$optim_result$value)) {
    value <- as.numeric(result$optim_result$value)
    if (length(value) == 1 && is.finite(value)) {
      return(value)
    }
  }
  stop("Could not extract a scalar BioGeoBEARS log-likelihood", call. = FALSE)
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
    if (length(parameters) == 1) {
      return(as.numeric(parameters[[1]]))
    }
  }
  stop("Could not extract BioGeoBEARS parameter: ", name, call. = FALSE)
}

extract_convergence <- function(result) {
  if (is.null(result$optim_result) || is.null(result$optim_result$convergence)) {
    return(NA_integer_)
  }
  as.integer(result$optim_result$convergence[[1]])
}

extract_message <- function(result) {
  if (is.null(result$optim_result) || is.null(result$optim_result$message)) {
    return("")
  }
  gsub("[\t\r\n]+", " ", as.character(result$optim_result$message[[1]]))
}

run_optimization <- function(case, repo_root, geog_path, start) {
  run_object <- NULL
  invisible(capture.output(
    run_object <- build_run(case, repo_root, geog_path, start, TRUE)
  ))
  result <- NULL
  invisible(capture.output(
    result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  ))
  list(
    start = start,
    lnL = extract_loglike(result),
    estimate = extract_param(result, as.character(case$free_parameter)),
    convergence = extract_convergence(result),
    message = extract_message(result)
  )
}

run_profile_point <- function(case, repo_root, geog_path, value) {
  run_object <- NULL
  invisible(capture.output(
    run_object <- build_run(case, repo_root, geog_path, value, FALSE)
  ))
  formals_names <- names(formals(bears_optim_run))
  if (!("skip_optim" %in% formals_names)) {
    stop("Installed BioGeoBEARS lacks skip_optim support", call. = FALSE)
  }
  result <- NULL
  invisible(capture.output(
    result <- bears_optim_run(
      BioGeoBEARS_run_object = run_object,
      skip_optim = TRUE,
      skip_optim_option = "return_loglike"
    )
  ))
  extract_loglike(result)
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
if (nrow(fixtures) == 0) {
  stop("Cladogenesis parameter optimization manifest is empty", call. = FALSE)
}
if (any(!(fixtures$free_parameter %in% supported_parameters))) {
  stop("Manifest contains an unsupported free_parameter", call. = FALSE)
}
if (any(tolower(fixtures$root_prior) != "flat")) {
  stop("Only the BioGeoBEARS flat-root convention is supported", call. = FALSE)
}

optimization_rows <- list()
profile_rows <- list()
for (index in seq_len(nrow(fixtures))) {
  case <- fixtures[index, , drop = FALSE]
  case_id <- as.character(case$case_id)
  free_parameter <- as.character(case$free_parameter)
  cat("Running BioGeoBEARS cladogenesis optimization: ", case_id, "\n", sep = "")

  tmp_dir <- tempfile(paste0("bgb-clado-", free_parameter, "-"))
  dir.create(tmp_dir)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  starts <- unique(parse_number_list(case$starts, "starts", case_id))
  profile_values <- unique(parse_number_list(case$profile_values, "profile_values", case_id))
  if (free_parameter %in% maxent_parameters) {
    starts <- expand_values(
      starts,
      c(-0.00025, 0, 0.00025),
      as.numeric(case$min),
      as.numeric(case$max)
    )
    profile_values <- expand_values(
      profile_values,
      c(-0.0005, -0.00025, 0, 0.00025, 0.0005),
      as.numeric(case$min),
      as.numeric(case$max)
    )
  }
  runs <- lapply(starts, function(start) {
    run_optimization(case, repo_root, geog_path, start)
  })
  converged <- vapply(runs, function(run) {
    is.finite(run$lnL) && identical(run$convergence, 0L)
  }, logical(1))
  if (!any(converged)) {
    stop("No converged BioGeoBEARS start for ", case_id, call. = FALSE)
  }
  candidates <- runs[converged]
  optimizer_best <- candidates[[which.max(vapply(candidates, function(run) run$lnL, numeric(1)))]]

  profile_loglikes <- numeric(length(profile_values))
  for (profile_index in seq_along(profile_values)) {
    value <- profile_values[[profile_index]]
    lnL <- run_profile_point(case, repo_root, geog_path, value)
    profile_loglikes[[profile_index]] <- lnL
    profile_rows[[length(profile_rows) + 1]] <- data.frame(
      case_id = case_id,
      free_parameter = free_parameter,
      value = sprintf("%.15g", value),
      biogeobears_lnL = sprintf("%.15f", lnL),
      stringsAsFactors = FALSE
    )
  }
  best_profile_index <- which.max(profile_loglikes)
  profile_best_lnL <- profile_loglikes[[best_profile_index]]
  profile_best_estimate <- profile_values[[best_profile_index]]
  if (profile_best_lnL > optimizer_best$lnL) {
    selected_lnL <- profile_best_lnL
    selected_estimate <- profile_best_estimate
    candidate_source <- "profile_grid"
  } else {
    selected_lnL <- optimizer_best$lnL
    selected_estimate <- optimizer_best$estimate
    candidate_source <- "optimizer"
  }

  optimization_rows[[length(optimization_rows) + 1]] <- data.frame(
    case_id = case_id,
    free_parameter = free_parameter,
    biogeobears_lnL = sprintf("%.15f", selected_lnL),
    biogeobears_estimate = sprintf("%.15g", selected_estimate),
    candidate_source = candidate_source,
    optimizer_lnL = sprintf("%.15f", optimizer_best$lnL),
    optimizer_estimate = sprintf("%.15g", optimizer_best$estimate),
    profile_best_lnL = sprintf("%.15f", profile_best_lnL),
    profile_best_estimate = sprintf("%.15g", profile_best_estimate),
    optimizer_gap = sprintf("%.15g", selected_lnL - optimizer_best$lnL),
    selected_start = sprintf("%.15g", optimizer_best$start),
    starts = paste(sprintf("%.15g", starts), collapse = ","),
    converged_starts = sum(converged),
    convergence = optimizer_best$convergence,
    message = optimizer_best$message,
    min = as.numeric(case$min),
    max = as.numeric(case$max),
    d = as.numeric(case$d),
    e = as.numeric(case$e),
    j = as.numeric(case$j),
    y = as.numeric(case$y),
    s = as.numeric(case$s),
    v = as.numeric(case$v),
    mx01 = as.numeric(case$mx01),
    mx01y = as.numeric(case$mx01y),
    mx01s = as.numeric(case$mx01s),
    mx01v = as.numeric(case$mx01v),
    mx01j = as.numeric(case$mx01j),
    max_range_size = as.integer(case$max_range_size),
    include_null_range = parse_bool(case$include_null_range),
    stringsAsFactors = FALSE
  )

  unlink(tmp_dir, recursive = TRUE)
}

optimization_output <- do.call(rbind, optimization_rows)
profile_output <- do.call(rbind, profile_rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
dir.create(dirname(profile_output_path), recursive = TRUE, showWarnings = FALSE)
write.table(optimization_output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
write.table(profile_output, file = profile_output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
cat("Wrote ", profile_output_path, "\n", sep = "")
