[CmdletBinding()]
param(
    [string]$OutputDir = "",
    [switch]$SkipBuild,
    [ValidateSet("local_worktree", "ci")]
    [string]$BuildOrigin = "local_worktree",
    [string]$SourceRevision = "",
    [string]$CiProvider = "",
    [string]$CiRunId = "",
    [string]$CiRepository = "",
    [string]$SigningCertificateThumbprint = "",
    [string]$TimestampServer = ""
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory = $true)][string]$Path,
        [Parameter(Mandatory = $true)][string]$Text
    )

    $encoding = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($Path, $Text, $encoding)
}

function Read-KeyValueTable {
    param([Parameter(Mandatory = $true)][string]$Path)

    $lines = [System.IO.File]::ReadAllLines($Path, [System.Text.Encoding]::UTF8)
    if ($lines.Count -eq 0 -or $lines[0] -ne "key`tvalue") {
        throw "Invalid key/value header in $Path"
    }
    $values = @{}
    for ($index = 1; $index -lt $lines.Count; $index++) {
        $line = $lines[$index]
        if ([string]::IsNullOrEmpty($line)) {
            continue
        }
        $separator = $line.IndexOf("`t")
        if ($separator -le 0) {
            throw "Invalid key/value record at $Path line $($index + 1)"
        }
        $key = $line.Substring(0, $separator)
        if ($values.ContainsKey($key)) {
            throw "Duplicate key '$key' in $Path"
        }
        $values[$key] = $line.Substring($separator + 1)
    }
    return $values
}

function Assert-SingleLineValue {
    param(
        [Parameter(Mandatory = $true)][string]$Name,
        [Parameter(Mandatory = $true)][string]$Value
    )

    if ([string]::IsNullOrWhiteSpace($Value) -or
        $Value.Contains("`t") -or $Value.Contains("`r") -or $Value.Contains("`n")) {
        throw "$Name must be a non-empty single-line value without tabs."
    }
}

function Find-CodeSigningCertificate {
    param([Parameter(Mandatory = $true)][string]$Thumbprint)

    $matches = @(
        foreach ($store in @("Cert:\CurrentUser\My", "Cert:\LocalMachine\My")) {
            if (Test-Path -LiteralPath $store) {
                Get-ChildItem -LiteralPath $store |
                    Where-Object {
                        $_.Thumbprint -eq $Thumbprint -and $_.HasPrivateKey
                    }
            }
        }
    )
    if ($matches.Count -ne 1) {
        throw "Expected exactly one code-signing certificate with private key and thumbprint $Thumbprint in CurrentUser/My or LocalMachine/My."
    }
    $certificate = $matches[0]
    $codeSigningEkus = @(
        $certificate.EnhancedKeyUsageList |
            Where-Object { $_.ObjectId.Value -eq "1.3.6.1.5.5.7.3.3" }
    )
    if ($codeSigningEkus.Count -ne 1) {
        throw "Certificate $Thumbprint is not valid for code signing."
    }
    $now = Get-Date
    if ($now -lt $certificate.NotBefore -or $now -gt $certificate.NotAfter) {
        throw "Certificate $Thumbprint is outside its validity period."
    }
    return $certificate
}

function Convert-ToPortableRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Path
    )

    return $Path.Substring($Root.Length + 1).Replace('\', '/')
}

function Move-DirectoryWithTransientRetry {
    param(
        [Parameter(Mandatory = $true)][string]$Source,
        [Parameter(Mandatory = $true)][string]$Destination
    )

    foreach ($delayMs in @(5, 10, 20, 40, 80, 160)) {
        try {
            Move-Item -LiteralPath $Source -Destination $Destination
            return
        }
        catch {
            $windowsError = $_.Exception.HResult -band 0xffff
            if ($windowsError -notin @(5, 32, 33)) {
                throw
            }
            Start-Sleep -Milliseconds $delayMs
        }
    }
    Move-Item -LiteralPath $Source -Destination $Destination
}

