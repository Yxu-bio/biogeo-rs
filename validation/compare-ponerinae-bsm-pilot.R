args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 3) {
  stop(
    paste(
      "Usage: Rscript validation/compare-ponerinae-bsm-pilot.R",
      "<biogeobears-samples.tsv> <rust-bsm-dir> <report.tsv>"
    ),
    call. = FALSE
  )
}

bgb_path <- normalizePath(args[[1]], winslash = "/", mustWork = TRUE)
rust_dir <- normalizePath(args[[2]], winslash = "/", mustWork = TRUE)
report_path <- args[[3]]

read_tsv <- function(path) {
  read.delim(path, check.names = FALSE, stringsAsFactors = FALSE)
}

read_rust_table <- function(file_name) {
  paths <- list.files(
    rust_dir,
    pattern = paste0("^", gsub("\\.", "\\\\.", file_name), "$"),
    recursive = TRUE,
    full.names = TRUE
  )
  paths <- paths[basename(dirname(paths)) != "checkpoints"]
  if (length(paths) == 0) {
    stop("Missing Rust BSM table: ", file_name, call. = FALSE)
  }
  do.call(rbind, lapply(sort(paths), read_tsv))
}

wide_matrix <- function(
  table,
  row_name,
  column_names,
  value_name,
  row_values = NULL,
  column_values = NULL
) {
  if (is.null(row_values)) {
    row_values <- sort(unique(as.integer(table[[row_name]])))
  }
  column_keys <- do.call(paste, c(table[column_names], sep = ":"))
  if (is.null(column_values)) {
    unique_columns <- unique(table[column_names])
    column_order <- do.call(
      order,
      lapply(unique_columns, function(values) as.integer(values))
    )
    unique_columns <- unique_columns[column_order, , drop = FALSE]
    column_values <- do.call(paste, c(unique_columns, sep = ":"))
  }
  matrix <- matrix(0, nrow = length(row_values), ncol = length(column_values))
  rownames(matrix) <- row_values
  colnames(matrix) <- column_values
  row_indexes <- match(as.integer(table[[row_name]]), row_values)
  column_indexes <- match(column_keys, column_values)
  matrix[cbind(row_indexes, column_indexes)] <- as.numeric(table[[value_name]])
  matrix
}

bgb <- read_tsv(bgb_path)
if (nrow(bgb) != 1) {
  stop("Ponerinae pilot requires exactly one BioGeoBEARS history", call. = FALSE)
}

rust_events <- read_rust_table("sample_event_counts.tsv")
rust_period_events <- read_rust_table("sample_period_event_counts.tsv")
rust_state_occupancy <- read_rust_table("sample_state_occupancy.tsv")
rust_period_occupancy <- read_rust_table("sample_period_state_occupancy.tsv")
sample_count <- length(unique(rust_events$sample))
if (sample_count < 2) {
  stop("Rust pilot requires at least two histories", call. = FALSE)
}

state_columns <- grep("^occupancy_state_[0-9]+$", names(bgb), value = TRUE)
period_state_columns <- grep(
  "^occupancy_q[0-9]+_state_[0-9]+$",
  names(bgb),
  value = TRUE
)
period_event_columns <- grep("^period_q[0-9]+_events$", names(bgb), value = TRUE)
if (length(state_columns) == 0 || length(period_state_columns) == 0 ||
    length(period_event_columns) == 0) {
  stop("BioGeoBEARS pilot table is missing occupancy or period columns", call. = FALSE)
}

state_index <- as.integer(sub("^occupancy_state_", "", state_columns))
state_columns <- state_columns[order(state_index)]
period_column_parts <- do.call(
  rbind,
  regmatches(
    period_state_columns,
    regexec("^occupancy_q([0-9]+)_state_([0-9]+)$", period_state_columns)
  )
)
period_order <- order(as.integer(period_column_parts[, 2]), as.integer(period_column_parts[, 3]))
period_state_columns <- period_state_columns[period_order]
period_event_index <- as.integer(sub("^period_q([0-9]+)_events$", "\\1", period_event_columns))
period_event_columns <- period_event_columns[order(period_event_index)]

sample_values <- sort(unique(as.integer(rust_events$sample)))
state_reference_path <- file.path(rust_dir, "states.tsv")
period_reference_path <- file.path(rust_dir, "periods.tsv")
if (file.exists(state_reference_path) && file.exists(period_reference_path)) {
  state_values <- sort(as.integer(read_tsv(state_reference_path)$state_index))
  period_values <- sort(as.integer(read_tsv(period_reference_path)$q_index))
  state_keys <- as.character(state_values)
  period_state_grid <- expand.grid(
    q_index = period_values,
    state_index = state_values,
    KEEP.OUT.ATTRS = FALSE,
    stringsAsFactors = FALSE
  )
  period_state_grid <- period_state_grid[
    order(period_state_grid$q_index, period_state_grid$state_index),
    ,
    drop = FALSE
  ]
  period_state_keys <- do.call(paste, c(period_state_grid, sep = ":"))
} else {
  state_keys <- NULL
  period_state_keys <- NULL
}

