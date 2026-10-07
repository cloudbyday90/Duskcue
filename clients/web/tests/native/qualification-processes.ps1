param([Parameter(Mandatory = $true)][string]$IdentityPath)

$ErrorActionPreference = 'Stop'
$identity = Get-Content -LiteralPath $IdentityPath -Raw | ConvertFrom-Json
$rows = @(Get-CimInstance Win32_Process -Property ProcessId, ParentProcessId, Name, CreationDate | ForEach-Object {
    if (-not $_.CreationDate) { return }
    [pscustomobject]@{
        pid = [int]$_.ProcessId
        parentPid = [int]$_.ParentProcessId
        name = $_.Name
        createdAt = $_.CreationDate.ToUniversalTime().ToString('o')
    }
})
$hostRow = $rows | Where-Object { $_.pid -eq $identity.host.pid } | Select-Object -First 1
$hostVerified = $false
if ($hostRow) {
    $started = [DateTimeOffset]::FromUnixTimeMilliseconds([long]$identity.host.startedAt)
    $created = [DateTimeOffset]::Parse($hostRow.createdAt)
    $hostVerified = $hostRow.parentPid -eq $identity.host.parentPid -and $hostRow.name -ieq $identity.host.name -and $created -ge $started.AddSeconds(-2) -and $created -le $started.AddSeconds(5)
}
$owned = [Collections.Generic.HashSet[int]]::new()
if ($hostVerified) { [void]$owned.Add([int]$identity.host.pid) }
foreach ($expected in @($identity.known)) {
    $actual = $rows | Where-Object { $_.pid -eq $expected.pid } | Select-Object -First 1
    if ($actual -and $actual.parentPid -eq $expected.parentPid -and $actual.name -ieq $expected.name -and $actual.createdAt -eq $expected.createdAt) {
        [void]$owned.Add([int]$actual.pid)
    }
}
$changed = $true
while ($changed) {
    $changed = $false
    foreach ($row in $rows) {
        if ($owned.Contains($row.parentPid) -and $owned.Add($row.pid)) { $changed = $true }
    }
}
$unverified = [Collections.Generic.HashSet[int]]::new()
if (-not $hostRow) {
    $started = [DateTimeOffset]::FromUnixTimeMilliseconds([long]$identity.host.startedAt).AddSeconds(-2)
    [void]$unverified.Add([int]$identity.host.pid)
    $changed = $true
    while ($changed) {
        $changed = $false
        foreach ($row in $rows) {
            if ($unverified.Contains($row.parentPid) -and -not $owned.Contains($row.pid) -and [DateTimeOffset]::Parse($row.createdAt) -ge $started -and $unverified.Add($row.pid)) { $changed = $true }
        }
    }
}
[pscustomobject]@{
    hostExists = [bool]$hostRow
    hostVerified = [bool]$hostVerified
    processes = @($rows | Where-Object { $owned.Contains($_.pid) })
    unverifiedExitedHostDescendants = @($rows | Where-Object { $unverified.Contains($_.pid) -and -not $owned.Contains($_.pid) })
} | ConvertTo-Json -Depth 6 -Compress