function New-PortableZipArchive {
    param(
        [Parameter(Mandatory = $true)][string]$SourceDir,
        [Parameter(Mandatory = $true)][string]$DestinationPath,
        [Parameter(Mandatory = $true)][string]$RootName
    )

    if ($RootName.Contains('/') -or $RootName.Contains('\')) {
        throw "ZIP root name must be a single path component."
    }
    if (Test-Path -LiteralPath $DestinationPath) {
        throw "Refusing to overwrite ZIP archive $DestinationPath"
    }

    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $sourceRoot = [System.IO.Path]::GetFullPath($SourceDir).TrimEnd([char[]]"\/")
    $files = @(
        Get-ChildItem -LiteralPath $sourceRoot -Recurse -File |
            Sort-Object { Convert-ToPortableRelativePath -Root $sourceRoot -Path $_.FullName }
    )
    if ($files.Count -eq 0) {
        throw "Cannot create an empty package archive."
    }

    $archiveStream = $null
    $archive = $null
    try {
        $archiveStream = [System.IO.File]::Open(
            $DestinationPath,
            [System.IO.FileMode]::CreateNew,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
        $archive = [System.IO.Compression.ZipArchive]::new(
            $archiveStream,
            [System.IO.Compression.ZipArchiveMode]::Create,
            $true,
            [System.Text.Encoding]::UTF8
        )
        foreach ($file in $files) {
            $relative = Convert-ToPortableRelativePath -Root $sourceRoot -Path $file.FullName
            $entryName = "$RootName/$relative"
            $entry = $archive.CreateEntry(
                $entryName,
                [System.IO.Compression.CompressionLevel]::Optimal
            )
            $inputStream = $null
            $entryStream = $null
            try {
                $inputStream = [System.IO.File]::OpenRead($file.FullName)
                $entryStream = $entry.Open()
                $inputStream.CopyTo($entryStream)
            }
            finally {
                if ($null -ne $entryStream) {
                    $entryStream.Dispose()
                }
                if ($null -ne $inputStream) {
                    $inputStream.Dispose()
                }
            }
        }
    }
    finally {
        if ($null -ne $archive) {
            $archive.Dispose()
        }
        if ($null -ne $archiveStream) {
            $archiveStream.Dispose()
        }
    }
}

if ($env:OS -ne "Windows_NT") {
    throw "This package builder only supports Windows hosts."
}

$repoRoot = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot ".."))
$releaseStatusPath = Join-Path $repoRoot "release-status.tsv"
$releaseStatus = Read-KeyValueTable -Path $releaseStatusPath
$expectedReleaseStatusKeys = @(
    "changelog_file",
    "format",
    "known_limitations_declared",
    "project_license_file",
    "project_license_status",
    "public_distribution_allowed",
    "release_class",
    "release_notes_file",
    "reproducibility_declared",
    "status",
    "third_party_licenses_dir",
    "third_party_notices_file",
    "version"
)
if ((@($releaseStatus.Keys | Sort-Object) -join "`n") -ne
    (($expectedReleaseStatusKeys | Sort-Object) -join "`n")) {
    throw "release-status.tsv keys do not match biogeo-release-status-v1."
}
if ($releaseStatus["format"] -ne "biogeo-release-status-v1" -or
    $releaseStatus["status"] -ne "complete" -or
    $releaseStatus["known_limitations_declared"] -ne "true" -or
    $releaseStatus["reproducibility_declared"] -ne "true" -or
    $releaseStatus["public_distribution_allowed"] -notin @("true", "false")) {
    throw "release-status.tsv contains an invalid release declaration."
}
foreach ($entry in @(
        @{ Name = "BuildOrigin"; Value = $BuildOrigin },
        @{ Name = "SourceRevision"; Value = $SourceRevision },
        @{ Name = "CiProvider"; Value = $CiProvider },
        @{ Name = "CiRunId"; Value = $CiRunId },
        @{ Name = "CiRepository"; Value = $CiRepository },
        @{ Name = "SigningCertificateThumbprint"; Value = $SigningCertificateThumbprint },
        @{ Name = "TimestampServer"; Value = $TimestampServer }
    )) {
    if (-not [string]::IsNullOrEmpty([string]$entry.Value) -and
        ([string]$entry.Value).IndexOfAny([char[]]"`t`r`n") -ge 0) {
        throw "$($entry.Name) must not contain tabs or newlines."
    }
}

