//! 휴지통 항목 **복원**(M3-3 — 삭제 undo). **원본 이식**: `app/Nexa.App/RecycleBin.cs`
//! (B-13u S2 — docs/33 §Undo/Redo "일반삭제(휴지통)").
//!
//! 휴지통 셸 폴더를 열거해 "원래 위치+이름"이 일치하는 항목을 찾아 셸 `undelete` 동사를
//! 실행한다(탐색기 Ctrl+Z와 동일 메커니즘). 이름 매칭은 정확 일치 우선, 실패 시 확장자
//! 숨김 표시를 감안해 확장자 제외 일치 폴백.
//!
//! 원본과의 차이: `StrRetToBufW`(shlwapi) 대신 STRRET을 직접 파싱 — 신규 임포트 DLL 0
//! (B3 게이트 보호). COM 초기화는 호출 지점에서 국소 수행(성공 시에만 균형 해제).

use std::path::{Path, PathBuf};

use windows::core::PCSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    IContextMenu, IEnumIDList, IShellFolder, IShellFolder2, SHGetDesktopFolder,
    SHGetSpecialFolderLocation, CMINVOKECOMMANDINFO, CSIDL_BITBUCKET, SHCONTF_FOLDERS,
    SHCONTF_INCLUDEHIDDEN, SHCONTF_NONFOLDERS,
};
use windows_core::Interface;

/// 휴지통 상세 컬럼(0=이름 · 1=원래 위치 — XP 이후 고정 인덱스, 원본 동일).
const COL_NAME: u32 = 0;
const COL_ORIGINAL_LOCATION: u32 = 1;

/// 원래 경로 목록에 해당하는 휴지통 항목들을 복원. 반환 = 복원 실행된 항목 수
/// (요청 수보다 작을 수 있음 — 다중 버전은 경로당 최초 일치 1건, 원본 동일).
pub fn restore_by_original_paths(original_paths: &[PathBuf]) -> usize {
    // 셸 COM은 STA 권장 — 이미 초기화된 스레드면 S_FALSE(균형 해제), 모드 불일치면 진행만.
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
    let restored = unsafe { restore_inner(original_paths) }.unwrap_or(0);
    if hr.is_ok() {
        unsafe { CoUninitialize() };
    }
    restored
}

unsafe fn restore_inner(original_paths: &[PathBuf]) -> windows::core::Result<usize> {
    let mut wanted: Vec<String> = original_paths
        .iter()
        .map(|p| p.to_string_lossy().trim_end_matches(['\\', '/']).to_owned())
        .collect();

    let desktop: IShellFolder = SHGetDesktopFolder()?;
    let bin_pidl = SHGetSpecialFolderLocation(None, CSIDL_BITBUCKET as i32)?;
    let result = (|| -> windows::core::Result<usize> {
        let bin2: IShellFolder2 = desktop.BindToObject(bin_pidl, None)?;
        let bin: IShellFolder = bin2.cast()?;

        let mut enum_opt: Option<IEnumIDList> = None;
        bin.EnumObjects(
            HWND::default(),
            (SHCONTF_FOLDERS.0 | SHCONTF_NONFOLDERS.0 | SHCONTF_INCLUDEHIDDEN.0) as u32,
            &mut enum_opt,
        )
        .ok()?;
        let e = enum_opt.ok_or_else(windows::core::Error::empty)?;

        let mut matched: Vec<*mut ITEMIDLIST> = Vec::new(); // 복원할 child PIDL(휴지통 기준)
        let mut all_pidls: Vec<*mut ITEMIDLIST> = Vec::new(); // 해제용 전체
        let restored = (|| -> windows::core::Result<usize> {
            loop {
                if wanted.is_empty() {
                    break;
                }
                let mut child: [*mut ITEMIDLIST; 1] = [std::ptr::null_mut()];
                let mut fetched = 0u32;
                if e.Next(&mut child, Some(&mut fetched)) != windows::Win32::Foundation::S_OK
                    || fetched != 1
                {
                    break;
                }
                all_pidls.push(child[0]);
                let name = details_of(&bin2, child[0], COL_NAME);
                let loc = details_of(&bin2, child[0], COL_ORIGINAL_LOCATION);
                if name.is_empty() || loc.is_empty() {
                    continue;
                }
                let loc = loc.trim_end_matches(['\\', '/']);
                let original = format!("{loc}\\{name}");
                let idx = wanted
                    .iter()
                    .position(|w| w.eq_ignore_ascii_case(&original))
                    .or_else(|| {
                        // 확장자 숨김 표시 폴백: 같은 폴더 + 확장자 제외 이름 일치
                        wanted.iter().position(|w| {
                            let p = Path::new(w);
                            let dir = p.parent().map(|d| d.to_string_lossy()).unwrap_or_default();
                            let stem = p
                                .file_stem()
                                .map(|s| s.to_string_lossy())
                                .unwrap_or_default();
                            dir.trim_end_matches(['\\', '/']).eq_ignore_ascii_case(loc)
                                && stem.eq_ignore_ascii_case(&name)
                        })
                    });
                if let Some(idx) = idx {
                    matched.push(child[0]);
                    wanted.remove(idx); // 경로당 최초 일치 1건(다중 버전은 후속 — 삭제 시각 비교)
                }
            }
            if matched.is_empty() {
                return Ok(0);
            }

            // 일치 항목들의 IContextMenu → "undelete" 동사(탐색기 복원과 동일 — 충돌은 셸 UI가 처리)
            let apidl: Vec<*const ITEMIDLIST> =
                matched.iter().map(|p| *p as *const ITEMIDLIST).collect();
            let icm: IContextMenu = bin.GetUIObjectOf(HWND::default(), &apidl, None)?;
            let inv = CMINVOKECOMMANDINFO {
                cbSize: std::mem::size_of::<CMINVOKECOMMANDINFO>() as u32,
                lpVerb: PCSTR(c"undelete".as_ptr() as *const u8),
                nShow: 1, // SW_SHOWNORMAL
                ..Default::default()
            };
            icm.InvokeCommand(&inv)?;
            Ok(matched.len())
        })();
        for p in all_pidls {
            CoTaskMemFree(Some(p as *const core::ffi::c_void));
        }
        restored
    })();
    CoTaskMemFree(Some(bin_pidl as *const core::ffi::c_void));
    result
}

