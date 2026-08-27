args <- commandArgs(trailingOnly = TRUE)
output_path <- if (length(args) >= 1) {
  args[[1]]
} else {
  "validation/golden/biogeobears-model-comparison.tsv"
}

source("validation/biogeobears/r-env.R")
configure_project_r()

if (!requireNamespace("BioGeoBEARS", quietly = TRUE)) {
  stop("BioGeoBEARS is not installed in the project R library", call. = FALSE)
}

suppressPackageStartupMessages(library(BioGeoBEARS))

comparison <- data.frame(
  model_id = c("DEC", "DEC+J", "custom"),
  LnL = c(-30.0, -28.0, -27.5),
  numparams = c(2, 3, 4),
  tips = c(19, 19, 19),
  stringsAsFactors = FALSE
)
comparison$AIC <- calc_AIC_vals(comparison$LnL, comparison$numparams)
comparison$AICc <- calc_AICc_vals(
  comparison$LnL,
  comparison$numparams,
  samplesize = comparison$tips
)
comparison <- AkaikeWeights_on_summary_table(comparison, colname_to_use = "AIC")
comparison <- AkaikeWeights_on_summary_table(comparison, colname_to_use = "AICc")

options(digits = 17)
write.table(
  comparison,
  file = output_path,
  sep = "\t",
  quote = FALSE,
  row.names = FALSE,
  col.names = TRUE
)
cat("Wrote", output_path, "\n")
