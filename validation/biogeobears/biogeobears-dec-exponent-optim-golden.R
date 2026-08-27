args <- commandArgs(trailingOnly = TRUE)
manifest_path <- if (length(args) >= 1) args[[1]] else "validation/exponent_optimization_fixtures.tsv"
output_path <- if (length(args) >= 2) args[[2]] else "validation/golden/biogeobears-dec-exponent-optim.tsv"
optimizer_backend <- if (length(args) >= 3) tolower(args[[3]]) else "optim"
if (!(optimizer_backend %in% c("optim", "optimx"))) {
  stop("optimizer_backend must be optim or optimx", call. = FALSE)
}

source("validation/biogeobears/r-env.R")
source("validation/biogeobears/biogeobears-fixture-modifiers.R")
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

case_number <- function(case, name) {
  if (!(name %in% names(case))) {
    stop("Fixture is missing numeric column: ", name, call. = FALSE)
  }
  value <- as.numeric(case[[name]])
  if (length(value) != 1 || !is.finite(value)) {
    stop("Fixture column must contain one finite number: ", name, call. = FALSE)
  }
  value
}

case_text <- function(case, name, default) {
  if (!(name %in% names(case))) {
    return(default)
  }
  value <- as.character(case[[name]])
  if (is.na(value) || !nzchar(value)) {
    return(default)
  }
  tolower(value)
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
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
  table <- model_object@params_table
  for (column in intersect(c("init", "est", "min", "max"), colnames(table))) {
    table[name, column] <- value
  }
  table[name, "type"] <- "fixed"
  model_object@params_table <- table
  model_object
}

