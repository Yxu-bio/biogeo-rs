args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else {
  "validation/detection_combination_fixtures.tsv"
}
output_path <- if (length(args) >= 2) args[[2]] else {
  "validation/golden/biogeobears-detection-combinations.tsv"
}

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

set_fixed_param <- function(model_object, name, value) {
  table <- model_object@params_table
  if (!(name %in% rownames(table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
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
  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  detections_path <- normalizePath(
    file.path(repo_root, case$detections),
    winslash = "/",
    mustWork = TRUE
  )
  controls_path <- normalizePath(
    file.path(repo_root, case$controls),
    winslash = "/",
    mustWork = TRUE
  )
  phy <- read.tree(tree_path)
  area_names <- colnames(read_detections(detections_path, phy = phy))

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
  run_object <- apply_fixture_dispersal_multipliers(
    run_object,
    case,
    repo_root,
    area_names = area_names
  )

  model_object <- run_object$BioGeoBEARS_model_object
  fixed_names <- c(
    "d", "e", "a", "b", "x", "n", "w", "u", "j", "y", "s", "v",
    "mx01", "mx01j", "mx01y", "mx01s", "mx01v", "mf", "dp", "fdp"
  )
  for (name in fixed_names) {
    model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
  }
  run_object$BioGeoBEARS_model_object <- calc_linked_params_BioGeoBEARS_model_object(model_object)
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
  cat("Running BioGeoBEARS detection combination: ", case$case_id, "\n", sep = "")
  rows[[index]] <- data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", run_case(case, repo_root)),
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
