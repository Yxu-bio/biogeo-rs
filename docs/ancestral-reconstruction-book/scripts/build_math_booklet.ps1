$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$bookRoot = Split-Path -Parent $scriptRoot
$sourceRoot = Join-Path $bookRoot "src\math-booklet"
$bookSourceRoot = Join-Path $bookRoot "src\book"
$assetRoot = Join-Path $bookRoot "assets"
$distRoot = Join-Path $bookRoot "dist\math-booklet"
$pandoc = "E:\Anaconda3\Scripts\pandoc.exe"

if (-not (Test-Path -LiteralPath $pandoc)) {
    throw "Pandoc not found at $pandoc"
}

$readme = Join-Path $sourceRoot "README.md"
$preface = Join-Path $sourceRoot "preface.md"
$core = Join-Path $bookSourceRoot "03-math-companion.md"
$selectedReading = Join-Path $sourceRoot "selected-reading.md"
$css = Join-Path $assetRoot "book.css"
$epubCss = Join-Path $assetRoot "epub.css"
$epubMathFilter = Join-Path $assetRoot "epub_math.lua"

foreach ($path in @($readme, $preface, $core, $selectedReading, $css, $epubCss, $epubMathFilter)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing booklet source: $path"
    }
}

$readmeLines = Get-Content -LiteralPath $readme -Encoding UTF8
$title = $readmeLines[0] -replace '^#\s*', ''
$subtitle = $readmeLines[2] -replace '^##\s*', ''
$versionText = $readmeLines[4].Trim().Trim('*').Trim()
$researchDateText = $readmeLines[5].Trim().Trim('*').Trim()
$metadata = @(
    '---',
    ('title: "{0}"' -f $title),
    ('subtitle: "{0}"' -f $subtitle),
    ('date: "{0} / {1}"' -f $versionText, $researchDateText),
    'lang: "zh-CN"',
    '---'
) -join "`n"

$coreText = Get-Content -LiteralPath $core -Raw -Encoding UTF8
$firstLineBreak = $coreText.IndexOf("`n")
if ($firstLineBreak -lt 0) {
    throw "Core chapter has no body: $core"
}

$coreHeading = $coreText.Substring(0, $firstLineBreak).Trim()
$coreBody = $coreText.Substring($firstLineBreak + 1).Trim()
$parts = @(
    $metadata.Trim(),
    $coreHeading,
    (Get-Content -LiteralPath $preface -Raw -Encoding UTF8).Trim(),
    $coreBody,
    (Get-Content -LiteralPath $selectedReading -Raw -Encoding UTF8).Trim()
)

$bookletMarkdown = Join-Path $distRoot "math-booklet.md"
$bookletHtml = Join-Path $distRoot "math-booklet.html"
$bookletEpub = Join-Path $distRoot "math-booklet.epub"

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

[System.IO.File]::WriteAllText(
    $bookletMarkdown,
    (($parts -join "`r`n`r`n") + "`r`n"),
    [System.Text.UTF8Encoding]::new($false)
)

& $pandoc `
    $bookletMarkdown `
    --from=markdown+tex_math_single_backslash `
    --to=html5 `
    --standalone `
    --section-divs `
    --toc `
    --toc-depth=3 `
    --mathml `
    --self-contained `
    --css=$css `
    --metadata=lang:zh-CN `
    --output=$bookletHtml

if ($LASTEXITCODE -ne 0) {
    throw "Pandoc HTML build failed with exit code $LASTEXITCODE"
}

Push-Location $assetRoot
try {
    & $pandoc `
        $bookletMarkdown `
        --from=markdown+tex_math_single_backslash `
        --to=epub3 `
        --toc `
        --toc-depth=3 `
        --epub-chapter-level=1 `
        --css=$epubCss `
        --lua-filter="epub_math.lua" `
        --metadata=lang:zh-CN `
        --output=$bookletEpub
    $epubExitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}

if ($epubExitCode -ne 0) {
    throw "Pandoc EPUB build failed with exit code $epubExitCode"
}

Get-Item -LiteralPath $bookletMarkdown, $bookletHtml, $bookletEpub |
    Select-Object Name, Length, LastWriteTime
