[CmdletBinding()]
param(
    [string]$PackageDir = $PSScriptRoot,
    [Parameter(Mandatory = $true)][string]$InstallDir,
    [string]$ExpectedSignerThumbprint = ""
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
        $value = $line.Substring($separator + 1)
        if ($values.ContainsKey($key)) {
            throw "Duplicate key '$key' in $Path"
        }
        $values[$key] = $value
    }
    return $values
}

function Convert-ToPortableRelativePath {
    param(
        [Parameter(Mandatory = $true)][string]$Root,
        [Parameter(Mandatory = $true)][string]$Path
    )

    return $Path.Substring($Root.Length + 1).Replace('\', '/')
}

if ($env:OS -ne "Windows_NT") {
    throw "This installer only supports Windows hosts."
}

$packageRoot = (Resolve-Path -LiteralPath $PackageDir -ErrorAction Stop).Path
if (-not (Test-Path -LiteralPath $packageRoot -PathType Container)) {
    throw "Package directory does not exist: $PackageDir"
}

$expectedRootEntries = @(
    "build-info.tsv",
    "biogeo-cli.exe",
    "CHANGELOG.md",
    "docs",
    "engine-source-manifest.tsv",
    "examples",
    "files.tsv",
    "install.ps1",
    "LICENSE",
    "LICENSE-STATUS.md",
    "package.tsv",
    "README.md",
    "release-status.tsv",
    "schemas",
    "THIRD-PARTY-NOTICES.md",
    "third-party-licenses"
)
$actualRootEntries = @(
    Get-ChildItem -LiteralPath $packageRoot |
        ForEach-Object { $_.Name } |
        Sort-Object
)
if (($actualRootEntries -join "`n") -ne (($expectedRootEntries | Sort-Object) -join "`n")) {
    throw "Package root entries do not match biogeo-windows-package-v3."
}

$metadataPath = Join-Path $packageRoot "package.tsv"
$metadata = Read-KeyValueTable -Path $metadataPath
$expectedMetadataKeys = @(
    "build_info",
    "executable",
    "file_count",
    "files_manifest",
    "format",
    "package_name",
    "project_license_status",
    "public_distribution_allowed",
    "release_class",
    "release_status",
    "schema_registry",
    "source_manifest",
    "status",
    "target",
    "version"
)
if ((@($metadata.Keys | Sort-Object) -join "`n") -ne (($expectedMetadataKeys | Sort-Object) -join "`n")) {
    throw "Package metadata keys do not match biogeo-windows-package-v3."
}
if ($metadata.format -ne "biogeo-windows-package-v3" -or $metadata.status -ne "complete") {
    throw "Unsupported or incomplete Windows package."
}
if ($metadata.executable -ne "biogeo-cli.exe" -or
    $metadata.schema_registry -ne "schemas/registry.tsv" -or
    $metadata.files_manifest -ne "files.tsv" -or
    $metadata.release_status -ne "release-status.tsv" -or
    $metadata.build_info -ne "build-info.tsv" -or
    $metadata.source_manifest -ne "engine-source-manifest.tsv") {
    throw "Package metadata names do not match the Windows package contract."
}
if ($metadata.package_name -notmatch "^[A-Za-z0-9._-]+$" -or
    $metadata.version -notmatch "^[0-9]+\.[0-9]+\.[0-9]+$" -or
    $metadata.target -notmatch "^[A-Za-z0-9_.-]+-pc-windows-msvc$") {
    throw "Package identity fields are invalid."
}

$releaseStatusPath = Join-Path $packageRoot $metadata.release_status
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
    (($expectedReleaseStatusKeys | Sort-Object) -join "`n") -or
    $releaseStatus.format -ne "biogeo-release-status-v1" -or
    $releaseStatus.status -ne "complete" -or
    $releaseStatus.version -ne $metadata.version -or
    $releaseStatus.release_class -ne $metadata.release_class -or
    $releaseStatus.project_license_status -ne $metadata.project_license_status -or
    $releaseStatus.public_distribution_allowed -ne $metadata.public_distribution_allowed -or
    $releaseStatus.known_limitations_declared -ne "true" -or
    $releaseStatus.reproducibility_declared -ne "true") {
    throw "Package release status is invalid or inconsistent with package.tsv."
}
if ($metadata.public_distribution_allowed -notin @("true", "false")) {
    throw "Package public_distribution_allowed must be true or false."
}
if ($releaseStatus.project_license_file -ne "LICENSE" -or
    $releaseStatus.third_party_notices_file -ne "THIRD-PARTY-NOTICES.md" -or
    $releaseStatus.third_party_licenses_dir -ne "third-party-licenses" -or
    $releaseStatus.changelog_file -ne "CHANGELOG.md" -or
    $releaseStatus.release_notes_file -ne "docs/v0.1-release-notes.md") {
    throw "Package release status refers to unexpected release files."
}

