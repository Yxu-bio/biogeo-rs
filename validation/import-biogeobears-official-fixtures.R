source("validation/r-env.R")
env <- configure_project_r()

package_root <- system.file(package = "BioGeoBEARS")
if (!nzchar(package_root)) {
  stop("BioGeoBEARS is not installed in the project-local R library", call. = FALSE)
}

read_lagrange_geography <- function(path) {
  lines <- readLines(path, warn = FALSE)
  header_match <- regexec("\\(([^)]*)\\)", lines[[1]])
  header_parts <- regmatches(lines[[1]], header_match)[[1]]
  if (length(header_parts) != 2) {
    stop("Could not parse area names from official geography header: ", lines[[1]], call. = FALSE)
  }
  area_names <- strsplit(trimws(header_parts[[2]]), "[[:space:]]+")[[1]]
  data_lines <- lines[nzchar(trimws(lines))][-1]
  fields <- strsplit(trimws(data_lines), "[[:space:]]+")
  tips <- vapply(fields, `[[`, character(1), 1)
  bitstrings <- vapply(fields, function(parts) parts[[length(parts)]], character(1))
  if (any(nchar(bitstrings) != length(area_names))) {
    stop("Official geography bitstring length does not match its area count", call. = FALSE)
  }
  values <- do.call(rbind, lapply(bitstrings, function(bits) as.integer(strsplit(bits, "")[[1]])))
  output <- data.frame(tip = tips, values, check.names = FALSE)
  names(output) <- c("tip", area_names)
  output
}

read_first_matrix <- function(path) {
  lines <- readLines(path, warn = FALSE)
  lines <- lines[nzchar(trimws(lines))]
  area_names <- strsplit(trimws(lines[[1]]), "[[:space:]]+")[[1]]
  rows <- strsplit(trimws(lines[seq_len(length(area_names)) + 1]), "[[:space:]]+")
  values <- do.call(rbind, lapply(rows, as.numeric))
  if (!identical(dim(values), c(length(area_names), length(area_names)))) {
    stop("Official matrix has unexpected dimensions: ", path, call. = FALSE)
  }
  output <- data.frame(from = area_names, values, check.names = FALSE)
  names(output) <- c("from", area_names)
  output
}

write_tsv <- function(value, path) {
  write.table(value, path, sep = "\t", quote = FALSE, row.names = FALSE)
}

read_official_blocks <- function(path) {
  lines <- trimws(readLines(path, warn = FALSE))
  blocks <- list()
  current <- character()
  for (line in lines) {
    if (line == "END") {
      break
    }
    if (!nzchar(line)) {
      if (length(current) > 0) {
        blocks[[length(blocks) + 1]] <- current
        current <- character()
      }
    } else {
      current <- c(current, line)
    }
  }
  if (length(current) > 0) {
    blocks[[length(blocks) + 1]] <- current
  }
  blocks
}

read_official_matrix_list <- function(path) {
  lapply(read_official_blocks(path), function(block) {
    area_names <- strsplit(block[[1]], "[[:space:]]+")[[1]]
    rows <- strsplit(block[-1], "[[:space:]]+")
    values <- do.call(rbind, lapply(rows, as.numeric))
    if (!identical(dim(values), c(length(area_names), length(area_names)))) {
      stop("Official matrix block has unexpected dimensions: ", path, call. = FALSE)
    }
    output <- data.frame(from = area_names, values, check.names = FALSE)
    names(output) <- c("from", area_names)
    output
  })
}

read_official_area_list <- function(path) {
  lapply(read_official_blocks(path), function(block) {
    if (length(block) != 2) {
      stop("Official area-size block must contain two lines: ", path, call. = FALSE)
    }
    area_names <- strsplit(block[[1]], "[[:space:]]+")[[1]]
    values <- as.numeric(strsplit(block[[2]], "[[:space:]]+")[[1]])
    if (length(values) != length(area_names)) {
      stop("Official area-size block has unexpected dimensions: ", path, call. = FALSE)
    }
    data.frame(area = area_names, size = values, check.names = FALSE)
  })
}

official_root <- file.path(package_root, "extdata", "examples")

