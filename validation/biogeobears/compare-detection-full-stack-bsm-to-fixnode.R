args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 4 || length(args) > 8) {
  stop(
    paste(
      "Usage: Rscript validation/biogeobears/compare-detection-full-stack-bsm-to-fixnode.R",
      "<bsm-dir> <node-golden.tsv> <split-golden.tsv> <report.tsv>",
      "[z-limit=7] [node-tv-limit=0.04] [split-tv-limit=0.06] [case-id]"
    ),
    call. = FALSE
  )
}

bsm_dir <- args[[1]]
node_golden_path <- args[[2]]
split_golden_path <- args[[3]]
report_path <- args[[4]]
z_limit <- if (length(args) >= 5) as.numeric(args[[5]]) else 7
node_tv_limit <- if (length(args) >= 6) as.numeric(args[[6]]) else 0.04
split_tv_limit <- if (length(args) >= 7) as.numeric(args[[7]]) else 0.06
case_id <- if (length(args) >= 8 && nzchar(args[[8]])) args[[8]] else NULL

if (!is.finite(z_limit) || z_limit <= 0) {
  stop("z-limit must be finite and positive", call. = FALSE)
}
if (!is.finite(node_tv_limit) || node_tv_limit <= 0) {
  stop("node-tv-limit must be finite and positive", call. = FALSE)
}
if (!is.finite(split_tv_limit) || split_tv_limit <= 0) {
  stop("split-tv-limit must be finite and positive", call. = FALSE)
}

read_tsv <- function(path) {
  if (!file.exists(path)) {
    stop("Missing TSV file: ", path, call. = FALSE)
  }
  read.delim(path, check.names = FALSE, stringsAsFactors = FALSE)
}

metadata <- read_tsv(file.path(bsm_dir, "metadata.tsv"))
if (!identical(names(metadata), c("key", "value")) || anyDuplicated(metadata$key)) {
  stop("BSM metadata has an invalid schema", call. = FALSE)
}
metadata_values <- setNames(as.character(metadata$value), metadata$key)
if (!identical(unname(metadata_values[["format"]]), "biogeo-bsm-tsv-v1")) {
  stop("Unsupported BSM directory format", call. = FALSE)
}
if (!identical(unname(metadata_values[["status"]]), "complete")) {
  stop("BSM directory is not complete", call. = FALSE)
}
sample_count <- as.integer(metadata_values[["samples"]])
if (length(sample_count) != 1 || is.na(sample_count) || sample_count < 100) {
  stop("BSM metadata contains an invalid sample count", call. = FALSE)
}

node_samples <- read_tsv(file.path(bsm_dir, "node_states.tsv"))
split_samples <- read_tsv(file.path(bsm_dir, "cladogenetic_splits.tsv"))
node_golden <- read_tsv(node_golden_path)
split_golden <- read_tsv(split_golden_path)
if (!is.null(case_id)) {
  if (!("case_id" %in% names(node_golden)) || !("case_id" %in% names(split_golden))) {
    stop("Case filtering requires case_id columns in both golden tables", call. = FALSE)
  }
  node_golden <- node_golden[node_golden$case_id == case_id, , drop = FALSE]
  split_golden <- split_golden[split_golden$case_id == case_id, , drop = FALSE]
  if (nrow(node_golden) == 0 || nrow(split_golden) == 0) {
    stop("Golden tables contain no rows for case: ", case_id, call. = FALSE)
  }
}

required_node_sample <- c("sample", "kind", "clade", "range_bits")
required_split_sample <- c(
  "sample", "clade", "left_clade", "right_clade",
  "ancestor_range_bits", "left_range_bits", "right_range_bits"
)
required_node_golden <- c("clade", "range_bits")
required_split_golden <- c(
  "clade", "left_clade", "right_clade", "ancestor_range_bits",
  "left_range_bits", "right_range_bits"
)
if (!all(required_node_sample %in% names(node_samples))) {
  stop("Node sample table is missing required columns", call. = FALSE)
}
if (!all(required_split_sample %in% names(split_samples))) {
  stop("Split sample table is missing required columns", call. = FALSE)
}
if (!all(required_node_golden %in% names(node_golden))) {
  stop("Node golden table is missing required columns", call. = FALSE)
}
if (!all(required_split_golden %in% names(split_golden))) {
  stop("Split golden table is missing required columns", call. = FALSE)
}

