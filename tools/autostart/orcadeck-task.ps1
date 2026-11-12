# OrcaDeck autostart as a normal (non-elevated) user task. No admin needed for this script.
#
#   .\orcadeck-task.ps1 install     copy orcadeck.exe + libunwind.dll to %LOCALAPPDATA%\OrcaDeck\bin, register task DISABLED
#   .\orcadeck-task.ps1 enable      enable the logon trigger (do this only once Stream Dock's own task is disabled)
#   .\orcadeck-task.ps1 editor      Desktop shortcut that opens the editor (attaches to the running `run`)
#   .\orcadeck-task.ps1 disable | uninstall | status
#
# Stream Dock and OrcaDeck use the same HID device and port 18618: never run both. Disabling Stream Dock's
# task needs an ELEVATED console (it runs at highest privileges):  schtasks /change /tn StreamDock /disable
# Log: %LOCALAPPDATA%\OrcaDeck\orcadeck.log (ponytail: never rotated; it only grows while the device is absent).
param(
    [ValidateSet('install', 'enable', 'editor', 'disable', 'uninstall', 'status')][string]$Action = 'status',
    [string]$Profile = 'R0KZ54Z0-063U-947X-31BI-GG95R8F4HT88',
    # Quota keys read this folder; without it they stay empty.
    [string]$Quota = $(if ($env:QUOTA_DIR) { $env:QUOTA_DIR } else { 'C:\Users\migue\Github\quota-monitor' }),
    [string]$Build = (Join-Path $PSScriptRoot '..\..\daemon\target\release')
)
$ErrorActionPreference = 'Stop'
$task = 'OrcaDeck'
$root = Join-Path $env:LOCALAPPDATA 'OrcaDeck'
$bin = Join-Path $root 'bin'

switch ($Action) {
    'install' {
        $exe = Join-Path $Build 'orcadeck.exe'
        $dll = 'C:\Tools\llvm-mingw-20261104-ucrt-x86_64\bin\libunwind.dll'
        foreach ($f in $exe, $dll) { if (-not (Test-Path $f)) { throw "missing $f" } }
        Get-Process orcadeck -ErrorAction SilentlyContinue | Stop-Process -Force
        New-Item -ItemType Directory -Force $bin | Out-Null
        Copy-Item $exe, $dll $bin -Force
        $sdProfile = Join-Path $env:APPDATA "HotSpot\StreamDock\profiles\$Profile.sdProfile"
        if (-not (Test-Path $sdProfile)) { throw "profile not found: $sdProfile" }
        # conhost --headless runs the .cmd without a visible console window.
        $cmd = "@echo off`r`nset PATH=%~dp0;%PATH%`r`nset QUOTA_DIR=$Quota`r`n`"%~dp0orcadeck.exe`" run `"$sdProfile`" 2>> `"$root\orcadeck.log`"`r`n"
        Set-Content -Path (Join-Path $bin 'orcadeck-run.cmd') -Value $cmd -Encoding ascii
        $act = New-ScheduledTaskAction -Execute 'conhost.exe' -Argument "--headless `"$bin\orcadeck-run.cmd`""
        $trg = New-ScheduledTaskTrigger -AtLogOn -User "$env:USERDOMAIN\$env:USERNAME"
        $set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero) `
            -RestartCount 999 -RestartInterval (New-TimeSpan -Minutes 1) -MultipleInstances IgnoreNew -Disable
        $prin = New-ScheduledTaskPrincipal -UserId "$env:USERDOMAIN\$env:USERNAME" -LogonType Interactive -RunLevel Limited
        Register-ScheduledTask -TaskName $task -Action $act -Trigger $trg -Settings $set -Principal $prin -Force | Out-Null
        "installed (disabled). bin: $bin"
    }
    'editor' {
        # Visible console on purpose: closing it stops the editor server (it only lives while attached).
        $lnk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'OrcaDeck editor.lnk'
        $sc = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk)
        $sc.TargetPath = Join-Path $bin 'orcadeck.exe'
        $sc.Arguments = 'edit'
        $sc.WorkingDirectory = $bin
        $sc.Save()
        "created $lnk"
    }
    'enable' { Enable-ScheduledTask -TaskName $task | Out-Null; 'enabled' }
    'disable' { Disable-ScheduledTask -TaskName $task | Out-Null; 'disabled' }
    'uninstall' {
        Stop-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $task -Confirm:$false
        Get-Process orcadeck -ErrorAction SilentlyContinue | Stop-Process -Force
        'removed task (files kept in ' + $bin + ')'
    }
    'status' {
        Get-ScheduledTask -TaskName $task -ErrorAction SilentlyContinue | Select-Object TaskName, State
        Get-ScheduledTask -TaskName StreamDock -ErrorAction SilentlyContinue | Select-Object TaskName, State
        Get-Process orcadeck, 'Stream Dock AJAZZ' -ErrorAction SilentlyContinue | Select-Object Id, ProcessName
    }
}
