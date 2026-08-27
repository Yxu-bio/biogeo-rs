args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/decj_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-decj-optim.tsv"
model_preset <- if (length(args) >= 3) toupper(args[[3]]) else "DEC"

if (!(model_preset %in% c("DEC", "DIVALIKE", "BAYAREALIKE"))) {
  stop("model_preset must be DEC, DIVALIKE, or BAYAREALIKE", call. = FALSE)
}

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

set_fixed_param <- function(model_object, name, value) {
  if (!(name %in% rownames(model_object@params_table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
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

preset_max_j <- function(preset) {
  if (preset == "DIVALIKE") {
    return(1.99999)
  }
  if (preset == "BAYAREALIKE") {
    return(0.99999)
  }
  2.99999
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
    if (name == "j" && length(par) >= 3) {
      return(par[[3]])
    }
  }

  stop("Could not extract BioGeoBEARS parameter: ", name, call. = FALSE)
}

extract_optim_convergence <- function(result) {
  if (is.null(result$optim_result) || is.null(result$optim_result$convergence)) {
    stop("BioGeoBEARS result did not contain an optimizer convergence code", call. = FALSE)
  }
  as.integer(result$optim_result$convergence)
}

extract_optim_message <- function(result) {
  if (is.null(result$optim_result) || is.null(result$optim_result$message)) {
    return("")
  }
  gsub("[\\t\\r\\n]+", " ", as.character(result$optim_result$message))
}

run_case <- function(case, repo_root) {
  tmp_dir <- tempfile("bgb-decj-optim-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  init_d <- 0.01
  init_e <- 0.01
  init_j <- 0.5
  min_rate <- 1e-12
  max_rate <- 10
  min_j <- 1e-5
  max_j <- preset_max_j(model_preset)

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
  run_object$BioGeoBEARS_model_object <- apply_model_preset(
    run_object$BioGeoBEARS_model_object,
    model_preset
  )
  run_object$BioGeoBEARS_model_object <- set_optional_range_size_params(
    run_object$BioGeoBEARS_model_object,
    case
  )
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
  run_object$BioGeoBEARS_model_object <- set_free_param(
    run_object$BioGeoBEARS_model_object,
    "j",
    init_j,
    min_j,
    max_j
  )

  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)

  list(
    lnL = extract_loglike(result),
    d = extract_param(result, "d"),
    e = extract_param(result, "e"),
    j = extract_param(result, "j"),
    init_d = init_d,
    init_e = init_e,
    init_j = init_j,
    min_rate = min_rate,
    max_rate = max_rate,
    min_j = min_j,
    max_j = max_j,
    convergence = extract_optim_convergence(result),
    message = extract_optim_message(result)
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
fixtures <- fixtures[tolower(fixtures$biogeobears_ready) == "true", , drop = FALSE]
if ("biogeobears_optimization_ready" %in% names(fixtures)) {
  fixtures <- fixtures[
    tolower(fixtures$biogeobears_optimization_ready) == "true",
    ,
    drop = FALSE
  ]
}

rows <- list()
for (i in seq_len(nrow(fixtures))) {
  case <- fixtures[i, , drop = FALSE]
  cat("Running BioGeoBEARS ", model_preset, "+J optimized fixture: ", case$case_id, "\n", sep = "")
  result <- run_case(case, repo_root)
  rows[[length(rows) + 1]] <- data.frame(
    case_id = case$case_id,
    biogeobears_lnL = sprintf("%.15f", result$lnL),
    biogeobears_d = sprintf("%.15g", result$d),
    biogeobears_e = sprintf("%.15g", result$e),
    biogeobears_j = sprintf("%.15g", result$j),
    init_d = result$init_d,
    init_e = result$init_e,
    init_j = result$init_j,
    min_rate = result$min_rate,
    max_rate = result$max_rate,
    min_j = result$min_j,
    max_j = result$max_j,
    biogeobears_convergence = result$convergence,
    biogeobears_message = result$message,
    max_range_size = case$max_range_size,
    include_null_range = case$include_null_range,
    model = paste0(model_preset, "+J"),
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