bits_text <- function(values) {
  format(as.numeric(values), scientific = FALSE, trim = TRUE)
}
node_key <- function(table) {
  paste(table$clade, bits_text(table$range_bits), sep = "|")
}
split_key <- function(table) {
  paste(
    table$clade,
    table$left_clade,
    table$right_clade,
    bits_text(table$ancestor_range_bits),
    bits_text(table$left_range_bits),
    bits_text(table$right_range_bits),
    sep = "|"
  )
}

probability_column <- function(table, label) {
  candidates <- c("fixnode_probability", "biogeobears_probability", "probability")
  matches <- candidates[candidates %in% names(table)]
  if (length(matches) == 0) {
    stop(label, " golden has no supported probability column", call. = FALSE)
  }
  matches[[1]]
}
node_golden$probability <- as.numeric(
  node_golden[[probability_column(node_golden, "Node")]]
)
split_golden$probability <- as.numeric(
  split_golden[[probability_column(split_golden, "Split")]]
)
if (any(!is.finite(node_golden$probability)) || any(node_golden$probability < 0)) {
  stop("Node golden contains invalid probabilities", call. = FALSE)
}
if (any(!is.finite(split_golden$probability)) || any(split_golden$probability < 0)) {
  stop("Split golden contains invalid probabilities", call. = FALSE)
}

node_golden$key <- node_key(node_golden)
split_golden$key <- split_key(split_golden)
if (anyDuplicated(node_golden$key)) {
  stop("Node golden contains duplicate keys", call. = FALSE)
}
if (anyDuplicated(split_golden$key)) {
  stop("Split golden contains duplicate keys", call. = FALSE)
}

check_group_sums <- function(table, label) {
  sums <- tapply(table$probability, table$clade, sum)
  max_delta <- max(abs(sums - 1))
  if (max_delta > 1e-10) {
    stop(label, " probabilities do not sum to one; max delta=", max_delta, call. = FALSE)
  }
}
check_group_sums(node_golden, "Node golden")
check_group_sums(split_golden, "Split golden")

internal_nodes <- node_samples[node_samples$kind != "tip", , drop = FALSE]
node_count <- length(unique(node_golden$clade))
split_count <- length(unique(split_golden$clade))
expected_samples <- seq_len(sample_count) - 1L
validate_sample_rows <- function(samples, rows_per_sample, label) {
  sample_ids <- as.integer(samples$sample)
  if (anyNA(sample_ids) || any(sample_ids < 0) || any(sample_ids >= sample_count)) {
    stop(label, " contains invalid sample indexes", call. = FALSE)
  }
  counts <- tabulate(sample_ids + 1L, nbins = sample_count)
  if (!identical(counts, rep.int(as.integer(rows_per_sample), sample_count))) {
    stop(label, " does not contain exactly one row per node and sample", call. = FALSE)
  }
}
validate_sample_rows(internal_nodes, node_count, "Node sample table")
validate_sample_rows(split_samples, split_count, "Split sample table")
if (!identical(sort(unique(as.integer(internal_nodes$sample))), expected_samples)) {
  stop("Node sample indexes are incomplete", call. = FALSE)
}
if (!identical(sort(unique(as.integer(split_samples$sample))), expected_samples)) {
  stop("Split sample indexes are incomplete", call. = FALSE)
}

