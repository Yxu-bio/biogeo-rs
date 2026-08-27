[CmdletBinding()]
param(
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Assert-LastExitCode {
    param([Parameter(Mandatory = $true)][string]$Operation)

    if ($LASTEXITCODE -ne 0) {
        throw "$Operation failed with exit code $LASTEXITCODE"
    }
}

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Remove-ValidatedSmokeRoot {
    param(
        [Parameter(Mandatory = $true)][string]$TargetRoot,
        [Parameter(Mandatory = $true)][string]$SmokeRoot
    )

    $targetFull = [System.IO.Path]::GetFullPath($TargetRoot).TrimEnd('\', '/')
    $smokeFull = [System.IO.Path]::GetFullPath($SmokeRoot).TrimEnd('\', '/')
    $prefix = $targetFull + [System.IO.Path]::DirectorySeparatorChar
    $name = [System.IO.Path]::GetFileName($smokeFull)
    if (-not $smokeFull.StartsWith($prefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        $name -notmatch "^windows-release-smoke-[0-9]+-[0-9a-f]+$") {
        throw "Refusing to remove unvalidated smoke path: $smokeFull"
    }
    if (Test-Path -LiteralPath $smokeFull) {
        Remove-Item -LiteralPath $smokeFull -Recurse -Force
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "Windows release validation must run on Windows."
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$targetRoot = Join-Path $repoRoot "target"
[System.IO.Directory]::CreateDirectory($targetRoot) | Out-Null
$smokeRoot = Join-Path $targetRoot "windows-release-smoke-$PID-$([Guid]::NewGuid().ToString('N'))"
$packageOutput = Join-Path $smokeRoot "packages"
$expandedRoot = Join-Path $smokeRoot "expanded"
$installRoot = Join-Path $smokeRoot "installed"
$analysisResult = Join-Path $smokeRoot "analysis-result"
$analysisWorkflow = Join-Path $smokeRoot "analysis-workflow"
$modelWorkflow = Join-Path $smokeRoot "model-workflow"
$publicExamplesOutput = Join-Path $smokeRoot "public-examples"
$realDataWorkflowOutput = Join-Path $smokeRoot "real-data-workflows"
$presetModifierOutput = Join-Path $smokeRoot "preset-modifier-matrix"
$stabilityOutput = Join-Path $smokeRoot "pc-stability-smoke"
$stabilityLowDiskOutput = Join-Path $smokeRoot "pc-stability-low-disk"
$analysisRequestDir = Join-Path $smokeRoot "统一 request with spaces"

try {
    [System.IO.Directory]::CreateDirectory($smokeRoot) | Out-Null
    $builder = Join-Path $repoRoot "packaging\build-windows-package.ps1"
    if ($SkipBuild) {
        & $builder -OutputDir $packageOutput -SkipBuild | Out-Host
    }
    else {
        & $builder -OutputDir $packageOutput | Out-Host
    }

    $packageDirs = @(Get-ChildItem -LiteralPath $packageOutput -Directory)
    $archives = @(Get-ChildItem -LiteralPath $packageOutput -File -Filter "*.zip")
    $checksums = @(Get-ChildItem -LiteralPath $packageOutput -File -Filter "*.zip.sha256")
    if ($packageDirs.Count -ne 1 -or $archives.Count -ne 1 -or $checksums.Count -ne 1) {
        throw "Package builder did not publish exactly one directory, ZIP, and checksum."
    }
    $duplicatePackageRejected = $false
    try {
        & $builder -OutputDir $packageOutput -SkipBuild | Out-Null
    }
    catch {
        $duplicatePackageRejected = $true
    }
    if (-not $duplicatePackageRejected) {
        throw "Package builder overwrote an existing release output."
    }
    $missingCertificateOutput = Join-Path $smokeRoot "missing-certificate-package"
    $missingCertificateRejected = $false
    try {
        & $builder `
            -OutputDir $missingCertificateOutput `
            -SkipBuild `
            -SigningCertificateThumbprint ("0" * 40) `
            -TimestampServer "https://timestamp.invalid" | Out-Null
    }
    catch {
        $missingCertificateRejected = $true
    }
    if (-not $missingCertificateRejected -or
        ((Test-Path -LiteralPath $missingCertificateOutput) -and
        @(Get-ChildItem -LiteralPath $missingCertificateOutput -Force).Count -ne 0)) {
        throw "Package builder accepted a missing signing certificate or left a partial package."
    }
    $checksumLine = [System.IO.File]::ReadAllText($checksums[0].FullName).Trim()
    $archiveHash = (Get-FileHash -LiteralPath $archives[0].FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($checksumLine -ne "$archiveHash  $($archives[0].Name)") {
        throw "ZIP SHA-256 sidecar does not match the archive."
    }

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archives[0].FullName)
    try {
        $entryNames = @($zip.Entries | ForEach-Object { $_.FullName })
        $expectedPrefix = "$($packageDirs[0].Name)/"
        if ($entryNames.Count -eq 0 -or
            @($entryNames | Where-Object { $_.Contains('\') }).Count -ne 0 -or
            @($entryNames | Where-Object {
                    -not $_.StartsWith($expectedPrefix, [System.StringComparison]::Ordinal)
                }).Count -ne 0 -or
            @($entryNames | Group-Object | Where-Object { $_.Count -ne 1 }).Count -ne 0 -or
            @($entryNames | Where-Object { $_ -eq "${expectedPrefix}biogeo-cli.exe" }).Count -ne 1 -or
            @($entryNames | Where-Object { $_ -eq "${expectedPrefix}package.tsv" }).Count -ne 1) {
            throw "ZIP entries do not satisfy the portable single-root package contract."
        }
    }
    finally {
        $zip.Dispose()
    }

    [System.IO.Directory]::CreateDirectory($expandedRoot) | Out-Null
    Expand-Archive -LiteralPath $archives[0].FullName -DestinationPath $expandedRoot
    $expandedPackages = @(Get-ChildItem -LiteralPath $expandedRoot -Directory)
    if ($expandedPackages.Count -ne 1) {
        throw "ZIP does not contain exactly one top-level package directory."
    }
    & (Join-Path $expandedPackages[0].FullName "install.ps1") -InstallDir $installRoot | Out-Host

    $pinnedUnsignedInstall = Join-Path $smokeRoot "pinned-unsigned-install"
    $pinnedUnsignedRejected = $false
    try {
        & (Join-Path $expandedPackages[0].FullName "install.ps1") `
            -InstallDir $pinnedUnsignedInstall `
            -ExpectedSignerThumbprint ("0" * 40) | Out-Null
    }
    catch {
        $pinnedUnsignedRejected = $true
    }
    if (-not $pinnedUnsignedRejected -or (Test-Path -LiteralPath $pinnedUnsignedInstall)) {
        throw "Installer accepted a publisher pin for an unsigned package."
    }

    $spoofPackage = Join-Path $smokeRoot "spoofed-signature-package"
    Copy-Item -LiteralPath $expandedPackages[0].FullName -Destination $spoofPackage -Recurse
    $spoofBuildInfoPath = Join-Path $spoofPackage "build-info.tsv"
    $spoofBuildInfo = [System.IO.File]::ReadAllText($spoofBuildInfoPath)
    $spoofBuildInfo = [regex]::Replace(
        $spoofBuildInfo,
        "(?m)^authenticode_status`tunsigned$",
        "authenticode_status`tvalid"
    )
    $spoofBuildInfo = [regex]::Replace(
        $spoofBuildInfo,
        "(?m)^authenticode_signer_thumbprint`tnone$",
        "authenticode_signer_thumbprint`t$("0" * 40)"
    )
    $spoofBuildInfo = [regex]::Replace(
        $spoofBuildInfo,
        "(?m)^authenticode_timestamp_status`tnone$",
        "authenticode_timestamp_status`tvalid"
    )
    $spoofBuildInfo = [regex]::Replace(
        $spoofBuildInfo,
        "(?m)^authenticode_timestamp_thumbprint`tnone$",
        "authenticode_timestamp_thumbprint`t$("1" * 40)"
    )
    $spoofBuildInfo = [regex]::Replace(
        $spoofBuildInfo,
        "(?m)^authenticode_timestamp_server`tnone$",
        "authenticode_timestamp_server`thttps://timestamp.invalid"
    )
    $spoofBuildInfo = [regex]::Replace(
        $spoofBuildInfo,
        "(?m)^authenticode_hash_algorithm`tnone$",
        "authenticode_hash_algorithm`tsha256"
    )
    Write-Utf8NoBom -Path $spoofBuildInfoPath -Text $spoofBuildInfo
    $spoofBuildInfoFile = Get-Item -LiteralPath $spoofBuildInfoPath
    $spoofBuildInfoHash = (Get-FileHash -LiteralPath $spoofBuildInfoPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $spoofManifestPath = Join-Path $spoofPackage "files.tsv"
    $spoofManifest = [System.IO.File]::ReadAllText($spoofManifestPath)
    $spoofManifest = [regex]::Replace(
        $spoofManifest,
        "(?m)^build-info\.tsv`t[0-9]+`t[0-9a-f]{64}$",
        "build-info.tsv`t$($spoofBuildInfoFile.Length)`t$spoofBuildInfoHash"
    )
    Write-Utf8NoBom -Path $spoofManifestPath -Text $spoofManifest
    $spoofInstall = Join-Path $smokeRoot "spoofed-signature-install"
    $spoofRejected = $false
    try {
        & (Join-Path $spoofPackage "install.ps1") `
            -InstallDir $spoofInstall `
            -ExpectedSignerThumbprint ("0" * 40) | Out-Null
    }
    catch {
        $spoofRejected = $true
    }
    if (-not $spoofRejected -or (Test-Path -LiteralPath $spoofInstall)) {
        throw "Installer trusted forged signature metadata instead of the executable signature."
    }

    $installationHash = (Get-FileHash -LiteralPath (Join-Path $installRoot "installation.tsv") `
            -Algorithm SHA256).Hash
    $duplicateInstallRejected = $false
    try {
        & (Join-Path $expandedPackages[0].FullName "install.ps1") -InstallDir $installRoot | Out-Null
    }
    catch {
        $duplicateInstallRejected = $true
    }
    if (-not $duplicateInstallRejected) {
        throw "Installer overwrote an existing installation."
    }
    $installationHashAfter = (Get-FileHash -LiteralPath (Join-Path $installRoot "installation.tsv") `
            -Algorithm SHA256).Hash
    if ($installationHashAfter -ne $installationHash) {
        throw "Rejected duplicate install modified the existing installation."
    }

    Add-Content -LiteralPath (Join-Path $expandedPackages[0].FullName "docs\windows-release.md") `
        -Value "corruption probe"
    $corruptInstall = Join-Path $smokeRoot "corrupt-install"
    $corruptionRejected = $false
    try {
        & (Join-Path $expandedPackages[0].FullName "install.ps1") -InstallDir $corruptInstall | Out-Null
    }
    catch {
        $corruptionRejected = $true
    }
    if (-not $corruptionRejected -or (Test-Path -LiteralPath $corruptInstall)) {
        throw "Installer accepted a modified payload or published a corrupt installation."
    }

    $installation = [System.IO.File]::ReadAllText((Join-Path $installRoot "installation.tsv"))
    if (-not $installation.StartsWith("key`tvalue`nformat`tbiogeo-windows-installation-v3`n") -or
        $installation -notmatch "(?m)^package_format`tbiogeo-windows-package-v3$" -or
        $installation -notmatch "(?m)^authenticode_status`tunsigned$" -or
        $installation -notmatch "(?m)^authenticode_signer_thumbprint`tnone$" -or
        $installation -notmatch "(?m)^authenticode_timestamp_status`tnone$" -or
        $installation -notmatch "(?m)^build_origin`tlocal_worktree$" -or
        $installation -notmatch "(?m)^source_revision_git_head_match`t(?:true|unavailable)$" -or
        $installation -notmatch "(?m)^release_class`tpublic_research_release_candidate$" -or
        $installation -notmatch "(?m)^project_license_status`tGPL-3\.0-or-later$" -or
        $installation -notmatch "(?m)^public_distribution_allowed`ttrue$") {
        throw "Installed directory does not declare the expected public GPL research-release contract."
    }
    $registry = [System.IO.File]::ReadAllText((Join-Path $installRoot "schemas\registry.tsv"))
    if (-not $registry.StartsWith("biogeo-schema-registry-v1`n")) {
        throw "Installed schema registry is invalid."
    }

    $releaseStatus = [System.IO.File]::ReadAllText((Join-Path $installRoot "release-status.tsv"))
    if ($releaseStatus -notmatch "(?m)^format`tbiogeo-release-status-v1$" -or
        $releaseStatus -notmatch "(?m)^version`t0\.1\.0$" -or
        $releaseStatus -notmatch "(?m)^known_limitations_declared`ttrue$" -or
        $releaseStatus -notmatch "(?m)^reproducibility_declared`ttrue$") {
        throw "Installed release-status.tsv is missing required release declarations."
    }
    $buildInfo = [System.IO.File]::ReadAllText((Join-Path $installRoot "build-info.tsv"))
    if ($buildInfo -notmatch "(?m)^format`tbiogeo-windows-build-info-v2$" -or
        $buildInfo -notmatch "(?m)^build_command`tcargo build --release --locked -p biogeo-cli$" -or
        $buildInfo -notmatch "(?m)^cargo_locked`ttrue$" -or
        $buildInfo -notmatch "(?m)^reproducibility_scope`tfunctional_not_bit_for_bit$" -or
        $buildInfo -notmatch "(?m)^build_origin`tlocal_worktree$" -or
        $buildInfo -notmatch "(?m)^source_revision_git_head_match`t(?:true|unavailable)$" -or
        $buildInfo -notmatch "(?m)^authenticode_status`tunsigned$" -or
        $buildInfo -notmatch "(?m)^authenticode_signer_thumbprint`tnone$" -or
        $buildInfo -notmatch "(?m)^authenticode_timestamp_status`tnone$" -or
        $buildInfo -notmatch "(?m)^publisher_pin_required_for_public`tfalse$" -or
        $buildInfo -notmatch "(?m)^trusted_ci_required_for_public`tfalse$" -or
        $buildInfo -notmatch "(?m)^source_manifest_sha256`t[0-9a-f]{64}$" -or
        $buildInfo -notmatch "(?m)^executable_sha256`t[0-9a-f]{64}$") {
        throw "Installed build-info.tsv is missing locked-build provenance."
    }
    foreach ($relative in @(
            "CHANGELOG.md",
            "README.md",
            "LICENSE",
            "LICENSE-STATUS.md",
            "THIRD-PARTY-NOTICES.md",
            "docs\source-and-license-audit.md",
            "docs\v0.1-release-notes.md",
            "engine-source-manifest.tsv"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $installRoot $relative) -PathType Leaf)) {
            throw "Installed research package omitted $relative."
        }
    }
    $projectLicense = [System.IO.File]::ReadAllText((Join-Path $installRoot "LICENSE"))
    if ($projectLicense -notmatch "GNU General Public License" -or
        $projectLicense -notmatch "Version 3, 29 June 2007") {
        throw "Installed LICENSE is not the expected GNU GPL version 3 text."
    }
    $thirdPartyLicenseFiles = @(
        Get-ChildItem -LiteralPath (Join-Path $installRoot "third-party-licenses") -Recurse -File
    )
    if ($thirdPartyLicenseFiles.Count -ne 37) {
        throw "Installed research package has $($thirdPartyLicenseFiles.Count) third-party license files; expected 37."
    }

    $cli = Join-Path $installRoot "biogeo-cli.exe"
    if ($installation -notmatch "(?m)^version`t([^`r`n]+)$") {
        throw "Installed metadata does not contain a version."
    }
    $installedVersion = $Matches[1]
    $versionOutput = & $cli --version
    Assert-LastExitCode -Operation "installed --version"
    if (($versionOutput -join "`n") -ne "biogeo-cli $installedVersion") {
        throw "Installed CLI version output does not match installation metadata."
    }

    $engineInfo = & $cli engine-info
    Assert-LastExitCode -Operation "installed engine-info"
    $engineInfoText = $engineInfo -join "`n"
    if ($engineInfoText -notmatch "(?m)^format`tbiogeo-engine-capabilities-v1$" -or
        $engineInfoText -notmatch "(?m)^status`tready$" -or
        $engineInfoText -notmatch "(?m)^engine_version`t$([regex]::Escape($installedVersion))$" -or
        $engineInfoText -notmatch "(?m)^compatibility_policy_version`tbiogeo-compatibility-policy-v1$" -or
        $engineInfoText -notmatch "(?m)^unknown_format_policy`treject$" -or
        $engineInfoText -notmatch "(?m)^unknown_field_policy`treject$" -or
        $engineInfoText -notmatch "(?m)^build_os`twindows$" -or
        $engineInfoText -notmatch "(?m)^build_profile`trelease$" -or
        $engineInfoText -notmatch "(?m)^supports_subcommand_help`ttrue$" -or
        $engineInfoText -notmatch "(?m)^supports_windows_process_telemetry`ttrue$") {
        throw "Installed CLI did not report the expected release capabilities."
    }
    $registryFormats = @(
        $registry -split "`n" |
            Select-Object -Skip 2 |
            Where-Object { -not [string]::IsNullOrWhiteSpace($_) } |
            ForEach-Object { ($_ -split "`t", 2)[0] } |
            Sort-Object
    )
    if ($engineInfoText -notmatch "(?m)^public_format_count`t([0-9]+)$") {
        throw "Installed engine capabilities do not contain public_format_count."
    }
    if ([int]$Matches[1] -ne $registryFormats.Count) {
        throw "Installed engine capability count does not match the schema registry."
    }
    if ($engineInfoText -notmatch "(?m)^public_formats`t([^`r`n]+)$") {
        throw "Installed engine capabilities do not contain public_formats."
    }
    $engineFormats = @($Matches[1] -split "," | Sort-Object)
    if (($engineFormats -join "`n") -ne ($registryFormats -join "`n")) {
        throw "Installed engine capabilities and schema registry format sets differ."
    }

    $bsmHelp = & $cli model-bsm --help
    Assert-LastExitCode -Operation "installed model-bsm --help"
    $bsmHelpText = $bsmHelp -join "`n"
    if ($bsmHelpText -notmatch "(?m)^Command: model-bsm$" -or
        $bsmHelpText -notmatch "--analysis-result <dir>" -or
        $bsmHelpText -notmatch "--bsm-threads <auto\|n>" -or
        $bsmHelpText -match "--tree <path>" -or
        $bsmHelpText -match "--d <rate>") {
        throw "Installed model-bsm command help is missing or not command-scoped."
    }

    $workflowHelp = & $cli analysis-workflow --help
    Assert-LastExitCode -Operation "installed analysis-workflow --help"
    $workflowHelpText = $workflowHelp -join "`n"
    if ($workflowHelpText -notmatch "(?m)^Command: analysis-workflow$" -or
        $workflowHelpText -notmatch "--resume" -or
        $workflowHelpText -match "--bsm-output-dir" -or
        $workflowHelpText -match "--bsm-resume") {
        throw "Installed analysis-workflow command help is missing or advertises owned options."
    }

    $modelWorkflowHelp = & $cli model-workflow --help
    Assert-LastExitCode -Operation "installed model-workflow --help"
    $modelWorkflowHelpText = $modelWorkflowHelp -join "`n"
    if ($modelWorkflowHelpText -notmatch "(?m)^Command: model-workflow$" -or
        $modelWorkflowHelpText -notmatch "--request <workflow.tsv>" -or
        $modelWorkflowHelpText -notmatch "--resume" -or
        $modelWorkflowHelpText -match "--tree <path>" -or
        $modelWorkflowHelpText -match "--bsm-samples") {
        throw "Installed model-workflow command help is missing or advertises request-owned options."
    }

    Copy-Item -LiteralPath (Join-Path $repoRoot "examples\analysis_request") `
        -Destination $analysisRequestDir -Recurse
    $analysisRequest = Join-Path $analysisRequestDir "analysis.tsv"
    $requestText = [System.IO.File]::ReadAllText($analysisRequest)
    $requestText = $requestText.Replace(
        "optimization_max_iterations`t50",
        "optimization_max_iterations`t2"
    )
    Write-Utf8NoBom -Path $analysisRequest -Text $requestText

    $plan = & $cli --error-format tsv analysis-plan --request $analysisRequest
    Assert-LastExitCode -Operation "installed analysis-plan"
    $planText = $plan -join "`n"
    if ($planText -notmatch "(?m)^format`tbiogeo-analysis-plan-v1$" -or
        $planText -notmatch "(?m)^status`tvalid$" -or
        $planText -notmatch "(?m)^portable`ttrue$") {
        throw "Installed CLI did not validate the portable unified analysis request."
    }

    $run = & $cli --error-format tsv analysis-run `
        --request $analysisRequest --output-dir $analysisResult
    Assert-LastExitCode -Operation "installed analysis-run"
    $runText = $run -join "`n"
    if ($runText -notmatch "(?m)^format`tbiogeo-analysis-run-v2$" -or
        $runText -notmatch "(?m)^analysis_result_format`tbiogeo-analysis-result-v2$" -or
        $runText -notmatch "(?m)^telemetry_provider`twindows_process_api$" -or
        $runText -notmatch "(?m)^process_telemetry_available`ttrue$" -or
        $runText -notmatch "(?m)^process_peak_working_set_bytes`t[1-9][0-9]*$") {
        throw "Installed CLI did not produce a v2 result through the unified request."
    }
    $inputs = [System.IO.File]::ReadAllText((Join-Path $analysisResult "inputs.tsv"))
    if ($inputs -notmatch "(?m)^analysis_request`t") {
        throw "Unified request provenance was not preserved in the analysis result."
    }

    $inspection = & $cli --error-format tsv analysis-result-inspect `
        --analysis-result $analysisResult --replay
    Assert-LastExitCode -Operation "installed analysis-result-inspect --replay"
    if (($inspection -join "`n") -notmatch "(?m)^format`tbiogeo-analysis-result-inspection-v1$" -or
        ($inspection -join "`n") -notmatch "(?m)^replay_validation`tpassed$") {
        throw "Installed CLI did not produce a replay-validated analysis result."
    }

    $workflow = & $cli --error-format tsv analysis-workflow `
        --request $analysisRequest --output-dir $analysisWorkflow `
        --bsm-samples 1 --bsm-threads 1 --seed 20260821 --deep
    Assert-LastExitCode -Operation "installed analysis-workflow"
    $workflowText = $workflow -join "`n"
    if ($workflowText -notmatch "(?m)^format`tbiogeo-analysis-workflow-v1$" -or
        $workflowText -notmatch "(?m)^status`tcomplete$" -or
        $workflowText -notmatch "(?m)^analysis_reused`tfalse$" -or
        $workflowText -notmatch "(?m)^bsm_output_level`tcompact$" -or
        $workflowText -notmatch "(?m)^bsm_completed_samples`t1$" -or
        $workflowText -notmatch "(?m)^bsm_validation`tdeep$" -or
        $workflowText -notmatch "(?m)^bsm_validation_status`tvalid$") {
        throw "Installed CLI did not complete and validate the unified analysis workflow."
    }
    if (-not (Test-Path -LiteralPath (Join-Path $analysisWorkflow "analysis-result\metadata.tsv") `
            -PathType Leaf) -or
        -not (Test-Path -LiteralPath (Join-Path $analysisWorkflow "bsm-result\metadata.tsv") `
            -PathType Leaf)) {
        throw "Installed analysis workflow did not publish both authoritative child results."
    }

    $workflowResume = & $cli --error-format tsv analysis-workflow `
        --request $analysisRequest --output-dir $analysisWorkflow `
        --bsm-samples 1 --bsm-threads 1 --seed 20260821 --deep --resume
    Assert-LastExitCode -Operation "installed analysis-workflow --resume"
    $workflowResumeText = $workflowResume -join "`n"
    if ($workflowResumeText -notmatch "(?m)^status`tcomplete$" -or
        $workflowResumeText -notmatch "(?m)^analysis_reused`ttrue$" -or
        $workflowResumeText -notmatch "(?m)^bsm_resumed`ttrue$") {
        throw "Installed CLI did not resume the completed analysis workflow."
    }

    $modelWorkflowRequest = Join-Path $repoRoot "examples\model_workflow\workflow.tsv"
    $modelPlan = & $cli --error-format tsv model-workflow-plan --request $modelWorkflowRequest
    Assert-LastExitCode -Operation "installed model-workflow-plan"
    $modelPlanText = $modelPlan -join "`n"
    if ($modelPlanText -notmatch "(?m)^format`tbiogeo-model-workflow-plan-v1$" -or
        $modelPlanText -notmatch "(?m)^status`tvalid$" -or
        $modelPlanText -notmatch "(?m)^candidate_models`t2$" -or
        $modelPlanText -notmatch "(?m)^bsm_requested_model_id`tDEC$") {
        throw "Installed CLI did not preflight the versioned multi-model request."
    }

    $modelRun = & $cli --error-format tsv model-workflow `
        --request $modelWorkflowRequest --output-dir $modelWorkflow
    Assert-LastExitCode -Operation "installed model-workflow"
    $modelRunText = $modelRun -join "`n"
    if ($modelRunText -notmatch "(?m)^format`tbiogeo-model-workflow-run-v1$" -or
        $modelRunText -notmatch "(?m)^status`tcomplete$" -or
        $modelRunText -notmatch "(?m)^selected_model_id`tDEC$" -or
        $modelRunText -notmatch "(?m)^bsm_completed_samples`t4$" -or
        $modelRunText -notmatch "(?m)^bsm_validation`tdeep$") {
        throw "Installed CLI did not complete the versioned multi-model workflow."
    }
    foreach ($relative in @(
            "metadata.tsv",
            "model-batch\comparison.tsv",
            "model-batch\model-averaged-ancestral-ranges.tsv",
            "selection.tsv",
            "bsm-result\metadata.tsv",
            "complete.tsv"
        )) {
        if (-not (Test-Path -LiteralPath (Join-Path $modelWorkflow $relative) -PathType Leaf)) {
            throw "Installed model workflow omitted $relative."
        }
    }

    $modelResume = & $cli --error-format tsv model-workflow `
        --request $modelWorkflowRequest --output-dir $modelWorkflow --resume
    Assert-LastExitCode -Operation "installed model-workflow --resume"
    $modelResumeText = $modelResume -join "`n"
    if ($modelResumeText -notmatch "(?m)^status`tcomplete$" -or
        $modelResumeText -notmatch "(?m)^model_batch_resumed`ttrue$" -or
        $modelResumeText -notmatch "(?m)^bsm_resumed`ttrue$") {
        throw "Installed CLI did not resume the completed multi-model workflow."
    }

    $publicExamplesCheck = Join-Path $repoRoot "validation\check-public-cli-examples.ps1"
    & $publicExamplesCheck -CliPath $cli -ExamplesRoot (Join-Path $installRoot "examples") `
        -OutputRoot $publicExamplesOutput -SkipBuild | Out-Host

    $realDataWorkflowCheck = Join-Path $repoRoot "validation\check-model-workflow-real-data.ps1"
    & $realDataWorkflowCheck -CliPath $cli -OutputRoot $realDataWorkflowOutput `
        -SkipBuild | Out-Host

    $presetModifierCheck = Join-Path $repoRoot "validation\check-preset-modifier-matrix.ps1"
    & $presetModifierCheck -CliPath $cli -OutputRoot $presetModifierOutput `
        -SkipBuild | Out-Host

    $treeInputCheck = Join-Path $repoRoot "validation\check-tree-input-equivalence.ps1"
    & $treeInputCheck -CliPath $cli -SkipBuild | Out-Host

    $largeStateSpaceCheck = Join-Path $repoRoot "validation\check-large-state-space-resources.ps1"
    & $largeStateSpaceCheck -CliPath $cli -SkipBuild | Out-Host

    $stabilityCheck = Join-Path $repoRoot "validation\check-windows-pc-stability.ps1"
    & $stabilityCheck `
        -CliPath $cli `
        -OutputRoot $stabilityOutput `
        -Cycles 1 `
        -BsmSamples 8 `
        -SkipBuild | Out-Host

    $lowDiskRejected = $false
    try {
        & $stabilityCheck `
            -CliPath $cli `
            -OutputRoot $stabilityLowDiskOutput `
            -Cycles 1 `
            -BsmSamples 1 `
            -MinimumFreeSpaceMb ([int]::MaxValue) `
            -SkipBuild | Out-Null
    }
    catch {
        $lowDiskRejected = $true
    }
    $lowDiskEvidence = [System.IO.File]::ReadAllText(
        (Join-Path $stabilityLowDiskOutput "evidence.tsv")
    )
    $lowDiskWorkDirectories = @(
        Get-ChildItem -LiteralPath (Join-Path $stabilityLowDiskOutput "work") -Directory
    )
    if (-not $lowDiskRejected -or
        $lowDiskEvidence -notmatch "(?m)^status`tfailed$" -or
        $lowDiskEvidence -notmatch "(?m)^completed_cycles`t0$" -or
        $lowDiskEvidence -notmatch "(?m)^total_bsm_samples`t0$" -or
        $lowDiskWorkDirectories.Count -ne 0) {
        throw "Windows stability preflight did not reject low disk space before starting work."
    }

    $previousHostEngine = $env:BIOGEO_RASP_HOST_ENGINE
    $previousHostRegistry = $env:BIOGEO_RASP_HOST_SCHEMA_REGISTRY
    try {
        $env:BIOGEO_RASP_HOST_ENGINE = $cli
        $env:BIOGEO_RASP_HOST_SCHEMA_REGISTRY = Join-Path $installRoot "schemas\registry.tsv"
        & cargo test --quiet -p biogeo-cli --test rasp_host_contract
        Assert-LastExitCode -Operation "installed RASP host contract"
    }
    finally {
        if ($null -eq $previousHostEngine) {
            Remove-Item Env:BIOGEO_RASP_HOST_ENGINE -ErrorAction SilentlyContinue
        }
        else {
            $env:BIOGEO_RASP_HOST_ENGINE = $previousHostEngine
        }
        if ($null -eq $previousHostRegistry) {
            Remove-Item Env:BIOGEO_RASP_HOST_SCHEMA_REGISTRY -ErrorAction SilentlyContinue
        }
        else {
            $env:BIOGEO_RASP_HOST_SCHEMA_REGISTRY = $previousHostRegistry
        }
    }

    Write-Output "Windows release validation passed."
}
finally {
    Remove-ValidatedSmokeRoot -TargetRoot $targetRoot -SmokeRoot $smokeRoot
}
