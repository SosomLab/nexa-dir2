# 우클릭 → 컨텍스트 메뉴 표시 시간: Nexa Dir vs 파일 탐색기(10-02 — docs/18 §2-2).
# 측정 = 입력 직후부터 새 최상위 가시 팝업(#32768)이 나타날 때까지 1ms 폴링.
#   Nexa  : 앱 창에 WM_RBUTTONDOWN/UP PostMessage(행 좌표) — 실제 우클릭 경로 그대로.
#   탐색기: explorer /select 창 전면 → Shift+F10(클래식 메뉴 = 같은 IContextMenu 경로)
#           + VK_APPS(Win11 모던 메뉴) 참고치.
# 사용: pwsh scripts/ctxmenu-timing.ps1 -Path <파일> -NexaHwnd <hwnd> -RowX 100 -RowY 151 [-Rounds 3]
#   -NexaHwnd 생략 시 Nexa 측정 생략. 메뉴는 ESC로 닫는다.
param(
  [Parameter(Mandatory)][string]$Path,
  [IntPtr]$NexaHwnd = [IntPtr]::Zero,
  [int]$RowX = 100, [int]$RowY = 151,
  [int]$Rounds = 3,
  [switch]$SkipExplorer
)
if (-not ('CtxT' -as [type])) {
Add-Type @"
using System; using System.Runtime.InteropServices; using System.Text; using System.Collections.Generic;
public class CtxT {
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint fl, UIntPtr ex);
  public static List<IntPtr> Menus() { var l = new List<IntPtr>(); EnumWindows((h, p) => { if (IsWindowVisible(h) && Cls(h) == "#32768") l.Add(h); return true; }, IntPtr.Zero); return l; }
  public static string Cls(IntPtr h) { var sb = new StringBuilder(256); GetClassNameW(h, sb, 256); return sb.ToString(); }
  public static string Title(IntPtr h) { var sb = new StringBuilder(512); GetWindowTextW(h, sb, 512); return sb.ToString(); }
  public static IntPtr FindCabinet(string prefix) { IntPtr f = IntPtr.Zero; EnumWindows((h, p) => { if (IsWindowVisible(h) && Cls(h) == "CabinetWClass" && Title(h).StartsWith(prefix)) { f = h; return false; } return true; }, IntPtr.Zero); return f; }
  public static void Key(byte vk, bool shift) { if (shift) keybd_event(0x10, 0, 0, UIntPtr.Zero); keybd_event(vk, 0, 0, UIntPtr.Zero); keybd_event(vk, 0, 2, UIntPtr.Zero); if (shift) keybd_event(0x10, 0, 2, UIntPtr.Zero); }
}
"@
}
function LP([int]$x, [int]$y) { [IntPtr]((($y -band 0xFFFF) -shl 16) -bor ($x -band 0xFFFF)) }
function Wait-Menu([System.Diagnostics.Stopwatch]$sw) {
  while ($sw.ElapsedMilliseconds -lt 10000) {
    if ([CtxT]::Menus().Count -gt 0) { return $sw.ElapsedMilliseconds }
    Start-Sleep -Milliseconds 1
  }
  return -1
}
function Close-Menu {
  Start-Sleep -Milliseconds 300
  for ($i = 0; $i -lt 3 -and [CtxT]::Menus().Count -gt 0; $i++) { [CtxT]::Key(0x1B, $false); Start-Sleep -Milliseconds 250 }
}
$res = @()
if ($NexaHwnd -ne [IntPtr]::Zero) {
  for ($r = 1; $r -le $Rounds; $r++) {
    [void][CtxT]::SetForegroundWindow($NexaHwnd); Start-Sleep -Milliseconds 300
    [void][CtxT]::PostMessageW($NexaHwnd, 0x200, [IntPtr]0, (LP $RowX $RowY))
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    [void][CtxT]::PostMessageW($NexaHwnd, 0x204, [IntPtr]2, (LP $RowX $RowY))
    [void][CtxT]::PostMessageW($NexaHwnd, 0x205, [IntPtr]0, (LP $RowX $RowY))
    $t = Wait-Menu $sw; Close-Menu
    $res += [pscustomobject]@{ App = 'Nexa Dir'; Mode = 'right-click'; Round = $r; Ms = $t }
  }
}
if (-not $SkipExplorer) {
  $folder = Split-Path (Split-Path $Path -Parent) -Leaf
  Start-Process explorer.exe -ArgumentList "/select,`"$Path`""
  $cab = [IntPtr]::Zero
  for ($i = 0; $i -lt 100 -and $cab -eq [IntPtr]::Zero; $i++) { Start-Sleep -Milliseconds 100; $cab = [CtxT]::FindCabinet($folder) }
  if ($cab -eq [IntPtr]::Zero) { Write-Warning "탐색기 창을 찾지 못함($folder)" } else {
    Start-Sleep -Milliseconds 1500
    for ($r = 1; $r -le $Rounds; $r++) {
      foreach ($m in @(@{ N = 'classic(Shift+F10)'; Vk = 0x79; S = $true }, @{ N = 'modern(Menu key)'; Vk = 0x5D; S = $false })) {
        [void][CtxT]::SetForegroundWindow($cab); Start-Sleep -Milliseconds 300
        $sw = [System.Diagnostics.Stopwatch]::StartNew()
        [CtxT]::Key([byte]$m.Vk, [bool]$m.S)
        $t = Wait-Menu $sw; Close-Menu
        $res += [pscustomobject]@{ App = 'Explorer'; Mode = $m.N; Round = $r; Ms = $t }
      }
    }
    [void][CtxT]::PostMessageW($cab, 0x10, [IntPtr]0, [IntPtr]0)
  }
}
$res | Format-Table -AutoSize | Out-String
