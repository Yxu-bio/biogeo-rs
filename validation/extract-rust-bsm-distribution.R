args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 2) {
  stop(
    paste0(
      "Usage: Rscript validation/extract-rust-bsm-distribution.R ",
      "<cli-output.txt|bsm-output-dir> <samples.tsv>"
    ),
    call. = FALSE
  )
}

input_path <- args[[1]]
output_path <- args[[2]]

read_tsv <- function(path) {
  if (!file.exists(path)) {
    stop("Missing Rust BSM table: ", path, call. = FALSE)
  }
  read.delim(
    path,
    check.names = FALSE,
    stringsAsFactors = FALSE
  )
}

if (dir.exists(input_path)) {
  metadata <- read_tsv(file.path(input_path, "metadata.tsv"))
  if (!identical(names(metadata), c("key", "value")) || anyDuplicated(metadata$key)) {
    stop("Rust BSM metadata must contain unique key/value rows", call. = FALSE)
  }
  metadata_values <- setNames(as.character(metadata$value), metadata$key)
  directory_format <- unname(metadata_values[["format"]])
  supported_formats <- c(
    "biogeo-bsm-tsv-v1",
    "biogeo-bsm-sharded-tsv-v1",
    "biogeo-bsm-full-tsv-v2",
    "biogeo-bsm-full-sharded-tsv-v2",
    "biogeo-bsm-compact-tsv-v2",
    "biogeo-bsm-compact-sharded-tsv-v2",
    "biogeo-bsm-summary-tsv-v2",
    "biogeo-bsm-summary-sharded-tsv-v2"
  )
  if (!directory_format %in% supported_formats) {
    stop("Unsupported Rust BSM directory format", call. = FALSE)
  }
  if (!identical(unname(metadata_values[["status"]]), "complete")) {
    stop("Rust BSM output directory is incomplete", call. = FALSE)
  }
  sample_count <- as.integer(metadata_values[["samples"]])
  is_sharded <- grepl("-sharded-tsv-", directory_format, fixed = TRUE)
  if (!is_sharded) {
    events <- read_tsv(file.path(input_path, "sample_event_counts.tsv"))
    period_events <- read_tsv(file.path(input_path, "sample_period_event_counts.tsv"))
    occupancy <- read_tsv(file.path(input_path, "sample_state_occupancy.tsv"))
    period_occupancy <- read_tsv(file.path(input_path, "sample_period_state_occupancy.tsv"))
  } else {
    manifest_path <- file.path(input_path, "manifest.tsv")
    if (!file.exists(manifest_path)) {
      stop("Missing Rust BSM shard manifest: ", manifest_path, call. = FALSE)
    }
    manifest_lines <- readLines(manifest_path, warn = FALSE, encoding = "UTF-8")
    shard_marker <- match("shards", manifest_lines)
    if (is.na(shard_marker) || shard_marker < 3L || shard_marker >= length(manifest_lines)) {
      stop("Rust BSM shard manifest has no shard table", call. = FALSE)
    }
    manifest_metadata <- read.delim(
      text = paste(manifest_lines[seq_len(shard_marker - 1L)], collapse = "\n"),
      check.names = FALSE,
      stringsAsFactors = FALSE
    )
    if (
      !identical(names(manifest_metadata), c("key", "value")) ||
      anyDuplicated(manifest_metadata$key)
    ) {
      stop("Rust BSM shard manifest metadata is invalid", call. = FALSE)
    }
    manifest_values <- setNames(as.character(manifest_metadata$value), manifest_metadata$key)
    if (!identical(unname(manifest_values[["format"]]), "biogeo-bsm-shard-manifest-v1")) {
      stop("Unsupported Rust BSM shard manifest format", call. = FALSE)
    }
    if (!identical(
      unname(manifest_values[["run_fingerprint"]]),
      unname(metadata_values[["run_fingerprint"]])
    )) {
      stop("Rust BSM metadata and shard manifest fingerprints differ", call. = FALSE)
    }
    shards <- read.delim(
      text = paste(manifest_lines[seq.int(shard_marker + 1L, length(manifest_lines))], collapse = "\n"),
      check.names = FALSE,
      stringsAsFactors = FALSE
    )
    required_shard_columns <- c(
      "shard_index", "sample_start", "sample_end_exclusive", "sample_count", "directory"
    )
    if (!all(required_shard_columns %in% names(shards)) || nrow(shards) < 1L) {
      stop("Rust BSM shard manifest table is invalid", call. = FALSE)
    }
    shards <- shards[order(as.integer(shards$shard_index)), , drop = FALSE]
    expected_indexes <- seq_len(nrow(shards)) - 1L
    starts <- as.integer(shards$sample_start)
    ends <- as.integer(shards$sample_end_exclusive)
    counts <- as.integer(shards$sample_count)
    expected_starts <- c(0L, head(ends, -1L))
    if (
      !identical(as.integer(shards$shard_index), expected_indexes) ||
      anyNA(c(starts, ends, counts)) ||
      !identical(starts, expected_starts) ||
      !identical(counts, ends - starts) ||
      tail(ends, 1L) != sample_count
    ) {
      stop("Rust BSM shard ranges are incomplete or unordered", call. = FALSE)
    }
    shard_dirs <- file.path(input_path, as.character(shards$directory))
    if (any(!dir.exists(shard_dirs))) {
      stop("Rust BSM shard manifest references a missing directory", call. = FALSE)
    }
    read_sharded_tsv <- function(name) {
      tables <- lapply(file.path(shard_dirs, name), read_tsv)
      reference_names <- names(tables[[1L]])
      if (any(vapply(tables, function(table) !identical(names(table), reference_names), logical(1)))) {
        stop("Rust BSM shard table schemas differ for ", name, call. = FALSE)
      }
      do.call(rbind, tables)
    }
    events <- read_sharded_tsv("sample_event_counts.tsv")
    period_events <- read_sharded_tsv("sample_period_event_counts.tsv")
    occupancy <- read_sharded_tsv("sample_state_occupancy.tsv")
    period_occupancy <- read_sharded_tsv("sample_period_state_occupancy.tsv")
  }
  states_reference_path <- file.path(input_path, "states.tsv")
  states_reference <- if (file.exists(states_reference_path)) {
    read_tsv(states_reference_path)
  } else {
    NULL
  }
} else {
  lines <- readLines(input_path, warn = FALSE, encoding = "UTF-8")

  read_section <- function(section, next_section) {
    start <- match(section, lines)
    finish <- match(next_section, lines)
    if (is.na(start) || is.na(finish) || finish <= start + 1L) {
      stop("Could not locate Rust CLI section: ", section, call. = FALSE)
    }
    section_lines <- lines[seq.int(start + 1L, finish - 1L)]
    read.delim(
      text = paste(section_lines, collapse = "\n"),
      check.names = FALSE,
      stringsAsFactors = FALSE
    )
  }

  sample_line <- grep("^bsm_samples\\t", lines, value = TRUE)
  if (length(sample_line) != 1) {
    stop("Rust CLI output did not contain exactly one bsm_samples line", call. = FALSE)
  }
  sample_count <- as.integer(strsplit(sample_line, "\t", fixed = TRUE)[[1]][[2]])
  events <- read_section("bsm_sample_event_counts", "bsm_sample_period_event_counts")
  period_events <- read_section("bsm_sample_period_event_counts", "bsm_sample_state_occupancy")
  occupancy <- read_section("bsm_sample_state_occupancy", "bsm_sample_period_state_occupancy")
  period_occupancy <- read_section(
    "bsm_sample_period_state_occupancy",
    "bsm_anagenetic_events"
  )
  states_reference <- NULL
}

