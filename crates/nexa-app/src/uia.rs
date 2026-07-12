//! UIA 1차(M2-7) — 원본 NFR-A1(스크린리더) 대응: WM_GETOBJECT 서버측 프로바이더.
//! 커스텀 드로잉이라 OS가 아는 접근성 트리가 없으므로 **리스트/항목 프래그먼트**를 직접 노출한다.
//!
//! 설계: **불변 스냅샷 모델** — WM_GETOBJECT/포커스 이벤트 시점에 UI 스레드에서 가시 행
//! 스냅샷(`Snap`)을 떠 `Arc`로 프로바이더에 담는다. UIA 콜백은 임의(MTA) 스레드에서 오므로
//! 창 상태(State) 직접 접근 금지 — 스냅샷만 읽는다(스레드 안전). 구조 변경 시 다음
//! WM_GETOBJECT가 새 스냅샷 루트를 반환한다(1차 한계: 트리 갱신 이벤트 미발행).

// windows-rs UIA 상수명(UIA_NamePropertyId 등)을 match 패턴에 그대로 사용
#![allow(non_upper_case_globals)]

use std::mem::ManuallyDrop;
use std::sync::Arc;

use windows::core::{implement, Result, BSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, VARIANT_BOOL, WPARAM};
use windows::Win32::System::Com::SAFEARRAY;
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayPutElement};
use windows::Win32::System::Variant::{
    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_BSTR, VT_I4,
};
use windows::Win32::UI::Accessibility::{
    IRawElementProviderFragment, IRawElementProviderFragmentRoot,
    IRawElementProviderFragmentRoot_Impl, IRawElementProviderFragment_Impl,
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, ISelectionItemProvider,
    ISelectionItemProvider_Impl, NavigateDirection, NavigateDirection_FirstChild,
    NavigateDirection_LastChild, NavigateDirection_NextSibling, NavigateDirection_Parent,
    NavigateDirection_PreviousSibling, ProviderOptions, ProviderOptions_ServerSideProvider,
    UIA_AutomationFocusChangedEventId, UIA_ControlTypePropertyId, UIA_HasKeyboardFocusPropertyId,
    UIA_IsKeyboardFocusablePropertyId, UIA_ListControlTypeId, UIA_ListItemControlTypeId,
    UIA_NamePropertyId, UIA_SelectionItemPatternId, UiaAppendRuntimeId, UiaClientsAreListening,
    UiaHostProviderFromHwnd, UiaRaiseAutomationEvent, UiaRect, UiaReturnRawElementProvider,
    UIA_PATTERN_ID, UIA_PROPERTY_ID,
};

/// `WM_GETOBJECT`의 UIA 루트 요청 식별자(uiautomationcoreapi.h `UiaRootObjectId`).
pub const UIA_ROOT_OBJECT_ID: i32 = -25;

/// 가시 행 1개의 접근성 스냅샷.
pub struct RowSnap {
    /// 트리 컬럼 텍스트(파일명) — UIA Name.
    pub name: String,
    pub selected: bool,
    /// 캐럿 행(키보드 포커스 대응).
    pub focused: bool,
    /// 화면 좌표 rect (x, y, w, h).
    pub rect: (i32, i32, i32, i32),
}

/// 활성 패널 리스트의 접근성 스냅샷(불변 — UIA 콜백 스레드 공유).
pub struct Snap {
    /// 리스트 Name = 활성 패널 경로.
    pub name: String,
    /// 리스트 화면 좌표 rect.
    pub rect: (i32, i32, i32, i32),
    /// 가시 범위의 전역 행 인덱스 시작(런타임 id 안정화용).
    pub first_row: usize,
    pub rows: Vec<RowSnap>,
}

/// VARIANT 수동 구성(0.62 Win32 VARIANT는 raw 유니온) — 소유권은 호출자(VariantClear)로 이전.
fn variant(vt: windows::Win32::System::Variant::VARENUM, val: VARIANT_0_0_0) -> VARIANT {
    VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: val,
            }),
        },
    }
}

fn v_str(s: &str) -> VARIANT {
    variant(
        VT_BSTR,
        VARIANT_0_0_0 {
            bstrVal: ManuallyDrop::new(BSTR::from(s)),
        },
    )
}

fn v_i4(n: i32) -> VARIANT {
    variant(VT_I4, VARIANT_0_0_0 { lVal: n })
}

fn v_bool(b: bool) -> VARIANT {
    variant(
        VT_BOOL,
        VARIANT_0_0_0 {
            boolVal: VARIANT_BOOL(if b { -1 } else { 0 }),
        },
    )
}

