param(
    [string]$Source = "E:\RASP\engines\lagrange-ng",
    [string]$Destination = "validation/tools/lagrange-ng"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..\..")
$sourcePath = Resolve-Path -LiteralPath $Source
$destinationPath = Join-Path $repoRoot $Destination

$sourceExe = Join-Path $sourcePath "lagrange-ng.exe"
if (-not (Test-Path -LiteralPath $sourceExe -PathType Leaf)) {
    throw "lagrange-ng.exe was not found in $sourcePath"
}

New-Item -ItemType Directory -Force -Path $destinationPath | Out-Null

Get-ChildItem -LiteralPath $sourcePath -Force | ForEach-Object {
    Copy-Item -LiteralPath $_.FullName -Destination $destinationPath -Recurse -Force
}

$destinationExe = Join-Path $destinationPath "lagrange-ng.exe"
if (-not (Test-Path -LiteralPath $destinationExe -PathType Leaf)) {
    throw "Copied lagrange-ng.exe was not found in $destinationPath"
}

Write-Host "Copied lagrange-ng to $destinationPath"
& $destinationExe --help
if ($LASTEXITCODE -ne 0) {
    Write-Host "lagrange-ng --help exited with code $LASTEXITCODE; copy still completed."
}
