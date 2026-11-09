# Long-lived helper: each line on stdin = bring Orca's main window to the front (restore if
# minimized, switch to its virtual desktop). Started once by plugin.js so a press costs ~ms.
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr h);
  [DllImport("user32.dll")] public static extern bool IsWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr p);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll")] public static extern int GetWindowTextLength(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetWindow(IntPtr h, uint cmd);
  public delegate bool EnumProc(IntPtr h, IntPtr p);
  // Top-level, visible, titled, unowned window of one of these processes (works across virtual desktops).
  public static IntPtr Find(uint[] pids) {
    IntPtr found = IntPtr.Zero;
    EnumWindows((h, p) => {
      uint pid; GetWindowThreadProcessId(h, out pid);
      if (Array.IndexOf(pids, pid) >= 0 && IsWindowVisible(h) && GetWindowTextLength(h) > 0 && GetWindow(h, 4) == IntPtr.Zero) { found = h; return false; }
      return true;
    }, IntPtr.Zero);
    return found;
  }
  public static void Raise(IntPtr h) {
    if (IsIconic(h)) ShowWindow(h, 9);            // SW_RESTORE
    keybd_event(0x12, 0, 0, UIntPtr.Zero);        // Alt down/up: lifts Windows' foreground lock
    keybd_event(0x12, 0, 2, UIntPtr.Zero);
    BringWindowToTop(h);
    SetForegroundWindow(h);
  }
}
'@
$h = [IntPtr]::Zero
while ($null -ne ($line = [Console]::In.ReadLine())) {
  if ($h -eq [IntPtr]::Zero -or -not [W]::IsWindow($h)) {
    $pids = [uint32[]]@(Get-Process orca -ErrorAction SilentlyContinue | ForEach-Object { $_.Id })
    $h = if ($pids.Count) { [W]::Find($pids) } else { [IntPtr]::Zero }
  }
  if ($h -ne [IntPtr]::Zero) { [W]::Raise($h); "raised $h" } else { 'orca window not found' }
}