fn uia_rect(r: (i32, i32, i32, i32)) -> UiaRect {
    UiaRect {
        left: r.0 as f64,
        top: r.1 as f64,
        width: r.2 as f64,
        height: r.3 as f64,
    }
}

/// 런타임 id SAFEARRAY([UiaAppendRuntimeId, n…]) — UIA가 요소 동일성 판정에 사용.
fn runtime_id(parts: &[i32]) -> Result<*mut SAFEARRAY> {
    unsafe {
        let sa = SafeArrayCreateVector(VT_I4, 0, parts.len() as u32 + 1);
        if sa.is_null() {
            return Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_OUTOFMEMORY,
            ));
        }
        let head = UiaAppendRuntimeId as i32;
        SafeArrayPutElement(sa, &0i32, &head as *const i32 as *const core::ffi::c_void)?;
        for (i, p) in parts.iter().enumerate() {
            let idx = i as i32 + 1;
            SafeArrayPutElement(sa, &idx, p as *const i32 as *const core::ffi::c_void)?;
        }
        Ok(sa)
    }
}

/// 리스트 루트 프로바이더 — 활성 패널의 가시 행들을 자식으로 노출(프래그먼트 루트).
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot
)]
pub struct ListProvider {
    hwnd: HWND,
    snap: Arc<Snap>,
}

/// 행 프로바이더 — ListItem + SelectionItem 패턴(선택 상태 읽기).
#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    ISelectionItemProvider
)]
pub struct RowProvider {
    hwnd: HWND,
    snap: Arc<Snap>,
    /// snap.rows 내 인덱스.
    idx: usize,
}

pub fn list_provider(hwnd: HWND, snap: Arc<Snap>) -> IRawElementProviderSimple {
    ListProvider { hwnd, snap }.into()
}

fn row_provider(hwnd: HWND, snap: Arc<Snap>, idx: usize) -> IRawElementProviderFragment {
    RowProvider { hwnd, snap, idx }.into()
}

impl IRawElementProviderSimple_Impl for ListProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, _id: UIA_PATTERN_ID) -> Result<windows::core::IUnknown> {
        Err(windows::core::Error::empty())
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        match id {
            UIA_NamePropertyId => Ok(v_str(&self.snap.name)),
            UIA_ControlTypePropertyId => Ok(v_i4(UIA_ListControlTypeId.0)),
            _ => Ok(VARIANT::default()),
        }
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        unsafe { UiaHostProviderFromHwnd(self.hwnd) }
    }
}

