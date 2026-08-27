param(
    [string]$CliPath = "target/release/biogeo-cli.exe",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
$temporaryRoot = Join-Path ([System.IO.Path]::GetTempPath()) (
    "biogeo-tree-input-contract-" + [Guid]::NewGuid().ToString("N")
)
Push-Location $root
try {
    if (-not $SkipBuild) {
        & cargo build --release -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }

    $cli = [System.IO.Path]::GetFullPath($CliPath)
    if (-not (Test-Path -LiteralPath $cli)) {
        throw "biogeo-cli executable not found: $cli"
    }

    $fixture = "validation/fixtures/biogeobears_official/bsm_3taxa_fossil"
    $common = @(
        "--ranges", "$fixture/ranges.tsv",
        "--d", "0.1",
        "--e", "0.2",
        "--max-range-size", "3",
        "--include-null-range",
        "--ancestral-probs",
        "--split-probs"
    )

    $newickOutput = & $cli dec --tree "$fixture/tree.nwk" @common
    if ($LASTEXITCODE -ne 0) {
        throw "Newick analysis failed with exit code $LASTEXITCODE"
    }
    $nexusOutput = & $cli dec --tree "$fixture/tree_ape.nex" @common
    if ($LASTEXITCODE -ne 0) {
        throw "NEXUS analysis failed with exit code $LASTEXITCODE"
    }
    $multiNexusOutput = & $cli dec --tree "$fixture/tree_ape_multi.nex" --tree-name official @common
    if ($LASTEXITCODE -ne 0) {
        throw "named multi-tree NEXUS analysis failed with exit code $LASTEXITCODE"
    }

    $newickText = $newickOutput -join "`n"
    $nexusText = $nexusOutput -join "`n"
    $selectedTreeLine = "tree_name`tofficial"
    $selectedTreeLines = @($multiNexusOutput | Where-Object { $_ -ceq $selectedTreeLine })
    if ($selectedTreeLines.Count -ne 1) {
        throw "named multi-tree NEXUS output did not contain exactly one '$selectedTreeLine' line"
    }
    $multiNexusSemanticOutput = @(
        $multiNexusOutput | Where-Object { $_ -cne $selectedTreeLine }
    )
    $multiNexusText = $multiNexusSemanticOutput -join "`n"
    if ($newickText -cne $nexusText -or $newickText -cne $multiNexusText) {
        $difference = Compare-Object $newickOutput $multiNexusSemanticOutput -SyncWindow 0 |
            Select-Object -First 10 |
            Out-String
        throw "Newick and selected NEXUS semantic outputs differ:`n$difference"
    }

    $converted = & $cli convert-tree --tree "$fixture/tree_ape_multi.nex" --tree-name official
    if ($LASTEXITCODE -ne 0) {
        throw "named NEXUS conversion failed with exit code $LASTEXITCODE"
    }
    if (($converted -join "`n") -cne "((human:0.91,chimp:1):1,gorilla:2);") {
        throw "named NEXUS conversion did not emit the official canonical Newick"
    }

    $savedErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $null = & $cli convert-tree --tree "$fixture/tree_ape_multi.nex" 2>$null
        $unnamedExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $savedErrorActionPreference
    }
    if ($unnamedExitCode -eq 0) {
        throw "multi-tree NEXUS unexpectedly succeeded without --tree-name"
    }

    [System.IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
    $quotedTree = Join-Path $temporaryRoot "quoted tree.nwk"
    $quotedNexus = Join-Path $temporaryRoot "quoted tree.nex"
    $quotedRanges = Join-Path $temporaryRoot "quoted ranges.tsv"
    [System.IO.File]::WriteAllText(
        $quotedTree,
        "('Taxon A','O''Brien');`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.File]::WriteAllText(
        $quotedNexus,
        ([char]0xfeff) + "#nExUs`nBEGIN TAXA; DIMENSIONS NTAX=2; END;`n" +
            "begin trees; [producer[metadata]] translate 1 'Taxon A', 2 'O''Brien'; " +
            "tree * 'analysis tree' = [&R] (1,2); endblock;`n",
        [System.Text.UTF8Encoding]::new($false)
    )
    [System.IO.File]::WriteAllText(
        $quotedRanges,
        "tip`tArea A`tArea B`nTaxon A`t1`t0`nO'Brien`t0`t1`n",
        [System.Text.UTF8Encoding]::new($false)
    )

    $savedErrorActionPreference = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $null = & $cli convert-tree --tree $quotedTree 2>$null
        $missingLengthExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $savedErrorActionPreference
    }
    if ($missingLengthExitCode -ne 2) {
        throw "missing branch lengths should fail with exit code 2 unless fill is explicit"
    }

    $quotedNewickOutput = & $cli convert-tree --tree $quotedTree `
        --fill-missing-branch-length 0.25
    if ($LASTEXITCODE -ne 0) {
        throw "explicit Newick branch-length fill failed with exit code $LASTEXITCODE"
    }
    $quotedNexusOutput = & $cli convert-tree --tree $quotedNexus `
        --tree-name "analysis tree" --fill-missing-branch-length 0.25
    if ($LASTEXITCODE -ne 0) {
        throw "explicit NEXUS branch-length fill failed with exit code $LASTEXITCODE"
    }
    $expectedQuotedTree = "('Taxon A':0.25,'O''Brien':0.25);"
    if (($quotedNewickOutput -join "`n") -cne $expectedQuotedTree -or
        ($quotedNexusOutput -join "`n") -cne $expectedQuotedTree) {
        throw "quoted Newick and translated NEXUS did not produce the same filled tree"
    }

    $validationOutput = & $cli validate-inputs --tree $quotedTree --ranges $quotedRanges `
        --fill-missing-branch-length 0.25
    if ($LASTEXITCODE -ne 0) {
        throw "quoted-label validation failed with exit code $LASTEXITCODE"
    }
    $validationText = $validationOutput -join "`n"
    if ($validationText -notmatch "(?m)^missing_branch_length_fill`t0\.25000000000000000$" -or
        $validationText -notmatch "(?m)^minimum_branch_length`t0\.25000000000000000$") {
        throw "validation did not preserve the explicit missing-branch-length policy"
    }

    $lnL = $newickOutput | Where-Object { $_ -like "lnL`t*" } | Select-Object -First 1
    [pscustomobject]@{
        fixture = "BioGeoBEARS BSM_3taxa/M3areas_allowed_wFossilBranch"
        exporter = "ape::write.nexus"
        semantic_lines_compared_per_format = $newickOutput.Count
        compared_inputs = 3
        selected_tree = "official"
        identical = $true
        strict_missing_length_rejected = $true
        explicit_fill_equivalent = $true
        quoted_space_labels_validated = $true
        lnL = ($lnL -split "`t", 2)[1]
    } | Format-List
}
finally {
    if (Test-Path -LiteralPath $temporaryRoot -PathType Container) {
        [System.IO.Directory]::Delete($temporaryRoot, $true)
    }
    Pop-Location
}