$sourceRevisionValue = $SourceRevision.Trim().ToLowerInvariant()
$gitAvailable = $null -ne (Get-Command git -ErrorAction SilentlyContinue)
$gitHeadRevision = "unavailable"
if ($gitAvailable) {
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $gitRevision = @(& git -C $repoRoot rev-parse --verify HEAD 2>$null)
        $gitRevisionExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
    if ($gitRevisionExitCode -eq 0 -and $gitRevision.Count -eq 1) {
        $candidateHeadRevision = ([string]$gitRevision[0]).Trim().ToLowerInvariant()
        if ($candidateHeadRevision -notmatch "^(?:[0-9a-f]{40}|[0-9a-f]{64})$") {
            throw "Git HEAD is not a full 40- or 64-character hexadecimal revision."
        }
        $gitHeadRevision = $candidateHeadRevision
    }
}
if ([string]::IsNullOrEmpty($sourceRevisionValue) -and $gitHeadRevision -ne "unavailable") {
    $sourceRevisionValue = $gitHeadRevision
}
if ([string]::IsNullOrEmpty($sourceRevisionValue)) {
    $sourceRevisionValue = "unavailable"
}
elseif ($sourceRevisionValue -notmatch "^(?:[0-9a-f]{40}|[0-9a-f]{64})$") {
    throw "SourceRevision must be a 40- or 64-character hexadecimal revision."
}
$sourceRevisionGitHeadMatch = if ($gitHeadRevision -eq "unavailable" -or
    $sourceRevisionValue -eq "unavailable") {
    "unavailable"
}
elseif ($sourceRevisionValue -eq $gitHeadRevision) {
    "true"
}
else {
    "false"
}
if ($gitHeadRevision -ne "unavailable" -and $sourceRevisionGitHeadMatch -ne "true") {
    throw "SourceRevision does not match the checked-out Git HEAD."
}

$sourceTreeStatus = "unavailable"
if ($gitAvailable -and $sourceRevisionValue -ne "unavailable") {
    $previousErrorAction = $ErrorActionPreference
    $ErrorActionPreference = "Continue"
    try {
        $gitStatus = @(& git -C $repoRoot status --porcelain --untracked-files=normal 2>$null)
        $gitStatusExitCode = $LASTEXITCODE
    }
    finally {
        $ErrorActionPreference = $previousErrorAction
    }
    if ($gitStatusExitCode -eq 0) {
        $sourceTreeStatus = if ($gitStatus.Count -eq 0) { "clean" } else { "dirty" }
    }
}

if ($BuildOrigin -eq "ci") {
    Assert-SingleLineValue -Name "CiProvider" -Value $CiProvider
    Assert-SingleLineValue -Name "CiRunId" -Value $CiRunId
    Assert-SingleLineValue -Name "CiRepository" -Value $CiRepository
    if ($sourceRevisionValue -eq "unavailable") {
        throw "CI builds require an explicit source revision."
    }
}
elseif (-not [string]::IsNullOrEmpty($CiProvider) -or
    -not [string]::IsNullOrEmpty($CiRunId) -or
    -not [string]::IsNullOrEmpty($CiRepository)) {
    throw "CI provenance fields require -BuildOrigin ci."
}

$ciProviderValue = if ($BuildOrigin -eq "ci") { $CiProvider } else { "none" }
$ciRunIdValue = if ($BuildOrigin -eq "ci") { $CiRunId } else { "none" }
$ciRepositoryValue = if ($BuildOrigin -eq "ci") { $CiRepository } else { "none" }

$signingThumbprint = $SigningCertificateThumbprint.Replace(" ", "").ToUpperInvariant()
if (-not [string]::IsNullOrEmpty($signingThumbprint) -and
    $signingThumbprint -notmatch "^[0-9A-F]{40}$") {
    throw "SigningCertificateThumbprint must be a 40-character certificate thumbprint."
}
if ([string]::IsNullOrEmpty($signingThumbprint) -ne [string]::IsNullOrEmpty($TimestampServer)) {
    throw "SigningCertificateThumbprint and TimestampServer must be supplied together."
}
if (-not [string]::IsNullOrEmpty($TimestampServer)) {
    $timestampUri = $null
    if (-not [Uri]::TryCreate($TimestampServer, [UriKind]::Absolute, [ref]$timestampUri) -or
        $timestampUri.Scheme -notin @("http", "https")) {
        throw "TimestampServer must be an absolute HTTP or HTTPS URL."
    }
}

