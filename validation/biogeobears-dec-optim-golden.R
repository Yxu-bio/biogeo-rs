args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/dec_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-dec-optim.tsv"
model_preset <- if (length(args) >= 3) toupper(args[[3]]) else "DEC"
optimizer_backend <- if (length(args) >= 4) tolower(args[[4]]) else "optim"

if (!(model_preset %in% c("DEC", "DIVALIKE", "BAYAREALIKE"))) {
  stop("model_preset must be DEC, DIVALIKE, or BAYAREALIKE", call. = FALSE)
}
if (!(optimizer_backend %in% c("optim", "optimx"))) {
  stop("optimizer_backend must be optim or optimx", call. = FALSE)
}

source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
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

set_param_type <- function(model_object, name, type) {
  if (!(name %in% rownames(model_object@params_table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }

  table <- model_object@params_table
  table[name, "type"] <- type
  model_object@params_table <- table
  model_object
}

apply_model_preset <- function(model_object, preset) {
  if (preset == "DEC") {
    return(model_object)
  }

  if (preset == "DIVALIKE") {
    model_object <- set_fixed_param(model_object, "s", 0)
    model_object <- set_param_type(model_object, "ysv", "2-j")
    model_object <- set_param_type(model_object, "ys", "ysv*1/2")
    model_object <- set_param_type(model_object, "y", "ysv*1/2")
    model_object <- set_param_type(model_object, "v", "ysv*1/2")
    return(set_fixed_param(model_object, "mx01v", 0.5))
  }

  model_object <- set_fixed_param(model_object, "s", 0)
  model_object <- set_fixed_param(model_object, "v", 0)
  model_object <- set_param_type(model_object, "ysv", "1-j")
  model_object <- set_param_type(model_object, "ys", "ysv*1/1")
  model_object <- set_param_type(model_object, "y", "1-j")
  set_fixed_param(model_object, "mx01y", 0.9999)
}

set_optional_range_size_params <- function(model_object, case) {
  for (name in c("mx01y", "mx01s", "mx01v", "mx01j")) {
    if (name %in% names(case) && !is.na(case[[name]]) && nzchar(as.character(case[[name]]))) {
      model_object <- set_fixed_param(model_object, name, as.numeric(case[[name]]))
    }
  }

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
    value <- as.numeric(result$optim_result$value)
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

  if (!is.null(result$optim_result) && !is.null(result$optim_result$par)) {
    par <- as.numeric(result$optim_result$par)
    par_names <- names(result$optim_result$par)
    if (!is.null(par_names) && name %in% par_names) {
      return(as.numeric(result$optim_result$par[[name]]))
    }
    if (name == "d" && length(par) >= 1) {
      return(par[[1]])
    }
    if (name == "e" && length(par) >= 2) {
      return(par[[2]])
    }
  }

  stop("Could not extract BioGeoBEARS parameter: ", name, call. = FALSE)
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

extract_optimizer_method <- function(result) {
  optim_result <- result$optim_result
  if (is.data.frame(optim_result) && length(rownames(optim_result)) >= 1) {
    return(rownames(optim_result)[[1]])
  }
  if (optimizer_backend == "optimx") "bobyqa" else "L-BFGS-B"
}

run_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-dec-optim-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  requested_min_branch_length <- numeric_case_value(case, "min_branch_length", 0)
  min_branch_length <- resolve_biogeobears_min_branch_length(
    tree_path,
    requested_min_branch_length
  )
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  init_d <- 0.01
  init_e <- 0.01
  min_rate <- 1e-12
  max_rate <- 10

  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
  run_object$max_range_size <- as.integer(case$max_range_size)
  run_object$include_null_range <- parse_bool(case$include_null_range)
  run_object$min_branchlength <- min_branch_length
  run_object$print_optim <- FALSE
  run_object$num_cores_to_use <- 1
  run_object$use_optimx <- optimizer_backend == "optimx"
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- FALSE
  run_object$speedup <- FALSE

  run_object <- readfiles_BioGeoBEARS_run(run_object)
  run_object$min_branchlength <- min_branch_length
  run_object <- apply_fixture_dispersal_multipliers(run_object, case, repo_root)
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
  run_object$BioGeoBEARS_model_object <- apply_model_preset(
    run_object$BioGeoBEARS_model_object,
    model_preset
  )
  run_object$BioGeoBEARS_model_object <- set_optional_range_size_params(
    run_object$BioGeoBEARS_model_object,
    case
  )

  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  list(
    lnL = extract_loglike(result),
    d = extract_param(result, "d"),
    e = extract_param(result, "e"),
    init_d = init_d,
    init_e = init_e,
    min_rate = min_rate,
    max_rate = max_rate,
    convergence = extract_convergence(result),
    optimizer = extract_optimizer_method(result)
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
fixtures <- fixtures[tolower(fixtures$biogeobears_ready) == "true", , drop = FALSE]
if ("biogeobears_optim_ready" %in% names(fixtures)) {
  fixtures <- fixtures[tolower(fixtures$biogeobears_optim_ready) == "true", , drop = FALSE]
}

rows <- list()
for (i in seq_len(nrow(fixtures))) {
  case <- fixtures[i, , drop = FALSE]
  cat("Running BioGeoBEARS ", model_preset, " optimized fixture: ", case$case_id, "\n", sep = "")
  result <- run_case(case, repo_root)
  rows[[length(rows) + 1]] <- data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", result$lnL),
    biogeobears_d = sprintf("%.15g", result$d),
    biogeobears_e = sprintf("%.15g", result$e),
    init_d = result$init_d,
    init_e = result$init_e,
    min_rate = result$min_rate,
    max_rate = result$max_rate,
    max_range_size = case$max_range_size,
    include_null_range = case$include_null_range,
    model = model_preset,
    optimizer = result$optimizer,
    convergence = result$convergence,
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
