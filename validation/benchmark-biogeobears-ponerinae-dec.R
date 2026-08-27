args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 12) {
  stop(
    paste(
      "Usage: Rscript validation/benchmark-biogeobears-ponerinae-dec.R",
      paste(
        "<tree> <ranges.data> <time_boundaries> <strata.tsv>",
        "<evaluate|optimize> <d_or_init_d> <e_or_init_e>",
        "<min_rate> <max_rate> <mx01> <repeats> <output_tsv>"
      )
    ),
    call. = FALSE
  )
}

tree_path <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
ranges_path <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
time_boundaries_path <- normalizePath(args[[3]], winslash = "/", mustWork = TRUE)
strata_path <- normalizePath(args[[4]], winslash = "/", mustWork = TRUE)
mode <- tolower(args[[5]])
d_value <- as.numeric(args[[6]])
e_value <- as.numeric(args[[7]])
min_rate <- as.numeric(args[[8]])
max_rate <- as.numeric(args[[9]])
mx01_value <- as.numeric(args[[10]])
repeats <- as.integer(args[[11]])
output_path <- args[[12]]

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

set_free_param <- function(model_object, name, init, min_value, max_value) {
  if (!(name %in% rownames(model_object@params_table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }

  table <- model_object@params_table
  table[name, "type"] <- "free"
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
  model_object@params_table <- table
  model_object
}

extract_loglike <- function(result) {
  if (is.numeric(result) && length(result) == 1) {
    return(as.numeric(result))
  }
  for (name in c("total_loglike", "loglike", "LnL")) {
    if (!is.null(result[[name]]) && is.numeric(result[[name]]) &&
        length(result[[name]]) == 1) {
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
    value <- as.numeric(result$optim_result$value[[1]])
    if (length(value) == 1 && is.finite(value)) {
      return(value)
    }
  }
  stop("Could not extract a scalar log-likelihood", call. = FALSE)
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

extract_optim_field <- function(result, candidates, default = NA) {
  optim_result <- result$optim_result
  if (is.null(optim_result)) {
    return(default)
  }
  if (is.data.frame(optim_result)) {
    for (name in candidates) {
      if (name %in% names(optim_result)) {
        return(optim_result[[name]][[1]])
      }
    }
  }
  for (name in candidates) {
    value <- optim_result[[name]]
    if (!is.null(value) && length(value) >= 1) {
      return(value[[1]])
    }
  }
  default
}

extract_evaluations <- function(result) {
  value <- extract_optim_field(result, c("fevals", "fncount", "function"), NA_real_)
  if (is.finite(as.numeric(value))) {
    return(as.integer(value))
  }
  counts <- result$optim_result$counts
  if (!is.null(counts) && length(counts) >= 1 && is.finite(as.numeric(counts[[1]]))) {
    return(as.integer(counts[[1]]))
  }
  NA_integer_
}

extract_iterations <- function(result) {
  value <- extract_optim_field(result, c("niter", "iterations", "iter"), NA_real_)
  if (is.finite(as.numeric(value))) as.integer(value) else NA_integer_
}

extract_converged <- function(result) {
  value <- extract_optim_field(result, c("convcode", "convergence"), NA_real_)
  if (is.finite(as.numeric(value))) as.integer(value) == 0 else NA
}

read_allowed_states <- function(path, expected_areas) {
  table <- read.delim(
    path,
    comment.char = "#",
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
  if (ncol(table) != length(expected_areas) + 1 || names(table)[[1]] != "range") {
    stop("Invalid allowed range table shape: ", path, call. = FALSE)
  }
  actual_areas <- names(table)[-1]
  if (!identical(actual_areas, expected_areas)) {
    stop(
      "Allowed range area order mismatch in ", path,
      ": expected ", paste(expected_areas, collapse = ","),
      ", got ", paste(actual_areas, collapse = ","),
      call. = FALSE
    )
  }

  bits <- as.matrix(table[, -1, drop = FALSE])
  storage.mode(bits) <- "integer"
  if (any(is.na(bits)) || any(!(bits %in% c(0L, 1L)))) {
    stop("Allowed range table must contain only 0/1 values: ", path, call. = FALSE)
  }

  lapply(seq_len(nrow(bits)), function(index) {
    occupied <- which(bits[index, ] == 1L) - 1L
    if (length(occupied) == 0) as.integer(NA) else as.integer(occupied)
  })
}

read_stratified_states <- function(strata_path, time_boundaries_path, expected_areas) {
  strata <- read.delim(
    strata_path,
    comment.char = "#",
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
  required <- c("oldest_age", "allowed_ranges")
  if (!all(required %in% names(strata))) {
    stop("Strata table must contain oldest_age and allowed_ranges columns", call. = FALSE)
  }

  boundaries <- scan(time_boundaries_path, what = numeric(), quiet = TRUE)
  if (nrow(strata) != length(boundaries) ||
      any(abs(as.numeric(strata$oldest_age) - boundaries) > 1e-10)) {
    stop("Strata oldest_age values do not match the BioGeoBEARS time boundaries", call. = FALSE)
  }

  base_dir <- dirname(strata_path)
  states <- lapply(strata$allowed_ranges, function(raw_path) {
    if (is.na(raw_path) || raw_path %in% c("", "-", "none")) {
      stop("Every Ponerinae stratum must provide allowed_ranges", call. = FALSE)
    }
    is_absolute <- grepl("^[A-Za-z]:[/\\\\]", raw_path) ||
      startsWith(raw_path, "/") || startsWith(raw_path, "\\")
    path <- if (is_absolute) {
      raw_path
    } else {
      file.path(base_dir, raw_path)
    }
    read_allowed_states(normalizePath(path, winslash = "/", mustWork = TRUE), expected_areas)
  })

  list(boundaries = boundaries, states = states)
}

if (!(mode %in% c("evaluate", "optimize"))) {
  stop("mode must be evaluate or optimize", call. = FALSE)
}
if (!is.finite(d_value) || !is.finite(e_value) || d_value <= 0 || e_value <= 0) {
  stop("d/e values must be finite and positive", call. = FALSE)
}
if (!is.finite(min_rate) || !is.finite(max_rate) || min_rate <= 0 || min_rate >= max_rate) {
  stop("rate bounds must be finite, positive, and increasing", call. = FALSE)
}
if (mode == "optimize" &&
    (d_value <= min_rate || d_value >= max_rate || e_value <= min_rate || e_value >= max_rate)) {
  stop("initial d/e must be strictly inside the optimization bounds", call. = FALSE)
}
if (!is.finite(mx01_value) || mx01_value < 0.00001 || mx01_value > 0.99999) {
  stop("mx01 must be finite and between 0.00001 and 0.99999", call. = FALSE)
}
if (is.na(repeats) || repeats < 1) {
  stop("repeats must be a positive integer", call. = FALSE)
}

tipranges <- getranges_from_LagrangePHYLIP(lgdata_fn = ranges_path)
areas <- getareas_from_tipranges_object(tipranges)
stratified <- read_stratified_states(strata_path, time_boundaries_path, areas)
state_counts <- vapply(stratified$states, length, integer(1))
master_state_count <- sum(choose(length(areas), 0:5))

run_object <- define_BioGeoBEARS_run()
run_object$trfn <- tree_path
run_object$geogfn <- ranges_path
run_object$max_range_size <- 5
run_object$include_null_range <- TRUE
run_object$min_branchlength <- 0
run_object$timesfn <- time_boundaries_path
run_object$print_optim <- FALSE
run_object$num_cores_to_use <- 1
run_object$on_NaN_error <- -1e50
run_object$use_optimx <- mode == "optimize"
run_object$return_condlikes_table <- TRUE
run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
run_object$calc_ancprobs <- FALSE
run_object$speedup <- mode == "optimize"

run_object <- readfiles_BioGeoBEARS_run(run_object)
run_object <- section_the_tree(
  inputs = run_object,
  make_master_table = TRUE,
  plot_pieces = FALSE,
  fossils_older_than = 0.001,
  cut_fossils = FALSE
)
run_object$lists_of_states_lists_0based <- stratified$states

model <- run_object$BioGeoBEARS_model_object
if (mode == "evaluate") {
  model <- set_fixed_param(model, "d", d_value)
  model <- set_fixed_param(model, "e", e_value)
} else {
  model <- set_free_param(model, "d", d_value, min_rate, max_rate)
  model <- set_free_param(model, "e", e_value, min_rate, max_rate)
}
model <- set_fixed_param(model, "j", 0)
for (name in c("mx01y", "mx01s", "mx01v", "mx01j")) {
  model <- set_fixed_param(model, name, mx01_value)
}
run_object$BioGeoBEARS_model_object <- model
run_object <- fix_BioGeoBEARS_params_minmax(BioGeoBEARS_run_object = run_object)
check_BioGeoBEARS_run(run_object)

run_once <- function() {
  result <- NULL
  elapsed <- system.time({
    invisible(capture.output({
      if (mode == "evaluate") {
        result <- bears_optim_run(
          BioGeoBEARS_run_object = run_object,
          skip_optim = TRUE,
          skip_optim_option = "return_loglike"
        )
      } else {
        result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
      }
    }))
  })[["elapsed"]]

  list(
    seconds = as.numeric(elapsed),
    lnL = extract_loglike(result),
    d = if (mode == "evaluate") d_value else extract_param(result, "d"),
    e = if (mode == "evaluate") e_value else extract_param(result, "e"),
    evaluations = if (mode == "evaluate") 1L else extract_evaluations(result),
    iterations = if (mode == "evaluate") 0L else extract_iterations(result),
    converged = if (mode == "evaluate") TRUE else extract_converged(result)
  )
}

rows <- lapply(seq_len(repeats), function(iteration) {
  result <- run_once()
  data.frame(
    tool = "biogeobears",
    mode = mode,
    iteration = iteration,
    seconds = sprintf("%.6f", result$seconds),
    lnL = sprintf("%.15f", result$lnL),
    d = sprintf("%.15g", result$d),
    e = sprintf("%.15g", result$e),
    states = as.integer(master_state_count),
    strata = length(stratified$states),
    allowed_range_counts = paste(state_counts, collapse = ","),
    evaluations = result$evaluations,
    iterations = result$iterations,
    converged = result$converged,
    optimizer = if (mode == "evaluate") "none" else "optimx-bobyqa",
    stringsAsFactors = FALSE
  )
})

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