set_free_param <- function(model_object, name, init, min_value, max_value) {
  if (!(name %in% rownames(model_object@params_table))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
  if (!(min_value < init && init < max_value)) {
    stop("Free parameter init must be strictly inside its bounds: ", name, call. = FALSE)
  }

  table <- model_object@params_table
  table[name, "type"] <- "free"
  for (column in intersect(c("init", "est"), colnames(table))) {
    table[name, column] <- init
  }
  table[name, "min"] <- min_value
  table[name, "max"] <- max_value
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
    par_names <- names(result$optim_result$par)
    if (!is.null(par_names) && name %in% par_names) {
      return(as.numeric(result$optim_result$par[[name]]))
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

extract_optimizer_method <- function(result, backend) {
  optim_result <- result$optim_result
  if (is.data.frame(optim_result) && length(rownames(optim_result)) >= 1) {
    return(rownames(optim_result)[[1]])
  }
  if (backend == "optimx") "bobyqa" else "L-BFGS-B"
}

classify_bound <- function(value, min_value, max_value) {
  tolerance <- max(1e-7, 1e-6 * (max_value - min_value))
  if (value <= min_value + tolerance) {
    return("lower")
  }
  if (value >= max_value - tolerance) {
    return("upper")
  }
  "interior"
}

run_case <- function(case, repo_root) {
  exponent_parameter <- tolower(as.character(case$exponent_parameter))
  if (!(exponent_parameter %in% c("x", "n", "u"))) {
    stop("exponent_parameter must be x, n, or u", call. = FALSE)
  }
  strategy <- case_text(case, "biogeobears_strategy", "free")
  if (!(strategy %in% c("free", "profile"))) {
    stop("biogeobears_strategy must be free or profile", call. = FALSE)
  }
  case_backend <- case_text(case, "biogeobears_optimizer", optimizer_backend)
  if (!(case_backend %in% c("optim", "optimx"))) {
    stop("biogeobears_optimizer must be optim or optimx", call. = FALSE)
  }

  init_d <- case_number(case, "init_d")
  init_e <- case_number(case, "init_e")
  min_rate <- case_number(case, "min_rate")
  max_rate <- case_number(case, "max_rate")
  init_exponent <- case_number(case, "init_exponent")
  min_exponent <- case_number(case, "min_exponent")
  max_exponent <- case_number(case, "max_exponent")

  tmp_dir <- tempfile("bgb-dec-exponent-optim-")
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
  run_object$use_optimx <- case_backend == "optimx"
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- FALSE
  run_object$speedup <- FALSE

  run_object <- readfiles_BioGeoBEARS_run(run_object)
  run_object <- apply_fixture_dispersal_multipliers(
    run_object,
    case,
    repo_root,
    free_parameter = exponent_parameter
  )
  model_object <- run_object$BioGeoBEARS_model_object
  model_object <- set_free_param(model_object, "d", init_d, min_rate, max_rate)
  model_object <- set_free_param(model_object, "e", init_e, min_rate, max_rate)
  model_object <- set_fixed_param(model_object, "j", 0)

  profile_points <- NA_integer_
  if (strategy == "free") {
    model_object <- set_free_param(
      model_object,
      exponent_parameter,
      init_exponent,
      min_exponent,
      max_exponent
    )
    run_object$BioGeoBEARS_model_object <- model_object
    check_BioGeoBEARS_run(run_object)
    result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
    exponent <- extract_param(result, exponent_parameter)
    optimizer <- extract_optimizer_method(result, case_backend)
  } else {
    expected_bound <- case_text(case, "expected_exponent_bound", "")
    if (!(expected_bound %in% c("lower", "upper"))) {
      stop("Profile strategy requires expected_exponent_bound lower or upper", call. = FALSE)
    }
    profile_points <- as.integer(case_number(case, "profile_points"))
    if (profile_points < 3) {
      stop("profile_points must be at least 3", call. = FALSE)
    }

    profile_exponents <- seq(min_exponent, max_exponent, length.out = profile_points)
    profile_results <- vector("list", profile_points)
    profile_lnls <- numeric(profile_points)
    for (index in seq_along(profile_exponents)) {
      candidate_run <- run_object
      candidate_run$BioGeoBEARS_model_object <- set_fixed_param(
        model_object,
        exponent_parameter,
        profile_exponents[[index]]
      )
      check_BioGeoBEARS_run(candidate_run)
      candidate_result <- bears_optim_run(BioGeoBEARS_run_object = candidate_run)
      candidate_convergence <- extract_convergence(candidate_result)
      if (is.na(candidate_convergence) || candidate_convergence != 0) {
        stop(
          "BioGeoBEARS profile point failed to converge at ",
          exponent_parameter,
          "=",
          profile_exponents[[index]],
          call. = FALSE
        )
      }
      profile_results[[index]] <- candidate_result
      profile_lnls[[index]] <- extract_loglike(candidate_result)
      cat(
        "  profile ",
        exponent_parameter,
        "=",
        format(profile_exponents[[index]], digits = 8),
        " lnL=",
        format(profile_lnls[[index]], digits = 12),
        "\n",
        sep = ""
      )
    }

    best_index <- which.max(profile_lnls)
    result <- profile_results[[best_index]]
    exponent <- profile_exponents[[best_index]]
    optimizer <- paste0(
      "profile-",
      profile_points,
      "x-",
      extract_optimizer_method(result, case_backend)
    )
  }

  list(
    lnL = extract_loglike(result),
    d = extract_param(result, "d"),
    e = extract_param(result, "e"),
    exponent = exponent,
    exponent_bound = classify_bound(exponent, min_exponent, max_exponent),
    convergence = extract_convergence(result),
    optimizer = optimizer,
    strategy = strategy,
    profile_points = profile_points
  )
}

repo_root <- env$repo_root
fixtures <- read.delim(manifest_path, check.names = FALSE, stringsAsFactors = FALSE)
fixtures <- fixtures[tolower(fixtures$biogeobears_ready) == "true", , drop = FALSE]

rows <- list()
for (i in seq_len(nrow(fixtures))) {
  case <- fixtures[i, , drop = FALSE]
  cat(
    "Running BioGeoBEARS DEC optimized fixture: ",
    case$case_id,
    " (",
    case$biogeobears_strategy,
    " ",
    case$exponent_parameter,
    ")\n",
    sep = ""
  )
  result <- run_case(case, repo_root)
  rows[[length(rows) + 1]] <- data.frame(
    case_id = case$case_id,
    exponent_parameter = case$exponent_parameter,
    biogeobears_lnL = sprintf("%.15f", result$lnL),
    biogeobears_d = sprintf("%.15g", result$d),
    biogeobears_e = sprintf("%.15g", result$e),
    biogeobears_exponent = sprintf("%.15g", result$exponent),
    exponent_bound = result$exponent_bound,
    init_d = case$init_d,
    init_e = case$init_e,
    min_rate = case$min_rate,
    max_rate = case$max_rate,
    init_exponent = case$init_exponent,
    min_exponent = case$min_exponent,
    max_exponent = case$max_exponent,
    model = "DEC",
    optimizer = result$optimizer,
    strategy = result$strategy,
    profile_points = result$profile_points,
    convergence = result$convergence,
    stringsAsFactors = FALSE
  )
}

output <- do.call(rbind, rows)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(output, file = output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
