$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $scriptRoot
$distRoot = Join-Path $root "dist\math-booklet"
$pandoc = "E:\Anaconda3\Scripts\pandoc.exe"
$python = "C:\Users\$env:USERNAME\.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe"
$markdown = Join-Path $distRoot "math-booklet.md"
$rawDocx = Join-Path $distRoot "math-booklet-wps.raw.docx"
$outputDocx = Join-Path $distRoot "math-booklet-wps.docx"
$styler = Join-Path $scriptRoot "build_wps_docx.py"

foreach ($path in @($pandoc, $python, $markdown, $styler)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing WPS build dependency: $path"
    }
}

& $pandoc `
    $markdown `
    --from=markdown+tex_math_single_backslash `
    --to=docx `
    --toc `
    --toc-depth=3 `
    --metadata=lang:zh-CN `
    --output=$rawDocx

if ($LASTEXITCODE -ne 0) {
    throw "Pandoc DOCX build failed with exit code $LASTEXITCODE"
}

& $python $styler $rawDocx $outputDocx
if ($LASTEXITCODE -ne 0) {
    throw "WPS DOCX styling failed with exit code $LASTEXITCODE"
}

Remove-Item -LiteralPath $rawDocx -Force
Get-Item -LiteralPath $outputDocx | Select-Object Name, Length, LastWriteTime
