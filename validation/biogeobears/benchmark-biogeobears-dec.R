args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 9) {
  stop(
    paste(
      "Usage: Rscript validation/biogeobears/benchmark-biogeobears-dec.R",
      paste(
        "<tree> <ranges> <d> <e> <max_range_size> <include_null_range>",
        "<mx01> <repeats> <output_tsv>"
      )
    ),
    call. = FALSE
  )
}

tree_path <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
ranges_path <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
d_value <- as.numeric(args[[3]])
e_value <- as.numeric(args[[4]])
max_range_size <- as.integer(args[[5]])
include_null_range_arg <- args[[6]]
mx01_value <- as.numeric(args[[7]])
repeats <- as.integer(args[[8]])
output_path <- args[[9]]

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

  stop("Could not extract a scalar log-likelihood from BioGeoBEARS result", call. = FALSE)
}

run_fixed_once <- function(run_object) {
  result <- NULL
  elapsed <- system.time({
    invisible(capture.output({
      result <- bears_optim_run(
        BioGeoBEARS_run_object = run_object,
        skip_optim = TRUE,
        skip_optim_option = "return_loglike"
      )
    }))
  })[["elapsed"]]

  list(seconds = as.numeric(elapsed), lnL = extract_loglike(result))
}

if (!is.finite(d_value) || !is.finite(e_value) || d_value < 0 || e_value < 0) {
  stop("d/e must be finite non-negative rates", call. = FALSE)
}
if (is.na(max_range_size) || max_range_size < 1) {
  stop("max_range_size must be a positive integer", call. = FALSE)
}
if (!is.finite(mx01_value) || mx01_value < 0.00001 || mx01_value > 0.99999) {
  stop("mx01 must be finite and between 0.00001 and 0.99999", call. = FALSE)
}
if (is.na(repeats) || repeats < 1) {
  stop("repeats must be a positive integer", call. = FALSE)
}

include_null_range <- parse_bool(include_null_range_arg)
tmp_dir <- tempfile("bgb-dec-benchmark-")
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
run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "d", d_value)
run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "e", e_value)
run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "j", 0)
for (name in c("mx01y", "mx01s", "mx01v", "mx01j")) {
  run_object$BioGeoBEARS_model_object <- set_fixed_param(
    run_object$BioGeoBEARS_model_object,
    name,
    mx01_value
  )
}
check_BioGeoBEARS_run(run_object)

invisible(run_fixed_once(run_object))

rows <- list()
for (iteration in seq_len(repeats)) {
  result <- run_fixed_once(run_object)
  rows[[length(rows) + 1]] <- data.frame(
    tool = "biogeobears",
    iteration = iteration,
    seconds = sprintf("%.6f", result$seconds),
    lnL = sprintf("%.15f", result$lnL),
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