$buildInfoPath = Join-Path $packageRoot $metadata.build_info
$buildInfo = Read-KeyValueTable -Path $buildInfoPath
$expectedBuildInfoKeys = @(
    "authenticode_hash_algorithm",
    "authenticode_signer_thumbprint",
    "authenticode_status",
    "authenticode_timestamp_server",
    "authenticode_timestamp_status",
    "authenticode_timestamp_thumbprint",
    "build_command",
    "build_origin",
    "build_profile",
    "cargo_lock_sha256",
    "cargo_locked",
    "cargo_version",
    "ci_provider",
    "ci_repository",
    "ci_run_id",
    "executable_sha256",
    "format",
    "llvm_version",
    "package_name",
    "reproducibility_scope",
    "rustc_commit_hash",
    "rustc_version",
    "source_file_count",
    "source_manifest",
    "source_manifest_sha256",
    "source_revision",
    "source_revision_git_head_match",
    "source_tree_status",
    "status",
    "target",
    "publisher_pin_required_for_public",
    "trusted_ci_required_for_public",
    "version",
    "zip_timestamps_normalized"
)
if ((@($buildInfo.Keys | Sort-Object) -join "`n") -ne (($expectedBuildInfoKeys | Sort-Object) -join "`n") -or
    $buildInfo.format -ne "biogeo-windows-build-info-v2" -or
    $buildInfo.status -ne "complete" -or
    $buildInfo.package_name -ne $metadata.package_name -or
    $buildInfo.version -ne $metadata.version -or
    $buildInfo.target -ne $metadata.target -or
    $buildInfo.build_profile -ne "release" -or
    $buildInfo.cargo_locked -ne "true" -or
    $buildInfo.source_manifest -ne $metadata.source_manifest -or
    $buildInfo.reproducibility_scope -ne "functional_not_bit_for_bit" -or
    $buildInfo.zip_timestamps_normalized -ne "false" -or
    $buildInfo.build_origin -notin @("local_worktree", "ci") -or
    $buildInfo.source_tree_status -notin @("clean", "dirty", "unavailable") -or
    $buildInfo.source_revision_git_head_match -notin @("true", "unavailable") -or
    $buildInfo.authenticode_status -notin @("unsigned", "valid") -or
    $buildInfo.authenticode_timestamp_status -notin @("none", "valid") -or
    $buildInfo.publisher_pin_required_for_public -ne "false" -or
    $buildInfo.trusted_ci_required_for_public -ne "false" -or
    $buildInfo.cargo_lock_sha256 -notmatch "^[0-9a-f]{64}$" -or
    $buildInfo.source_manifest_sha256 -notmatch "^[0-9a-f]{64}$" -or
    $buildInfo.executable_sha256 -notmatch "^[0-9a-f]{64}$") {
    throw "Package build information is invalid or inconsistent with package.tsv."
}
$actualExecutableHash = (Get-FileHash -LiteralPath (Join-Path $packageRoot $metadata.executable) `
        -Algorithm SHA256).Hash.ToLowerInvariant()
$actualSourceManifestHash = (Get-FileHash -LiteralPath (Join-Path $packageRoot $metadata.source_manifest) `
        -Algorithm SHA256).Hash.ToLowerInvariant()
if ($actualExecutableHash -ne $buildInfo.executable_sha256 -or
    $actualSourceManifestHash -ne $buildInfo.source_manifest_sha256) {
    throw "Package build information hashes do not match the payload."
}

