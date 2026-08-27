fixture_optional_path <- function(case, column) {
  if (!(column %in% names(case))) {
    return(NULL)
  }
  value <- as.character(case[[column]])
  if (is.na(value) || !nzchar(value) || value == "-") {
    return(NULL)
  }
  value
}

read_fixture_dispersal_matrix <- function(matrix_path, area_names) {
  matrix_table <- read.delim(
    matrix_path,
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
  if (ncol(matrix_table) < 2 || names(matrix_table)[[1]] != "from") {
    stop("Dispersal multiplier table must start with a 'from' column: ", matrix_path, call. = FALSE)
  }

  row_names <- as.character(matrix_table[[1]])
  matrix_values <- as.matrix(matrix_table[-1])
  storage.mode(matrix_values) <- "double"
  rownames(matrix_values) <- row_names
  if (!identical(colnames(matrix_values), area_names)) {
    stop("Dispersal multiplier columns do not match range-table areas", call. = FALSE)
  }
  if (!identical(rownames(matrix_values), area_names)) {
    stop("Dispersal multiplier rows do not match range-table areas", call. = FALSE)
  }
  if (any(!is.finite(matrix_values)) || any(matrix_values < 0)) {
    stop("Dispersal multipliers must be finite and non-negative", call. = FALSE)
  }
  matrix_values
}

read_fixture_binary_area_matrix <- function(matrix_path, area_names) {
  matrix_values <- read_fixture_dispersal_matrix(matrix_path, area_names)
  if (any(!(matrix_values %in% c(0, 1)))) {
    stop("Range-state constraint matrices must contain only 0 and 1: ", matrix_path, call. = FALSE)
  }
  matrix_values
}

read_fixture_extirpation_multipliers <- function(vector_path, area_names) {
  vector_table <- read.delim(
    vector_path,
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
  if (!identical(names(vector_table), c("area", "multiplier"))) {
    stop("Extirpation multiplier table must contain 'area' and 'multiplier' columns: ", vector_path, call. = FALSE)
  }
  if (!identical(as.character(vector_table$area), area_names)) {
    stop("Extirpation multiplier rows do not match range-table areas", call. = FALSE)
  }
  values <- as.numeric(vector_table$multiplier)
  if (any(!is.finite(values)) || any(values < 0)) {
    stop("Extirpation multipliers must be finite and non-negative", call. = FALSE)
  }
  values
}

read_fixture_area_sizes <- function(vector_path, area_names) {
  vector_table <- read.delim(
    vector_path,
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
  if (!identical(names(vector_table), c("area", "size"))) {
    stop("Area-size table must contain 'area' and 'size' columns: ", vector_path, call. = FALSE)
  }
  if (!identical(as.character(vector_table$area), area_names)) {
    stop("Area-size rows do not match range-table areas", call. = FALSE)
  }
  values <- as.numeric(vector_table$size)
  if (any(!is.finite(values)) || any(values <= 0)) {
    stop("Area sizes must be finite and positive", call. = FALSE)
  }
  values
}

write_biogeobears_distance_file <- function(matrices, area_names) {
  output_path <- tempfile("biogeobears-distances-", fileext = ".txt")
  lines <- character()
  for (matrix_values in matrices) {
    rows <- vapply(
      seq_len(nrow(matrix_values)),
      function(row_index) paste(
        format(
          matrix_values[row_index, ],
          digits = 17,
          scientific = FALSE,
          trim = TRUE
        ),
        collapse = "\t"
      ),
      character(1)
    )
    lines <- c(lines, paste(area_names, collapse = "\t"), rows, "")
  }
  writeLines(c(lines, "END", ""), output_path, useBytes = TRUE)
  output_path
}

set_fixture_fixed_param <- function(run_object, name, value) {
  params <- run_object$BioGeoBEARS_model_object@params_table
  if (!(name %in% rownames(params))) {
    stop("BioGeoBEARS model has no parameter named: ", name, call. = FALSE)
  }
  for (column in intersect(c("init", "est", "min", "max"), colnames(params))) {
    params[name, column] <- value
  }
  params[name, "type"] <- "fixed"
  run_object$BioGeoBEARS_model_object@params_table <- params
  run_object
}

resolve_biogeobears_min_branch_length <- function(
  tree_path,
  requested,
  wrapper_default = 1e-6
) {
  if (length(requested) != 1 || !is.finite(requested) || requested < 0) {
    stop("min_branch_length must be one finite non-negative number", call. = FALSE)
  }

  tree <- ape::read.tree(tree_path)
  requested_hooks <- tree$edge.length < requested
  wrapper_hooks <- tree$edge.length < wrapper_default
  if (!identical(requested_hooks, wrapper_hooks)) {
    differing_edges <- which(requested_hooks != wrapper_hooks)
    stop(
      "BioGeoBEARS 1.1.3 bears_optim_run() uses min_branchlength=1e-6 in its helper likelihood path; ",
      "the requested value ", requested, " changes hook classification on edge(s) ",
      paste(differing_edges, collapse = ","),
      ". Use a lower-level BioGeoBEARS audit before generating this golden.",
      call. = FALSE
    )
  }

  wrapper_default
}

apply_fixture_dispersal_multipliers <- function(
    run_object,
    case,
    repo_root,
    free_parameter = NULL,
    area_names = NULL) {
  free_parameters <- unique(as.character(free_parameter))
  if (is.null(free_parameter)) {
    free_parameters <- character(0)
  }
  if (any(!(free_parameters %in% c("x", "n", "u")))) {
    stop("free_parameter must contain only 'x', 'n', and/or 'u'", call. = FALSE)
  }
  is_free <- function(name) name %in% free_parameters

  static_path <- fixture_optional_path(case, "dispersal_multipliers")
  strata_path <- fixture_optional_path(case, "dispersal_strata")
  distance_path <- fixture_optional_path(case, "distance_matrix")
  distance_exponent <- fixture_optional_path(case, "distance_exponent")
  environment_path <- fixture_optional_path(case, "environment_distance_matrix")
  environment_exponent <- fixture_optional_path(case, "environment_distance_exponent")
  extirpation_path <- fixture_optional_path(case, "extirpation_multipliers")
  area_sizes_path <- fixture_optional_path(case, "area_sizes")
  area_exponent <- fixture_optional_path(case, "area_exponent")
  if (!is.null(static_path) && !is.null(strata_path)) {
    stop("Fixture cannot specify both dispersal_multipliers and dispersal_strata", call. = FALSE)
  }
  if (!is.null(extirpation_path) && !is.null(area_sizes_path)) {
    stop("Fixture cannot specify both extirpation_multipliers and area_sizes", call. = FALSE)
  }
  if (is_free("x") && is.null(distance_path) && is.null(strata_path)) {
    stop("Free x fixture must specify distance_matrix", call. = FALSE)
  }
  if (is_free("n") && is.null(environment_path) && is.null(strata_path)) {
    stop("Free n fixture must specify environment_distance_matrix", call. = FALSE)
  }
  if (is_free("u") && is.null(area_sizes_path) && is.null(strata_path)) {
    stop("Free u fixture must specify area_sizes", call. = FALSE)
  }
  if (is_free("x") && !is.null(distance_exponent)) {
    stop("Free x fixture must leave distance_exponent blank", call. = FALSE)
  }
  if (is_free("n") && !is.null(environment_exponent)) {
    stop("Free n fixture must leave environment_distance_exponent blank", call. = FALSE)
  }
  if (is_free("u") && !is.null(area_exponent)) {
    stop("Free u fixture must leave area_exponent blank", call. = FALSE)
  }
  if (is.null(distance_path) && is.null(strata_path) && !is.null(distance_exponent)) {
    stop("Fixture cannot specify distance_exponent without distance_matrix", call. = FALSE)
  }
  if (!is.null(distance_path) && is.null(distance_exponent) && !is_free("x")) {
    stop("Fixture must specify distance_matrix and distance_exponent together", call. = FALSE)
  }
  if (is.null(environment_path) && is.null(strata_path) && !is.null(environment_exponent)) {
    stop(
      "Fixture cannot specify environment_distance_exponent without environment_distance_matrix",
      call. = FALSE
    )
  }
  if (!is.null(environment_path) && is.null(environment_exponent) && !is_free("n")) {
    stop("Fixture must specify environment_distance_matrix and environment_distance_exponent together", call. = FALSE)
  }
  if (is.null(area_sizes_path) && is.null(strata_path) && !is.null(area_exponent)) {
    stop("Fixture cannot specify area_exponent without area_sizes", call. = FALSE)
  }
  if (!is.null(area_sizes_path) && is.null(area_exponent) && !is_free("u")) {
    stop("Fixture must specify area_sizes and area_exponent together", call. = FALSE)
  }
  if (is.null(static_path) && is.null(strata_path) && is.null(distance_path) && is.null(environment_path) && is.null(extirpation_path) && is.null(area_sizes_path)) {
    return(run_object)
  }

  if (is.null(area_names)) {
    ranges_path <- normalizePath(
      file.path(repo_root, as.character(case$ranges)),
      winslash = "/",
      mustWork = TRUE
    )
    area_names <- names(read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE))[-1]
  }
  area_names <- as.character(area_names)
  if (length(area_names) == 0 || any(!nzchar(area_names)) || anyDuplicated(area_names)) {
    stop("area_names must contain unique, non-empty names", call. = FALSE)
  }

  num_periods <- 1L
  extended_schedule <- NULL
  schedule_path <- NULL
  if (!is.null(strata_path)) {
    schedule_path <- normalizePath(
      file.path(repo_root, strata_path),
      winslash = "/",
      mustWork = TRUE
    )
    schedule <- read.delim(schedule_path, check.names = FALSE, stringsAsFactors = FALSE)
    legacy_header <- c("oldest_age", "matrix")
    extended_header <- c(
      "oldest_age",
      "matrix",
      "distance_matrix",
      "environment_distance_matrix",
      "area_sizes"
    )
    constrained_header <- c(extended_header, "areas_allowed", "areas_adjacency")
    if (!(identical(names(schedule), legacy_header)
          || identical(names(schedule), extended_header)
          || identical(names(schedule), constrained_header)) || nrow(schedule) == 0) {
      stop("Strata table has an unsupported header", call. = FALSE)
    }
    ages <- as.numeric(schedule$oldest_age)
    if (any(!is.finite(ages)) || any(ages <= 0) || is.unsorted(ages, strictly = TRUE)) {
      stop("Anagenetic oldest-age boundaries must be finite, positive, and strictly increasing", call. = FALSE)
    }
    num_periods <- length(ages)
    run_object$timeperiods <- ages
    if (identical(names(schedule), extended_header)
        || identical(names(schedule), constrained_header)) {
      extended_schedule <- schedule
    } else {
      matrix_paths <- file.path(dirname(schedule_path), as.character(schedule$matrix))
      run_object$list_of_dispersal_multipliers_mats <- lapply(
        matrix_paths,
        function(path) read_fixture_dispersal_matrix(
          normalizePath(path, winslash = "/", mustWork = TRUE),
          area_names
        )
      )
    }
  }

  is_missing_schedule_path <- function(value) {
    is.na(value) || !nzchar(value) || value %in% c("-", "none", "NONE")
  }
  load_schedule_column <- function(column, reader, default_value) {
    values <- as.character(extended_schedule[[column]])
    if (all(vapply(values, is_missing_schedule_path, logical(1)))) {
      return(NULL)
    }
    lapply(values, function(value) {
      if (is_missing_schedule_path(value)) {
        return(default_value)
      }
      reader(
        normalizePath(
          file.path(dirname(schedule_path), value),
          winslash = "/",
          mustWork = TRUE
        ),
        area_names
      )
    })
  }

  if (!is.null(extended_schedule)) {
    if (!is.null(static_path)) {
      stop("Extended strata cannot be combined with a static manual matrix", call. = FALSE)
    }
    run_object$list_of_dispersal_multipliers_mats <- load_schedule_column(
      "matrix",
      read_fixture_dispersal_matrix,
      matrix(1, nrow = length(area_names), ncol = length(area_names))
    )
    run_object$list_of_distances_mats <- load_schedule_column(
      "distance_matrix",
      read_fixture_dispersal_matrix,
      matrix(1, nrow = length(area_names), ncol = length(area_names))
    )
    run_object$list_of_envdistances_mats <- load_schedule_column(
      "environment_distance_matrix",
      read_fixture_dispersal_matrix,
      matrix(1, nrow = length(area_names), ncol = length(area_names))
    )
    run_object$list_of_area_of_areas <- load_schedule_column(
      "area_sizes",
      read_fixture_area_sizes,
      rep(1, length(area_names))
    )
    if ("areas_allowed" %in% names(extended_schedule)) {
      all_areas <- matrix(
        1,
        nrow = length(area_names),
        ncol = length(area_names),
        dimnames = list(area_names, area_names)
      )
      run_object$list_of_areas_allowed_mats <- load_schedule_column(
        "areas_allowed",
        read_fixture_binary_area_matrix,
        all_areas
      )
      run_object$list_of_areas_adjacency_mats <- load_schedule_column(
        "areas_adjacency",
        read_fixture_binary_area_matrix,
        all_areas
      )
      if (!is.null(run_object$list_of_areas_allowed_mats)) {
        run_object$areas_allowed_fn <- write_biogeobears_distance_file(
          run_object$list_of_areas_allowed_mats,
          area_names
        )
      }
      if (!is.null(run_object$list_of_areas_adjacency_mats)) {
        run_object$areas_adjacency_fn <- write_biogeobears_distance_file(
          run_object$list_of_areas_adjacency_mats,
          area_names
        )
      }
    }
    if (!is.null(distance_path) && !is.null(run_object$list_of_distances_mats)) {
      stop("Extended strata and distance_matrix both provide geographic distances", call. = FALSE)
    }
    if (!is.null(environment_path) && !is.null(run_object$list_of_envdistances_mats)) {
      stop("Extended strata and environment_distance_matrix both provide environmental distances", call. = FALSE)
    }
    if ((!is.null(area_sizes_path) || !is.null(extirpation_path))
        && !is.null(run_object$list_of_area_of_areas)) {
      stop("Extended strata and static extirpation inputs both provide area effects", call. = FALSE)
    }
  }

  if (!is.null(static_path)) {
    matrix_path <- normalizePath(
      file.path(repo_root, static_path),
      winslash = "/",
      mustWork = TRUE
    )
    run_object$list_of_dispersal_multipliers_mats <- replicate(
      num_periods,
      read_fixture_dispersal_matrix(matrix_path, area_names),
      simplify = FALSE
    )
  }
  if (!is.null(run_object$list_of_dispersal_multipliers_mats)) {
    run_object$dispersal_multipliers_fn <- write_biogeobears_distance_file(
      run_object$list_of_dispersal_multipliers_mats,
      area_names
    )
    run_object <- set_fixture_fixed_param(run_object, "w", 1)
  }

  if (!is.null(distance_path)) {
    matrix_path <- normalizePath(
      file.path(repo_root, distance_path),
      winslash = "/",
      mustWork = TRUE
    )
    run_object$list_of_distances_mats <- replicate(
      num_periods,
      read_fixture_dispersal_matrix(matrix_path, area_names),
      simplify = FALSE
    )
  }
  if (!is.null(run_object$list_of_distances_mats)) {
    run_object$distsfn <- write_biogeobears_distance_file(
      run_object$list_of_distances_mats,
      area_names
    )
    if (!is_free("x")) {
      exponent <- as.numeric(distance_exponent)
      if (length(exponent) != 1 || !is.finite(exponent)) {
        stop("Distance exponent must be one finite number", call. = FALSE)
      }
      run_object <- set_fixture_fixed_param(run_object, "x", exponent)
    }
  } else if (is_free("x")) {
    stop("Free x fixture did not provide geographic distances", call. = FALSE)
  }

  if (!is.null(environment_path)) {
    matrix_path <- normalizePath(
      file.path(repo_root, environment_path),
      winslash = "/",
      mustWork = TRUE
    )
    run_object$list_of_envdistances_mats <- replicate(
      num_periods,
      read_fixture_dispersal_matrix(matrix_path, area_names),
      simplify = FALSE
    )
  }
  if (!is.null(run_object$list_of_envdistances_mats)) {
    run_object$envdistsfn <- write_biogeobears_distance_file(
      run_object$list_of_envdistances_mats,
      area_names
    )
    if (!is_free("n")) {
      exponent <- as.numeric(environment_exponent)
      if (length(exponent) != 1 || !is.finite(exponent)) {
        stop("Environmental distance exponent must be one finite number", call. = FALSE)
      }
      run_object <- set_fixture_fixed_param(run_object, "n", exponent)
    }
  } else if (is_free("n")) {
    stop("Free n fixture did not provide environmental distances", call. = FALSE)
  }

  if (!is.null(extirpation_path)) {
    vector_path <- normalizePath(
      file.path(repo_root, extirpation_path),
      winslash = "/",
      mustWork = TRUE
    )
    run_object$list_of_area_of_areas <- replicate(
      num_periods,
      read_fixture_extirpation_multipliers(vector_path, area_names),
      simplify = FALSE
    )
    run_object <- set_fixture_fixed_param(run_object, "u", 1)
  } else if (!is.null(area_sizes_path)) {
    vector_path <- normalizePath(
      file.path(repo_root, area_sizes_path),
      winslash = "/",
      mustWork = TRUE
    )
    run_object$list_of_area_of_areas <- replicate(
      num_periods,
      read_fixture_area_sizes(vector_path, area_names),
      simplify = FALSE
    )
  }
  if (!is.null(run_object$list_of_area_of_areas) && is.null(extirpation_path)) {
    if (!is_free("u")) {
      exponent <- as.numeric(area_exponent)
      if (length(exponent) != 1 || !is.finite(exponent)) {
        stop("Area exponent must be one finite number", call. = FALSE)
      }
      run_object <- set_fixture_fixed_param(run_object, "u", exponent)
    }
  } else if (is_free("u")) {
    stop("Free u fixture did not provide area sizes", call. = FALSE)
  }

  if (!is.null(strata_path)) {
    run_object <- section_the_tree(
      inputs = run_object,
      make_master_table = TRUE,
      plot_pieces = FALSE
    )
  }
  run_object
}