if ([string]::IsNullOrWhiteSpace($OutputDir)) {
    $outputRoot = Join-Path $repoRoot "dist"
}
elseif ([System.IO.Path]::IsPathRooted($OutputDir)) {
    $outputRoot = [System.IO.Path]::GetFullPath($OutputDir)
}
else {
    $outputRoot = [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $OutputDir))
}

Push-Location $repoRoot
try {
    $metadataText = (& cargo metadata --no-deps --format-version 1 --locked | Out-String)
    if ($LASTEXITCODE -ne 0) {
        throw "cargo metadata failed with exit code $LASTEXITCODE"
    }
    $metadata = $metadataText | ConvertFrom-Json
    $cliPackages = @($metadata.packages | Where-Object { $_.name -eq "biogeo-cli" })
    if ($cliPackages.Count -ne 1) {
        throw "Expected exactly one biogeo-cli package in cargo metadata."
    }
    $version = [string]$cliPackages[0].version
    if ($releaseStatus["version"] -ne $version) {
        throw "release-status.tsv version does not match the biogeo-cli package version."
    }

    $rustcInfo = @(& rustc -vV)
    if ($LASTEXITCODE -ne 0) {
        throw "rustc -vV failed with exit code $LASTEXITCODE"
    }
    $hostLine = @($rustcInfo | Where-Object { $_ -like "host: *" })
    if ($hostLine.Count -ne 1) {
        throw "Could not determine the Rust host target."
    }
    $target = $hostLine[0].Substring("host: ".Length).Trim()
    if ($target -notmatch "^[A-Za-z0-9_.-]+-pc-windows-msvc$") {
        throw "Unsupported Windows package target '$target'; an MSVC Windows host is required."
    }

    $cargoInfo = @(& cargo -V)
    if ($LASTEXITCODE -ne 0 -or $cargoInfo.Count -ne 1) {
        throw "cargo -V failed or returned an unexpected result."
    }

    if (-not $SkipBuild) {
        & cargo build --release --locked -p biogeo-cli
        if ($LASTEXITCODE -ne 0) {
            throw "cargo build failed with exit code $LASTEXITCODE"
        }
    }
}
finally {
    Pop-Location
}

$executable = Join-Path ([string]$metadata.target_directory) "release\biogeo-cli.exe"
if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
    throw "Release executable does not exist: $executable"
}

$packageName = "biogeo-cli-$version-$target"
if (Test-Path -LiteralPath $outputRoot -PathType Leaf) {
    throw "Output path is a file: $outputRoot"
}
[System.IO.Directory]::CreateDirectory($outputRoot) | Out-Null
$finalPackageDir = Join-Path $outputRoot $packageName
$finalArchive = Join-Path $outputRoot "$packageName.zip"
$finalChecksum = "$finalArchive.sha256"
foreach ($path in @($finalPackageDir, $finalArchive, $finalChecksum)) {
    if (Test-Path -LiteralPath $path) {
        throw "Release output already exists and will not be overwritten: $path"
    }
}

$token = "$PID-$([Guid]::NewGuid().ToString('N'))"
$stagingRoot = Join-Path $outputRoot ".$packageName.stage-$token"
$packageRoot = Join-Path $stagingRoot $packageName
$temporaryArchive = Join-Path $outputRoot ".$packageName.$token.zip"
$publishedPackage = $false
$publishedArchive = $false

