# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026 Duskcue Contributors
#
# This program is free software: licensed under AGPL-3.0
# See LICENSE file for details.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') {
    throw 'Hosted prerequisite metadata is restricted to a GitHub-hosted Windows job.'
}

[Console]::Error.WriteLine('tonight-hosted:runtime_registry')
$runtimeClient = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$runtimeKeys = @("HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$runtimeClient", "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$runtimeClient")
$runtimeVersions = @(foreach ($runtimeKey in $runtimeKeys) {
    if (Test-Path -LiteralPath $runtimeKey) {
        $runtimeValue = (Get-ItemProperty -LiteralPath $runtimeKey -Name pv -ErrorAction SilentlyContinue).pv
        if ($runtimeValue) { [string]$runtimeValue }
    }
})
$desktopBounds = $null
[Console]::Error.WriteLine('tonight-hosted:display')
try {
    Add-Type -AssemblyName System.Windows.Forms
    $bounds = [System.Windows.Forms.SystemInformation]::VirtualScreen
    $desktopBounds = [ordered]@{ width = $bounds.Width; height = $bounds.Height; x = $bounds.X; y = $bounds.Y }
} catch {
    $desktopBounds = [ordered]@{ unavailable = $true }
}

[Console]::Error.WriteLine('tonight-hosted:session')
$sessionId = (Get-Process -Id $PID).SessionId
[Console]::Error.WriteLine('tonight-hosted:complete')
[ordered]@{
    webviewVersions = @($runtimeVersions | Select-Object -Unique)
    userInteractive = [System.Environment]::UserInteractive
    sessionId = $sessionId
    display = $desktopBounds
} | ConvertTo-Json -Compress -Depth 4
