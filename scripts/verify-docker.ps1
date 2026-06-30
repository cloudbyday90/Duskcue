param(
    [string]$Image = "duskcue:docker-smoke",
    [switch]$SkipBuild,
    [int]$Port = 0,
    [int]$TimeoutSeconds = 180,
    [switch]$KeepResources
)

$ErrorActionPreference = "Stop"

if ($Port -eq 0) {
    $Port = Get-Random -Minimum 49152 -Maximum 60999
}

$suffix = [guid]::NewGuid().ToString("N").Substring(0, 12)
$container = "duskcue-smoke-$suffix"
$dataVolume = "duskcue-smoke-data-$suffix"
$cacheVolume = "duskcue-smoke-cache-$suffix"
$mediaDir = Join-Path ([System.IO.Path]::GetTempPath()) "duskcue-smoke-media-$suffix"

function Invoke-Docker {
    & docker @args
    if ($LASTEXITCODE -ne 0) {
        throw "docker $($args -join ' ') failed with exit code $LASTEXITCODE"
    }
}

function Cleanup {
    if ($KeepResources) {
        Write-Host "Keeping resources: $container, $dataVolume, $cacheVolume, $mediaDir"
        return
    }

    docker rm -f $container *> $null
    docker volume rm $dataVolume $cacheVolume *> $null
    if (Test-Path $mediaDir) {
        Remove-Item -LiteralPath $mediaDir -Recurse -Force
    }
}

function Test-HttpOk {
    param([string]$Uri)

    curl.exe --fail --silent --show-error --max-time 5 $Uri *> $null
    return $LASTEXITCODE -eq 0
}

function Get-HttpStatus {
    param([string]$Uri)

    $status = & curl.exe --silent --output NUL --write-out "%{http_code}" --max-time 5 $Uri
    if ($LASTEXITCODE -ne 0) {
        return 0
    }
    return [int]$status
}

try {
    Invoke-Docker version *> $null
    New-Item -ItemType Directory -Path $mediaDir | Out-Null

    if (-not $SkipBuild) {
        Invoke-Docker build --target runtime -t $Image .
    }

    Invoke-Docker volume create $dataVolume *> $null
    Invoke-Docker volume create $cacheVolume *> $null

    Invoke-Docker run -d `
        --name $container `
        --read-only `
        --security-opt no-new-privileges `
        --cap-drop ALL `
        --cap-add CHOWN `
        --cap-add SETUID `
        --cap-add SETGID `
        --tmpfs "/data/transcode:size=512m,mode=1777" `
        --tmpfs "/var/run/postgresql:uid=1000,gid=1000,mode=770" `
        --tmpfs "/tmp:size=64m,mode=1777" `
        -e PUID=1000 `
        -e PGID=1000 `
        -e DUSKCUE_ENVIRONMENT=production `
        -e DUSKCUE_LOG_LEVEL=info `
        -p "127.0.0.1:${Port}:48027" `
        -v "${dataVolume}:/data" `
        -v "${cacheVolume}:/cache" `
        -v "${mediaDir}:/media/test:ro" `
        $Image *> $null

    $deadline = (Get-Date).AddSeconds($TimeoutSeconds)
    $ready = $false
    do {
        Start-Sleep -Seconds 2
        if (Test-HttpOk "http://127.0.0.1:$Port/health/ready") {
            $ready = $true
            break
        } else {
            $state = docker inspect -f "{{.State.Status}}" $container 2>$null
            if ($state -ne "running") {
                docker logs $container
                throw "Container exited before readiness"
            }
        }
    } while ((Get-Date) -lt $deadline)

    if (-not $ready) {
        docker logs $container
        throw "Timed out waiting for readiness"
    }

    Invoke-Docker exec --user 1000:1000 $container pg_isready -q -h /var/run/postgresql -U duskcue
    Invoke-Docker exec --user 1000:1000 $container test -S /var/run/postgresql/.s.PGSQL.5432
    Invoke-Docker exec --user 1000:1000 $container test -w /data
    Invoke-Docker exec --user 1000:1000 $container test -w /cache
    Invoke-Docker exec --user 1000:1000 $container test -w /data/transcode

    if (-not (Test-HttpOk "http://127.0.0.1:$Port/health/live")) {
        throw "Liveness check failed"
    }

    $apiStatus = Get-HttpStatus "http://127.0.0.1:$Port/api/v1/events"
    if ($apiStatus -ge 500 -or $apiStatus -eq 0) {
        throw "SSE route returned $apiStatus"
    }

    Invoke-Docker stop --time 120 $container *> $null
    Invoke-Docker start $container *> $null

    $deadline = (Get-Date).AddSeconds(90)
    do {
        Start-Sleep -Seconds 2
        if (Test-HttpOk "http://127.0.0.1:$Port/health/ready") {
            Write-Host "Docker smoke verification passed on http://127.0.0.1:$Port"
            exit 0
        }
    } while ((Get-Date) -lt $deadline)

    docker logs $container
    throw "Container did not become ready after restart"
} finally {
    Cleanup
}
