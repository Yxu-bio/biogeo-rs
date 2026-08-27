args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 11) {
  stop(
    paste(
      "Usage: Rscript validation/biogeobears/benchmark-biogeobears-dec-optimize.R",
      paste(
        "<tree> <ranges> <max_range_size> <include_null_range> <mx01>",
        "<init_d> <init_e> <min_rate> <max_rate> <repeats> <output_tsv>"
      )
    ),
    call. = FALSE
  )
}

tree_path <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
ranges_path <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
max_range_size <- as.integer(args[[3]])
include_null_range_arg <- args[[4]]
mx01_value <- as.numeric(args[[5]])
init_d <- as.numeric(args[[6]])
init_e <- as.numeric(args[[7]])
min_rate <- as.numeric(args[[8]])
max_rate <- as.numeric(args[[9]])
repeats <- as.integer(args[[10]])
output_path <- args[[11]]

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
    value <- as.numeric(result$optim_result$value[[1]])
    if (length(value) == 1 && is.finite(value)) {
      return(value)
    }
  }

  stop("Could not extract a scalar log-likelihood from BioGeoBEARS result", call. = FALSE)
}

extract_param <- function(result, name) {
  if (!is.null(result$outputs)) {
    table <- result$outputs@params_table
    if (name %in% rownames(table)) {
      return(as.numeric(table[name, "est"]))
    }
  }

  if (!is.null(result$optim_result)) {
    optim_result <- result$optim_result
    if (is.data.frame(optim_result)) {
      parameter_columns <- grep("^p[0-9]+$", names(optim_result), value = TRUE)
      index <- match(name, c("d", "e"))
      if (!is.na(index) && length(parameter_columns) >= index) {
        return(as.numeric(optim_result[[parameter_columns[[index]]]][[1]]))
      }
    }
    if (!is.null(optim_result$par)) {
      parameters <- optim_result$par
      if (!is.null(names(parameters)) && name %in% names(parameters)) {
        return(as.numeric(parameters[[name]]))
      }
      index <- match(name, c("d", "e"))
      if (!is.na(index) && length(parameters) >= index) {
        return(as.numeric(parameters[[index]]))
      }
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
  if (!is.null(counts)) {
    for (name in c("function", "fn", "fevals")) {
      if (!is.null(counts[[name]]) && is.finite(as.numeric(counts[[name]]))) {
        return(as.integer(counts[[name]]))
      }
    }
    if (length(counts) >= 1 && is.finite(as.numeric(counts[[1]]))) {
      return(as.integer(counts[[1]]))
    }
  }

  NA_integer_
}

extract_gradient_evaluations <- function(result) {
  value <- extract_optim_field(result, c("gevals", "grcount", "gradient"), NA_real_)
  if (is.finite(as.numeric(value))) {
    return(as.integer(value))
  }

  counts <- result$optim_result$counts
  if (!is.null(counts) && !is.null(counts[["gradient"]]) &&
      is.finite(as.numeric(counts[["gradient"]]))) {
    return(as.integer(counts[["gradient"]]))
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

extract_optimizer <- function(result) {
  optim_result <- result$optim_result
  if (is.data.frame(optim_result) && length(rownames(optim_result)) >= 1) {
    return(rownames(optim_result)[[1]])
  }
  if (is.list(optim_result) && all(c("par", "value", "counts", "convergence") %in% names(optim_result))) {
    return("optim-L-BFGS-B")
  }
  class_name <- class(optim_result)
  if (length(class_name) >= 1) class_name[[1]] else "unknown"
}

run_optimized_once <- function(run_object) {
  result <- NULL
  elapsed <- system.time({
    invisible(capture.output({
      result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
    }))
  })[["elapsed"]]

  list(
    seconds = as.numeric(elapsed),
    lnL = extract_loglike(result),
    d = extract_param(result, "d"),
    e = extract_param(result, "e"),
    evaluations = extract_evaluations(result),
    gradient_evaluations = extract_gradient_evaluations(result),
    iterations = extract_iterations(result),
    converged = extract_converged(result),
    optimizer = extract_optimizer(result)
  )
}

if (is.na(max_range_size) || max_range_size < 1) {
  stop("max_range_size must be a positive integer", call. = FALSE)
}
if (!is.finite(mx01_value) || mx01_value < 0.00001 || mx01_value > 0.99999) {
  stop("mx01 must be finite and between 0.00001 and 0.99999", call. = FALSE)
}
if (!is.finite(min_rate) || !is.finite(max_rate) || min_rate <= 0 || min_rate >= max_rate) {
  stop("rate bounds must be finite, positive, and increasing", call. = FALSE)
}
if (!is.finite(init_d) || !is.finite(init_e) || init_d <= min_rate || init_d >= max_rate ||
    init_e <= min_rate || init_e >= max_rate) {
  stop("init_d/init_e must be finite and strictly inside the rate bounds", call. = FALSE)
}
if (is.na(repeats) || repeats < 1) {
  stop("repeats must be a positive integer", call. = FALSE)
}

include_null_range <- parse_bool(include_null_range_arg)
tmp_dir <- tempfile("bgb-dec-optimize-benchmark-")
dir.create(tmp_dir)
on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)
geog_path <- file.path(tmp_dir, "geog.data")
write_lagrange_geog(ranges_path, geog_path)

run_object <- define_BioGeoBEARS_run()
run_object$trfn <- tree_path
run_object$geogfn <- geog_path
run_object$max_range_size <- max_range_size
run_object$include_null_range <- include_null_range
run_object$min_branchlength <- 0
run_object$print_optim <- FALSE
run_object$num_cores_to_use <- 1
run_object$use_optimx <- FALSE
run_object$return_condlikes_table <- TRUE
run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
run_object$calc_ancprobs <- FALSE
run_object$speedup <- FALSE

run_object <- readfiles_BioGeoBEARS_run(run_object)
run_object$BioGeoBEARS_model_object <- set_free_param(
  run_object$BioGeoBEARS_model_object,
  "d",
  init_d,
  min_rate,
  max_rate
)
run_object$BioGeoBEARS_model_object <- set_free_param(
  run_object$BioGeoBEARS_model_object,
  "e",
  init_e,
  min_rate,
  max_rate
)
run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "j", 0)
for (name in c("mx01y", "mx01s", "mx01v", "mx01j")) {
  run_object$BioGeoBEARS_model_object <- set_fixed_param(
    run_object$BioGeoBEARS_model_object,
    name,
    mx01_value
  )
}
check_BioGeoBEARS_run(run_object)

rows <- list()
for (iteration in seq_len(repeats)) {
  result <- run_optimized_once(run_object)
  rows[[length(rows) + 1]] <- data.frame(
    tool = "biogeobears",
    iteration = iteration,
    seconds = sprintf("%.6f", result$seconds),
    lnL = sprintf("%.15f", result$lnL),
    d = sprintf("%.15g", result$d),
    e = sprintf("%.15g", result$e),
    evaluations = result$evaluations,
    gradient_evaluations = result$gradient_evaluations,
    iterations = result$iterations,
    converged = result$converged,
    optimizer = result$optimizer,
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
