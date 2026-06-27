# Duskcue — Self-hosted media streaming server
# Copyright (C) 2026-2026 Duskcue Contributors
#
# This program is free software: you can redistribute it and/or modify
# it under the terms of the GNU Affero General Public License as published by
# the Free Software Foundation, either version 3 of the License, or
# (at your option) any later version.
#
# This program is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
# GNU Affero General Public License for more details.
#
# You should have received a copy of the GNU Affero General Public License
# along with this program. If not, see <https://www.gnu.org/licenses/>.

[CmdletBinding()]
param(
    [int]$Port = 55432,
    [string]$PostgresImage = "postgres:18-alpine",
    [string]$Database = "duskcue_migration",
    [string]$User = "duskcue",
    [switch]$RunTests,
    [switch]$KeepAlive
)

$ErrorActionPreference = "Stop"
$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$composeFile = Join-Path $repoRoot "docker\compose.migrations.yml"
$projectName = "duskcue-migrations-$([guid]::NewGuid().ToString("N").Substring(0, 12))"
$password = [guid]::NewGuid().ToString("N")
$databaseUrl = "postgresql://${User}:${password}@127.0.0.1:${Port}/${Database}"
$previousDuskcueDatabaseUrl = $env:DUSKCUE_DATABASE_URL
$previousDatabaseUrl = $env:DATABASE_URL
$previousImage = $env:DUSKCUE_MIGRATION_POSTGRES_IMAGE
$previousPort = $env:DUSKCUE_MIGRATION_POSTGRES_PORT
$previousDb = $env:DUSKCUE_MIGRATION_POSTGRES_DB
$previousUser = $env:DUSKCUE_MIGRATION_POSTGRES_USER
$previousPassword = $env:DUSKCUE_MIGRATION_POSTGRES_PASSWORD

function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)]
        [string]$FilePath,
        [Parameter(Mandatory = $true)]
        [string[]]$Arguments
    )

    & $FilePath @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Command failed with exit code ${LASTEXITCODE}: $FilePath $($Arguments -join ' ')"
    }
}

function Test-CommandAvailable {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Name
    )

    if (-not (Get-Command $Name -ErrorAction SilentlyContinue)) {
        throw "Required command not found on PATH: $Name"
    }
}

function Wait-PostgresReady {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ComposeFile,
        [Parameter(Mandatory = $true)]
        [string]$ProjectName,
        [Parameter(Mandatory = $true)]
        [string]$User,
        [Parameter(Mandatory = $true)]
        [string]$Database,
        [int]$Attempts = 60
    )

    for ($attempt = 1; $attempt -le $Attempts; $attempt++) {
        docker compose -f $ComposeFile -p $ProjectName exec -T postgres pg_isready -U $User -d $Database *> $null
        if ($LASTEXITCODE -eq 0) {
            return
        }

        Start-Sleep -Seconds 2
    }

    throw "PostgreSQL did not become ready within $($Attempts * 2) seconds"
}

try {
    Test-CommandAvailable -Name "docker"
    Test-CommandAvailable -Name "cargo"
    Invoke-Checked -FilePath "docker" -Arguments @("compose", "version")

    $env:DUSKCUE_MIGRATION_POSTGRES_IMAGE = $PostgresImage
    $env:DUSKCUE_MIGRATION_POSTGRES_PORT = $Port.ToString()
    $env:DUSKCUE_MIGRATION_POSTGRES_DB = $Database
    $env:DUSKCUE_MIGRATION_POSTGRES_USER = $User
    $env:DUSKCUE_MIGRATION_POSTGRES_PASSWORD = $password
    $env:DUSKCUE_DATABASE_URL = $databaseUrl
    $env:DATABASE_URL = $databaseUrl

    Write-Host "Starting disposable PostgreSQL with Compose project '$projectName'"
    Invoke-Checked -FilePath "docker" -Arguments @("compose", "-f", $composeFile, "-p", $projectName, "up", "-d")
    Wait-PostgresReady -ComposeFile $composeFile -ProjectName $projectName -User $User -Database $Database

    Write-Host "Running embedded SQLx migrations against disposable PostgreSQL"
    Push-Location $repoRoot
    try {
        Invoke-Checked -FilePath "cargo" -Arguments @("run", "-p", "duskcue", "--bin", "verify_migrations")
    }
    finally {
        Pop-Location
    }

    if ($RunTests) {
        Write-Host "Running server tests against disposable PostgreSQL"
        Push-Location $repoRoot
        try {
            Invoke-Checked -FilePath "cargo" -Arguments @("test", "-p", "duskcue")
        }
        finally {
            Pop-Location
        }
    }

    Write-Host "Migration verification completed successfully"
}
finally {
    if ($KeepAlive) {
        Write-Host "Keeping Compose project '$projectName' alive for inspection"
        Write-Host "DATABASE_URL=$databaseUrl"
    } else {
        Write-Host "Cleaning up Compose project '$projectName'"
        docker compose -f $composeFile -p $projectName down -v --remove-orphans
    }

    $env:DUSKCUE_DATABASE_URL = $previousDuskcueDatabaseUrl
    $env:DATABASE_URL = $previousDatabaseUrl
    $env:DUSKCUE_MIGRATION_POSTGRES_IMAGE = $previousImage
    $env:DUSKCUE_MIGRATION_POSTGRES_PORT = $previousPort
    $env:DUSKCUE_MIGRATION_POSTGRES_DB = $previousDb
    $env:DUSKCUE_MIGRATION_POSTGRES_USER = $previousUser
    $env:DUSKCUE_MIGRATION_POSTGRES_PASSWORD = $previousPassword
}
