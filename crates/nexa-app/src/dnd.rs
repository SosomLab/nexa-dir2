//! OLE **DnD**(M3-5 S3/S4) — 수신 `IDropTarget`/`RegisterDragDrop`(외부→앱 드롭) +
//! 발신 `DoDragDrop`/`IDropSource`(앱→외부·탐색기).
//! **원본 대응**: `OleDropTarget.cs`(DND-EXT)·`OnRowDragStarting`(StorageItems — dir2는
//! 셸 `SHCreateDataObject`로 대체) + docs/33 **B-14dnd**(디스크별 기본 동작: 같은 볼륨=이동/
//! 다른 볼륨=복사 · Ctrl=복사 강제/Shift=이동 강제).
//!
//! 콜백은 OLE 모달 드래그/드롭 루프 중 **UI 스레드(STA)** 에서 온다 — 드롭 대상 판정과 전송
//! 시작은 win.rs 훅([`DropHooks`])에 위임(State 접근 지점을 win.rs 한곳으로 격리).

use std::cell::RefCell;
use std::path::PathBuf;

use windows::core::implement;
use windows::Win32::Foundation::{
    DATA_S_SAMEFORMATETC, DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS,
    DV_E_FORMATETC, E_NOTIMPL, E_OUTOFMEMORY, HWND, OLE_E_ADVISENOTSUPPORTED, POINTL, S_OK,
};
use windows::Win32::System::Com::{
    IDataObject, IDataObject_Impl, IEnumFORMATETC, DVASPECT_CONTENT, FORMATETC, STGMEDIUM,
    STGMEDIUM_0, TYMED_HGLOBAL,
};
use windows::Win32::System::Ole::CF_HDROP;
use windows::Win32::System::Ole::{
    DoDragDrop, IDropSource, IDropSource_Impl, IDropTarget, IDropTarget_Impl, ReleaseStgMedium,
    DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_MOVE, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::{
    MK_CONTROL, MK_LBUTTON, MK_SHIFT, MODIFIERKEYS_FLAGS,
};
use windows::Win32::UI::Shell::HDROP;

use crate::clipboard::paths_from_hdrop;

/// win.rs가 제공하는 드롭 훅 — 대상 폴더 판정·전송 시작(State 접근 격리).
pub struct DropHooks {
    /// 화면 좌표의 드롭 대상 폴더(폴더 행=그 폴더·그 외=해당 패널 루트·창 밖/부적합=None).
    pub dest_at: unsafe fn(HWND, i32, i32) -> Option<PathBuf>,
    /// 드롭 확정 — 전송 엔진 시작(undo 기록 포함).
    pub drop: unsafe fn(HWND, Vec<PathBuf>, PathBuf, nexa_ops::Op),
    /// 드래그 위치 추적(X-32) — DragEnter/DragOver 화면 좌표(엣지 자동 스크롤·탭/폴더
    /// 호버 대기 판정. 호스트가 폴링 타이머를 무장해 정지 커서도 계속 판정).
    pub track: unsafe fn(HWND, i32, i32),
    /// 드래그 이탈/종료(X-32) — 추적 타이머·호버 대기 해제.
    pub leave: unsafe fn(HWND),
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

/// 드래그 발신원(S4) — 표준 종료 판정·기본 커서(원본 WinUI DragStarting의 OLE 대응).
#[implement(IDropSource)]
struct DropSource;

impl IDropSource_Impl for DropSource_Impl {
    fn QueryContinueDrag(
        &self,
        escape_pressed: windows_core::BOOL,
        keys: MODIFIERKEYS_FLAGS,
    ) -> windows_core::HRESULT {
        if escape_pressed.as_bool() {
            return DRAGDROP_S_CANCEL; // Esc = 취소(원본 ESC 취소 계승)
        }
        if keys.0 & MK_LBUTTON.0 == 0 {
            return DRAGDROP_S_DROP; // 왼쪽 버튼 해제 = 드롭 확정
        }
        S_OK
    }

    fn GiveFeedback(&self, _effect: DROPEFFECT) -> windows_core::HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// 발신 데이터 객체 — **CF_HDROP을 직접 제공**하는 최소 IDataObject.
/// 실기 QA(07-13): `SHCreateDataObject`(절대 PIDL)는 셸 IDList 포맷만 내고 CF_HDROP을
/// 렌더링하지 않아 탐색기·자기 수신부가 드롭을 거부(🚫) → 직접 구현으로 교체.
/// 부수 효과: 같은 부모 폴더 제약이 없어 교차폴더 선택 드래그도 지원.
#[implement(IDataObject)]
struct FileListDataObject {
    paths: Vec<PathBuf>,
}

/// CF_HDROP·TYMED_HGLOBAL·DVASPECT_CONTENT 요청인가.
fn is_hdrop_fmt(fmt: &FORMATETC) -> bool {
    fmt.cfFormat == CF_HDROP.0
        && fmt.tymed & TYMED_HGLOBAL.0 as u32 != 0
        && fmt.dwAspect == DVASPECT_CONTENT.0
}

impl IDataObject_Impl for FileListDataObject_Impl {
    fn GetData(&self, fmt: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        if fmt.is_null() || !is_hdrop_fmt(unsafe { &*fmt }) {
            return Err(DV_E_FORMATETC.into());
        }
        let hmem = unsafe { crate::clipboard::hglobal_file_list(&self.paths) }
            .ok_or_else(|| windows::core::Error::from(E_OUTOFMEMORY))?;
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: hmem }, // 소유권은 수신자(ReleaseStgMedium)
            pUnkForRelease: std::mem::ManuallyDrop::new(None),
        })
    }

    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn QueryGetData(&self, fmt: *const FORMATETC) -> windows_core::HRESULT {
        if !fmt.is_null() && is_hdrop_fmt(unsafe { &*fmt }) {
            S_OK
        } else {
            DV_E_FORMATETC
        }
    }

    fn GetCanonicalFormatEtc(
        &self,
        _: *const FORMATETC,
        out: *mut FORMATETC,
    ) -> windows_core::HRESULT {
        if !out.is_null() {
            unsafe { (*out).ptd = std::ptr::null_mut() };
        }
        DATA_S_SAMEFORMATETC
    }

    fn SetData(
        &self,
        _: *const FORMATETC,
        _: *const STGMEDIUM,
        _: windows_core::BOOL,
    ) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into()) // 대상의 Performed DropEffect 통지는 무시(α — 원본 미삭제 방향)
    }

    fn EnumFormatEtc(&self, direction: u32) -> windows::core::Result<IEnumFORMATETC> {
        use windows::Win32::System::Com::DATADIR_GET;
        use windows::Win32::UI::Shell::SHCreateStdEnumFmtEtc;
        if direction != DATADIR_GET.0 as u32 {
            return Err(E_NOTIMPL.into());
        }
        let fmt = FORMATETC {
            cfFormat: CF_HDROP.0,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };
        unsafe { SHCreateStdEnumFmtEtc(&[fmt]) }
    }

    fn DAdvise(
        &self,
        _: *const FORMATETC,
        _: u32,
        _: windows_core::Ref<windows::Win32::System::Com::IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }

    fn EnumDAdvise(&self) -> windows::core::Result<windows::Win32::System::Com::IEnumSTATDATA> {
        Err(OLE_E_ADVISENOTSUPPORTED.into())
    }
}

