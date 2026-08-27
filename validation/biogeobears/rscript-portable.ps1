function Invoke-PortableRScript {
    param(
        [Parameter(Mandatory = $true)][string[]]$Arguments,
        [string]$FailureMessage = "Rscript failed"
    )

    $changedLocaleVariables = @{}
    try {
        if ([Environment]::OSVersion.Platform -eq [PlatformID]::Win32NT) {
            foreach ($name in @("LANG", "LC_ALL", "LC_CTYPE", "LC_COLLATE", "LC_MONETARY", "LC_TIME")) {
                $value = [Environment]::GetEnvironmentVariable($name, "Process")
                if ($null -ne $value -and $value -match "(?i)^C\.UTF-?8$") {
                    $changedLocaleVariables[$name] = $value
                    [Environment]::SetEnvironmentVariable($name, $null, "Process")
                }
            }
        }

        & Rscript @Arguments
        $exitCode = $LASTEXITCODE
        if ($exitCode -ne 0) {
            throw "$FailureMessage with exit code $exitCode"
        }
    }
    finally {
        foreach ($entry in $changedLocaleVariables.GetEnumerator()) {
            [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value, "Process")
        }
    }
}