impl IRawElementProviderFragment_Impl for ListProvider_Impl {
    fn Navigate(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment> {
        match dir {
            d if d == NavigateDirection_FirstChild && !self.snap.rows.is_empty() => {
                Ok(row_provider(self.hwnd, self.snap.clone(), 0))
            }
            d if d == NavigateDirection_LastChild && !self.snap.rows.is_empty() => Ok(
                row_provider(self.hwnd, self.snap.clone(), self.snap.rows.len() - 1),
            ),
            _ => Err(windows::core::Error::empty()),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut()) // 호스트(HWND) 프로바이더가 제공
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(uia_rect(self.snap.rect))
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(ListProvider {
            hwnd: self.hwnd,
            snap: self.snap.clone(),
        }
        .into())
    }
}

impl IRawElementProviderFragmentRoot_Impl for ListProvider_Impl {
    fn ElementProviderFromPoint(&self, _x: f64, y: f64) -> Result<IRawElementProviderFragment> {
        let idx = self
            .snap
            .rows
            .iter()
            .position(|r| y >= r.rect.1 as f64 && y < (r.rect.1 + r.rect.3) as f64);
        match idx {
            Some(i) => Ok(row_provider(self.hwnd, self.snap.clone(), i)),
            None => Err(windows::core::Error::empty()),
        }
    }
    fn GetFocus(&self) -> Result<IRawElementProviderFragment> {
        match self.snap.rows.iter().position(|r| r.focused) {
            Some(i) => Ok(row_provider(self.hwnd, self.snap.clone(), i)),
            None => Err(windows::core::Error::empty()),
        }
    }
}

impl RowProvider_Impl {
    fn row(&self) -> &RowSnap {
        &self.snap.rows[self.idx]
    }
}

impl IRawElementProviderSimple_Impl for RowProvider_Impl {
    fn ProviderOptions(&self) -> Result<ProviderOptions> {
        Ok(ProviderOptions_ServerSideProvider)
    }
    fn GetPatternProvider(&self, id: UIA_PATTERN_ID) -> Result<windows::core::IUnknown> {
        if id == UIA_SelectionItemPatternId {
            let p: ISelectionItemProvider = RowProvider {
                hwnd: self.hwnd,
                snap: self.snap.clone(),
                idx: self.idx,
            }
            .into();
            return Ok(p.into());
        }
        Err(windows::core::Error::empty())
    }
    fn GetPropertyValue(&self, id: UIA_PROPERTY_ID) -> Result<VARIANT> {
        match id {
            UIA_NamePropertyId => Ok(v_str(&self.row().name)),
            UIA_ControlTypePropertyId => Ok(v_i4(UIA_ListItemControlTypeId.0)),
            UIA_HasKeyboardFocusPropertyId => Ok(v_bool(self.row().focused)),
            UIA_IsKeyboardFocusablePropertyId => Ok(v_bool(true)),
            _ => Ok(VARIANT::default()),
        }
    }
    fn HostRawElementProvider(&self) -> Result<IRawElementProviderSimple> {
        Err(windows::core::Error::empty())
    }
}

impl IRawElementProviderFragment_Impl for RowProvider_Impl {
    fn Navigate(&self, dir: NavigateDirection) -> Result<IRawElementProviderFragment> {
        match dir {
            d if d == NavigateDirection_Parent => Ok(ListProvider {
                hwnd: self.hwnd,
                snap: self.snap.clone(),
            }
            .into()),
            d if d == NavigateDirection_NextSibling && self.idx + 1 < self.snap.rows.len() => {
                Ok(row_provider(self.hwnd, self.snap.clone(), self.idx + 1))
            }
            d if d == NavigateDirection_PreviousSibling && self.idx > 0 => {
                Ok(row_provider(self.hwnd, self.snap.clone(), self.idx - 1))
            }
            _ => Err(windows::core::Error::empty()),
        }
    }
    fn GetRuntimeId(&self) -> Result<*mut SAFEARRAY> {
        // 전역 행 인덱스 기반 — 스크롤에도 같은 행이면 같은 id
        runtime_id(&[(self.snap.first_row + self.idx) as i32])
    }
    fn BoundingRectangle(&self) -> Result<UiaRect> {
        Ok(uia_rect(self.row().rect))
    }
    fn GetEmbeddedFragmentRoots(&self) -> Result<*mut SAFEARRAY> {
        Ok(std::ptr::null_mut())
    }
    fn SetFocus(&self) -> Result<()> {
        Ok(())
    }
    fn FragmentRoot(&self) -> Result<IRawElementProviderFragmentRoot> {
        Ok(ListProvider {
            hwnd: self.hwnd,
            snap: self.snap.clone(),
        }
        .into())
    }
}

impl ISelectionItemProvider_Impl for RowProvider_Impl {
    fn Select(&self) -> Result<()> {
        Ok(()) // 읽기 전용 1차 — 선택 조작은 후속(M5-3)
    }
    fn AddToSelection(&self) -> Result<()> {
        Ok(())
    }
    fn RemoveFromSelection(&self) -> Result<()> {
        Ok(())
    }
    fn IsSelected(&self) -> Result<windows::core::BOOL> {
        Ok(self.row().selected.into())
    }
    fn SelectionContainer(&self) -> Result<IRawElementProviderSimple> {
        Ok(ListProvider {
            hwnd: self.hwnd,
            snap: self.snap.clone(),
        }
        .into())
    }
}

/// UIA 클라이언트(스크린리더 등)가 붙어 있는가 — 이벤트 발행 가드(무클라이언트 시 비용 0).
pub fn listening() -> bool {
    unsafe { UiaClientsAreListening().as_bool() }
}

/// `WM_GETOBJECT` 처리 — UIA 루트 요청이면 스냅샷 루트를 반환.
pub unsafe fn return_provider(
    hwnd: HWND,
    wparam: WPARAM,
    lparam: LPARAM,
    snap: Arc<Snap>,
) -> LRESULT {
    let provider = list_provider(hwnd, snap);
    UiaReturnRawElementProvider(hwnd, wparam, lparam, &provider)
}

/// 캐럿 이동 통지 — 포커스 변경 이벤트(스크린리더가 행 이름을 읽는 트리거).
pub unsafe fn raise_focus(hwnd: HWND, snap: Arc<Snap>) {
    if let Some(i) = snap.rows.iter().position(|r| r.focused) {
        let row: IRawElementProviderSimple = RowProvider {
            hwnd,
            snap: snap.clone(),
            idx: i,
        }
        .into();
        let _ = UiaRaiseAutomationEvent(&row, UIA_AutomationFocusChangedEventId);
    }
}
