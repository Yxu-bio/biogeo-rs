args <- commandArgs(trailingOnly = TRUE)
if (length(args) < 3 || length(args) > 6) {
  stop(
    paste(
      "Usage: Rscript validation/biogeobears/compare-bsm-distributions.R",
      "<biogeobears-samples.tsv> <rust-samples.tsv> <report.tsv>",
      "[max-mean-z=5] [ks-multiplier=2] [max-period-share-difference=0.02]"
    ),
    call. = FALSE
  )
}

bgb_path <- args[[1]]
rust_path <- args[[2]]
report_path <- args[[3]]
max_mean_z <- if (length(args) >= 4) as.numeric(args[[4]]) else 5
ks_multiplier <- if (length(args) >= 5) as.numeric(args[[5]]) else 2
max_period_share_difference <- if (length(args) >= 6) as.numeric(args[[6]]) else 0.02

if (!is.finite(max_mean_z) || max_mean_z <= 0) {
  stop("max-mean-z must be finite and positive", call. = FALSE)
}
if (!is.finite(ks_multiplier) || ks_multiplier <= 0) {
  stop("ks-multiplier must be finite and positive", call. = FALSE)
}
if (!is.finite(max_period_share_difference) || max_period_share_difference <= 0) {
  stop("max-period-share-difference must be finite and positive", call. = FALSE)
}

bgb <- read.delim(bgb_path, check.names = FALSE, stringsAsFactors = FALSE)
rust <- read.delim(rust_path, check.names = FALSE, stringsAsFactors = FALSE)
if (nrow(bgb) < 100 || nrow(rust) < 100) {
  stop("Distribution comparison requires at least 100 maps from each implementation", call. = FALSE)
}
if (!identical(names(bgb), names(rust))) {
  stop("BioGeoBEARS and Rust sample tables have different schemas", call. = FALSE)
}
if (any(bgb$manual_fallback_branches != 0)) {
  stop("BioGeoBEARS sample contains manual fallback histories", call. = FALSE)
}
if (any(rust$manual_fallback_branches != 0)) {
  stop("Rust sample unexpectedly reports fallback histories", call. = FALSE)
}

required_columns <- c(
  "anagenetic_total", "range_expansion", "local_extirpation",
  "cladogenetic_total", "range_copying", "subset_sympatry", "vicariance",
  "founder_event", "total_branch_time", "period_q0_events", "period_q1_events",
  paste0("occupancy_state_", 0:7),
  as.vector(outer(paste0("occupancy_q", 0:1, "_state_"), 0:7, paste0))
)
missing_columns <- setdiff(required_columns, names(bgb))
if (length(missing_columns) > 0) {
  stop("Sample table is missing columns: ", paste(missing_columns, collapse = ", "), call. = FALSE)
}

validate_invariants <- function(samples, label) {
  if (any(samples$anagenetic_total != samples$range_expansion + samples$local_extirpation)) {
    stop(label, " anagenetic event counts do not sum by type", call. = FALSE)
  }
  if (any(samples$cladogenetic_total !=
          samples$range_copying + samples$subset_sympatry +
            samples$vicariance + samples$founder_event)) {
    stop(label, " cladogenetic event counts do not sum by type", call. = FALSE)
  }
  if (any(samples$anagenetic_total != samples$period_q0_events + samples$period_q1_events)) {
    stop(label, " period event counts do not sum to the total", call. = FALSE)
  }
  state_columns <- paste0("occupancy_state_", 0:7)
  if (any(abs(rowSums(samples[, state_columns, drop = FALSE]) - 5) > 1e-8)) {
    stop(label, " state occupancy does not sum to tree branch time", call. = FALSE)
  }
  period_state_columns <- lapply(
    0:1,
    function(q_index) paste0("occupancy_q", q_index, "_state_", 0:7)
  )
  period_sums <- lapply(
    period_state_columns,
    function(columns) rowSums(samples[, columns, drop = FALSE])
  )
  if (any(abs(period_sums[[1]] + period_sums[[2]] - 5) > 1e-8)) {
    stop(label, " period-state occupancy does not sum to tree branch time", call. = FALSE)
  }
}