$normalizedExpectedSigner = $ExpectedSignerThumbprint.Replace(" ", "").ToLowerInvariant()
if (-not [string]::IsNullOrEmpty($normalizedExpectedSigner) -and
    $normalizedExpectedSigner -notmatch "^[0-9a-f]{40}$") {
    throw "ExpectedSignerThumbprint must be a 40-character certificate thumbprint."
}
$executablePath = Join-Path $packageRoot $metadata.executable
$actualSignature = Get-AuthenticodeSignature -FilePath $executablePath
if ($buildInfo.authenticode_status -eq "unsigned") {
    if ([string]$actualSignature.Status -ne "NotSigned" -or
        $buildInfo.authenticode_signer_thumbprint -ne "none" -or
        $buildInfo.authenticode_timestamp_status -ne "none" -or
        $buildInfo.authenticode_timestamp_thumbprint -ne "none" -or
        $buildInfo.authenticode_timestamp_server -ne "none" -or
        $buildInfo.authenticode_hash_algorithm -ne "none") {
        throw "Unsigned package signature metadata is inconsistent with the executable."
    }
    if (-not [string]::IsNullOrEmpty($normalizedExpectedSigner)) {
        throw "ExpectedSignerThumbprint was supplied for an unsigned package."
    }
}
else {
    if ([string]$actualSignature.Status -ne "Valid" -or
        $null -eq $actualSignature.SignerCertificate -or
        $null -eq $actualSignature.TimeStamperCertificate -or
        $buildInfo.authenticode_signer_thumbprint -notmatch "^[0-9a-f]{40}$" -or
        $buildInfo.authenticode_timestamp_thumbprint -notmatch "^[0-9a-f]{40}$" -or
        $buildInfo.authenticode_timestamp_server -notmatch "^https?://" -or
        $buildInfo.authenticode_hash_algorithm -ne "sha256" -or
        $actualSignature.SignerCertificate.Thumbprint.ToLowerInvariant() -ne
            $buildInfo.authenticode_signer_thumbprint -or
        $actualSignature.TimeStamperCertificate.Thumbprint.ToLowerInvariant() -ne
            $buildInfo.authenticode_timestamp_thumbprint) {
        throw "Authenticode signature, signer, or trusted timestamp verification failed."
    }
    if (-not [string]::IsNullOrEmpty($normalizedExpectedSigner) -and
        $normalizedExpectedSigner -ne $buildInfo.authenticode_signer_thumbprint) {
        throw "The executable signer does not match ExpectedSignerThumbprint."
    }
}
$sourceManifestLines = [System.IO.File]::ReadAllLines(
    (Join-Path $packageRoot $metadata.source_manifest),
    [System.Text.Encoding]::UTF8
)
[uint64]$sourceFileCount = 0
if ($sourceManifestLines.Count -lt 2 -or $sourceManifestLines[0] -ne "path`tbytes`tsha256" -or
    -not [uint64]::TryParse($buildInfo.source_file_count, [ref]$sourceFileCount) -or
    [uint64]($sourceManifestLines.Count - 1) -ne $sourceFileCount) {
    throw "Package engine source manifest is invalid."
}
if (@(Get-ChildItem -LiteralPath (Join-Path $packageRoot "third-party-licenses") -Recurse -File).Count -lt 3) {
    throw "Package third-party license bundle is empty or incomplete."
}

[uint64]$declaredFileCount = 0
if (-not [uint64]::TryParse($metadata.file_count, [ref]$declaredFileCount)) {
    throw "Package file_count is invalid."
}

$manifestPath = Join-Path $packageRoot "files.tsv"
$manifestLines = [System.IO.File]::ReadAllLines($manifestPath, [System.Text.Encoding]::UTF8)
if ($manifestLines.Count -eq 0 -or $manifestLines[0] -ne "path`tbytes`tsha256") {
    throw "Invalid package file manifest header."
}

