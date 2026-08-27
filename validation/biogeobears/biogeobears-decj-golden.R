args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/decj_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-decj.tsv"

source("validation/biogeobears/r-env.R")
env <- configure_project_r()
source("validation/biogeobears/biogeobears-fixture-modifiers.R")

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

run_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-decj-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

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
  run_object <- apply_fixture_dispersal_multipliers(run_object, case, repo_root)
  run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "d", as.numeric(case$d))
  run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "e", as.numeric(case$e))
  run_object$BioGeoBEARS_model_object <- set_fixed_param(run_object$BioGeoBEARS_model_object, "j", as.numeric(case$j))
  run_object$BioGeoBEARS_model_object <- set_optional_range_size_params(
    run_object$BioGeoBEARS_model_object,
    case
  )

  check_BioGeoBEARS_run(run_object)

  formals_names <- names(formals(bears_optim_run))
  if ("skip_optim" %in% formals_names) {
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
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
fixtures <- fixtures[tolower(fixtures$biogeobears_ready) == "true", , drop = FALSE]

rows <- list()
for (i in seq_len(nrow(fixtures))) {
  case <- fixtures[i, , drop = FALSE]
  cat("Running BioGeoBEARS DEC+J fixture: ", case$case_id, "\n", sep = "")
  lnL <- run_case(case, repo_root)
  rows[[length(rows) + 1]] <- data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", lnL),
    d = case$d,
    e = case$e,
    j = case$j,
    max_range_size = case$max_range_size,
    include_null_range = case$include_null_range,
    mx01y = numeric_case_value(case, "mx01y", 0.0001),
    mx01s = numeric_case_value(case, "mx01s", 0.0001),
    mx01v = numeric_case_value(case, "mx01v", 0.0001),
    mx01j = numeric_case_value(case, "mx01j", 0.0001),
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
