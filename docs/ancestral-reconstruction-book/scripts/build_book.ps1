$ErrorActionPreference = "Stop"

$scriptRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $scriptRoot
$sourceRoot = Join-Path $root "src\book"
$assetRoot = Join-Path $root "assets"
$distRoot = Join-Path $root "dist\book"
$pandoc = "E:\Anaconda3\Scripts\pandoc.exe"

if (-not (Test-Path -LiteralPath $pandoc)) {
    throw "Pandoc not found at $pandoc"
}

$chapters = @(
    "00-preface.md",
    "01-question-map.md",
    "02-history.md",
    "03-probability-engine.md",
    "03-math-companion.md",
    "04-discrete-traits.md",
    "05-continuous-traits.md",
    "06-historical-biogeography.md",
    "07-sse-ghost-lineages.md",
    "08-sequences-phylogeography-fossils.md",
    "09-software-atlas.md",
    "10-workflow.md",
    "11-failure-modes.md",
    "12-bgb-rust-roadmap.md",
    "glossary.md",
    "bibliography.md"
)

foreach ($chapter in $chapters) {
    $path = Join-Path $sourceRoot $chapter
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing chapter: $path"
    }
}

$readmeLines = Get-Content -LiteralPath (Join-Path $root "README.md") -Encoding UTF8
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

$parts = [System.Collections.Generic.List[string]]::new()
$parts.Add($metadata.Trim())
foreach ($chapter in $chapters) {
    $parts.Add((Get-Content -LiteralPath (Join-Path $sourceRoot $chapter) -Raw -Encoding UTF8).Trim())
}

$bookMarkdown = Join-Path $distRoot "book.md"
$bookHtml = Join-Path $distRoot "book.html"
$bookEpub = Join-Path $distRoot "book.epub"
$css = Join-Path $assetRoot "book.css"
$epubCss = Join-Path $assetRoot "epub.css"
$epubMathFilter = Join-Path $assetRoot "epub_math.lua"

foreach ($path in @($css, $epubCss, $epubMathFilter)) {
    if (-not (Test-Path -LiteralPath $path)) {
        throw "Missing build asset: $path"
    }
}

New-Item -ItemType Directory -Force -Path $distRoot | Out-Null

[System.IO.File]::WriteAllText(
    $bookMarkdown,
    (($parts -join "`r`n`r`n") + "`r`n"),
    [System.Text.UTF8Encoding]::new($false)
)

& $pandoc `
    $bookMarkdown `
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
    --output=$bookHtml

if ($LASTEXITCODE -ne 0) {
    throw "Pandoc HTML build failed with exit code $LASTEXITCODE"
}

Push-Location $assetRoot
try {
    & $pandoc `
        $bookMarkdown `
        --from=markdown+tex_math_single_backslash `
        --to=epub3 `
        --toc `
        --toc-depth=3 `
        --epub-chapter-level=1 `
        --css=$epubCss `
        --lua-filter="epub_math.lua" `
        --metadata=lang:zh-CN `
        --output=$bookEpub
    $epubExitCode = $LASTEXITCODE
}
finally {
    Pop-Location
}

if ($epubExitCode -ne 0) {
    throw "Pandoc EPUB build failed with exit code $epubExitCode"
}

Get-Item -LiteralPath $bookMarkdown, $bookHtml, $bookEpub |
    Select-Object Name, Length, LastWriteTime
