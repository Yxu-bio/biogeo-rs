find_repo_root <- function(start = getwd()) {
  path <- normalizePath(start, winslash = "/", mustWork = TRUE)
  repeat {
    if (file.exists(file.path(path, "Cargo.toml")) && dir.exists(file.path(path, "validation"))) {
      return(path)
    }

    parent <- dirname(path)
    if (identical(parent, path)) {
      stop("Could not locate repository root from: ", start, call. = FALSE)
    }
    path <- parent
  }
}

project_r_minor <- function() {
  minor <- strsplit(R.version$minor, ".", fixed = TRUE)[[1]][[1]]
  paste0("R-", R.version$major, ".", minor)
}

configure_project_r <- function(repo_root = find_repo_root(), create = TRUE) {
  repo_root <- normalizePath(repo_root, winslash = "/", mustWork = TRUE)
  local_lib <- file.path(repo_root, "validation", "r-lib", project_r_minor())
  cache_dir <- file.path(repo_root, "validation", "r-cache")

  if (create) {
    dir.create(local_lib, recursive = TRUE, showWarnings = FALSE)
    dir.create(cache_dir, recursive = TRUE, showWarnings = FALSE)
  }

  base_lib <- normalizePath(file.path(R.home(), "library"), winslash = "/", mustWork = TRUE)
  .libPaths(c(local_lib, base_lib))
  Sys.setenv(
    R_LIBS_USER = local_lib,
    R_USER_CACHE_DIR = cache_dir,
    R_REMOTES_NO_ERRORS_FROM_WARNINGS = "true"
  )
  options(repos = c(CRAN = "https://cloud.r-project.org"))

  invisible(list(
    repo_root = repo_root,
    local_lib = normalizePath(local_lib, winslash = "/", mustWork = FALSE),
    cache_dir = normalizePath(cache_dir, winslash = "/", mustWork = FALSE),
    lib_paths = .libPaths()
  ))
}
