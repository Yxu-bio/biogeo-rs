param(
    [Parameter(Mandatory=$true)][string]$AuditRoot,
    [Parameter(Mandatory=$true)][string]$OutputDirectory,
    [string]$Exe = 'target/release/biogeo-cli.exe',
    [switch]$IncludeRosa
)
$ErrorActionPreference = 'Stop'
$exePath = (Resolve-Path -LiteralPath $Exe).Path
if (Test-Path -LiteralPath $OutputDirectory) { throw 'Use a new output directory.' }
$out = (New-Item -ItemType Directory -Path $OutputDirectory).FullName
function Read-Metadata([string]$Path) {
    $values = @{}
    Import-Csv -LiteralPath $Path -Delimiter "`t" | ForEach-Object { $values[$_.key] = $_.value }
    return $values
}
$rows = foreach ($case in Get-ChildItem -LiteralPath (Join-Path $AuditRoot 'fits') -Directory) {
    if ($case.Name -notmatch '^(toy|psychotria)_' -and !($IncludeRosa -and $case.Name.StartsWith('rosa_'))) { continue }
    $inputDir = Join-Path $case.FullName 'rust/biogeo-rs-single'
    $request = Join-Path $inputDir 'analysis.tsv'
    if (!(Test-Path -LiteralPath $request)) { continue }
    $dest = Join-Path $out $case.Name
    $watch = [Diagnostics.Stopwatch]::StartNew()
    & $exePath analysis-run --request $request --output-dir $dest > "$dest.stdout.tsv" 2> "$dest.stderr.log"
    if ($LASTEXITCODE -ne 0) { throw "Fit failed: $($case.Name); see $dest.stderr.log" }
    $elapsed = $watch.Elapsed.TotalSeconds
    & $exePath analysis-result-inspect --analysis-result $dest --replay > "$dest.inspect.tsv" 2> "$dest.inspect.stderr.log"
    if ($LASTEXITCODE -ne 0) { throw "Replay failed: $($case.Name)" }
    $meta = Read-Metadata "$dest/metadata.tsv"
    $old = Read-Metadata "$inputDir/result/metadata.tsv"
    $diagnostics = Get-Content -LiteralPath "$dest/optimization-starts.tsv" | Select-Object -Skip 3 | ConvertFrom-Csv -Delimiter "`t"
    $selected = @($diagnostics | Where-Object selected -eq 'true')[0]
    $row = [pscustomobject]@{
        case=$case.Name; seconds=$elapsed; old_lnl=$old.lnL; lnl=$meta.lnL
        delta_lnl=([double]$meta.lnL - [double]$old.lnL)
        old_converged=$old.optimization_converged; converged=$meta.optimization_converged
        iterations=$meta.optimization_iterations; evaluations=$meta.optimization_evaluations
        starts=$meta.optimization_starts; selected_start=$selected.start; stop_reason=$selected.stop_reason
        replay='passed'
    }
    Write-Host ($row | ConvertTo-Json -Compress)
    $row
}
$rows | Export-Csv -LiteralPath "$out/summary.csv" -NoTypeInformation -Encoding utf8
Get-FileHash -LiteralPath $exePath -Algorithm SHA256 | Format-List | Out-File "$out/executable-sha256.txt"