bsm_source <- file.path(official_root, "BSM_3taxa", "M3areas_allowed")
bsm_output <- file.path(
  env$repo_root,
  "validation",
  "fixtures",
  "biogeobears_official",
  "bsm_3taxa_areas_allowed"
)
dir.create(bsm_output, recursive = TRUE, showWarnings = FALSE)
bsm_tree_copied <- file.copy(
  file.path(bsm_source, "tree.newick"),
  file.path(bsm_output, "tree.nwk"),
  overwrite = TRUE
)
if (!bsm_tree_copied) {
  stop("Could not copy the official BSM_3taxa tree", call. = FALSE)
}
write_tsv(
  read_lagrange_geography(file.path(bsm_source, "geog.data")),
  file.path(bsm_output, "ranges.tsv")
)
bsm_timeperiods <- as.numeric(readLines(file.path(bsm_source, "timeperiods.txt"), warn = FALSE))
bsm_allowed <- read_official_matrix_list(file.path(bsm_source, "areas_allowed_noC.txt"))
if (length(bsm_allowed) != length(bsm_timeperiods)) {
  stop("Official BSM_3taxa areas-allowed matrix count does not match timeperiods", call. = FALSE)
}
bsm_schedule <- vector("list", length(bsm_timeperiods))
for (index in seq_along(bsm_timeperiods)) {
  allowed_name <- sprintf("period_%02d_allowed.tsv", index)
  write_tsv(bsm_allowed[[index]], file.path(bsm_output, allowed_name))
  bsm_schedule[[index]] <- data.frame(
    oldest_age = bsm_timeperiods[[index]],
    matrix = "-",
    distance_matrix = "-",
    environment_distance_matrix = "-",
    area_sizes = "-",
    areas_allowed = allowed_name,
    areas_adjacency = "-",
    check.names = FALSE
  )
}
write_tsv(do.call(rbind, bsm_schedule), file.path(bsm_output, "anagenetic_strata.tsv"))

psychotria_source <- file.path(official_root, "Psychotria_M4_dists")
psychotria_output <- file.path(
  env$repo_root,
  "validation",
  "fixtures",
  "biogeobears_official",
  "psychotria_m4"
)
dir.create(psychotria_output, recursive = TRUE, showWarnings = FALSE)
psychotria_tree_copied <- file.copy(
  file.path(psychotria_source, "Psychotria_5.2.newick"),
  file.path(psychotria_output, "tree.nwk"),
  overwrite = TRUE
)
if (!psychotria_tree_copied) {
  stop("Could not copy the official Psychotria tree", call. = FALSE)
}
write_tsv(
  read_lagrange_geography(file.path(psychotria_source, "Psychotria_geog.data")),
  file.path(psychotria_output, "ranges.tsv")
)
write_tsv(
  read_first_matrix(file.path(psychotria_source, "Hawaii_KOMH_distances_max1.txt")),
  file.path(psychotria_output, "distances.tsv")
)

area_lines <- readLines(
  file.path(psychotria_source, "Hawaii_KOMH_area_of_areas.txt"),
  warn = FALSE
)
area_lines <- area_lines[nzchar(trimws(area_lines))]
area_names <- strsplit(trimws(area_lines[[1]]), "[[:space:]]+")[[1]]
area_values <- as.numeric(strsplit(trimws(area_lines[[2]]), "[[:space:]]+")[[1]])
area_sizes <- data.frame(
  area = area_names,
  size = area_values,
  check.names = FALSE
)
write_tsv(area_sizes, file.path(psychotria_output, "area_sizes.tsv"))
geometric_mean <- exp(mean(log(area_sizes$size)))
normalized_area_sizes <- area_sizes
normalized_area_sizes$size <- normalized_area_sizes$size / geometric_mean
write_tsv(
  normalized_area_sizes,
  file.path(psychotria_output, "area_sizes_geomean1.tsv")
)