validate_invariants(bgb, "BioGeoBEARS")
validate_invariants(rust, "Rust")

bgb$period_q0_fraction <- ifelse(
  bgb$anagenetic_total > 0,
  bgb$period_q0_events / bgb$anagenetic_total,
  NA_real_
)
bgb$period_q1_fraction <- ifelse(
  bgb$anagenetic_total > 0,
  bgb$period_q1_events / bgb$anagenetic_total,
  NA_real_
)
rust$period_q0_fraction <- ifelse(
  rust$anagenetic_total > 0,
  rust$period_q0_events / rust$anagenetic_total,
  NA_real_
)
rust$period_q1_fraction <- ifelse(
  rust$anagenetic_total > 0,
  rust$period_q1_events / rust$anagenetic_total,
  NA_real_
)

empirical_ks <- function(x, y) {
  x <- sort(x)
  y <- sort(y)
  points <- sort(unique(c(x, y)))
  max(abs(findInterval(points, x) / length(x) - findInterval(points, y) / length(y)))
}

safe_quantile <- function(x, probability) {
  unname(quantile(x, probs = probability, names = FALSE, type = 7))
}

compare_scalar <- function(group, metric, bgb_values, rust_values) {
  bgb_values <- as.numeric(bgb_values)
  rust_values <- as.numeric(rust_values)
  bgb_values <- bgb_values[is.finite(bgb_values)]
  rust_values <- rust_values[is.finite(rust_values)]
  if (length(bgb_values) < 100 || length(rust_values) < 100) {
    stop("Metric ", metric, " has fewer than 100 finite observations", call. = FALSE)
  }

  bgb_mean <- mean(bgb_values)
  rust_mean <- mean(rust_values)
  difference <- rust_mean - bgb_mean
  pooled_mc_se <- sqrt(var(bgb_values) / length(bgb_values) + var(rust_values) / length(rust_values))
  deterministic <- !is.finite(pooled_mc_se) || pooled_mc_se < 1e-14
  mean_z <- if (deterministic) {
    if (abs(difference) <= 1e-12) 0 else Inf
  } else {
    abs(difference) / pooled_mc_se
  }
  ks_d <- empirical_ks(bgb_values, rust_values)
  ks_limit <- ks_multiplier * sqrt(
    (length(bgb_values) + length(rust_values)) /
      (length(bgb_values) * length(rust_values))
  )
  passed <- mean_z <= max_mean_z && ks_d <= ks_limit

  data.frame(
    group = group,
    metric = metric,
    bgb_n = length(bgb_values),
    rust_n = length(rust_values),
    bgb_mean = bgb_mean,
    rust_mean = rust_mean,
    difference = difference,
    pooled_mc_se = pooled_mc_se,
    mean_z = mean_z,
    ks_d = ks_d,
    ks_limit = ks_limit,
    bgb_sd = sd(bgb_values),
    rust_sd = sd(rust_values),
    bgb_q05 = safe_quantile(bgb_values, 0.05),
    bgb_q50 = safe_quantile(bgb_values, 0.50),
    bgb_q95 = safe_quantile(bgb_values, 0.95),
    rust_q05 = safe_quantile(rust_values, 0.05),
    rust_q50 = safe_quantile(rust_values, 0.50),
    rust_q95 = safe_quantile(rust_values, 0.95),
    absolute_limit = NA_real_,
    pass = passed,
    stringsAsFactors = FALSE
  )
}

ratio_stats <- function(events, total) {
  events <- as.numeric(events)
  total <- as.numeric(total)
  ratio <- sum(events) / sum(total)
  influence <- (events - ratio * total) / mean(total)
  c(ratio = ratio, se = sd(influence) / sqrt(length(influence)))
}