if (length(sample_count) != 1 || is.na(sample_count) || sample_count < 1) {
  stop("Rust CLI output contained an invalid BSM sample count", call. = FALSE)
}

expected_samples <- seq_len(sample_count) - 1L
if (!identical(as.integer(events$sample), expected_samples)) {
  stop("Rust event summary sample indexes are incomplete or unordered", call. = FALSE)
}

q_indexes <- sort(unique(as.integer(period_events$q_index)))
state_indexes <- if (is.null(states_reference)) {
  sort(unique(as.integer(occupancy$state_index)))
} else {
  sort(unique(as.integer(states_reference$state_index)))
}
if (!identical(q_indexes, c(0L, 1L))) {
  stop("Official Rust BSM output must contain q_index 0 and 1", call. = FALSE)
}
if (!identical(state_indexes, 0:7)) {
  stop("Official Rust BSM output must contain state indexes 0 through 7", call. = FALSE)
}

expected_bits <- c(0, 1, 2, 4, 3, 5, 6, 7)
observed_bits <- if (!is.null(states_reference)) {
  states_reference <- states_reference[
    order(as.integer(states_reference$state_index)),
    ,
    drop = FALSE
  ]
  as.numeric(states_reference$range_bits)
} else {
  state_rows <- occupancy[occupancy$sample == 0, , drop = FALSE]
  state_rows <- state_rows[order(as.integer(state_rows$state_index)), , drop = FALSE]
  as.numeric(state_rows$range_bits)
}
if (!identical(observed_bits, expected_bits)) {
  stop("Rust and BioGeoBEARS state ordering do not match", call. = FALSE)
}