/// 휴지통 상세 컬럼 텍스트 — STRRET 직접 파싱(WSTR 우선·CSTR/OFFSET는 ANSI 최선, 실패 격리).
unsafe fn details_of(folder: &IShellFolder2, pidl: *mut ITEMIDLIST, col: u32) -> String {
    let mut sd = windows::Win32::UI::Shell::Common::SHELLDETAILS::default();
    if folder
        .GetDetailsOf(pidl as *const ITEMIDLIST, col, &mut sd)
        .is_err()
    {
        return String::new(); // 개별 항목 실패 격리(원본 동일)
    }
    // SHELLDETAILS는 packed(1) — str 필드를 정렬된 지역으로 복사 후 파싱(참조 금지).
    let mut strret = sd.str;
    strret_to_string(&mut strret, pidl)
}

/// STRRET → String. WSTR은 CoTaskMem 해제까지 책임진다(원본 StrRetToBufW 대체 — shlwapi 회피).
unsafe fn strret_to_string(s: &mut STRRET, pidl: *mut ITEMIDLIST) -> String {
    const WSTR: u32 = 0; // STRRET_WSTR
    const OFFSET: u32 = 1; // STRRET_OFFSET
    const CSTR: u32 = 2; // STRRET_CSTR
    match s.uType {
        WSTR => {
            let p = s.Anonymous.pOleStr;
            if p.is_null() {
                return String::new();
            }
            let out = p.to_string().unwrap_or_default();
            CoTaskMemFree(Some(p.0 as *const core::ffi::c_void));
            out
        }
        CSTR => {
            let bytes = &s.Anonymous.cStr;
            let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..len]).into_owned()
        }
        OFFSET => {
            let base = pidl as *const u8;
            let p = base.add(s.Anonymous.uOffset as usize);
            let mut len = 0usize;
            while *p.add(len) != 0 {
                len += 1;
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(p, len)).into_owned()
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 휴지통으로 삭제(테스트 전용 — win.rs delete_to_recycle_bin과 동일 SHFileOperationW).
    unsafe fn recycle(path: &Path) -> bool {
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::UI::Shell::{
            SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_SILENT, FO_DELETE,
            SHFILEOPSTRUCTW,
        };
        let mut list: Vec<u16> = path.as_os_str().encode_wide().collect();
        list.push(0);
        list.push(0);
        let mut op = SHFILEOPSTRUCTW {
            wFunc: FO_DELETE,
            pFrom: PCWSTR(list.as_ptr()),
            fFlags: (FOF_ALLOWUNDO.0 | FOF_NOCONFIRMATION.0 | FOF_SILENT.0) as u16,
            ..Default::default()
        };
        SHFileOperationW(&mut op) == 0 && !op.fAnyOperationsAborted.as_bool()
    }

    /// 실 휴지통 왕복 — 삭제 → 복원 → 원위치 확인. 실제 셸 휴지통을 건드리므로 #[ignore]
    /// (수동/실기 QA 전용). 고유 이름이라 다른 항목과 충돌하지 않고, 성공 시 자기 정리.
    #[test]
    #[ignore = "실 휴지통 부수효과 — 수동 실행(cargo test -p nexa-app -- --ignored)"]
    fn recycle_round_trip_restores_original() {
        let dir = std::env::temp_dir().join(format!("nexa_recycle_qa_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("복원대상_{}.txt", std::process::id()));
        std::fs::write(&file, "restore me").unwrap();

        assert!(unsafe { recycle(&file) }, "휴지통 삭제 실패");
        assert!(!file.exists(), "삭제 후 원위치 비어야 함");

        let restored = restore_by_original_paths(std::slice::from_ref(&file));
        assert_eq!(restored, 1, "1건 복원 보고");
        assert!(file.exists(), "원위치로 복원되어야 함");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "restore me");

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
