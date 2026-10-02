# UI 자동 조작·캡처 하네스(10-02 — 실기 QA 자동화의 기반. docs/18 §2-2).
# 원칙: SendInput 대신 **PostMessage**로 앱 창에 직접 마우스/휠/키 메시지를 보낸다 — 사용자
# 포커스를 훔치지 않고, 창이 가려져 있어도 동작한다. 캡처는 PrintWindow(PW_RENDERFULLCONTENT).
#
# 사용(도트 소싱):
#   . scripts/ui-qa.ps1
#   $p = Start-NexaDev                       # target\release\nexa-app.exe 기동(NO_COLOR 제거)
#   $h = Get-NexaHwnd $p.Id
#   Capture-Win $h out.png                  # 클라이언트 영역 PNG
#   Click $h 760 481 ; Click $h 760 481 -Right
#   Drag  $h 720 595 800 635                # 좌버튼 드래그(8단계 MOUSEMOVE)
#   Wheel $h 1320 640 -120 -Shift           # Shift+휠 = 가로 · -Horizontal = WM_MOUSEHWHEEL
#   Key   $h 0x41 -Ctrl                     # Ctrl+A
#   Close-Win $h                            # WM_CLOSE = 세션 저장 정상 종료(kill 금지)
# 좌표는 **클라이언트 좌표**(캡처 PNG의 픽셀 좌표와 동일).
Add-Type -AssemblyName System.Drawing
if (-not ('NxUi' -as [type])) {
Add-Type @"
using System;
using System.Runtime.InteropServices;
using System.Text;
public class NxUi {
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll")] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  public static string Title(IntPtr h) { var sb = new StringBuilder(512); GetWindowTextW(h, sb, 512); return sb.ToString(); }
}
"@
}

function Start-NexaDev([string]$Exe = "$PSScriptRoot\..\target\release\nexa-app.exe", [int]$WaitMs = 3000) {
  Remove-Item env:NO_COLOR -ErrorAction SilentlyContinue   # 앱 안 pwsh 색 보존(memory 09-04)
  $p = Start-Process (Resolve-Path $Exe) -PassThru
  Start-Sleep -Milliseconds $WaitMs
  $p
}

function Get-NexaHwnd([int]$ProcId) {
  $h = [IntPtr]::Zero
  for ($i = 0; $i -lt 50 -and $h -eq [IntPtr]::Zero; $i++) {
    $h = [IntPtr](Get-Process -Id $ProcId -ErrorAction Stop).MainWindowHandle
    if ($h -eq [IntPtr]::Zero) { Start-Sleep -Milliseconds 100 }
  }
  $h
}
function Get-Title($h) { [NxUi]::Title($h) }

function Get-ClientSize($h) {
  $r = New-Object NxUi+RECT; [void][NxUi]::GetClientRect($h, [ref]$r)
  @{ W = $r.R - $r.L; H = $r.B - $r.T }
}

function Capture-Win($h, $Path) {
  $s = Get-ClientSize $h
  $bmp = New-Object System.Drawing.Bitmap $s.W, $s.H
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $dc = $g.GetHdc()
  [void][NxUi]::PrintWindow($h, $dc, 3)   # PW_CLIENTONLY | PW_RENDERFULLCONTENT
  $g.ReleaseHdc($dc); $g.Dispose()
  $bmp.Save($Path, [System.Drawing.Imaging.ImageFormat]::Png); $bmp.Dispose()
  "$Path $($s.W)x$($s.H)"
}

function LP([int]$x, [int]$y) { [IntPtr]((($y -band 0xFFFF) -shl 16) -bor ($x -band 0xFFFF)) }

function Click($h, $x, $y, [switch]$Right, [switch]$Double) {
  $dn = if ($Right) { 0x204 } else { 0x201 }; $up = if ($Right) { 0x205 } else { 0x202 }
  $mk = if ($Right) { 2 } else { 1 }
  [void][NxUi]::PostMessageW($h, 0x200, [IntPtr]0, (LP $x $y))
  [void][NxUi]::PostMessageW($h, $dn, [IntPtr]$mk, (LP $x $y))
  Start-Sleep -Milliseconds 40
  [void][NxUi]::PostMessageW($h, $up, [IntPtr]0, (LP $x $y))
  if ($Double) {
    Start-Sleep -Milliseconds 60
    [void][NxUi]::PostMessageW($h, 0x203, [IntPtr]1, (LP $x $y))
    [void][NxUi]::PostMessageW($h, 0x202, [IntPtr]0, (LP $x $y))
  }
}

function Drag($h, $x1, $y1, $x2, $y2, [int]$Steps = 8, [int]$StepMs = 15) {
  [void][NxUi]::PostMessageW($h, 0x200, [IntPtr]0, (LP $x1 $y1))
  [void][NxUi]::PostMessageW($h, 0x201, [IntPtr]1, (LP $x1 $y1))
  for ($i = 1; $i -le $Steps; $i++) {
    $x = [int]($x1 + ($x2 - $x1) * $i / $Steps); $y = [int]($y1 + ($y2 - $y1) * $i / $Steps)
    [void][NxUi]::PostMessageW($h, 0x200, [IntPtr]1, (LP $x $y))
    Start-Sleep -Milliseconds $StepMs
  }
  [void][NxUi]::PostMessageW($h, 0x202, [IntPtr]0, (LP $x2 $y2))
}

function Wheel($h, $x, $y, [int]$Delta, [switch]$Shift, [switch]$Ctrl, [switch]$Horizontal) {
  $p = New-Object NxUi+POINT; $p.X = $x; $p.Y = $y; [void][NxUi]::ClientToScreen($h, [ref]$p)
  $keys = 0; if ($Shift) { $keys = $keys -bor 4 }; if ($Ctrl) { $keys = $keys -bor 8 }
  $w = [IntPtr]((($Delta -band 0xFFFF) -shl 16) -bor $keys)
  $msg = if ($Horizontal) { 0x20E } else { 0x20A }
  [void][NxUi]::PostMessageW($h, $msg, $w, (LP $p.X $p.Y))
}

function Key($h, [int]$vk, [switch]$Ctrl, [switch]$Shift) {
  if ($Ctrl) { [void][NxUi]::PostMessageW($h, 0x100, [IntPtr]0x11, [IntPtr]0) }
  if ($Shift) { [void][NxUi]::PostMessageW($h, 0x100, [IntPtr]0x10, [IntPtr]0) }
  [void][NxUi]::PostMessageW($h, 0x100, [IntPtr]$vk, [IntPtr]0)
  [void][NxUi]::PostMessageW($h, 0x101, [IntPtr]$vk, [IntPtr]0)
  if ($Shift) { [void][NxUi]::PostMessageW($h, 0x101, [IntPtr]0x10, [IntPtr]0) }
  if ($Ctrl) { [void][NxUi]::PostMessageW($h, 0x101, [IntPtr]0x11, [IntPtr]0) }
}

function Close-Win($h) { [void][NxUi]::PostMessageW($h, 0x10, [IntPtr]0, [IntPtr]0) }

# 프로세스 실측 한 줄(WorkingSet/Private/CPU) — 성능 점검용.
function Get-NexaStats([int]$ProcId) {
  $p = Get-Process -Id $ProcId
  [pscustomobject]@{ Pid = $p.Id; WS_MB = [math]::Round($p.WorkingSet64 / 1MB, 1); Private_MB = [math]::Round($p.PrivateMemorySize64 / 1MB, 1); CPU_s = [math]::Round($p.TotalProcessorTime.TotalSeconds, 2); Title = $p.MainWindowTitle }
}
