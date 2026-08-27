args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/abw_profile_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-abw-profile.tsv"

source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
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

write_lagrange_geog <- function(ranges_path, output_path) {
  ranges <- read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE)
  area_names <- names(ranges)[-1]
  lines <- c(
    paste(nrow(ranges), length(area_names), paste0("(", paste(area_names, collapse = " "), ")"), sep = "\t"),
    vapply(
      seq_len(nrow(ranges)),
      function(index) paste0(ranges[[1]][[index]], "\t", paste0(ranges[index, -1], collapse = "")),
      character(1)
    )
  )
  writeLines(lines, output_path, useBytes = TRUE)
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

run_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-abw-")
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

  model_object <- run_object$BioGeoBEARS_model_object
  for (name in c("d", "e", "a", "b", "w")) {
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
  extract_loglike(result)
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
rows <- vector("list", nrow(fixtures))
for (index in seq_len(nrow(fixtures))) {
  case <- fixtures[index, , drop = FALSE]
  cat("Running BioGeoBEARS a/b/w profile: ", case$case_id, "\n", sep = "")
  rows[[index]] <- data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", run_case(case, repo_root)),
    d = case$d,
    e = case$e,
    a = case$a,
    b = case$b,
    w = case$w,
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
