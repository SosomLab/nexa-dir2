//! fsprobe — **폴더 변경 저비용 프로브**(08-11 사용자 QA: "외부·클라우드에서 저장·
//! 동기화되면 목록이 안 바뀌어 갱신 버튼을 눌러야 한다").
//!
//! ## 왜 watcher만으로 부족한가
//!
//! [`crate::watcher`]의 `ReadDirectoryChangesW`는 **로컬 NTFS 규약**이다. 다음 경우에는
//! 구독이 성공(`is_alive()=true`)해도 통지가 **아예 오지 않거나 유실**된다 —
//! 이때 watcher는 죽은 게 아니라 **조용한** 것이라 07-31의 생존 자가 치유로도 못 잡는다.
//!
//! - **클라우드 가상 드라이브**(Google Drive `G:` DriveFS 등) — 파일 시스템 필터가
//!   디렉터리 변경 통지를 올려 주지 않는 구현이 있다.
//! - **네트워크 경로**(SMB) — 서버·프로토콜 버전에 따라 통지가 부분적이다.
//! - **동기화 클라이언트의 대량 반영** — 통지 버퍼 오버플로(`ERROR_NOTIFY_ENUM_DIR`)는
//!   watcher가 복구하지만, 그 밖의 유실은 다음 변경이 올 때까지 드러나지 않는다.
//!
//! ## 프로브 규약
//!
//! 통지에 **의존하지 않고** 폴더 상태를 싸게 요약해 직전 값과 비교한다. 비교가 다르면
//! 재로드 계기를 만들 뿐, 재로드 자체는 기존 디바운스 경로(win.rs)가 수행한다.
//!
//! - **1단계(항상·O(1))** = 폴더 자신의 mtime. NTFS는 항목 추가·삭제·이름 변경 시
//!   디렉터리 엔트리를 갱신하므로 **"파일이 생겼다/없어졌다"는 이것만으로 잡힌다**.
//! - **2단계(상한 내 1회 열거)** = 항목 수 · 총 바이트 · 최신 자식 mtime. 1단계가
//!   못 잡는 **내용만 바뀐 저장**(같은 이름 덮어쓰기 — 크기·수정일 열이 낡는다)을 잡는다.
//!   항목이 [`SCAN_CAP`]을 넘으면 **열거를 포기**하고 1단계만 남긴다(대형 폴더에서
//!   폴링 비용이 터지지 않게 — 그 경우 내용 변경은 watcher 통지·수동 F5 몫).
//!
//! 폴링 주기·중단 조건은 win.rs 소관이다(포그라운드일 때만 — DR-2 상주 규율).
//! 이 모듈은 `std::fs`만 쓰므로 **전 플랫폼 컴파일·테스트**된다(08-02 비Windows 교훈).

use std::path::Path;
use std::time::SystemTime;

/// 2단계 열거 상한 — 이 수를 넘는 폴더는 1단계(폴더 mtime)만 본다.
/// 근거: 10k 폴더 열거 실측이 수 ms대(P1 100k 첫 렌더 115ms 중 열거 포함)이므로
/// 4k는 폴링 주기(3s) 대비 무시할 수준이고, 그 이상은 자동 갱신보다 비용이 앞선다.
pub const SCAN_CAP: usize = 4_096;

/// 폴더 상태 요약 — 같은 폴더의 두 시점을 비교하는 용도로만 의미가 있다.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DirSig {
    /// 폴더 자신의 mtime(1단계). 조회 실패 = `None`.
    pub dir_mtime: Option<SystemTime>,
    /// 2단계 성립 여부 — false면 아래 세 값은 무의미(비교에서 제외).
    pub scanned: bool,
    pub count: u64,
    pub bytes: u64,
    /// 자식 중 최신 mtime.
    pub newest: Option<SystemTime>,
}

