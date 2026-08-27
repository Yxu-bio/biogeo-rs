$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $scriptRoot
$bookDist = Join-Path $root "dist\book"
$mathDist = Join-Path $root "dist\math-booklet"
$tempRoot = Join-Path `
    ([System.IO.Path]::GetTempPath()) `
    ("ancestral-reconstruction-book-pdf-" + [guid]::NewGuid().ToString("N"))

$edgeCandidates = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft\Edge\Application\msedge.exe"),
    (Join-Path $env:ProgramFiles "Microsoft\Edge\Application\msedge.exe")
)
$edge = $edgeCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1

if (-not $edge) {
    throw "Microsoft Edge was not found. Install Edge or update build_pdfs.ps1 with a Chromium executable."
}

$jobs = @(
    @{
        Html = Join-Path $bookDist "book.html"
        Pdf = Join-Path $bookDist "ancestral-reconstruction-book.pdf"
    },
    @{
        Html = Join-Path $mathDist "math-booklet.html"
        Pdf = Join-Path $mathDist "ancestral-reconstruction-math-booklet.pdf"
    }
)

New-Item -ItemType Directory -Force -Path $bookDist, $mathDist, $tempRoot | Out-Null

foreach ($job in $jobs) {
    if (-not (Test-Path -LiteralPath $job.Html)) {
        throw "Missing HTML source: $($job.Html)"
    }

    $profile = Join-Path $tempRoot ([guid]::NewGuid().ToString("N"))
    $stdout = Join-Path $tempRoot ((Split-Path -Leaf $job.Pdf) + ".stdout.log")
    $stderr = Join-Path $tempRoot ((Split-Path -Leaf $job.Pdf) + ".stderr.log")
    $url = ([System.Uri]::new((Resolve-Path -LiteralPath $job.Html).Path)).AbsoluteUri
    $arguments = @(
        "--headless=new",
        "--disable-gpu",
        "--disable-extensions",
        "--no-first-run",
        "--no-default-browser-check",
        "--no-pdf-header-footer",
        ("--user-data-dir=`"{0}`"" -f $profile),
        ("--print-to-pdf=`"{0}`"" -f $job.Pdf),
        ("`"{0}`"" -f $url)
    )

    try {
        $process = Start-Process `
            -FilePath $edge `
            -ArgumentList $arguments `
            -WindowStyle Hidden `
            -Wait `
            -PassThru `
            -RedirectStandardOutput $stdout `
            -RedirectStandardError $stderr

        if ($process.ExitCode -ne 0) {
            throw "Edge PDF build failed with exit code $($process.ExitCode): $($job.Pdf)"
        }
        if (-not (Test-Path -LiteralPath $job.Pdf)) {
            throw "Edge exited successfully but did not create: $($job.Pdf)"
        }
    }
    finally {
        $resolvedTempRoot = [System.IO.Path]::GetFullPath($tempRoot)
        $resolvedProfile = [System.IO.Path]::GetFullPath($profile)
        if ($resolvedProfile.StartsWith($resolvedTempRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
            Remove-Item -LiteralPath $profile -Recurse -Force -ErrorAction SilentlyContinue
        }
    }
}

Get-Item -LiteralPath ($jobs | ForEach-Object { $_.Pdf }) |
    Select-Object Name, Length, LastWriteTime
