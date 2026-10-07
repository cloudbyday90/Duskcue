[CmdletBinding()]
param(
    [switch]$Once,
    [ValidateRange(250, 60000)]
    [int]$IntervalMs = 2000
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)

function Read-Counter {
    param([object]$Value)
    if ($null -eq $Value) {
        throw [System.IO.InvalidDataException]::new('A required memory counter is unavailable.')
    }
    return [uint64]$Value
}

try {
    if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
        throw [System.PlatformNotSupportedException]::new('Windows memory measurement is required.')
    }
    $operatingSystem = Get-CimInstance Win32_OperatingSystem -Property TotalVisibleMemorySize -OperationTimeoutSec 10
    $totalPhysicalBytes = (Read-Counter $operatingSystem.TotalVisibleMemorySize) * 1024
    if ($totalPhysicalBytes -eq 0) {
        throw [System.IO.InvalidDataException]::new('Total physical memory is unavailable.')
    }
    do {
        $started = [System.Diagnostics.Stopwatch]::StartNew()
        $timestamp = [System.DateTime]::UtcNow.ToString('o')
        $memory = Get-CimInstance Win32_PerfFormattedData_PerfOS_Memory -Property AvailableBytes, CommittedBytes, CommitLimit -OperationTimeoutSec 10
        $processCounters = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Property IDProcess, CreatingProcessID, Name, PrivateBytes, WorkingSet -OperationTimeoutSec 10
        $processes = @(foreach ($process in $processCounters) {
            $processId = Read-Counter $process.IDProcess
            if ($processId -gt 0 -and $process.Name -ne '_Total') {
                [ordered]@{
                    pid = $processId
                    parentPid = Read-Counter $process.CreatingProcessID
                    name = [string]($process.Name -replace '#\d+$', '')
                    privateBytes = Read-Counter $process.PrivateBytes
                    workingSetBytes = Read-Counter $process.WorkingSet
                }
            }
        })
        $sample = [ordered]@{
            timestamp = $timestamp
            totalPhysicalBytes = $totalPhysicalBytes
            availablePhysicalBytes = Read-Counter $memory.AvailableBytes
            committedBytes = Read-Counter $memory.CommittedBytes
            commitLimitBytes = Read-Counter $memory.CommitLimit
            processes = $processes
        }
        if ($sample.commitLimitBytes -eq 0 -or $sample.availablePhysicalBytes -gt $totalPhysicalBytes) {
            throw [System.IO.InvalidDataException]::new('The memory counters are inconsistent.')
        }
        Write-Output ($sample | ConvertTo-Json -Compress -Depth 4)
        $started.Stop()
        if (-not $Once) {
            $remainingMs = [math]::Max(0, $IntervalMs - $started.ElapsedMilliseconds)
            if ($remainingMs -gt 0) {
                Start-Sleep -Milliseconds $remainingMs
            }
        }
    } while (-not $Once)
} catch {
    [ordered]@{
        timestamp = [System.DateTime]::UtcNow.ToString('o')
        error = [ordered]@{
            code = 'MEMORY_MEASUREMENT_FAILED'
            message = 'Windows memory measurement failed.'
            type = $_.Exception.GetType().Name
        }
    } | ConvertTo-Json -Compress -Depth 3 | Write-Output
    exit 1
}
