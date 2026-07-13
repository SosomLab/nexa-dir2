//! OLE **DnD 수신**(M3-5 S3) — `IDropTarget`/`RegisterDragDrop`: 외부(탐색기·다른 앱)→앱 드롭.
//! **원본 대응**: `OleDropTarget.cs`(DND-EXT — 상승 프로세스 OLE 폴백) + docs/33 **B-14dnd**
//! (디스크별 기본 동작: 같은 볼륨=이동/다른 볼륨=복사 · Ctrl=복사 강제/Shift=이동 강제).
//!
//! 콜백은 OLE 모달 드래그 루프 중 **UI 스레드(STA)** 에서 온다 — 드롭 대상 판정과 전송 시작은
//! win.rs 훅([`DropHooks`])에 위임(State 접근 지점을 win.rs 한곳으로 격리).

use std::cell::RefCell;
use std::path::PathBuf;

use windows::core::implement;
use windows::Win32::Foundation::{HWND, POINTL};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, ReleaseStgMedium, DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE,
    DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{MK_CONTROL, MK_SHIFT, MODIFIERKEYS_FLAGS};
use windows::Win32::UI::Shell::HDROP;

use crate::clipboard::paths_from_hdrop;

/// win.rs가 제공하는 드롭 훅 — 대상 폴더 판정·전송 시작(State 접근 격리).
pub struct DropHooks {
    /// 화면 좌표의 드롭 대상 폴더(폴더 행=그 폴더·그 외=해당 패널 루트·창 밖/부적합=None).
    pub dest_at: unsafe fn(HWND, i32, i32) -> Option<PathBuf>,
    /// 드롭 확정 — 전송 엔진 시작(undo 기록 포함).
    pub drop: unsafe fn(HWND, Vec<PathBuf>, PathBuf, nexa_ops::Op),
}

/// 외부 드래그 수신 대상(창 1개 전역 — RegisterDragDrop이 수명 보유).
#[implement(IDropTarget)]
pub struct DropTarget {
    hwnd: HWND,
    hooks: DropHooks,
    /// DragEnter에서 추출한 페이로드(CF_HDROP 경로들) — Drop까지 유지.
    paths: RefCell<Vec<PathBuf>>,
}

impl DropTarget {
    pub fn new(hwnd: HWND, hooks: DropHooks) -> Self {
        Self {
            hwnd,
            hooks,
            paths: RefCell::new(Vec::new()),
        }
    }
}

impl DropTarget_Impl {
    /// 연산 결정(원본 B-14dnd): Ctrl=복사 · Shift=이동 · 기본=같은 볼륨이면 이동/다르면 복사.
    fn op_for(&self, keys: MODIFIERKEYS_FLAGS, dest: &std::path::Path) -> nexa_ops::Op {
        if keys.0 & MK_CONTROL.0 != 0 {
            return nexa_ops::Op::Copy;
        }
        if keys.0 & MK_SHIFT.0 != 0 {
            return nexa_ops::Op::Move;
        }
        let paths = self.paths.borrow();
        match paths.first() {
            Some(src) if nexa_ops::same_volume(src, dest) => nexa_ops::Op::Move,
            Some(_) => nexa_ops::Op::Copy,
            None => nexa_ops::Op::Copy,
        }
    }

    /// 현재 좌표·수정키의 (대상 폴더, 연산, 커서 효과) — DragOver/Drop 공용.
    unsafe fn resolve(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
    ) -> (Option<PathBuf>, nexa_ops::Op, DROPEFFECT) {
        if self.paths.borrow().is_empty() {
            return (None, nexa_ops::Op::Copy, DROPEFFECT_NONE);
        }
        let Some(dest) = (self.hooks.dest_at)(self.hwnd, pt.x, pt.y) else {
            return (None, nexa_ops::Op::Copy, DROPEFFECT_NONE);
        };
        // 자기 자신/하위로의 드롭 금지(원본 🚫 — 엔진도 재차 방어)
        let paths = self.paths.borrow();
        if paths.iter().any(|p| nexa_ops::is_same_or_sub(p, &dest)) {
            return (None, nexa_ops::Op::Copy, DROPEFFECT_NONE);
        }
        drop(paths);
        let op = self.op_for(keys, &dest);
        let effect = if op == nexa_ops::Op::Move {
            DROPEFFECT_MOVE
        } else {
            DROPEFFECT_COPY
        };
        (Some(dest), op, effect)
    }
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: windows::core::Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        // CF_HDROP 페이로드 추출(1회) — 없으면 수신 거부(effect NONE)
        let mut paths = Vec::new();
        if let Some(data) = pdataobj.as_ref() {
            let fmt = FORMATETC {
                cfFormat: CF_HDROP.0,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: -1,
                tymed: TYMED_HGLOBAL.0 as u32,
            };
            if let Ok(mut medium) = unsafe { data.GetData(&fmt) } {
                paths = unsafe { paths_from_hdrop(HDROP(medium.u.hGlobal.0)) };
                unsafe { ReleaseStgMedium(&mut medium) };
            }
        }
        *self.paths.borrow_mut() = paths;
        unsafe {
            *effect = self.resolve(keys, pt).2;
        }
        Ok(())
    }

    fn DragOver(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe {
            *effect = self.resolve(keys, pt).2;
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        self.paths.borrow_mut().clear();
        Ok(())
    }

    fn Drop(
        &self,
        _pdataobj: windows::core::Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        let (dest, op, fx) = unsafe { self.resolve(keys, pt) };
        // **최적화 이동(optimized move) 규약**: 이동은 우리(타깃)가 전송 엔진으로 직접 수행하므로
        // 소스에 DROPEFFECT_NONE을 반환 — MOVE를 돌려주면 소스(탐색기)가 원본을 삭제해
        // 비동기 전송과 경쟁(교차 볼륨 이동 중 원본 소실 위험). 복사는 그대로 COPY.
        unsafe {
            *effect = if fx == DROPEFFECT_MOVE {
                DROPEFFECT_NONE
            } else {
                fx
            };
        }
        let paths = std::mem::take(&mut *self.paths.borrow_mut());
        if let Some(dest) = dest {
            if !paths.is_empty() {
                unsafe { (self.hooks.drop)(self.hwnd, paths, dest, op) };
            }
        }
        Ok(())
    }
}