/// `path`의 현재 서명. 폴더가 아니거나 접근 불가면 `None`
/// (가상 루트 `::PC::`·`::CLOUD:` 등은 여기 걸린다 — **네트워크 호출 없음**).
pub fn probe(path: &Path) -> Option<DirSig> {
    let md = std::fs::metadata(path).ok()?;
    if !md.is_dir() {
        return None;
    }
    let mut sig = DirSig {
        dir_mtime: md.modified().ok(),
        ..Default::default()
    };
    let Ok(rd) = std::fs::read_dir(path) else {
        return Some(sig); // 열거 불가(권한 등) = 1단계만
    };
    let (mut count, mut bytes, mut newest) = (0u64, 0u64, None::<SystemTime>);
    for ent in rd {
        let Ok(ent) = ent else { continue };
        count += 1;
        if count as usize > SCAN_CAP {
            return Some(sig); // 상한 초과 = 2단계 포기(scanned=false 유지)
        }
        // symlink_metadata = 링크를 따라가지 않는다(원격 대상 stat로 폴링이 느려지는 것 방지)
        if let Ok(m) = ent.metadata().or_else(|_| ent.path().symlink_metadata()) {
            bytes = bytes.wrapping_add(m.len());
            if let Ok(t) = m.modified() {
                if newest.is_none_or(|n| t > n) {
                    newest = Some(t);
                }
            }
        }
    }
    sig.scanned = true;
    sig.count = count;
    sig.bytes = bytes;
    sig.newest = newest;
    Some(sig)
}

/// 두 서명이 **같은 폴더의 다른 상태**인가 — true면 재로드 계기.
/// 2단계가 한쪽만 성립하면(상한을 오갔다 = 항목 수가 크게 변함) 변경으로 본다.
pub fn changed(old: &DirSig, new: &DirSig) -> bool {
    if old.dir_mtime != new.dir_mtime {
        return true;
    }
    if old.scanned != new.scanned {
        return true;
    }
    old.scanned && (old.count != new.count || old.bytes != new.bytes || old.newest != new.newest)
}

/// 디바운스 **연장 상한** 판정(win.rs 공용) — 첫 통지로부터 `max_ms`를 넘겼으면
/// 더 미루지 않는다. 이것이 없으면 동기화 클라이언트가 디바운스 창(300ms)보다
/// 촘촘히 변경을 쏟는 동안 타이머가 **무한히 밀려** 동기화가 끝날 때까지 화면이
/// 멈춘 것처럼 보인다(사용자 보고의 "동기화 중 갱신 안 됨" 경로).
pub fn debounce_should_extend(since_ms: u64, now_ms: u64, max_ms: u64) -> bool {
    now_ms.saturating_sub(since_ms) < max_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("nexa_probe_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// 파일이 **생기면** 잡힌다 — 사용자 보고의 지배적 경로(외부 저장·동기화 반영).
    #[test]
    fn probe_detects_added_file() {
        let dir = tmp("add");
        let a = probe(&dir).expect("폴더 서명");
        std::fs::write(dir.join("new.txt"), b"x").unwrap();
        let b = probe(&dir).expect("폴더 서명");
        assert!(changed(&a, &b), "파일 추가가 서명에 안 잡힘");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// **내용만** 바뀐 덮어쓰기도 잡힌다 — 폴더 mtime은 그대로일 수 있으므로
    /// 2단계(크기·최신 mtime)가 필요한 자리.
    #[test]
    fn probe_detects_content_only_change() {
        let dir = tmp("content");
        let f = dir.join("a.bin");
        std::fs::write(&f, b"short").unwrap();
        let a = probe(&dir).expect("폴더 서명");
        std::fs::write(&f, b"a much longer body").unwrap();
        let b = probe(&dir).expect("폴더 서명");
        assert!(changed(&a, &b), "내용 변경(크기 증가)이 서명에 안 잡힘");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 변화가 없으면 재로드하지 않는다(폴링이 매 틱 재로드를 유발하면 안 된다).
    #[test]
    fn probe_is_stable_without_change() {
        let dir = tmp("stable");
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        let a = probe(&dir).expect("폴더 서명");
        let b = probe(&dir).expect("폴더 서명");
        assert!(!changed(&a, &b), "무변경인데 변경으로 판정 — 폴링 재로드 폭주");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 가상 루트·없는 경로는 `None`(네트워크·셸 호출로 새지 않는다).
    #[test]
    fn probe_none_for_non_directory() {
        assert!(probe(Path::new("::PC::")).is_none());
        assert!(probe(Path::new("::CLOUD:onedrive:x")).is_none());
    }

    /// 디바운스는 상한까지만 연장된다 — 상한 초과 시 재무장 금지.
    #[test]
    fn debounce_extension_is_capped() {
        assert!(debounce_should_extend(1_000, 1_200, 1_000), "상한 내 = 연장");
        assert!(
            !debounce_should_extend(1_000, 2_000, 1_000),
            "상한 도달 = 연장 중단(그래야 동기화 폭주 중에도 화면이 갱신된다)"
        );
    }
}