rust_state_matrix <- wide_matrix(
  rust_state_occupancy,
  "sample",
  "state_index",
  "occupancy_time",
  row_values = sample_values,
  column_values = state_keys
)
rust_period_matrix <- wide_matrix(
  rust_period_occupancy,
  "sample",
  c("q_index", "state_index"),
  "occupancy_time",
  row_values = sample_values,
  column_values = period_state_keys
)
if (ncol(rust_state_matrix) != length(state_columns) ||
    ncol(rust_period_matrix) != length(period_state_columns)) {
  stop("Rust and BioGeoBEARS occupancy dimensions differ", call. = FALSE)
}

branch_time <- as.numeric(bgb$total_branch_time[[1]])
if (!is.finite(branch_time) || branch_time <= 0 ||
    abs(sum(as.numeric(bgb[1, state_columns])) - branch_time) > 1e-6 ||
    abs(sum(as.numeric(bgb[1, period_state_columns])) - branch_time) > 1e-6) {
  stop("BioGeoBEARS occupancy does not sum to total branch time", call. = FALSE)
}
if (any(abs(rowSums(rust_state_matrix) - branch_time) > 1e-6) ||
    any(abs(rowSums(rust_period_matrix) - branch_time) > 1e-6)) {
  stop("Rust occupancy does not sum to total branch time", call. = FALSE)
}

rows <- list()
add_metric <- function(
  metric,
  bgb_value,
  rust_values,
  interpretation = "descriptive",
  range_tolerance = 0
) {
  rust_values <- as.numeric(rust_values)
  rows[[length(rows) + 1L]] <<- data.frame(
    metric = metric,
    biogeobears_value = as.numeric(bgb_value),
    rust_mean = mean(rust_values),
    rust_sd = stats::sd(rust_values),
    rust_min = min(rust_values),
    rust_max = max(rust_values),
    biogeobears_within_rust_range =
      as.numeric(bgb_value) >= min(rust_values) - range_tolerance &&
        as.numeric(bgb_value) <= max(rust_values) + range_tolerance,
    interpretation = interpretation,
    stringsAsFactors = FALSE
  )
}

event_columns <- c(
  "anagenetic_total", "range_expansion", "local_extirpation",
  "cladogenetic_total", "range_copying", "subset_sympatry", "vicariance",
  "founder_event", "total_branch_time"
)
for (column in event_columns) {
  add_metric(
    column,
    bgb[[column]][[1]],
    rust_events[[column]],
    range_tolerance = if (column == "total_branch_time") 1e-6 else 0
  )
}

for (q_index in seq_along(period_event_columns) - 1L) {
  rust_values <- rust_period_events$anagenetic_event_count[
    as.integer(rust_period_events$q_index) == q_index
  ]
  add_metric(
    paste0("period_q", q_index, "_events"),
    bgb[[paste0("period_q", q_index, "_events")]][[1]],
    rust_values
  )
}

occupancy_distance <- function(matrix, reference) {
  0.5 * rowSums(abs(sweep(matrix, 2, reference, "-"))) / branch_time
}
rust_state_mean <- colMeans(rust_state_matrix)
rust_period_mean <- colMeans(rust_period_matrix)
add_metric(
  "state_occupancy_tv_from_rust_mean",
  0.5 * sum(abs(as.numeric(bgb[1, state_columns]) - rust_state_mean)) / branch_time,
  occupancy_distance(rust_state_matrix, rust_state_mean),
  "descriptive; one BioGeoBEARS history"
)
add_metric(
  "period_state_occupancy_tv_from_rust_mean",
  0.5 * sum(abs(as.numeric(bgb[1, period_state_columns]) - rust_period_mean)) / branch_time,
  occupancy_distance(rust_period_matrix, rust_period_mean),
  "descriptive; one BioGeoBEARS history"
)

for (column in c(
  "manual_fallback_branches", "forbidden_state_transitions",
  "forbidden_state_endpoints", "forbidden_state_time"
)) {
  rust_values <- if (column %in% names(rust_events)) {
    rust_events[[column]]
  } else {
    rep(0, sample_count)
  }
  add_metric(
    column,
    bgb[[column]][[1]],
    rust_values,
    "diagnostic; BioGeoBEARS fallback must not be a semantic golden"
  )
}

report <- do.call(rbind, rows)
dir.create(dirname(report_path), recursive = TRUE, showWarnings = FALSE)
write.table(report, report_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", report_path, "\n", sep = "")
cat(
  "Ponerinae BSM pilot: ", nrow(report), " metrics; ", sample_count,
  " Rust histories; BioGeoBEARS fallback branches=",
  bgb$manual_fallback_branches[[1]], "\n",
  sep = ""
)
