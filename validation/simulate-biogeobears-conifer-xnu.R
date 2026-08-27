source("validation/r-env.R")
source("validation/biogeobears-fixture-modifiers.R")
env <- configure_project_r()

suppressPackageStartupMessages({
  library(ape)
  library(rexpokit)
  library(cladoRcpp)
  library(BioGeoBEARS)
})

fixture_dir <- file.path(
  env$repo_root,
  "validation",
  "fixtures",
  "biogeobears_official",
  "conifer_decx"
)
tree_path <- file.path(fixture_dir, "tree.nwk")
observed_ranges_path <- file.path(fixture_dir, "ranges.tsv")
distance_path <- file.path(fixture_dir, "distances.tsv")
if (!all(file.exists(tree_path, observed_ranges_path, distance_path))) {
  stop("Run validation/import-biogeobears-official-fixtures.R first", call. = FALSE)
}

area_names <- names(read.delim(observed_ranges_path, check.names = FALSE))[-1]
expected_areas <- c("A", "D", "F", "G", "H", "I")
if (!identical(area_names, expected_areas)) {
  stop("Official Conifer area order changed", call. = FALSE)
}

environment_coordinates <- rbind(
  A = c(0, 0),
  D = c(1, 3),
  F = c(4, 1),
  G = c(2, 2),
  H = c(5, 4),
  I = c(3, 5)
)
environment_values <- as.matrix(dist(environment_coordinates)) + 1
diag(environment_values) <- 0
environment <- data.frame(from = area_names, environment_values, check.names = FALSE)
names(environment) <- c("from", area_names)
environment_path <- file.path(fixture_dir, "sim_environment_distances.tsv")
write.table(environment, environment_path, sep = "\t", quote = FALSE, row.names = FALSE)

area_values <- c(A = 0.5, D = 2.0, F = 4.5, G = 0.9, H = 3.2, I = 1.4)
area_values <- area_values / exp(mean(log(area_values)))
area_sizes <- data.frame(area = area_names, size = as.numeric(area_values), check.names = FALSE)
area_sizes_path <- file.path(fixture_dir, "sim_area_sizes_geomean1.tsv")
write.table(area_sizes, area_sizes_path, sep = "\t", quote = FALSE, row.names = FALSE)

write_lagrange_geog <- function(ranges_path, output_path) {
  ranges <- read.delim(ranges_path, check.names = FALSE, stringsAsFactors = FALSE)
  areas <- names(ranges)[-1]
  lines <- c(
    paste(nrow(ranges), length(areas), paste0("(", paste(areas, collapse = " "), ")"), sep = "\t"),
    vapply(
      seq_len(nrow(ranges)),
      function(index) paste0(ranges[[1]][[index]], "\t", paste0(ranges[index, -1], collapse = "")),
      character(1)
    )
  )
  writeLines(lines, output_path, useBytes = TRUE)
}

temporary_geog <- tempfile("conifer-observed-", fileext = ".data")
on.exit(unlink(temporary_geog), add = TRUE)
write_lagrange_geog(observed_ranges_path, temporary_geog)

true_parameters <- c(d = 0.02, e = 0.015, x = -0.5, n = 0.8, u = -0.7)
run_object <- define_BioGeoBEARS_run()
run_object$trfn <- normalizePath(tree_path, winslash = "/", mustWork = TRUE)
run_object$geogfn <- temporary_geog
run_object$max_range_size <- 3
run_object$include_null_range <- FALSE
run_object$min_branchlength <- 0
run_object <- readfiles_BioGeoBEARS_run(run_object)

case <- data.frame(
  ranges = "validation/fixtures/biogeobears_official/conifer_decx/ranges.tsv",
  dispersal_multipliers = "",
  dispersal_strata = "",
  distance_matrix = "validation/fixtures/biogeobears_official/conifer_decx/distances.tsv",
  distance_exponent = true_parameters[["x"]],
  environment_distance_matrix = "validation/fixtures/biogeobears_official/conifer_decx/sim_environment_distances.tsv",
  environment_distance_exponent = true_parameters[["n"]],
  extirpation_multipliers = "",
  area_sizes = "validation/fixtures/biogeobears_official/conifer_decx/sim_area_sizes_geomean1.tsv",
  area_exponent = true_parameters[["u"]],
  stringsAsFactors = FALSE
)
run_object <- apply_fixture_dispersal_multipliers(run_object, case, env$repo_root)
run_object <- set_fixture_fixed_param(run_object, "d", true_parameters[["d"]])
run_object <- set_fixture_fixed_param(run_object, "e", true_parameters[["e"]])
run_object <- set_fixture_fixed_param(run_object, "j", 0)

returned_mats <- get_Qmat_COOmat_from_BioGeoBEARS_run_object(
  BioGeoBEARS_run_object = run_object,
  BioGeoBEARS_model_object = run_object$BioGeoBEARS_model_object,
  max_range_size = 3,
  include_null_range = FALSE,
  timeperiod_i = 1
)
states_list <- returned_mats$states_list
root_state <- which(vapply(
  states_list,
  function(state) identical(as.integer(state), c(0L, 3L)),
  logical(1)
))
if (length(root_state) != 1) {
  stop("Could not locate the A+G root range", call. = FALSE)
}

set.seed(20260712)
tree <- read.tree(tree_path)
simulated_states <- simulate_biogeog_history(
  phy = tree,
  Qmat = returned_mats$Qmat,
  COO_probs_columnar = returned_mats$COO_weights_columnar,
  index_Qmat_0based_of_starting_state = root_state - 1L
)

tip_states <- simulated_states[seq_along(tree$tip.label)] + 1L
range_values <- matrix(0L, nrow = length(tip_states), ncol = length(area_names))
for (tip_index in seq_along(tip_states)) {
  occupied <- states_list[[tip_states[[tip_index]]]] + 1L
  range_values[tip_index, occupied] <- 1L
}
simulated_ranges <- data.frame(tip = tree$tip.label, range_values, check.names = FALSE)
names(simulated_ranges) <- c("tip", area_names)
output_path <- file.path(fixture_dir, "sim_ranges_seed20260712.tsv")
write.table(simulated_ranges, output_path, sep = "\t", quote = FALSE, row.names = FALSE)

parameter_table <- data.frame(
  parameter = names(true_parameters),
  value = as.numeric(true_parameters),
  check.names = FALSE
)
write.table(
  parameter_table,
  file.path(fixture_dir, "sim_true_parameters.tsv"),
  sep = "\t",
  quote = FALSE,
  row.names = FALSE
)

cat("Wrote BioGeoBEARS-simulated 197-tip Conifer x/n/u fixture to ", output_path, "\n", sep = "")
