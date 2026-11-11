# Smoke test for the release binary: help, offline render and standalone editor.
# No HID access, no `run`, no port 18618, no real %APPDATA% state.
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Exe
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

if (-not (Test-Path -LiteralPath $Exe -PathType Leaf)) {
    Write-Host "SMOKE FAILED: executable not found: $Exe" -ForegroundColor Red
    exit 1
}
$exePath = (Resolve-Path -LiteralPath $Exe).Path

$tempBase = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [System.IO.Path]::GetTempPath() }
$tempRoot = Join-Path $tempBase ("orcadeck-smoke-" + [guid]::NewGuid().ToString('N'))
$profilesDir = Join-Path $tempRoot 'profiles'
$pluginsDir = Join-Path $tempRoot 'plugins'
$dataDir = Join-Path $tempRoot 'data'
$outDir = Join-Path $tempRoot 'out'
$editProc = $null
$failure = $null
$cleanupFailure = $null

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

try {
    New-Item -ItemType Directory -Path $profilesDir, $pluginsDir, $dataDir, $outDir | Out-Null

    # Synthetic scene: three actions with a bundled 4x4 PNG; render fills the other keys black.
    $profileDir = Join-Path $profilesDir 'smoke.sdProfile'
    $imagesDir = Join-Path $profileDir 'Images'
    New-Item -ItemType Directory -Path $imagesDir | Out-Null
    $png = [Convert]::FromBase64String('iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVR4nGM4oaEBRwzEcQDRQxGBSNLB6wAAAABJRU5ErkJggg==')
    [System.IO.File]::WriteAllBytes((Join-Path $imagesDir 'one.png'), $png)
    [System.IO.File]::WriteAllBytes((Join-Path $imagesDir 'two.png'), $png)
    [System.IO.File]::WriteAllBytes((Join-Path $imagesDir 'three.png'), $png)
    @'
{
  "Name": "smoke",
  "Actions": {
    "0,0": { "UUID": "com.example.smoke.one", "Name": "one", "State": 0, "States": [ { "Image": "one.png" } ] },
    "1,0": { "UUID": "com.example.smoke.two", "Name": "two", "State": 0, "States": [ { "Image": "two.png" } ] },
    "2,0": { "UUID": "com.example.smoke.three", "Name": "three", "State": 0, "States": [ { "Image": "three.png" } ] }
  }
}
'@ | Set-Content -LiteralPath (Join-Path $profileDir 'manifest.json') -Encoding utf8

    # Isolate every daemon path; QUOTA_DIR keeps built-in actions away from %APPDATA% too.
    $env:ORCADECK_PROFILES = $profilesDir
    $env:ORCADECK_PLUGINS = $pluginsDir
    $env:ORCADECK_DATA = $dataDir
    $env:QUOTA_DIR = Join-Path $tempRoot 'quota'

    # 1) help exits 0 and prints the usage.
    $help = (& $exePath help 2>&1 | Out-String)
    Assert-True ($LASTEXITCODE -eq 0) "orcadeck help exited $LASTEXITCODE"
    Assert-True ($help -match 'uso: orcadeck') 'orcadeck help did not print the usage banner'
    Assert-True ($help -match 'render <perfil>') 'orcadeck help did not list render'

    # 2) render the synthetic scene: 15 keys (3 actions + black fill), all valid JPEGs.
    $render = (& $exePath render $profileDir $outDir 2>&1 | Out-String)
    Assert-True ($LASTEXITCODE -eq 0) "orcadeck render exited ${LASTEXITCODE}: $render"
    $jpegs = @(Get-ChildItem -LiteralPath $outDir -Filter 'key*.jpg' -File | Sort-Object Name)
    Assert-True ($jpegs.Count -eq 15) "expected 15 JPEGs, found $($jpegs.Count)"
    foreach ($jpeg in $jpegs) {
        $bytes = [System.IO.File]::ReadAllBytes($jpeg.FullName)
        Assert-True ($bytes.Length -gt 2) "$($jpeg.Name) is empty"
        Assert-True ($bytes[0] -eq 0xFF -and $bytes[1] -eq 0xD8) "$($jpeg.Name) is not a JPEG (no FF D8)"
        Assert-True ($bytes.Length -le 9000) "$($jpeg.Name) is $($bytes.Length) bytes (limit 9000)"
    }

    # 3) edit on a free loopback port, then root and a tokenless API call.
    $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
    $listener.Start()
    try { $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port } finally { $listener.Stop() }

    $editOut = Join-Path $tempRoot 'edit.out.txt'
    $editErr = Join-Path $tempRoot 'edit.err.txt'
    $editProc = Start-Process -FilePath $exePath -ArgumentList @('edit', '--port', "$port", '--no-open') `
        -PassThru -RedirectStandardOutput $editOut -RedirectStandardError $editErr -WindowStyle Hidden

    $deadline = [DateTime]::UtcNow.AddSeconds(10)
    $ready = $false
    while ([DateTime]::UtcNow -lt $deadline) {
        $editProc.Refresh()
        if ($editProc.HasExited) { break }
        $client = [System.Net.Sockets.TcpClient]::new()
        try {
            if ($client.ConnectAsync('127.0.0.1', $port).Wait(200) -and $client.Connected) { $ready = $true; break }
        } catch {
            # refused while the server is still binding; retry until the deadline
        } finally {
            $client.Dispose()
        }
        Start-Sleep -Milliseconds 100
    }
    $editProc.Refresh()
    if (-not $ready) {
        $tail = Get-Content -LiteralPath $editErr -Raw -ErrorAction SilentlyContinue
        $tail += Get-Content -LiteralPath $editOut -Raw -ErrorAction SilentlyContinue
        throw "orcadeck edit did not accept TCP on 127.0.0.1:$port within 10 s (exited=$($editProc.HasExited)): $tail"
    }

    $root = Invoke-WebRequest -Uri "http://127.0.0.1:$port/" -TimeoutSec 5
    Assert-True ($root.StatusCode -eq 200) "editor root returned HTTP $($root.StatusCode)"
    $api = Invoke-WebRequest -Uri "http://127.0.0.1:$port/api/scenes" -SkipHttpErrorCheck -TimeoutSec 5
    Assert-True ($api.StatusCode -in @(401, 403)) "tokenless /api/scenes returned HTTP $($api.StatusCode), expected 401/403"

    Stop-Process -Id $editProc.Id -Force -ErrorAction SilentlyContinue
    [void]$editProc.WaitForExit(5000)
    Assert-True (-not (Get-Process -Id $editProc.Id -ErrorAction SilentlyContinue)) "orcadeck edit process $($editProc.Id) is still running"
} catch {
    $failure = $_.Exception.Message
} finally {
    try {
        if ($editProc) {
            $editProc.Refresh()
            if (-not $editProc.HasExited) {
                Stop-Process -Id $editProc.Id -Force -ErrorAction SilentlyContinue
                [void]$editProc.WaitForExit(5000)
            }
            if (Get-Process -Id $editProc.Id -ErrorAction SilentlyContinue) {
                throw "orcadeck edit process $($editProc.Id) survived Stop-Process"
            }
        }
    } catch {
        $cleanupFailure = "process cleanup: $($_.Exception.Message)"
    }
    try {
        if (Test-Path -LiteralPath $tempRoot) {
            Remove-Item -LiteralPath $tempRoot -Recurse -Force
        }
        Assert-True (-not (Test-Path -LiteralPath $tempRoot)) "temporary directory was not removed: $tempRoot"
    } catch {
        $cleanupFailure = (@($cleanupFailure, "temp cleanup: $($_.Exception.Message)") | Where-Object { $_ }) -join '; '
    }
}

if ($failure -or $cleanupFailure) {
    $parts = @($failure, $cleanupFailure) | Where-Object { $_ }
    Write-Host "SMOKE FAILED: $($parts -join '; ')" -ForegroundColor Red
    exit 1
}
Write-Host 'SMOKE OK: help, render (15 JPEGs <= 9000 B), edit (HTTP 200 + tokenless API 401/403), no process or temp files left'
exit 0
