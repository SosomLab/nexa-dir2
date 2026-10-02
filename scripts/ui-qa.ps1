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
#   Click-Child $ph 85 233                  # 대화상자(설정 창) 자식 컨트롤 클릭(부모 클라이언트 좌표)
#   Find-TopWindow 'NexaPrefs' $p.Id ; Find-TopWindow '#32768'   # 설정 창 · 팝업 메뉴 hwnd
#   Real-Click $sx $sy                      # 실제 커서 클릭(메뉴 항목 선택 전용 — 화면 좌표)
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
  [DllImport("user32.dll")] public static extern bool ScreenToClient(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern IntPtr ChildWindowFromPointEx(IntPtr h, POINT p, uint f);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, uint d, UIntPtr e);
  public delegate bool EnumProc(IntPtr h, IntPtr l);
  [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
  [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassNameW(IntPtr h, StringBuilder s, int n);
  // 클래스명으로 최상위 가시 창 찾기(pid 0 = 전 프로세스) — 설정 창(NexaPrefs)·팝업 메뉴(#32768) 등.
  public static IntPtr FindTop(uint pid, string cls) {
    IntPtr f = IntPtr.Zero;
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); var sb = new StringBuilder(64); GetClassNameW(h, sb, 64);
      if ((pid == 0 || p == pid) && IsWindowVisible(h) && sb.ToString() == cls) { f = h; return false; } return true; }, IntPtr.Zero);
    return f;
  }
  // **실제** 마우스 클릭(화면 좌표) — 메뉴 루프처럼 PostMessage 좌표를 무시하는 대상에만 쓴다(커서를 잠깐 움직였다 되돌린다).
  public static void RealClick(int x, int y) {
    POINT old; GetCursorPos(out old); SetCursorPos(x, y); System.Threading.Thread.Sleep(60);
    mouse_event(2, 0, 0, 0, UIntPtr.Zero); mouse_event(4, 0, 0, 0, UIntPtr.Zero);
    System.Threading.Thread.Sleep(60); SetCursorPos(old.X, old.Y);
  }
  [DllImport("user32.dll")] public static extern bool MapWindowPoints(IntPtr from, IntPtr to, ref POINT p, uint n);
  // 가려진 창에서도 동작하도록 z순서와 무관하게 부모에서 자식으로 내려간다(CWP_SKIPINVISIBLE|SKIPTRANSPARENT).
  public static IntPtr DeepChild(IntPtr top, ref POINT pt) {
    IntPtr cur = top;
    while (true) {
      IntPtr c = ChildWindowFromPointEx(cur, pt, 0x1 | 0x4);
      if (c == IntPtr.Zero || c == cur) return cur;
      MapWindowPoints(cur, c, ref pt, 1);
      cur = c;
    }
  }
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint f);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowTextW(IntPtr h, StringBuilder s, int n);
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

# 대화상자(설정 창 등)의 **자식 컨트롤**을 부모 클라이언트 좌표로 클릭 — 그 점의 자식 창을 찾아
# 그 창 좌표로 변환해 보낸다(최상위 창에 보내면 자식에 닿지 않는다 · 창이 가려져 있어도 동작).
# 반환 = 클릭한 자식 hwnd.
function Click-Child($top, $x, $y, [switch]$Right) {
  $pt = New-Object NxUi+POINT; $pt.X = $x; $pt.Y = $y
  $c = [NxUi]::DeepChild($top, [ref]$pt)
  Click $c $pt.X $pt.Y -Right:$Right
  $c
}

# 클래스명으로 최상위 창 찾기(설정 창 'NexaPrefs' · 팝업 메뉴 '#32768'). $ProcId 0 = 전 프로세스.
function Find-TopWindow([string]$Class, [uint32]$ProcId = 0) { [NxUi]::FindTop($ProcId, $Class) }
function Get-WindowRect($h) { $r = New-Object NxUi+RECT; [void][NxUi]::GetWindowRect($h, [ref]$r); $r }
# 실제 커서 클릭(화면 좌표) — TrackPopupMenu 메뉴는 posted 좌표를 무시하므로 메뉴 항목 선택에만 쓴다.
function Real-Click([int]$sx, [int]$sy) { [NxUi]::RealClick($sx, $sy) }

function Close-Win($h) { [void][NxUi]::PostMessageW($h, 0x10, [IntPtr]0, [IntPtr]0) }

# 프로세스 실측 한 줄(WorkingSet/Private/CPU) — 성능 점검용.
function Get-NexaStats([int]$ProcId) {
  $p = Get-Process -Id $ProcId
  [pscustomobject]@{ Pid = $p.Id; WS_MB = [math]::Round($p.WorkingSet64 / 1MB, 1); Private_MB = [math]::Round($p.PrivateMemorySize64 / 1MB, 1); CPU_s = [math]::Round($p.TotalProcessorTime.TotalSeconds, 2); Title = $p.MainWindowTitle }
}
