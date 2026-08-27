args <- commandArgs(trailingOnly = TRUE)
output_path <- if (length(args) >= 1) args[[1]] else {
  "validation/golden/biogeobears-mx01r-audit.tsv"
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

write_lagrange_geog <- function(ranges_path, output_path) {
  ranges <- read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE)
  if (ncol(ranges) < 2 || names(ranges)[[1]] != "tip") {
    stop("Range table must have first column named 'tip': ", ranges_path, call. = FALSE)
  }

  area_names <- names(ranges)[-1]
  bits <- apply(ranges[, -1, drop = FALSE], 1, paste0, collapse = "")
  lines <- c(
    paste(
      nrow(ranges),
      length(area_names),
      paste0("(", paste(area_names, collapse = " "), ")"),
      sep = "\t"
    ),
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

extract_loglike <- function(result) {
  if (is.numeric(result) && length(result) == 1) {
    return(as.numeric(result))
  }

  for (name in c("total_loglikelihood", "total_loglike", "lnL", "loglike", "LnL")) {
    value <- result[[name]]
    if (!is.null(value) && is.numeric(value) && length(value) == 1) {
      return(as.numeric(value))
    }
  }

  if (!is.null(result$outputs)) {
    for (name in c("total_loglikelihood", "total_loglike", "lnL", "loglike", "LnL")) {
      value <- tryCatch(slot(result$outputs, name), error = function(e) NULL)
      if (!is.null(value) && is.numeric(value) && length(value) == 1) {
        return(as.numeric(value))
      }
    }
  }

  stop("Could not extract a scalar log-likelihood from BioGeoBEARS result", call. = FALSE)
}

extract_cladogenesis_signature <- function(result, run_object, include_null_range) {
  period_count <- if (is.null(run_object$timeperiods)) {
    1L
  } else {
    length(run_object$timeperiods)
  }

  unlist(lapply(seq_len(period_count), function(timeperiod_i) {
    matrices <- get_Qmat_COOmat_from_res(
      result,
      timeperiod_i = timeperiod_i,
      include_null_range = include_null_range
    )
    coo <- matrices$COO_weights_columnar
    rowsums <- matrices$Rsp_rowsums
    if (is.null(coo) || length(coo) < 4 || is.null(rowsums)) {
      stop(
        "BioGeoBEARS result did not contain cladogenesis weights for period ",
        timeperiod_i,
        call. = FALSE
      )
    }
    c(
      as.numeric(coo[[1]]),
      as.numeric(coo[[2]]),
      as.numeric(coo[[3]]),
      as.numeric(coo[[4]]),
      as.numeric(rowsums)
    )
  }), use.names = FALSE)
}

extract_split_signature <- function(result, tree) {
  states <- result$inputs$all_geog_states_list_usually_inferred_from_areas_maxareas
  posterior <- result$ML_marginal_prob_each_state_at_branch_top_AT_node
  uppass <- result$relative_probs_of_each_state_at_branch_top_AT_node_UPPASS
  downpass <- result$relative_probs_of_each_state_at_branch_bottom_below_node_DOWNPASS
  if (is.null(states) || is.null(posterior) || is.null(uppass) || is.null(downpass)) {
    stop("BioGeoBEARS result did not contain ancestral probability tables", call. = FALSE)
  }

  internal_nodes <- seq.int(length(tree$tip.label) + 1, length(tree$tip.label) + tree$Nnode)
  matrices <- get_Qmat_COOmat_from_res(
    result,
    timeperiod_i = 1,
    include_null_range = TRUE
  )
  coo <- matrices$COO_weights_columnar

  split_probabilities <- unlist(lapply(internal_nodes, function(node) {
    children <- tree$edge[tree$edge[, 1] == node, 2]
    if (length(children) != 2) {
      stop("Expected a binary tree at node ", node, call. = FALSE)
    }
    as.numeric(calc_uppass_scenario_probs_new2(
      probs_ancstate = uppass[node, ],
      COO_weights_columnar = coo,
      numstates = length(states),
      include_null_range = TRUE,
      left_branch_downpass_likes = downpass[children[[1]], ],
      right_branch_downpass_likes = downpass[children[[2]], ],
      Rsp_rowsums = matrices$Rsp_rowsums
    ))
  }), use.names = FALSE)

  root_node <- length(tree$tip.label) + 1
  list(
    root = as.numeric(posterior[root_node, ]),
    uppass = as.numeric(uppass),
    downpass = as.numeric(downpass),
    split = split_probabilities
  )
}

run_case <- function(case, repo_root, mx01r, calculate_ancestral) {
  tmp_dir <- tempfile("bgb-mx01r-")
  dir.create(tmp_dir)
  on.exit(unlink(tmp_dir, recursive = TRUE), add = TRUE)

  tree_path <- normalizePath(file.path(repo_root, case$tree), winslash = "/", mustWork = TRUE)
  ranges_path <- normalizePath(file.path(repo_root, case$ranges), winslash = "/", mustWork = TRUE)
  geog_path <- file.path(tmp_dir, "geog.data")
  write_lagrange_geog(ranges_path, geog_path)

  include_null_range <- parse_bool(case$include_null_range)
  run_object <- define_BioGeoBEARS_run()
  run_object$trfn <- tree_path
  run_object$geogfn <- geog_path
  run_object$max_range_size <- as.integer(case$max_range_size)
  run_object$include_null_range <- include_null_range
  run_object$min_branchlength <- 0
  run_object$print_optim <- FALSE
  run_object$num_cores_to_use <- 1
  run_object$use_optimx <- FALSE
  run_object$return_condlikes_table <- TRUE
  run_object$calc_TTL_loglike_from_condlikes_table <- TRUE
  run_object$calc_ancprobs <- calculate_ancestral
  run_object$speedup <- FALSE

  run_object <- readfiles_BioGeoBEARS_run(run_object)
  run_object <- apply_fixture_dispersal_multipliers(run_object, case, repo_root)
  model <- run_object$BioGeoBEARS_model_object
  model <- set_fixed_param(model, "d", as.numeric(case$d))
  model <- set_fixed_param(model, "e", as.numeric(case$e))
  model <- set_fixed_param(model, "j", 0)
  model <- set_fixed_param(model, "mx01r", mx01r)
  run_object$BioGeoBEARS_model_object <- model

  check_BioGeoBEARS_run(run_object)
  result <- bears_optim_run(BioGeoBEARS_run_object = run_object)
  tree <- read.tree(tree_path)
  ancestral <- if (calculate_ancestral) extract_split_signature(result, tree) else NULL

  list(
    lnL = extract_loglike(result),
    clado = extract_cladogenesis_signature(result, run_object, include_null_range),
    root = if (is.null(ancestral)) numeric(0) else ancestral$root,
    uppass = if (is.null(ancestral)) numeric(0) else ancestral$uppass,
    downpass = if (is.null(ancestral)) numeric(0) else ancestral$downpass,
    split = if (is.null(ancestral)) numeric(0) else ancestral$split
  )
}

max_abs_delta <- function(current, baseline) {
  if (length(current) != length(baseline)) {
    return(Inf)
  }
  if (length(current) == 0) {
    return(NA_real_)
  }
  if (!identical(is.na(current), is.na(baseline))) {
    return(Inf)
  }
  observed <- !is.na(current)
  if (!identical(is.finite(current[observed]), is.finite(baseline[observed]))) {
    return(Inf)
  }
  finite <- observed & is.finite(current) & is.finite(baseline)
  if (!all(current[observed & !finite] == baseline[observed & !finite])) {
    return(Inf)
  }
  if (!any(finite)) {
    return(0)
  }
  max(abs(current[finite] - baseline[finite]))
}

format_metric <- function(value) {
  if (is.na(value)) {
    return("NA")
  }
  format(value, digits = 17, scientific = TRUE, trim = TRUE)
}

repo_root <- env$repo_root
dec_cases <- read.delim(
  file.path(repo_root, "validation", "dec_fixtures.tsv"),
  check.names = FALSE,
  stringsAsFactors = FALSE
)
stratified_cases <- read.delim(
  file.path(repo_root, "validation", "time_stratified_raw_fixtures.tsv"),
  check.names = FALSE,
  stringsAsFactors = FALSE
)

cases <- list(
  list(
    id = "five_area_eight_tip_mosaic_null",
    kind = "complex_static",
    calculate_ancestral = TRUE,
    case = dec_cases[dec_cases$case_id == "five_area_eight_tip_mosaic_null", , drop = FALSE]
  ),
  list(
    id = "psychotria_m4b_official_stratified",
    kind = "official_stratified",
    calculate_ancestral = FALSE,
    case = stratified_cases[
      stratified_cases$case_id == "psychotria_m4b_official_stratified",
      ,
      drop = FALSE
    ]
  )
)
if (any(vapply(cases, function(item) nrow(item$case) != 1, logical(1)))) {
  stop("mx01r audit fixture lookup did not return exactly one row", call. = FALSE)
}

mx01r_values <- c(0.0001, 0.5, 0.9999)
rows <- list()
for (item in cases) {
  signatures <- lapply(mx01r_values, function(value) {
    message("mx01r audit: ", item$id, ", mx01r=", value)
    run_case(
      item$case[1, , drop = FALSE],
      repo_root,
      value,
      item$calculate_ancestral
    )
  })
  baseline <- signatures[[which(mx01r_values == 0.5)]]

  for (index in seq_along(mx01r_values)) {
    signature <- signatures[[index]]
    deltas <- c(
      lnL = abs(signature$lnL - baseline$lnL),
      root = max_abs_delta(signature$root, baseline$root),
      uppass = max_abs_delta(signature$uppass, baseline$uppass),
      downpass = max_abs_delta(signature$downpass, baseline$downpass),
      split = max_abs_delta(signature$split, baseline$split),
      clado = max_abs_delta(signature$clado, baseline$clado)
    )
    finite_deltas <- deltas[!is.na(deltas)]
    exactly_unchanged <- all(finite_deltas == 0)
    if (!exactly_unchanged) {
      stop(
        "mx01r changed a BioGeoBEARS likelihood output for ",
        item$id,
        " at mx01r=",
        mx01r_values[[index]],
        ": ",
        paste(names(finite_deltas), finite_deltas, collapse = ", "),
        call. = FALSE
      )
    }

    rows[[length(rows) + 1]] <- data.frame(
      biogeobears_version = as.character(packageVersion("BioGeoBEARS")),
      case_id = item$id,
      case_kind = item$kind,
      mx01r = format(mx01r_values[[index]], digits = 17, trim = TRUE),
      lnL = format(signature$lnL, digits = 17, trim = TRUE),
      root_state_count = length(signature$root),
      split_scenario_count = length(signature$split),
      cladogenesis_signature_count = length(signature$clado),
      max_abs_lnL_delta = format_metric(deltas[["lnL"]]),
      max_abs_root_delta = format_metric(deltas[["root"]]),
      max_abs_uppass_delta = format_metric(deltas[["uppass"]]),
      max_abs_downpass_delta = format_metric(deltas[["downpass"]]),
      max_abs_split_delta = format_metric(deltas[["split"]]),
      max_abs_cladogenesis_delta = format_metric(deltas[["clado"]]),
      exactly_unchanged = exactly_unchanged,
      stringsAsFactors = FALSE
    )
  }
}

if (!grepl("^(/|[A-Za-z]:[/\\\\])", output_path)) {
  output_path <- file.path(repo_root, output_path)
}
output_path <- normalizePath(output_path, winslash = "/", mustWork = FALSE)
dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(
  do.call(rbind, rows),
  output_path,
  sep = "\t",
  quote = FALSE,
  row.names = FALSE,
  na = "NA"
)
cat("Wrote mx01r runtime audit to ", output_path, "\n", sep = "")
