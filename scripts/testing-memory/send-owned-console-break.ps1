# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
# Licensed under AGPL-3.0. See LICENSE for details.

param([Parameter(Mandatory=$true)][int]$TargetPid)
$ErrorActionPreference = 'Stop'
$resource = $env:DUSKCUE_TEST_RESOURCE_ID
$parsed = [guid]::Parse($resource)
$target = Get-CimInstance Win32_Process -Filter "ProcessId=$TargetPid"
$expected = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../target/debug/duskcue.exe'))
if (-not $target -or $target.ExecutablePath -ne $expected -or -not $target.CommandLine.Contains($parsed.ToString())) { throw 'Owned signal target identity failed.' }
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class OwnedSignal {
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool FreeConsole();
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool AttachConsole(uint processId);
 [DllImport("kernel32.dll", SetLastError=true)] public static extern bool GenerateConsoleCtrlEvent(uint ctrlEvent, uint processGroupId);
}
'@
[void][OwnedSignal]::FreeConsole()
if (-not [OwnedSignal]::AttachConsole([uint32]$TargetPid)) { throw 'Owned console attachment failed.' }
if (-not [OwnedSignal]::GenerateConsoleCtrlEvent(1,[uint32]$TargetPid)) { throw 'Owned console signal failed.' }
[void][OwnedSignal]::FreeConsole()
