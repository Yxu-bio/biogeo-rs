# Development validation only; installed runtime users do not need R.
args <- commandArgs(trailingOnly = TRUE)
if (length(args) != 3L) stop("Usage: Rscript compare-bsm-joint-routes.R BSM_DIR SOURCE_DIR OUTPUT_DIR")
source("validation/biogeobears/r-env.R")
configure_project_r(create = FALSE)
suppressPackageStartupMessages(library(BioGeoBEARS))
suppressPackageStartupMessages(library(cladoRcpp))
read_tsv <- function(path) read.delim(path, check.names = FALSE, stringsAsFactors = FALSE, quote = "")
bsm <- args[1]; sources <- args[2]; output <- args[3]
dir.create(output, recursive = TRUE, showWarnings = FALSE)
read_chunks <- function(pattern) do.call(rbind, lapply(
  sort(list.files(file.path(sources, "shards"), pattern = pattern, full.names = TRUE)), read_tsv))
events <- read_chunks("^sources-.*[.]tsv$")
routes <- read_chunks("^route-candidates-.*[.]tsv$")
states <- read_tsv(file.path(bsm, "states.tsv"))
matrices <- lapply(c("young.tsv", "old.tsv"), function(name)
  as.matrix(read_tsv(file.path("examples/bsm_joint_routes", name))[, -1]))
members <- function(index) {
  bits <- states$range_bits[match(index, states$state_index)]
  which(bitwAnd(as.integer(bits), c(1L,2L,4L,8L)) != 0L)
}
event_index <- match(routes$event_id, events$event_id)
stopifnot(!anyNA(event_index), !anyDuplicated(events$event_id))
expected <- numeric(nrow(routes))
multi_ids <- character()
for (i in seq_len(nrow(routes))) {
  e <- events[event_index[i], ]
  stopifnot(e$sample == routes$sample[i])
  src <- members(e$source_state_index); dst <- members(e$target_state_index)
  if (e$event_kind == "range_expansion") dst <- setdiff(dst, src)
  stopifnot((routes$source_area_index[i]+1L) %in% src, (routes$target_area_index[i]+1L) %in% dst)
  if (e$event_kind == "range_switching") {
    expected[i] <- 1
  } else {
    matrix <- matrices[[e$q_index+1L]]
    expected[i] <- matrix[routes$source_area_index[i]+1L, routes$target_area_index[i]+1L] / sum(matrix[src,dst,drop=FALSE])
  }
  if (e$event_kind == "founder_event" && length(dst)>1L) multi_ids <- c(multi_ids,e$event_id)
}
difference <- max(abs(expected-routes$conditional_probability))
stopifnot(difference < 1e-14, length(unique(multi_ids))>0L)
stopifnot(all(abs(tapply(routes$conditional_probability,routes$event_id,sum)-1)<1e-12))
# Independent check using the actual cladoRcpp split-weight constructor.
# Each call leaves only one source-target matrix entry active, then extracts
# the raw AB -> (AB, CD) contribution before ancestor-row normalization.
ranges <- rcpp_areas_list_to_states_list(areas=LETTERS[1:4], maxareas=4, include_null_range=FALSE)
bits <- vapply(ranges,function(x) sum(2^x),0.0)
ab <- which(bits==3)-1L; cd <- which(bits==12)-1L
split_weight <- function(matrix, j=1, size_weight=1) {
  sizes <- matrix(0,4,4); sizes[2,2] <- size_weight
  coo <- rcpp_calc_anclikes_sp_COOweights_faster(
    Rcpp_leftprobs=rep(1,length(ranges)), Rcpp_rightprobs=rep(1,length(ranges)),
    l=ranges, s=0, v=0, j=j, y=0, dmat=matrix, maxent01j=sizes,
    max_minsize_as_function_of_ancsize=rep(4L,4), printmat=FALSE)
  take <- coo[[1]]==ab & coo[[2]]==ab & coo[[3]]==cd
  sum(coo[[4]][take])
}
pairs <- data.frame(source=c(1,1,2,2), target=c(3,4,3,4))
checks <- list()
set.seed(20260930)
for (q in 0:1) {
  mat <- matrices[[q+1L]]
  weights <- mat[cbind(pairs$source,pairs$target)]
  p <- weights/sum(weights)
  for (scaling in c(1,0.2)) {
    pieces <- vapply(seq_len(nrow(pairs)),function(i) {
      isolated <- matrix(0,4,4)
      isolated[pairs$source[i],pairs$target[i]] <- weights[i]
      split_weight(isolated,j=0.5,size_weight=scaling)
    },0.0)
    whole <- split_weight(mat,j=0.5,size_weight=scaling)
    stopifnot(abs(sum(pieces)-whole) < 2e-7, max(abs(pieces/sum(pieces)-p)) < 2e-7)
    checks[[length(checks)+1L]] <- data.frame(
      q_index=q,size_weight=scaling,raw_split_weight=whole,
      sum_component_weights=sum(pieces),max_probability_difference=max(abs(pieces/sum(pieces)-p)))
  }
  # R's original source helper has no multi-target API. This is explicitly a
  # conditional categorical draw using probabilities from its C++ split components.
  draws <- tabulate(sample.int(4,200000,replace=TRUE,prob=p),nbins=4)
  z <- abs(draws-200000*p)/sqrt(200000*p*(1-p))
  stopifnot(all(z<6))
  write.table(data.frame(source_area_index=pairs$source-1,target_area_index=pairs$target-1,
    probability=p,R_draws=draws,R_frequency=draws/200000,z_score=z),
    file.path(output,paste0("R-conditional-draws-q",q,".tsv")),sep="\t",quote=FALSE,row.names=FALSE)
}
stopifnot(split_weight(matrix(0,4,4))==0)
write.table(do.call(rbind,checks),file.path(output,"cladoRcpp-components.tsv"),sep="\t",quote=FALSE,row.names=FALSE)
report <- data.frame(key=c("status","R_version","BioGeoBEARS_version","cladoRcpp_version",
    "events","routes","multi_area_founder_events","max_R_probability_difference",
    "conditional_draws_per_period","distribution_threshold"),
  value=c("passed",getRversion() |> as.character(),as.character(packageVersion("BioGeoBEARS")),
    as.character(packageVersion("cladoRcpp")),nrow(events),nrow(routes),length(unique(multi_ids)),
    format(difference,digits=17),200000,"each_pair_less_than_6_sigma"))
write.table(report,file.path(output,"comparison.tsv"),sep="\t",quote=FALSE,row.names=FALSE)
print(report,row.names=FALSE)
