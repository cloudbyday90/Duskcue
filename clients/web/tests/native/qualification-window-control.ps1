# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026 Duskcue Contributors
#
# This program is free software: licensed under AGPL-3.0
# See LICENSE file for details.

param([Parameter(Mandatory = $true)][string]$IdentityPath, [switch]$RestoreOnly)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') { throw 'Window actions require a GitHub-hosted Windows qualification job.' }
$identity = Get-Content -LiteralPath $IdentityPath -Raw | ConvertFrom-Json
if ($identity.context.sourceCommit -ne $env:GITHUB_SHA -or $identity.context.runId -ne $env:GITHUB_RUN_ID -or $identity.context.attempt -ne $env:GITHUB_RUN_ATTEMPT -or $identity.context.job -ne $env:GITHUB_JOB) { throw 'Window identity belongs to another hosted job.' }
if ($identity.identifier -notmatch '^com\.duskcue\.tonightqualification\.t[a-f0-9]{32}$') { throw 'A unique native qualification identifier is required.' }
$qualificationId = $identity.identifier.Substring($identity.identifier.LastIndexOf('.t') + 2)
$expectedDirectory = [IO.Path]::GetFullPath((Join-Path $env:GITHUB_WORKSPACE ".cache/tonight-desktop/$qualificationId"))
if ([IO.Path]::GetFullPath($IdentityPath) -ine (Join-Path $expectedDirectory 'native-window-identity.json')) { throw 'Window identity is outside its owned qualification directory.' }
$expectedExecutable = [IO.Path]::GetFullPath((Join-Path $env:GITHUB_WORKSPACE 'target/debug/duskcue-tonight-qualification.exe'))
if ($identity.host.path -ine $expectedExecutable -or $identity.host.name -ne 'duskcue-tonight-qualification.exe' -or $identity.host.sha256 -notmatch '^[a-f0-9]{64}$') { throw 'Window identity is not the expected qualification executable.' }

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
public static class TonightOwnedWindow {
    private delegate bool WindowCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(WindowCallback callback, IntPtr parameter);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    public static IntPtr[] Find(uint expected) {
        var windows = new List<IntPtr>();
        WindowCallback callback = (window, parameter) => {
            uint process;
            GetWindowThreadProcessId(window, out process);
            if (process == expected && GetWindow(window, 4) == IntPtr.Zero && IsWindowVisible(window)) windows.Add(window);
            return true;
        };
        if (!EnumWindows(callback, IntPtr.Zero)) throw new InvalidOperationException("Owned window enumeration failed.");
        return windows.ToArray();
    }
}
'@

function Assert-OwnedHost {
    $row = Get-CimInstance Win32_Process -Filter "ProcessId = $([int]$identity.host.pid)" -OperationTimeoutSec 5
    if (-not $row -or $row.ParentProcessId -ne $identity.host.parentPid -or $row.Name -ine $identity.host.name -or $row.ExecutablePath -ine $expectedExecutable -or -not $row.CreationDate -or $row.CreationDate.ToUniversalTime().ToString('o') -ne $identity.host.createdAt) { throw 'The native process identity changed; no window was controlled.' }
    $digest = (Get-FileHash -LiteralPath $expectedExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($digest -ne $identity.host.sha256) { throw 'The native executable hash changed; no window was controlled.' }
    return [ordered]@{ pid = [int]$row.ProcessId; parentPid = [int]$row.ParentProcessId; name = $row.Name; path = $row.ExecutablePath; createdAt = $row.CreationDate.ToUniversalTime().ToString('o'); sha256 = $digest }
}

$pinnedWindow = [IntPtr]::Zero
$actions = [Collections.Generic.List[object]]::new()
function Assert-OwnedWindow {
    $actual = Assert-OwnedHost
    $candidates = @([TonightOwnedWindow]::Find([uint32]$identity.host.pid))
    if ($candidates.Count -ne 1 -or ($pinnedWindow -ne [IntPtr]::Zero -and $candidates[0] -ne $pinnedWindow)) { throw 'The native HWND is missing, ambiguous or changed.' }
    if ($identity.host.handle -and $candidates[0].ToInt64().ToString() -ne $identity.host.handle) { throw 'The pinned native HWND changed.' }
    return [pscustomobject]@{ actual = $actual; window = $candidates[0] }
}

function Invoke-OwnedWindow {
    param([string]$Action)
    $verified = Assert-OwnedWindow
    $command = if ($Action -eq 'minimize') { 6 } elseif ($Action -eq 'restore') { 9 } else { throw 'Only minimize and restore window actions are permitted.' }
    if ($Action -eq 'minimize' -or [TonightOwnedWindow]::IsIconic($verified.window)) {
        if (-not [TonightOwnedWindow]::ShowWindowAsync($verified.window, $command)) { throw 'Owned window action was not accepted.' }
    }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    while ([TonightOwnedWindow]::IsIconic($verified.window) -ne ($Action -eq 'minimize')) {
        if ($timer.ElapsedMilliseconds -gt 5000) { throw 'Owned native window did not reach its requested state.' }
        Start-Sleep -Milliseconds 25
    }
    $confirmed = Assert-OwnedWindow
    $result = [ordered]@{ action = $Action; handle = $confirmed.window.ToInt64().ToString(); iconic = [TonightOwnedWindow]::IsIconic($confirmed.window); host = $confirmed.actual; handles = @($confirmed.window.ToInt64().ToString()); observedAt = [DateTime]::UtcNow.ToString('o') }
    $actions.Add($result)
    return $result
}

$finallyRestored = $false
$finalError = $null
try {
    $initial = Assert-OwnedWindow
    $pinnedWindow = $initial.window
    if ($RestoreOnly) {
        Invoke-OwnedWindow 'restore' | ConvertTo-Json -Depth 6 -Compress
    } else {
        if ([TonightOwnedWindow]::IsIconic($pinnedWindow)) { throw 'The native qualification window was already minimized.' }
        [ordered]@{ id = 0; ready = $true; handle = $pinnedWindow.ToInt64().ToString(); handles = @($pinnedWindow.ToInt64().ToString()); host = $initial.actual } | ConvertTo-Json -Depth 6 -Compress | Write-Output
        while ($null -ne ($line = [Console]::ReadLine())) {
            $request = $line | ConvertFrom-Json
            try {
                if ($request.action -eq 'close') { break }
                $reply = Invoke-OwnedWindow $request.action
                $reply.id = $request.id
                $reply | ConvertTo-Json -Depth 6 -Compress | Write-Output
            } catch {
                [ordered]@{ id = $request.id; error = $_.Exception.Message } | ConvertTo-Json -Compress | Write-Output
                throw
            }
        }
    }
} finally {
    try {
        if ($pinnedWindow -ne [IntPtr]::Zero) { Invoke-OwnedWindow 'restore' | Out-Null; $finallyRestored = $true }
    } catch { $finalError = $_.Exception.Message }
    [ordered]@{ actions = $actions.ToArray(); finallyRestored = $finallyRestored; restoreError = $finalError } | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $expectedDirectory $(if ($RestoreOnly) { 'native-window-fallback-restore.json' } else { 'native-window-actions.json' })) -Encoding utf8NoBOM
    if ($finalError) { throw $finalError }
}
