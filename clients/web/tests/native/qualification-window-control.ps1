# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026 Duskcue Contributors
#
# This program is free software: licensed under AGPL-3.0
# See LICENSE file for details.

param([Parameter(Mandatory = $true)][string]$IdentityPath, [switch]$RestoreOnly)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted' -or $env:RUNNER_OS -ne 'Windows') { throw 'Window actions require a GitHub-hosted Windows qualification job.' }
$identity = Get-Content -LiteralPath $IdentityPath -Raw | ConvertFrom-Json -DateKind String
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
using System.Text;
public sealed class TonightWindowCandidate {
    public IntPtr Handle { get; set; }
    public IntPtr Owner { get; set; }
    public bool Visible { get; set; }
    public bool Iconic { get; set; }
    public bool ClientRectReadable { get; set; }
    public int ClientWidth { get; set; }
    public int ClientHeight { get; set; }
    public string ClassName { get; set; }
}
public static class TonightOwnedWindow {
    [StructLayout(LayoutKind.Sequential)] private struct Rect { public int Left, Top, Right, Bottom; }
    private delegate bool WindowCallback(IntPtr window, IntPtr parameter);
    [DllImport("user32.dll")] private static extern bool EnumWindows(WindowCallback callback, IntPtr parameter);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
    [DllImport("user32.dll")] private static extern IntPtr GetWindow(IntPtr window, uint command);
    [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr window);
    [DllImport("user32.dll")] public static extern bool ShowWindowAsync(IntPtr window, int command);
    [DllImport("user32.dll")] private static extern bool GetClientRect(IntPtr window, out Rect rectangle);
    [DllImport("user32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)] private static extern int GetClassNameW(IntPtr window, StringBuilder name, int length);
    public static TonightWindowCandidate[] Find(uint expected) {
        var windows = new List<TonightWindowCandidate>();
        string failure = null;
        WindowCallback callback = (window, parameter) => {
            uint process;
            GetWindowThreadProcessId(window, out process);
            if (process == expected) {
                if (windows.Count >= 64) { failure = "Owned window metadata exceeded its bound."; return false; }
                Rect rectangle;
                bool readable = GetClientRect(window, out rectangle);
                var name = new StringBuilder(256);
                if (GetClassNameW(window, name, name.Capacity) == 0) { failure = "Owned window class could not be read."; return false; }
                windows.Add(new TonightWindowCandidate { Handle = window, Owner = GetWindow(window, 4), Visible = IsWindowVisible(window), Iconic = IsIconic(window),
                    ClientRectReadable = readable, ClientWidth = rectangle.Right - rectangle.Left, ClientHeight = rectangle.Bottom - rectangle.Top, ClassName = name.ToString() });
            }
            return true;
        };
        if (!EnumWindows(callback, IntPtr.Zero)) throw new InvalidOperationException(failure ?? "Owned window enumeration failed.");
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
$windowSnapshots = [Collections.Generic.List[object]]::new()
function Select-OwnedContentWindows {
    param([object[]]$Windows, [IntPtr]$PinnedWindow = [IntPtr]::Zero)
    @($Windows | Where-Object {
        $_.Visible -eq $true -and $_.Owner -eq [IntPtr]::Zero -and $_.ClientRectReadable -eq $true -and
        (($_.ClientWidth -gt 0 -and $_.ClientHeight -gt 0) -or ($PinnedWindow -ne [IntPtr]::Zero -and $_.Handle -eq $PinnedWindow -and $_.Iconic -eq $true))
    })
}
function Assert-OwnedWindow {
    $actual = Assert-OwnedHost
    $knownWindow = $pinnedWindow
    if ($knownWindow -eq [IntPtr]::Zero -and $identity.host.handle) {
        if ($identity.host.handle -notmatch '^[1-9][0-9]*$') { throw 'The persisted native HWND is invalid.' }
        $knownWindow = [IntPtr][long]$identity.host.handle
    }
    $windows = @([TonightOwnedWindow]::Find([uint32]$identity.host.pid))
    $candidates = @(Select-OwnedContentWindows $windows -PinnedWindow $knownWindow)
    $metadata = @($windows | ForEach-Object { [ordered]@{ handle = $_.Handle.ToInt64().ToString(); owner = $_.Owner.ToInt64().ToString(); visible = $_.Visible; iconic = $_.Iconic; clientRectReadable = $_.ClientRectReadable; width = $_.ClientWidth; height = $_.ClientHeight; className = $_.ClassName } })
    if ($windowSnapshots.Count -ge 128) { throw 'Owned window snapshots exceeded their bound.' }
    $windowSnapshots.Add([ordered]@{ pid = [int]$identity.host.pid; eligibleCount = $candidates.Count; windows = $metadata; observedAt = [DateTime]::UtcNow.ToString('o') })
    if ($candidates.Count -ne 1 -or ($knownWindow -ne [IntPtr]::Zero -and $candidates[0].Handle -ne $knownWindow)) { throw "The native content HWND is missing, ambiguous or changed ($($candidates.Count) eligible of $($windows.Count) exact-PID windows)." }
    if ($identity.host.handle -and $candidates[0].Handle.ToInt64().ToString() -ne $identity.host.handle) { throw 'The pinned native HWND changed.' }
    return [pscustomobject]@{ actual = $actual; window = $candidates[0].Handle }
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
    [ordered]@{ actions = $actions.ToArray(); windowSnapshots = $windowSnapshots.ToArray(); finallyRestored = $finallyRestored; restoreError = $finalError } | ConvertTo-Json -Depth 7 | Set-Content -LiteralPath (Join-Path $expectedDirectory $(if ($RestoreOnly) { 'native-window-fallback-restore.json' } else { 'native-window-actions.json' })) -Encoding utf8NoBOM
    if ($finalError) { throw $finalError }
}
