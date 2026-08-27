param(
    [string]$ProjectCopy = "validation/tools/lagrange-ng/lagrange-ng.exe",
    [string]$Fallback = "E:\RASP\engines\lagrange-ng\lagrange-ng.exe"
)

$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$projectExe = Join-Path $repoRoot $ProjectCopy

if (Test-Path -LiteralPath $projectExe -PathType Leaf) {
    Write-Output (Resolve-Path -LiteralPath $projectExe)
    exit 0
}

if (Test-Path -LiteralPath $Fallback -PathType Leaf) {
    Write-Output (Resolve-Path -LiteralPath $Fallback)
    exit 0
}

throw "lagrange-ng.exe was not found at project copy or fallback path"
