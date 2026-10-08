# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026 Duskcue Contributors
#
# This program is free software: licensed under AGPL-3.0
# See LICENSE file for details.

[CmdletBinding()]
param([switch]$AllowWebViewInstall)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows' -or $env:RUNNER_ARCH -ne 'X64') {
    throw 'Prerequisite installation is restricted to a disposable GitHub-hosted Windows x64 job.'
}
if (-not $env:GITHUB_WORKSPACE -or -not $env:RUNNER_TEMP -or -not $env:GITHUB_ENV -or -not $env:GITHUB_OUTPUT) {
    throw 'The hosted workspace, temporary directory and workflow output files are required.'
}
$ciWorkspace = (Get-Item -LiteralPath $env:GITHUB_WORKSPACE).FullName
$ciTemporary = (Get-Item -LiteralPath $env:RUNNER_TEMP).FullName
if ((Get-Item -LiteralPath $ciTemporary).Attributes -band [System.IO.FileAttributes]::ReparsePoint) { throw 'The hosted temporary directory is an unexpected reparse point.' }
$ciToolRoot = Join-Path $ciTemporary "duskcue-tonight-tools-$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $ciToolRoot | Out-Null
$ciToolRoot = (Get-Item -LiteralPath $ciToolRoot).FullName

function Assert-OwnedToolPath {
    param([string]$Path)
    $absolute = [System.IO.Path]::GetFullPath($Path)
    if (-not $absolute.StartsWith($ciToolRoot + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) {
        throw 'The prerequisite artifact is outside its owned temporary directory.'
    }
    return $absolute
}

function Read-WebViewVersions {
    $client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    foreach ($key in @("HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$client", "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$client")) {
        if (Test-Path -LiteralPath $key) {
            $value = (Get-ItemProperty -LiteralPath $key -Name pv -ErrorAction SilentlyContinue).pv
            if ($value -match '^\d+\.\d+\.\d+\.\d+$' -and $value -ne '0.0.0.0') { [string]$value }
        }
    }
}

$ciProof = [ordered]@{
    version = 1
    kind = 'native-hosted-installation'
    sourceCommit = $env:GITHUB_SHA
    runId = $env:GITHUB_RUN_ID
    attempt = $env:GITHUB_RUN_ATTEMPT
    job = $env:GITHUB_JOB
    ownedDirectory = $ciToolRoot
    status = 'in_progress'
}
$ownership = Assert-OwnedToolPath (Join-Path $ciToolRoot 'ownership.json')
$ciProof | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $ownership -Encoding utf8NoBOM
$ciEvidenceDirectory = Join-Path $ciWorkspace '.cache/tonight-desktop'
New-Item -ItemType Directory -Path $ciEvidenceDirectory -Force | Out-Null
$ciEvidence = Join-Path $ciEvidenceDirectory 'hosted-ci-installation.json'

try {
    $ffmpegUrl = 'https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-8.1.2-essentials_build.zip'
    $ffmpegHash = 'db580001caa24ac104c8cb856cd113a87b0a443f7bdf47d8c12b1d740584a2ec'
    $archive = Assert-OwnedToolPath (Join-Path $ciToolRoot 'ffmpeg-8.1.2-essentials_build.zip')
    Invoke-WebRequest -Uri $ffmpegUrl -OutFile $archive -TimeoutSec 180
    if ((Get-Item -LiteralPath $archive).Length -gt 256MB -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $ffmpegHash) {
        throw 'The pinned FFmpeg archive failed its size or SHA-256 check; nothing was extracted.'
    }
    $expanded = Assert-OwnedToolPath (Join-Path $ciToolRoot 'expanded')
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [System.IO.Compression.ZipFile]::OpenRead($archive)
    try {
        if ($zip.Entries.Count -gt 2000) { throw 'The FFmpeg archive has an unexpected entry count.' }
        [long]$expandedBytes = 0
        foreach ($entry in $zip.Entries) {
            $expandedBytes += $entry.Length
            if ($expandedBytes -gt 512MB -or $entry.FullName -notlike 'ffmpeg-8.1.2-essentials_build/*') { throw 'The pinned FFmpeg archive has an unexpected layout or expanded size.' }
            $entryPath = Assert-OwnedToolPath (Join-Path $expanded $entry.FullName)
            if (-not $entryPath.StartsWith($expanded + [System.IO.Path]::DirectorySeparatorChar, [System.StringComparison]::OrdinalIgnoreCase)) { throw 'Archive extraction would leave its owned output directory.' }
        }
    } finally { $zip.Dispose() }
    Expand-Archive -LiteralPath $archive -DestinationPath $expanded
    $ffmpeg = Assert-OwnedToolPath (Join-Path $expanded 'ffmpeg-8.1.2-essentials_build/bin/ffmpeg.exe')
    if (-not (Test-Path -LiteralPath $ffmpeg -PathType Leaf)) { throw 'The pinned FFmpeg executable is missing.' }
    $ciProof.ffmpeg = [ordered]@{ version = '8.1.2'; url = $ffmpegUrl; sha256 = $ffmpegHash; path = $ffmpeg; globalInstall = $false }

    $runtimeVersions = @(Read-WebViewVersions)
    if (-not $runtimeVersions.Count) {
        if (-not $AllowWebViewInstall) { throw 'WebView2 Runtime is missing; explicitly permit its installation in this disposable hosted job.' }
        $bootstrapperUrl = 'https://go.microsoft.com/fwlink/p/?LinkId=2124703'
        $bootstrapper = Assert-OwnedToolPath (Join-Path $ciToolRoot 'MicrosoftEdgeWebview2Setup.exe')
        Invoke-WebRequest -Uri $bootstrapperUrl -OutFile $bootstrapper -TimeoutSec 120
        if ((Get-Item -LiteralPath $bootstrapper).Length -gt 16MB) { throw 'The Evergreen bootstrapper is unexpectedly large.' }
        $signature = Get-AuthenticodeSignature -LiteralPath $bootstrapper
        if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid -or $signature.SignerCertificate.Subject -notmatch '(^|,\s*)O=Microsoft Corporation(,|$)') {
            throw 'The Evergreen bootstrapper lacks a valid Microsoft Authenticode signature; it will not be run.'
        }
        $ciProof.webviewBootstrapper = [ordered]@{ url = $bootstrapperUrl; sha256 = (Get-FileHash -LiteralPath $bootstrapper -Algorithm SHA256).Hash.ToLowerInvariant(); signer = $signature.SignerCertificate.Subject; thumbprint = $signature.SignerCertificate.Thumbprint }
        $installerStartedAt = [DateTime]::UtcNow
        $installer = Start-Process -FilePath $bootstrapper -ArgumentList '/silent', '/install' -WindowStyle Hidden -PassThru
        if (-not $installer.WaitForExit(180000)) {
            $actual = Get-CimInstance Win32_Process -Filter "ProcessId = $($installer.Id)"
            if ($actual -and $actual.ParentProcessId -eq $PID -and $actual.Name -eq 'MicrosoftEdgeWebview2Setup.exe' -and $actual.CreationDate.ToUniversalTime() -ge $installerStartedAt.AddSeconds(-2) -and $actual.CreationDate.ToUniversalTime() -le $installerStartedAt.AddSeconds(5)) {
                & taskkill.exe /PID $installer.Id /T /F | Out-Null
            }
            throw 'The owned Evergreen installer exceeded its deadline.'
        }
        $runtimeVersions = @(Read-WebViewVersions)
        if (-not $runtimeVersions.Count) { throw "The Evergreen installer did not establish the Runtime (exit $($installer.ExitCode))." }
    }
    $ciProof.webviewVersions = $runtimeVersions
    $ciProof.status = 'passed'
    $env:DUSKCUE_TEST_FFMPEG = $ffmpeg
    "DUSKCUE_TEST_FFMPEG=$ffmpeg" | Out-File -LiteralPath $env:GITHUB_ENV -Append -Encoding utf8NoBOM
    "ffmpeg=$ffmpeg" | Out-File -LiteralPath $env:GITHUB_OUTPUT -Append -Encoding utf8NoBOM
} catch {
    $ciProof.status = 'failed'
    $ciProof.error = $_.Exception.Message
    throw
} finally {
    $ciProof | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $ciEvidence -Encoding utf8NoBOM
}