try {
    [System.IO.Directory]::CreateDirectory($packageRoot) | Out-Null
    [System.IO.Directory]::CreateDirectory((Join-Path $packageRoot "schemas")) | Out-Null
    [System.IO.Directory]::CreateDirectory((Join-Path $packageRoot "docs")) | Out-Null

    $packagedExecutable = Join-Path $packageRoot "biogeo-cli.exe"
    Copy-Item -LiteralPath $executable -Destination $packagedExecutable

    $authenticodeStatus = "unsigned"
    $authenticodeSignerThumbprint = "none"
    $authenticodeTimestampStatus = "none"
    $authenticodeTimestampThumbprint = "none"
    $authenticodeTimestampServer = "none"
    $authenticodeHashAlgorithm = "none"
    if ([string]::IsNullOrEmpty($signingThumbprint)) {
        $existingSignature = Get-AuthenticodeSignature -FilePath $packagedExecutable
        if ([string]$existingSignature.Status -ne "NotSigned") {
            throw "The release executable is already signed; provide SigningCertificateThumbprint so the builder can verify its signing identity."
        }
    }
    else {
        $certificate = Find-CodeSigningCertificate -Thumbprint $signingThumbprint
        $signature = Set-AuthenticodeSignature `
            -FilePath $packagedExecutable `
            -Certificate $certificate `
            -HashAlgorithm SHA256 `
            -TimestampServer $TimestampServer `
            -IncludeChain All
        if ([string]$signature.Status -ne "Valid") {
            throw "Authenticode signing did not produce a valid trusted signature: $($signature.Status) $($signature.StatusMessage)"
        }
        $verifiedSignature = Get-AuthenticodeSignature -FilePath $packagedExecutable
        if ([string]$verifiedSignature.Status -ne "Valid" -or
            $null -eq $verifiedSignature.SignerCertificate -or
            $verifiedSignature.SignerCertificate.Thumbprint -ne $signingThumbprint -or
            $null -eq $verifiedSignature.TimeStamperCertificate) {
            throw "The signed executable failed independent Authenticode signer or timestamp verification."
        }
        $authenticodeStatus = "valid"
        $authenticodeSignerThumbprint = $verifiedSignature.SignerCertificate.Thumbprint.ToLowerInvariant()
        $authenticodeTimestampStatus = "valid"
        $authenticodeTimestampThumbprint = $verifiedSignature.TimeStamperCertificate.Thumbprint.ToLowerInvariant()
        $authenticodeTimestampServer = $TimestampServer
        $authenticodeHashAlgorithm = "sha256"
    }
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "install-windows-package.ps1") `
        -Destination (Join-Path $packageRoot "install.ps1")

    foreach ($name in @(
            "README.md",
            "CITATION.cff",
            "CHANGELOG.md",
            "LICENSE",
            "LICENSE-STATUS.md",
            "THIRD-PARTY-NOTICES.md",
            "release-status.tsv"
        )) {
        $source = Join-Path $repoRoot $name
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Required release declaration is missing: $source"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $packageRoot $name)
    }

    $thirdPartyLicenses = Join-Path $repoRoot $releaseStatus["third_party_licenses_dir"]
    if (-not (Test-Path -LiteralPath $thirdPartyLicenses -PathType Container) -or
        @(Get-ChildItem -LiteralPath $thirdPartyLicenses -Recurse -File).Count -eq 0) {
        throw "Required third-party license texts are missing: $thirdPartyLicenses"
    }
    Copy-Item -LiteralPath $thirdPartyLicenses -Destination $packageRoot -Recurse

    $schemaFiles = @(Get-ChildItem -LiteralPath (Join-Path $repoRoot "schemas") -File | Sort-Object Name)
    if ($schemaFiles.Count -eq 0) {
        throw "No schema files were found."
    }
    foreach ($file in $schemaFiles) {
        Copy-Item -LiteralPath $file.FullName -Destination (Join-Path $packageRoot "schemas\$($file.Name)")
    }

    $documentation = @(
        "ambiguous-ranges.md",
        "analysis-request.md",
        "analysis-result.md",
        "analysis-workflow.md",
        "biogeobears-chinese-tutorial.md",
        "bsm-inspection.md",
        "bsm-output-formats.md",
        "cli-tutorial.md",
        "command-line-help.md",
        "compatibility-policy.md",
        "dataset-batch.md",
        "detection-model.md",
        "engine-capabilities.md",
        "biogeobears-parity-matrix.md",
        "framework-architecture.md",
        "input-validation.md",
        "installation.md",
        "legacy-input-import.md",
        "model-average.md",
        "model-batch.md",
        "model-workflow.md",
        "parameter-table.md",
        "performance-benchmark.md",
        "progress-and-cancellation.md",
        "random-fossil-placement.md",
        "rasp-cli-integration.md",
        "rasp-host-state-machine.md",
        "source-and-license-audit.md",
        "tree-input-and-fossil-tips.md",
        "v0.1-release-notes.md",
        "windows-pc-stability.md",
        "windows-trusted-distribution.md",
        "windows-release.md"
    )
    foreach ($name in $documentation) {
        $source = Join-Path $repoRoot "docs\$name"
        if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
            throw "Required release documentation is missing: $source"
        }
        Copy-Item -LiteralPath $source -Destination (Join-Path $packageRoot "docs\$name")
    }

    $examples = Join-Path $repoRoot "examples"
    if (-not (Test-Path -LiteralPath $examples -PathType Container)) {
        throw "Required release examples are missing: $examples"
    }
    Copy-Item -LiteralPath $examples -Destination $packageRoot -Recurse

    $sourceFiles = @(
        Get-Item -LiteralPath (Join-Path $repoRoot "Cargo.toml"), (Join-Path $repoRoot "Cargo.lock")
        Get-ChildItem -LiteralPath (Join-Path $repoRoot "crates") -Recurse -File |
            Where-Object { $_.Name -eq "Cargo.toml" -or $_.Extension -eq ".rs" }
    ) | Sort-Object { Convert-ToPortableRelativePath -Root $repoRoot -Path $_.FullName }
    $sourceManifestRows = New-Object System.Collections.Generic.List[string]
    foreach ($file in $sourceFiles) {
        $relative = Convert-ToPortableRelativePath -Root $repoRoot -Path $file.FullName
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $sourceManifestRows.Add("$relative`t$($file.Length)`t$hash")
    }
    $sourceManifestPath = Join-Path $packageRoot "engine-source-manifest.tsv"
    $sourceManifestText = @("path`tbytes`tsha256") + @($sourceManifestRows)
    Write-Utf8NoBom -Path $sourceManifestPath -Text "$($sourceManifestText -join "`n")`n"

    $sourceManifestHash = (Get-FileHash -LiteralPath $sourceManifestPath -Algorithm SHA256).Hash.ToLowerInvariant()
    $cargoLockHash = (Get-FileHash -LiteralPath (Join-Path $repoRoot "Cargo.lock") -Algorithm SHA256).Hash.ToLowerInvariant()
    $executableHash = (Get-FileHash -LiteralPath $packagedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    $rustcVersion = [string]$rustcInfo[0]
    $rustcCommit = [string](@($rustcInfo | Where-Object { $_ -like "commit-hash: *" })[0]).Substring("commit-hash: ".Length)
    $llvmVersion = [string](@($rustcInfo | Where-Object { $_ -like "LLVM version: *" })[0]).Substring("LLVM version: ".Length)
    $buildInfo = @(
        "key`tvalue",
        "format`tbiogeo-windows-build-info-v2",
        "status`tcomplete",
        "package_name`t$packageName",
        "version`t$version",
        "target`t$target",
        "build_profile`trelease",
        "build_command`tcargo build --release --locked -p biogeo-cli",
        "cargo_locked`ttrue",
        "cargo_version`t$([string]$cargoInfo[0])",
        "rustc_version`t$rustcVersion",
        "rustc_commit_hash`t$rustcCommit",
        "llvm_version`t$llvmVersion",
        "cargo_lock_sha256`t$cargoLockHash",
        "source_manifest`tengine-source-manifest.tsv",
        "source_manifest_sha256`t$sourceManifestHash",
        "source_file_count`t$($sourceManifestRows.Count)",
        "executable_sha256`t$executableHash",
        "build_origin`t$BuildOrigin",
        "source_revision`t$sourceRevisionValue",
        "source_revision_git_head_match`t$sourceRevisionGitHeadMatch",
        "source_tree_status`t$sourceTreeStatus",
        "ci_provider`t$ciProviderValue",
        "ci_run_id`t$ciRunIdValue",
        "ci_repository`t$ciRepositoryValue",
        "authenticode_status`t$authenticodeStatus",
        "authenticode_signer_thumbprint`t$authenticodeSignerThumbprint",
        "authenticode_timestamp_status`t$authenticodeTimestampStatus",
        "authenticode_timestamp_thumbprint`t$authenticodeTimestampThumbprint",
        "authenticode_timestamp_server`t$authenticodeTimestampServer",
        "authenticode_hash_algorithm`t$authenticodeHashAlgorithm",
        "publisher_pin_required_for_public`tfalse",
        "trusted_ci_required_for_public`tfalse",
        "reproducibility_scope`tfunctional_not_bit_for_bit",
        "zip_timestamps_normalized`tfalse"
    ) -join "`n"
    Write-Utf8NoBom -Path (Join-Path $packageRoot "build-info.tsv") -Text "$buildInfo`n"

    $payloadFiles = @(
        Get-ChildItem -LiteralPath $packageRoot -Recurse -File |
            Sort-Object { $_.FullName.Substring($packageRoot.Length + 1).Replace('\', '/') }
    )
    $manifestRows = New-Object System.Collections.Generic.List[string]
    foreach ($file in $payloadFiles) {
        $relative = $file.FullName.Substring($packageRoot.Length + 1).Replace('\', '/')
        $hash = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        $manifestRows.Add("$relative`t$($file.Length)`t$hash")
    }

    $packageMetadata = @(
        "key`tvalue",
        "format`tbiogeo-windows-package-v3",
        "status`tcomplete",
        "package_name`t$packageName",
        "version`t$version",
        "target`t$target",
        "executable`tbiogeo-cli.exe",
        "schema_registry`tschemas/registry.tsv",
        "files_manifest`tfiles.tsv",
        "release_status`trelease-status.tsv",
        "release_class`t$($releaseStatus['release_class'])",
        "project_license_status`t$($releaseStatus['project_license_status'])",
        "public_distribution_allowed`t$($releaseStatus['public_distribution_allowed'])",
        "build_info`tbuild-info.tsv",
        "source_manifest`tengine-source-manifest.tsv",
        "file_count`t$($manifestRows.Count)"
    ) -join "`n"
    Write-Utf8NoBom -Path (Join-Path $packageRoot "package.tsv") -Text "$packageMetadata`n"

    $manifestText = @("path`tbytes`tsha256") + @($manifestRows)
    Write-Utf8NoBom -Path (Join-Path $packageRoot "files.tsv") -Text "$(($manifestText -join "`n"))`n"

    $smokeOutput = & (Join-Path $packageRoot "biogeo-cli.exe") --help 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Packaged executable smoke test failed: $($smokeOutput -join [Environment]::NewLine)"
    }

    New-PortableZipArchive `
        -SourceDir $packageRoot `
        -DestinationPath $temporaryArchive `
        -RootName $packageName
    if (-not (Test-Path -LiteralPath $temporaryArchive -PathType Leaf)) {
        throw "Archive creation did not produce $temporaryArchive"
    }

    Move-Item -LiteralPath $temporaryArchive -Destination $finalArchive
    $publishedArchive = $true
    Move-DirectoryWithTransientRetry -Source $packageRoot -Destination $finalPackageDir
    $publishedPackage = $true
    Remove-Item -LiteralPath $stagingRoot -Force

    $archiveHash = (Get-FileHash -LiteralPath $finalArchive -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Utf8NoBom -Path $finalChecksum -Text "$archiveHash  $packageName.zip`n"
}
catch {
    if (Test-Path -LiteralPath $temporaryArchive) {
        Remove-Item -LiteralPath $temporaryArchive -Force
    }
    if (Test-Path -LiteralPath $stagingRoot) {
        Remove-Item -LiteralPath $stagingRoot -Recurse -Force
    }
    if ($publishedPackage -and (Test-Path -LiteralPath $finalPackageDir)) {
        Remove-Item -LiteralPath $finalPackageDir -Recurse -Force
    }
    if ($publishedArchive -and (Test-Path -LiteralPath $finalArchive)) {
        Remove-Item -LiteralPath $finalArchive -Force
    }
    if (Test-Path -LiteralPath $finalChecksum) {
        Remove-Item -LiteralPath $finalChecksum -Force
    }
    throw
}

Write-Output "package_dir`t$finalPackageDir"
Write-Output "archive`t$finalArchive"
Write-Output "archive_sha256`t$archiveHash"
Write-Output "release_class`t$($releaseStatus['release_class'])"
Write-Output "public_distribution_allowed`t$($releaseStatus['public_distribution_allowed'])"
Write-Output "build_origin`t$BuildOrigin"
Write-Output "authenticode_status`t$authenticodeStatus"
