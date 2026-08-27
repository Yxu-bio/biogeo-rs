source("validation/r-env.R")
configure_project_r()
suppressPackageStartupMessages(library(BioGeoBEARS))

args <- commandArgs(trailingOnly = TRUE)
output_file <- if (length(args) > 0) args[[1]] else ""

params <- BioGeoBEARS_model_defaults()
params$parameter <- rownames(params)
params <- params[, c("parameter", "type", "init", "min", "max", "note", "desc")]

write.table(
  params,
  file = output_file,
  row.names = FALSE,
  col.names = TRUE,
  quote = FALSE,
  sep = "\t"
)
