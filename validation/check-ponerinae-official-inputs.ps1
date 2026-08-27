param(
    [Parameter(Mandatory = $true)]
    [ValidateNotNullOrEmpty()]
    [string]$DatasetDir,

    [string]$CliPath = "target/release/biogeo-cli.exe",

    [switch]$KeepRun
)

$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

function Resolve-RepoPath {
    param([string]$Path)
    if ([System.IO.Path]::IsPathRooted($Path)) {
        return [System.IO.Path]::GetFullPath($Path)
    }
    return [System.IO.Path]::GetFullPath((Join-Path $repoRoot $Path))
}

function Require-File {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
        throw "Required Ponerinae input does not exist: $Path"
    }
}

function Invoke-Biogeo {
    param([string[]]$Arguments)
    $output = & $cli @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "biogeo-cli failed with exit code ${LASTEXITCODE}: $($Arguments -join ' ')"
    }
    return $output -join "`n"
}

function Read-Field {
    param(
        [string]$Text,
        [string]$Name
    )
    $prefix = "$Name`t"
    $line = $Text -split "`r?`n" | Where-Object { $_.StartsWith($prefix) } | Select-Object -First 1
    if ($null -eq $line) {
        throw "CLI output is missing field $Name"
    }
    return $line.Substring($prefix.Length)
}

$cli = Resolve-RepoPath $CliPath
$dataset = [System.IO.Path]::GetFullPath($DatasetDir)
$finalInputs = Join-Path $dataset "final_inputs"
$tree = Join-Path $finalInputs "Ponerinae_MCC_phylogeny_1534t_short_names.tree"
$dataRanges = Join-Path $finalInputs "lagrange_area_data_file_7_regions_PaleA.data"
$csvRanges = Join-Path $dataset "taxa_bioregions_7areas_matrix.csv"
$boundaries = Join-Path $finalInputs "time_boundaries.txt"
$adjacency = Join-Path $finalInputs "Dore_2024_BioGeoBears_Adjacency_matrix_7areas_7TS.txt"
$taxonMap = Join-Path $repoRoot "validation/reference/ponerinae-short-tree-taxon-map.tsv"
$areaMap = Join-Path $repoRoot "validation/reference/ponerinae-area-map.tsv"

foreach ($path in @($cli, $tree, $dataRanges, $csvRanges, $boundaries, $adjacency, $taxonMap, $areaMap)) {
    Require-File $path
}

$runsRoot = Join-Path $repoRoot "validation/benchmark-runs"
New-Item -ItemType Directory -Force -Path $runsRoot | Out-Null
$runRoot = Join-Path $runsRoot ("ponerinae-official-input-check-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $runRoot | Out-Null

try {
    $strataDir = Join-Path $runRoot "strata"
    $import = Invoke-Biogeo @(
        "convert-biogeobears-strata",
        "--time-boundaries", $boundaries,
        "--adjacency-matrices", $adjacency,
        "--adjacency-range-rule", "edge-covered",
        "--max-range-size", "5",
        "--output-dir", $strataDir
    )
    $counts = Read-Field $import "allowed_range_counts"
    if ($counts -ne "36,36,27,20,24,20,38") {
        throw "Ponerinae allowed-state counts drifted: $counts"
    }

    $canonicalRanges = Join-Path $runRoot "ranges-short-names.tsv"
    $converted = Invoke-Biogeo @(
        "convert-ranges",
        "--ranges", $csvRanges,
        "--taxon-map", $taxonMap,
        "--area-map", $areaMap
    )
    [System.IO.File]::WriteAllText(
        $canonicalRanges,
        $converted + "`n",
        [System.Text.UTF8Encoding]::new($false)
    )

    $validation = Invoke-Biogeo @(
        "validate-inputs",
        "--tree", $tree,
        "--ranges", $canonicalRanges
    )
    foreach ($expected in @{
            tips = "1534"
            areas = "7"
            range_rows = "1534"
            maximum_observed_range_size = "3"
            binary = "true"
            ultrametric = "true"
        }.GetEnumerator()) {
        $actual = Read-Field $validation $expected.Key
        if ($actual -ne $expected.Value) {
            throw "Ponerinae $($expected.Key) drifted: expected $($expected.Value), got $actual"
        }
    }

    $common = @(
        "--tree", $tree,
        "--d", "0.01",
        "--e", "0.01",
        "--max-range-size", "5",
        "--include-null-range",
        "--dispersal-strata", (Join-Path $strataDir "strata.tsv")
    )
    $fromCsv = Invoke-Biogeo (@("dec", "--ranges", $canonicalRanges) + $common)
    $fromData = Invoke-Biogeo (@("dec", "--ranges", $dataRanges) + $common)
    $csvLnL = Read-Field $fromCsv "lnL"
    $dataLnL = Read-Field $fromData "lnL"
    if ($csvLnL -ne $dataLnL) {
        throw "Canonical CSV and BioGeoBEARS .data paths disagree: $csvLnL versus $dataLnL"
    }
    if ($csvLnL -ne "-3279.174634278399026") {
        throw "Ponerinae fixed DEC regression drifted: $csvLnL"
    }

    Write-Output "format`tbiogeo-ponerinae-input-check-v1"
    Write-Output "status`tpassed"
    Write-Output "tips`t1534"
    Write-Output "areas`t7"
    Write-Output "states`t120"
    Write-Output "strata`t7"
    Write-Output "allowed_range_counts`t$counts"
    Write-Output "lnL`t$csvLnL"
    Write-Output "csv_data_lnl_identical`ttrue"
    Write-Output "run_directory`t$(if ($KeepRun) { $runRoot } else { 'removed' })"
}
finally {
    if (-not $KeepRun -and (Test-Path -LiteralPath $runRoot)) {
        $resolvedRuns = [System.IO.Path]::GetFullPath($runsRoot).TrimEnd('\') + '\'
        $resolvedRun = [System.IO.Path]::GetFullPath($runRoot)
        if (-not $resolvedRun.StartsWith($resolvedRuns, [System.StringComparison]::OrdinalIgnoreCase)) {
            throw "Refusing to remove validation directory outside benchmark-runs: $resolvedRun"
        }
        Remove-Item -LiteralPath $resolvedRun -Recurse -Force
    }
}