$records = New-Object System.Collections.Generic.List[object]
$declaredPaths = @{}
$packagePrefix = $packageRoot.TrimEnd('\', '/') + [System.IO.Path]::DirectorySeparatorChar
for ($index = 1; $index -lt $manifestLines.Count; $index++) {
    $line = $manifestLines[$index]
    if ([string]::IsNullOrEmpty($line)) {
        continue
    }
    $fields = @($line.Split("`t"))
    if ($fields.Count -ne 3) {
        throw "Invalid package manifest record at line $($index + 1)."
    }
    $relative = $fields[0]
    if ($relative -notmatch "^[A-Za-z0-9._-]+(?:/[A-Za-z0-9._-]+)*$" -or
        $declaredPaths.ContainsKey($relative)) {
        throw "Invalid or duplicate package path '$relative'."
    }
    [uint64]$expectedBytes = 0
    if (-not [uint64]::TryParse($fields[1], [ref]$expectedBytes) -or
        $fields[2] -notmatch "^[0-9a-fA-F]{64}$") {
        throw "Invalid size or SHA-256 for package path '$relative'."
    }
    $sourcePath = [System.IO.Path]::GetFullPath(
        (Join-Path $packageRoot $relative.Replace('/', [System.IO.Path]::DirectorySeparatorChar))
    )
    if (-not $sourcePath.StartsWith($packagePrefix, [System.StringComparison]::OrdinalIgnoreCase) -or
        -not (Test-Path -LiteralPath $sourcePath -PathType Leaf)) {
        throw "Package path escapes the package or is missing: '$relative'."
    }
    $file = Get-Item -LiteralPath $sourcePath
    $actualHash = (Get-FileHash -LiteralPath $sourcePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ([uint64]$file.Length -ne $expectedBytes -or $actualHash -ne $fields[2].ToLowerInvariant()) {
        throw "Package payload verification failed for '$relative'."
    }
    $declaredPaths[$relative] = $true
    $records.Add([PSCustomObject]@{
            Relative = $relative
            Source = $sourcePath
        })
}
if ([uint64]$records.Count -ne $declaredFileCount) {
    throw "Package file_count does not match files.tsv."
}

$actualPayloadPaths = @(
    Get-ChildItem -LiteralPath $packageRoot -Recurse -File |
        Where-Object { $_.Name -notin @("package.tsv", "files.tsv") -or $_.DirectoryName -ne $packageRoot } |
        ForEach-Object { Convert-ToPortableRelativePath -Root $packageRoot -Path $_.FullName } |
        Sort-Object
)
$manifestPayloadPaths = @($records | ForEach-Object { $_.Relative } | Sort-Object)
if (($actualPayloadPaths -join "`n") -ne ($manifestPayloadPaths -join "`n")) {
    throw "Package contains an unlisted payload file or omits a payload file."
}

$registryPath = Join-Path $packageRoot $metadata.schema_registry.Replace('/', '\')
$registryLines = [System.IO.File]::ReadAllLines($registryPath, [System.Text.Encoding]::UTF8)
if ($registryLines.Count -lt 2 -or $registryLines[0] -ne "biogeo-schema-registry-v1") {
    throw "Packaged schema registry is invalid."
}

if ([System.IO.Path]::IsPathRooted($InstallDir)) {
    $installPath = [System.IO.Path]::GetFullPath($InstallDir)
}
else {
    $installPath = [System.IO.Path]::GetFullPath((Join-Path (Get-Location).Path $InstallDir))
}
if (Test-Path -LiteralPath $installPath) {
    throw "Install directory already exists and will not be overwritten: $installPath"
}
$installName = [System.IO.Path]::GetFileName($installPath.TrimEnd('\', '/'))
$installParent = [System.IO.Path]::GetDirectoryName($installPath.TrimEnd('\', '/'))
if ([string]::IsNullOrWhiteSpace($installName) -or [string]::IsNullOrWhiteSpace($installParent)) {
    throw "InstallDir must name a new directory below an existing or creatable parent."
}
[System.IO.Directory]::CreateDirectory($installParent) | Out-Null
$staging = Join-Path $installParent ".$installName.install-$PID-$([Guid]::NewGuid().ToString('N'))"

try {
    [System.IO.Directory]::CreateDirectory($staging) | Out-Null
    foreach ($record in $records) {
        $destination = Join-Path $staging $record.Relative.Replace('/', [System.IO.Path]::DirectorySeparatorChar)
        $parent = [System.IO.Path]::GetDirectoryName($destination)
        [System.IO.Directory]::CreateDirectory($parent) | Out-Null
        Copy-Item -LiteralPath $record.Source -Destination $destination
    }
    Copy-Item -LiteralPath $metadataPath -Destination (Join-Path $staging "package.tsv")
    Copy-Item -LiteralPath $manifestPath -Destination (Join-Path $staging "files.tsv")

    $installation = @(
        "key`tvalue",
        "format`tbiogeo-windows-installation-v3",
        "status`tcomplete",
        "package_format`tbiogeo-windows-package-v3",
        "package_name`t$($metadata.package_name)",
        "version`t$($metadata.version)",
        "target`t$($metadata.target)",
        "executable`tbiogeo-cli.exe",
        "files_manifest`tfiles.tsv",
        "payload_verified`ttrue",
        "authenticode_status`t$($buildInfo.authenticode_status)",
        "authenticode_signer_thumbprint`t$($buildInfo.authenticode_signer_thumbprint)",
        "authenticode_timestamp_status`t$($buildInfo.authenticode_timestamp_status)",
        "build_origin`t$($buildInfo.build_origin)",
        "source_revision`t$($buildInfo.source_revision)",
        "source_revision_git_head_match`t$($buildInfo.source_revision_git_head_match)",
        "release_class`t$($metadata.release_class)",
        "project_license_status`t$($metadata.project_license_status)",
        "public_distribution_allowed`t$($metadata.public_distribution_allowed)"
    ) -join "`n"
    Write-Utf8NoBom -Path (Join-Path $staging "installation.tsv") -Text "$installation`n"

    $smokeOutput = & (Join-Path $staging "biogeo-cli.exe") --help 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "Installed executable smoke test failed: $($smokeOutput -join [Environment]::NewLine)"
    }
    Move-Item -LiteralPath $staging -Destination $installPath
}
catch {
    if (Test-Path -LiteralPath $staging) {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
    throw
}

foreach ($line in $installation.Split("`n") | Select-Object -Skip 1) {
    if (-not [string]::IsNullOrEmpty($line)) {
        Write-Output $line
    }
}
Write-Output "install_dir`t$installPath"