island_ages <- c(K = 5.1, O = 3.7, M = 1.9, H = 0.5)
if (!identical(names(island_ages), area_sizes$area)) {
  stop("Official Psychotria area order changed", call. = FALSE)
}
environment <- outer(island_ages, island_ages, function(left, right) 1 + abs(left - right))
diag(environment) <- 0
environment <- data.frame(from = names(island_ages), environment, check.names = FALSE)
names(environment) <- c("from", names(island_ages))
write_tsv(environment, file.path(psychotria_output, "island_age_distances.tsv"))

conifer_source <- file.path(official_root, "395lab", "conifer_DEC+x_traits_models")
conifer_output <- file.path(
  env$repo_root,
  "validation",
  "fixtures",
  "biogeobears_official",
  "conifer_decx"
)
dir.create(conifer_output, recursive = TRUE, showWarnings = FALSE)
conifer_tree_copied <- file.copy(
  file.path(conifer_source, "tree.newick"),
  file.path(conifer_output, "tree.nwk"),
  overwrite = TRUE
)
if (!conifer_tree_copied) {
  stop("Could not copy the official Conifer tree", call. = FALSE)
}
write_tsv(
  read_lagrange_geography(file.path(conifer_source, "geog.data")),
  file.path(conifer_output, "ranges.tsv")
)
write_tsv(
  read_first_matrix(file.path(conifer_source, "modern_distances_subset.txt")),
  file.path(conifer_output, "distances.tsv")
)

psychotria_stratified_source <- file.path(official_root, "Psychotria_M4b_dists_stratified")
psychotria_stratified_output <- file.path(
  env$repo_root,
  "validation",
  "fixtures",
  "biogeobears_official",
  "psychotria_m4_stratified"
)
dir.create(psychotria_stratified_output, recursive = TRUE, showWarnings = FALSE)
stratified_tree_copied <- file.copy(
  file.path(psychotria_stratified_source, "Psychotria_5.2.newick"),
  file.path(psychotria_stratified_output, "tree.nwk"),
  overwrite = TRUE
)
if (!stratified_tree_copied) {
  stop("Could not copy the official stratified Psychotria tree", call. = FALSE)
}
write_tsv(
  read_lagrange_geography(file.path(psychotria_stratified_source, "Psychotria_geog.data")),
  file.path(psychotria_stratified_output, "ranges.tsv")
)
timeperiods <- as.numeric(readLines(
  file.path(psychotria_stratified_source, "Hawaii_timeperiods.txt"),
  warn = FALSE
))
distance_matrices <- read_official_matrix_list(
  file.path(psychotria_stratified_source, "Hawaii_KOMH_distances.txt")
)
manual_matrices <- read_official_matrix_list(
  file.path(psychotria_stratified_source, "Hawaii_KOMH_dispersal_multipliers.txt")
)
area_vectors <- read_official_area_list(
  file.path(psychotria_stratified_source, "Hawaii_KOMH_area_of_areas.txt")
)
if (length(distance_matrices) != length(timeperiods)
    || length(manual_matrices) != length(timeperiods)
    || length(area_vectors) != length(timeperiods)) {
  stop("Official stratified Psychotria modifier counts do not match timeperiods", call. = FALSE)
}
schedule_rows <- vector("list", length(timeperiods))
for (index in seq_along(timeperiods)) {
  suffix <- sprintf("period_%02d", index)
  manual_name <- paste0(suffix, "_manual.tsv")
  distance_name <- paste0(suffix, "_distance.tsv")
  area_name <- paste0(suffix, "_area_sizes.tsv")
  write_tsv(manual_matrices[[index]], file.path(psychotria_stratified_output, manual_name))
  write_tsv(distance_matrices[[index]], file.path(psychotria_stratified_output, distance_name))
  write_tsv(area_vectors[[index]], file.path(psychotria_stratified_output, area_name))
  schedule_rows[[index]] <- data.frame(
    oldest_age = timeperiods[[index]],
    matrix = manual_name,
    distance_matrix = distance_name,
    environment_distance_matrix = "-",
    area_sizes = area_name,
    check.names = FALSE
  )
}
write_tsv(
  do.call(rbind, schedule_rows),
  file.path(psychotria_stratified_output, "anagenetic_strata.tsv")
)

cat("Imported official BioGeoBEARS fixtures from ", package_root, "\n", sep = "")
