[CmdletBinding()]
param(
    [string]$Evidence = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$culture = [Globalization.CultureInfo]::InvariantCulture
$repoRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot "..\.."))

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

function Read-KeyValueTable {
    param([Parameter(Mandatory = $true)][string]$Path)

    $rows = @(Import-Csv -LiteralPath $Path -Delimiter "`t")
    $values = @{}
    foreach ($row in $rows) {
        if ([string]::IsNullOrWhiteSpace($row.key) -or $values.ContainsKey($row.key)) {
            throw "Invalid or duplicate key in $Path"
        }
        $values[$row.key] = [string]$row.value
    }
    return $values
}

function Invoke-CargoGate {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    Write-Host "`n== $Name =="
    & cargo @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed with exit code $LASTEXITCODE"
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "The v0.1 release-candidate gate currently requires Windows."
}

$releaseStatus = Read-KeyValueTable -Path (Join-Path $repoRoot "release-status.tsv")
if ($releaseStatus["format"] -ne "biogeo-release-status-v1" -or
    $releaseStatus["status"] -ne "complete" -or
    $releaseStatus["release_class"] -ne "public_research_release_candidate" -or
    $releaseStatus["project_license_status"] -ne "GPL-3.0-or-later" -or
    $releaseStatus["public_distribution_allowed"] -ne "true") {
    throw "The v0.1 check expects the current public GPL research-release declaration."
}

if ([string]::IsNullOrWhiteSpace($Evidence)) {
    $stamp = [DateTime]::UtcNow.ToString("yyyyMMddTHHmmssZ", $culture)
    $evidencePath = Join-Path $repoRoot "validation\benchmark-runs\v0.1-release-candidate-$stamp-$PID.tsv"
}
elseif ([IO.Path]::IsPathRooted($Evidence)) {
    $evidencePath = [IO.Path]::GetFullPath($Evidence)
}
else {
    $evidencePath = [IO.Path]::GetFullPath((Join-Path $repoRoot $Evidence))
}
if (Test-Path -LiteralPath $evidencePath) {
    throw "Release-candidate evidence already exists and will not be overwritten: $evidencePath"
}
[IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($evidencePath)) | Out-Null

$started = [DateTime]::UtcNow
$stopwatch = [Diagnostics.Stopwatch]::StartNew()
Push-Location $repoRoot
try {
    Invoke-CargoGate -Name "Rust format" -Arguments @("fmt", "--all", "--", "--check")
    Invoke-CargoGate `
        -Name "Rust Clippy" `
        -Arguments @("clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings")

    Write-Host "`n== Full scientific and framework semantics =="
    & (Join-Path $PSScriptRoot "check-framework-semantics.ps1")

    Write-Host "`n== Locked Windows package, install, workflows, and RASP host =="
    & (Join-Path $PSScriptRoot "check-windows-release.ps1")
}
finally {
    Pop-Location
    $stopwatch.Stop()
}

$metadata = ((& cargo metadata --no-deps --format-version 1 --locked | Out-String) | ConvertFrom-Json)
if ($LASTEXITCODE -ne 0) {
    throw "cargo metadata failed while writing release-candidate evidence."
}
$cliPackages = @($metadata.packages | Where-Object { $_.name -eq "biogeo-cli" })
if ($cliPackages.Count -ne 1) {
    throw "Could not identify exactly one biogeo-cli package for release evidence."
}
$rustcInfo = @(& rustc -vV)
if ($LASTEXITCODE -ne 0) {
    throw "rustc -vV failed while writing release-candidate evidence."
}
$cargoVersion = @(& cargo -V)
if ($LASTEXITCODE -ne 0 -or $cargoVersion.Count -ne 1) {
    throw "cargo -V failed while writing release-candidate evidence."
}
$target = [string](@($rustcInfo | Where-Object { $_ -like "host: *" })[0]).Substring("host: ".Length)
$lockHash = (Get-FileHash -LiteralPath (Join-Path $repoRoot "Cargo.lock") -Algorithm SHA256).Hash.ToLowerInvariant()
$completed = [DateTime]::UtcNow
$evidenceText = @(
    "key`tvalue",
    "format`tbiogeo-v0.1-release-candidate-evidence-v2",
    "status`tpassed",
    "version`t$([string]$cliPackages[0].version)",
    "release_class`t$($releaseStatus['release_class'])",
    "project_license_status`t$($releaseStatus['project_license_status'])",
    "public_distribution_allowed`t$($releaseStatus['public_distribution_allowed'])",
    "target`t$target",
    "rustc_version`t$([string]$rustcInfo[0])",
    "cargo_version`t$([string]$cargoVersion[0])",
    "cargo_lock_sha256`t$lockHash",
    "cargo_locked_tests`tpassed",
    "clippy_warnings_denied`tpassed",
    "framework_semantics`tpassed",
    "windows_package_install_workflows`tpassed",
    "windows_research_package_contract`tpassed",
    "windows_pc_stability_smoke`tpassed",
    "storage_full_checkpoint_recovery`tpassed",
    "started_utc`t$($started.ToString('o', $culture))",
    "completed_utc`t$($completed.ToString('o', $culture))",
    "elapsed_seconds`t$($stopwatch.Elapsed.TotalSeconds.ToString('R', $culture))"
) -join "`n"
Write-Utf8NoBom -Path $evidencePath -Text "$evidenceText`n"

Write-Output "format`tbiogeo-v0.1-release-candidate-check-v2"
Write-Output "status`tpassed"
Write-Output "version`t$([string]$cliPackages[0].version)"
Write-Output "release_class`t$($releaseStatus['release_class'])"
Write-Output "public_distribution_allowed`t$($releaseStatus['public_distribution_allowed'])"
Write-Output "evidence`t$evidencePath"
Write-Output "elapsed_seconds`t$($stopwatch.Elapsed.TotalSeconds.ToString('0.###', $culture))"