/// 앱→외부 드래그 시작(S4) — 경로들을 CF_HDROP IDataObject로 담아 `DoDragDrop`
/// (모달 — 반환 시 드래그 종료). 반환: 드롭이 수행됐는가(취소=false).
///
/// 이동 결과 처리: 대상이 최적화 이동이면 NONE이 돌아온다(파일은 이미 이동됨). MOVE가
/// 돌아와도 **원본을 삭제하지 않는다**(α — 비최적화 대상의 이동은 복사로 남는 안전 방향.
/// 탐색기는 파일 드롭에서 최적화 이동을 수행하므로 실사용 영향 없음). 호출자는 반환값과
/// 무관하게 재로드로 수렴.
///
/// # Safety
/// UI 스레드에서 호출(OLE STA — OleInitialize 완료 전제). 모달 루프 동안 wndproc 재진입 —
/// 호출자는 State 가변 참조를 넘기지 말 것(shellmenu와 동일 규약).
pub unsafe fn begin_drag(paths: &[PathBuf]) -> bool {
    if paths.is_empty() {
        return false;
    }
    let data: IDataObject = FileListDataObject {
        paths: paths.to_vec(),
    }
    .into();
    let source: IDropSource = DropSource.into();
    let mut effect = DROPEFFECT_NONE;
    let hr = DoDragDrop(
        &data,
        &source,
        DROPEFFECT_COPY | DROPEFFECT_MOVE,
        &mut effect,
    );
    hr == DRAGDROP_S_DROP
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
            (self.hooks.track)(self.hwnd, pt.x, pt.y); // 추적 시작(X-32)
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
            (self.hooks.track)(self.hwnd, pt.x, pt.y); // 위치 갱신(X-32)
        }
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        self.paths.borrow_mut().clear();
        unsafe { (self.hooks.leave)(self.hwnd) }; // 추적 해제(X-32)
        Ok(())
    }

    fn Drop(
        &self,
        _pdataobj: windows::core::Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe { (self.hooks.leave)(self.hwnd) }; // 드롭 = 드래그 종료 — 추적 해제(X-32)
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
