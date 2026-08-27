source("validation/biogeobears/r-env.R")

env <- configure_project_r()
cat("Repository root: ", env$repo_root, "\n", sep = "")
cat("Project R library: ", env$local_lib, "\n", sep = "")
cat("R library search path:\n")
cat(paste0("  - ", .libPaths(), collapse = "\n"), "\n", sep = "")

cran_packages <- c(
  "remotes",
  "ape",
  "optimx",
  "plotrix",
  "gdata",
  "GenSA",
  "rexpokit",
  "cladoRcpp",
  "phylobase",
  "phytools",
  "devtools",
  "FD",
  "minqa",
  "expm",
  "fdrtool",
  "httr",
  "statmod",
  "SparseM",
  "spam",
  "stringr"
)

installed <- rownames(installed.packages(lib.loc = env$local_lib))
missing_cran <- setdiff(cran_packages, installed)

if (length(missing_cran) > 0) {
  cat("Installing CRAN packages into project library:\n")
  cat(paste0("  - ", missing_cran, collapse = "\n"), "\n", sep = "")
  install.packages(missing_cran, lib = env$local_lib, dependencies = TRUE)
} else {
  cat("All CRAN package dependencies are already present in the project library.\n")
}

install_archive_package <- function(package, version) {
  if (requireNamespace(package, quietly = TRUE)) {
    return(invisible(TRUE))
  }

  url <- sprintf(
    "https://cran.r-project.org/src/contrib/Archive/%s/%s_%s.tar.gz",
    package,
    package,
    version
  )
  cat("Installing archived CRAN package ", package, " from ", url, "\n", sep = "")
  install.packages(url, lib = env$local_lib, repos = NULL, type = "source")
  if (!requireNamespace(package, quietly = TRUE)) {
    stop("Failed to install archived package: ", package, call. = FALSE)
  }
}

install_archive_package("MultinomialCI", "1.2")

if (!requireNamespace("BioGeoBEARS", quietly = TRUE)) {
  source_dir <- file.path(env$cache_dir, "BioGeoBEARS-src")
  if (dir.exists(file.path(source_dir, ".git"))) {
    cat("Updating cached BioGeoBEARS source at ", source_dir, "\n", sep = "")
    status <- system2("git", c("-C", shQuote(source_dir), "pull", "--ff-only"))
  } else {
    cat("Cloning BioGeoBEARS source into ", source_dir, "\n", sep = "")
    status <- system2(
      "git",
      c("clone", "https://github.com/nmatzke/BioGeoBEARS.git", shQuote(source_dir))
    )
  }
  if (!identical(status, 0L)) {
    stop("Failed to fetch BioGeoBEARS source with git", call. = FALSE)
  }

  cat("Installing BioGeoBEARS from cached source into project library.\n")
  r_bin <- file.path(R.home("bin"), "R")
  status <- system2(
    r_bin,
    c(
      "CMD",
      "INSTALL",
      "--no-multiarch",
      "--with-keep.source",
      "-l",
      shQuote(env$local_lib),
      shQuote(source_dir)
    )
  )
  if (!identical(status, 0L)) {
    stop("Failed to install BioGeoBEARS from cached source", call. = FALSE)
  }
} else {
  cat("BioGeoBEARS is already present in the project library.\n")
}

required_packages <- c("ape", "rexpokit", "cladoRcpp", "BioGeoBEARS")
missing_required <- required_packages[
  !vapply(required_packages, requireNamespace, logical(1), quietly = TRUE)
]
if (length(missing_required) > 0) {
  stop(
    "Local BioGeoBEARS setup incomplete; missing: ",
    paste(missing_required, collapse = ", "),
    call. = FALSE
  )
}

cat("Local BioGeoBEARS setup complete.\n")
cat("BioGeoBEARS version: ", as.character(packageVersion("BioGeoBEARS")), "\n", sep = "")