compare_aggregate_period_share <- function(q_index) {
  event_column <- paste0("period_q", q_index, "_events")
  bgb_stats <- ratio_stats(bgb[[event_column]], bgb$anagenetic_total)
  rust_stats <- ratio_stats(rust[[event_column]], rust$anagenetic_total)
  difference <- unname(rust_stats[["ratio"]] - bgb_stats[["ratio"]])
  pooled_mc_se <- sqrt(bgb_stats[["se"]] ^ 2 + rust_stats[["se"]] ^ 2)
  mean_z <- if (pooled_mc_se < 1e-14) {
    if (abs(difference) <= 1e-12) 0 else Inf
  } else {
    abs(difference) / pooled_mc_se
  }
  passed <- mean_z <= max_mean_z && abs(difference) <= max_period_share_difference

  data.frame(
    group = "period_share_aggregate",
    metric = paste0("q", q_index),
    bgb_n = nrow(bgb),
    rust_n = nrow(rust),
    bgb_mean = unname(bgb_stats[["ratio"]]),
    rust_mean = unname(rust_stats[["ratio"]]),
    difference = difference,
    pooled_mc_se = pooled_mc_se,
    mean_z = mean_z,
    ks_d = NA_real_,
    ks_limit = NA_real_,
    bgb_sd = NA_real_,
    rust_sd = NA_real_,
    bgb_q05 = NA_real_,
    bgb_q50 = NA_real_,
    bgb_q95 = NA_real_,
    rust_q05 = NA_real_,
    rust_q50 = NA_real_,
    rust_q95 = NA_real_,
    absolute_limit = max_period_share_difference,
    pass = passed,
    stringsAsFactors = FALSE
  )
}

metric_groups <- c(
  anagenetic_total = "event_count",
  range_expansion = "event_type_count",
  local_extirpation = "event_type_count",
  cladogenetic_total = "event_count",
  range_copying = "event_type_count",
  subset_sympatry = "event_type_count",
  vicariance = "event_type_count",
  founder_event = "event_type_count",
  period_q0_events = "period_event_count",
  period_q1_events = "period_event_count",
  period_q0_fraction = "period_fraction_per_map",
  period_q1_fraction = "period_fraction_per_map",
  total_branch_time = "occupancy_invariant"
)
for (state_index in 0:7) {
  metric_groups[[paste0("occupancy_state_", state_index)]] <- "state_occupancy_time"
}
for (q_index in 0:1) {
  for (state_index in 0:7) {
    metric_groups[[paste0("occupancy_q", q_index, "_state_", state_index)]] <-
      "period_state_occupancy_time"
  }
}

rows <- vector("list", length(metric_groups) + 2L)
row_index <- 0L
for (metric in names(metric_groups)) {
  row_index <- row_index + 1L
  rows[[row_index]] <- compare_scalar(
    unname(metric_groups[[metric]]),
    metric,
    bgb[[metric]],
    rust[[metric]]
  )
}
for (q_index in 0:1) {
  row_index <- row_index + 1L
  rows[[row_index]] <- compare_aggregate_period_share(q_index)
}

report <- do.call(rbind, rows)
dir.create(dirname(report_path), recursive = TRUE, showWarnings = FALSE)
write.table(report, report_path, sep = "\t", quote = FALSE, row.names = FALSE, na = "NA")

cat(
  "Compared ", nrow(bgb), " BioGeoBEARS stochastic histories with ",
  nrow(rust), " Rust stochastic histories.\n",
  sep = ""
)
cat(
  "Thresholds: mean_z <= ",
  max_mean_z,
  ", KS D <= ",
  ks_multiplier,
  " * sqrt((n1+n2)/(n1*n2)), aggregate period-share difference <= ",
  max_period_share_difference,
  ".\n",
  sep = ""
)

failures <- report[!report$pass, , drop = FALSE]
if (nrow(failures) > 0) {
  cat("Distribution mismatches:\n")
  for (row_index in seq_len(nrow(failures))) {
    failure <- failures[row_index, , drop = FALSE]
    cat(
      "  ",
      failure$group,
      "/",
      failure$metric,
      ": mean_z=",
      format(failure$mean_z, digits = 6),
      ", ks_d=",
      format(failure$ks_d, digits = 6),
      ", difference=",
      format(failure$difference, digits = 6),
      "\n",
      sep = ""
    )
  }
  cat("Wrote failing report: ", report_path, "\n", sep = "")
  quit(save = "no", status = 1)
}

cat("All ", nrow(report), " distribution checks passed.\n", sep = "")
cat("Wrote ", report_path, "\n", sep = "")