complete_sparse_table <- function(table, keys, value_column) {
  grid <- expand.grid(keys, KEEP.OUT.ATTRS = FALSE, stringsAsFactors = FALSE)
  names(grid) <- names(keys)
  merged <- merge(grid, table, by = names(keys), all.x = TRUE, sort = FALSE)
  merged[[value_column]][is.na(merged[[value_column]])] <- 0
  merged
}

occupancy <- complete_sparse_table(
  occupancy,
  list(sample = expected_samples, state_index = state_indexes),
  "occupancy_time"
)
period_occupancy <- complete_sparse_table(
  period_occupancy,
  list(
    sample = expected_samples,
    q_index = q_indexes,
    state_index = state_indexes
  ),
  "occupancy_time"
)

samples <- data.frame(
  sample = as.integer(events$sample),
  anagenetic_total = as.integer(events$anagenetic_total),
  range_expansion = as.integer(events$range_expansion),
  local_extirpation = as.integer(events$local_extirpation),
  cladogenetic_total = as.integer(events$cladogenetic_total),
  range_copying = as.integer(events$range_copying),
  subset_sympatry = as.integer(events$subset_sympatry),
  vicariance = as.integer(events$vicariance),
  founder_event = as.integer(events$founder_event),
  total_branch_time = as.numeric(events$total_branch_time),
  manual_fallback_branches = 0L,
  max_branch_tries = 0L,
  stringsAsFactors = FALSE
)

for (q_index in q_indexes) {
  values <- period_events[period_events$q_index == q_index, , drop = FALSE]
  values <- values[order(as.integer(values$sample)), , drop = FALSE]
  if (!identical(as.integer(values$sample), expected_samples)) {
    stop("Rust period event summary is incomplete", call. = FALSE)
  }
  samples[[paste0("period_q", q_index, "_events")]] <-
    as.integer(values$anagenetic_event_count)
}

for (state_index in state_indexes) {
  values <- occupancy[occupancy$state_index == state_index, , drop = FALSE]
  values <- values[order(as.integer(values$sample)), , drop = FALSE]
  if (!identical(as.integer(values$sample), expected_samples)) {
    stop("Rust state occupancy summary is incomplete", call. = FALSE)
  }
  samples[[paste0("occupancy_state_", state_index)]] <- as.numeric(values$occupancy_time)
}

for (q_index in q_indexes) {
  for (state_index in state_indexes) {
    values <- period_occupancy[
      period_occupancy$q_index == q_index & period_occupancy$state_index == state_index,
      ,
      drop = FALSE
    ]
    values <- values[order(as.integer(values$sample)), , drop = FALSE]
    if (!identical(as.integer(values$sample), expected_samples)) {
      stop("Rust period-state occupancy summary is incomplete", call. = FALSE)
    }
    samples[[paste0("occupancy_q", q_index, "_state_", state_index)]] <-
      as.numeric(values$occupancy_time)
  }
}

range_switching <- if ("range_switching" %in% names(events)) {
  as.integer(events$range_switching)
} else {
  rep.int(0L, nrow(events))
}
if (any(samples$anagenetic_total !=
        samples$range_expansion + samples$local_extirpation + range_switching)) {
  stop("Rust anagenetic event counts do not sum by type", call. = FALSE)
}
if (any(samples$cladogenetic_total !=
        samples$range_copying + samples$subset_sympatry +
          samples$vicariance + samples$founder_event)) {
  stop("Rust cladogenetic event counts do not sum by type", call. = FALSE)
}
period_columns <- paste0("period_q", q_indexes, "_events")
if (any(rowSums(samples[, period_columns, drop = FALSE]) != samples$anagenetic_total)) {
  stop("Rust period event counts do not sum to the anagenetic total", call. = FALSE)
}
occupancy_columns <- paste0("occupancy_state_", state_indexes)
if (any(abs(rowSums(samples[, occupancy_columns, drop = FALSE]) - 5) > 1e-8)) {
  stop("Rust state occupancy does not sum to the tree branch time", call. = FALSE)
}

dir.create(dirname(output_path), recursive = TRUE, showWarnings = FALSE)
write.table(samples, output_path, sep = "\t", quote = FALSE, row.names = FALSE)
cat("Wrote ", output_path, "\n", sep = "")
