args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/xnu_optimization_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-dec-xnu-optim.tsv"

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

set_param <- function(model_object, name, type, init, min_value, max_value) {
  table <- model_object@params_table
  table[name, "type"] <- type
  table[name, "init"] <- init
  table[name, "est"] <- init
  table[name, "min"] <- min_value
  table[name, "max"] <- max_value
  model_object@params_table <- table
  model_object
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
  for (name in c("total_loglikelihood", "lnL", "loglike", "log_likelihood")) {
    value <- result[[name]]
    if (!is.null(value) && is.numeric(value) && length(value) == 1 && is.finite(value)) {
      return(as.numeric(value))
    }
  }
  if (!is.null(result$outputs)) {
    for (name in c("total_loglikelihood", "lnL")) {
      value <- tryCatch(slot(result$outputs, name), error = function(e) NULL)
      if (!is.null(value) && is.numeric(value) && length(value) == 1 && is.finite(value)) {
        return(as.numeric(value))
      }
    }
  }
  if (!is.null(result$optim_result) && !is.null(result$optim_result$value)) {
    return(as.numeric(result$optim_result$value[[1]]))
  }
  stop("Could not extract BioGeoBEARS log-likelihood", call. = FALSE)
}

extract_convergence <- function(result) {
  optim_result <- result$optim_result
  if (is.data.frame(optim_result) && "convcode" %in% names(optim_result)) {
    return(as.integer(optim_result$convcode[[1]]))
  }
  if (!is.null(optim_result$convergence)) {
    return(as.integer(optim_result$convergence[[1]]))
  }
  NA_integer_
}

run_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-xnu-")
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
    free_parameter = c("x", "n", "u")
  )

  model_object <- run_object$BioGeoBEARS_model_object
  model_object <- set_param(
    model_object, "d", "free", as.numeric(case$init_d),
    as.numeric(case$min_rate), as.numeric(case$max_rate)
  )
  model_object <- set_param(
    model_object, "e", "free", as.numeric(case$init_e),
    as.numeric(case$min_rate), as.numeric(case$max_rate)
  )
  for (name in c("x", "n", "u")) {
    model_object <- set_param(
      model_object, name, "free", as.numeric(case[[paste0("init_", name)]]),
      as.numeric(case[[paste0("min_", name)]]),
      as.numeric(case[[paste0("max_", name)]])
    )
  }
  model_object <- set_fixed_param(model_object, "j", 0)
  run_object$BioGeoBEARS_model_object <- model_object
  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  estimates <- result$outputs@params_table
  optimizer_result <- result$optim_result
  optimizer_name <- if (is.data.frame(optimizer_result)) rownames(optimizer_result)[[1]] else "optimx"
  optimizer_kkt1 <- if (is.data.frame(optimizer_result) && "kkt1" %in% names(optimizer_result)) {
    as.character(optimizer_result$kkt1[[1]])
  } else {
    NA_character_
  }
  optimizer_kkt2 <- if (is.data.frame(optimizer_result) && "kkt2" %in% names(optimizer_result)) {
    as.character(optimizer_result$kkt2[[1]])
  } else {
    NA_character_
  }
  optimizer_seconds <- if (is.data.frame(optimizer_result) && "xtime" %in% names(optimizer_result)) {
    as.numeric(optimizer_result$xtime[[1]])
  } else {
    NA_real_
  }

  data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", extract_loglike(result)),
    biogeobears_d = sprintf("%.15g", estimates["d", "est"]),
    biogeobears_e = sprintf("%.15g", estimates["e", "est"]),
    biogeobears_x = sprintf("%.15g", estimates["x", "est"]),
    biogeobears_n = sprintf("%.15g", estimates["n", "est"]),
    biogeobears_u = sprintf("%.15g", estimates["u", "est"]),
    optimizer = optimizer_name,
    convergence = extract_convergence(result),
    kkt1 = optimizer_kkt1,
    kkt2 = optimizer_kkt2,
    optimizer_seconds = optimizer_seconds,
    stringsAsFactors = FALSE
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
fixtures <- fixtures[tolower(fixtures$biogeobears_ready) == "true", , drop = FALSE]
rows <- vector("list", nrow(fixtures))
for (index in seq_len(nrow(fixtures))) {
  cat("Running BioGeoBEARS joint d/e/x/n/u fixture: ", fixtures$case_id[[index]], "\n", sep = "")
  rows[[index]] <- run_case(fixtures[index, , drop = FALSE], repo_root)
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
