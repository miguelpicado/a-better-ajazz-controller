# Installs the plugin into Ajazz Stream Dock and restarts it.
$ErrorActionPreference = 'Stop'
$src = Join-Path $PSScriptRoot 'com.miguelpicado.orcadock.sdPlugin'
$dst = Join-Path $env:APPDATA 'HotSpot\StreamDock\plugins\com.miguelpicado.orcadock.sdPlugin'
$exe = 'C:\Program Files (x86)\Stream Dock AJAZZ Global\Stream Dock AJAZZ.exe'
Push-Location $src; npm install --silent --no-audit --no-fund; Pop-Location
Get-Process "Stream Dock AJAZZ" -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep 2
# A forced stop leaves plugin processes behind: kill node20 whose parent is gone.
Get-CimInstance Win32_Process -Filter "name='node20.exe'" |
  Where-Object { -not (Get-Process -Id $_.ParentProcessId -ErrorAction SilentlyContinue) } |
  ForEach-Object { Stop-Process -Id $_.ProcessId -Force }
robocopy $src $dst /MIR /XF plugin.log /NFL /NDL /NJH /NJS | Out-Null
Start-Process $exe
Start-Sleep 3
# Right after a kill Stream Dock sometimes refuses to start: try once more.
if (-not (Get-Process "Stream Dock AJAZZ" -ErrorAction SilentlyContinue)) { Start-Process $exe }
"deployed to $dst"