internal_nodes$key <- node_key(internal_nodes)
split_samples$key <- split_key(split_samples)
extra_node_keys <- setdiff(unique(internal_nodes$key), node_golden$key)
extra_split_keys <- setdiff(unique(split_samples$key), split_golden$key)
if (length(extra_node_keys) > 0) {
  stop("BSM sampled node states absent from the fixnode golden", call. = FALSE)
}
if (length(extra_split_keys) > 0) {
  stop("BSM sampled split scenarios absent from the corrected golden", call. = FALSE)
}

compare_distribution <- function(golden, observed_keys, group) {
  observed_table <- table(observed_keys)
  observed <- as.integer(observed_table[golden$key])
  observed[is.na(observed)] <- 0L
  empirical <- observed / sample_count
  expected <- golden$probability
  delta <- empirical - expected
  standard_error <- sqrt(expected * (1 - expected) / sample_count)
  concentration_limit <- z_limit * standard_error + z_limit ^ 2 / (3 * sample_count)
  deterministic_zero <- expected <= 1e-15
  deterministic_one <- expected >= 1 - 1e-15
  passed <- abs(delta) <= concentration_limit
  passed[deterministic_zero] <- observed[deterministic_zero] == 0L
  passed[deterministic_one] <- observed[deterministic_one] == sample_count
  z_score <- rep(NA_real_, length(expected))
  stochastic <- standard_error > 0
  z_score[stochastic] <- abs(delta[stochastic]) / standard_error[stochastic]

  data.frame(
    group = group,
    clade = golden$clade,
    key = golden$key,
    expected_probability = expected,
    observed_count = observed,
    empirical_probability = empirical,
    difference = delta,
    absolute_difference = abs(delta),
    standard_error = standard_error,
    z_score = z_score,
    concentration_limit = concentration_limit,
    pass = passed,
    stringsAsFactors = FALSE
  )
}

node_report <- compare_distribution(node_golden, internal_nodes$key, "node_state")
split_report <- compare_distribution(split_golden, split_samples$key, "split_scenario")
report <- rbind(node_report, split_report)

total_variation <- function(report) {
  tapply(report$absolute_difference, report$clade, sum) / 2
}
node_tv <- total_variation(node_report)
split_tv <- total_variation(split_report)
max_node_tv <- max(node_tv)
max_split_tv <- max(split_tv)

dir.create(dirname(report_path), recursive = TRUE, showWarnings = FALSE)
write.table(report, report_path, sep = "\t", quote = FALSE, row.names = FALSE, na = "NA")

finite_z <- report$z_score[is.finite(report$z_score)]
max_z <- if (length(finite_z) == 0) 0 else max(finite_z)
cat(
  "Compared ", sample_count, " Rust stochastic histories with ",
  nrow(node_golden), " node posterior probabilities and ",
  nrow(split_golden), " split posterior probabilities.\n",
  sep = ""
)
cat(
  "max_z=", format(max_z, digits = 7),
  ", max_node_tv=", format(max_node_tv, digits = 7),
  ", max_split_tv=", format(max_split_tv, digits = 7), "\n",
  sep = ""
)

failures <- report[!report$pass, , drop = FALSE]
if (nrow(failures) > 0 || max_node_tv > node_tv_limit || max_split_tv > split_tv_limit) {
  if (nrow(failures) > 0) {
    failures <- failures[order(failures$absolute_difference, decreasing = TRUE), , drop = FALSE]
    cat("Probability failures (up to 10):\n")
    print(utils::head(failures[, c(
      "group", "clade", "expected_probability", "empirical_probability",
      "absolute_difference", "z_score", "concentration_limit"
    )], 10), row.names = FALSE)
  }
  if (max_node_tv > node_tv_limit) {
    cat("Node total-variation limit exceeded: ", max_node_tv, " > ", node_tv_limit, "\n", sep = "")
  }
  if (max_split_tv > split_tv_limit) {
    cat("Split total-variation limit exceeded: ", max_split_tv, " > ", split_tv_limit, "\n", sep = "")
  }
  stop("BSM posterior-distribution validation failed", call. = FALSE)
}

cat("All BSM posterior-distribution checks passed.\n")
cat("Wrote ", report_path, "\n", sep = "")
