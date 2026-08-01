//! 클라우드 API 탐색(X-37 2차 — [ADR-0006](../../../docs/27-adr-0006-cloud-oauth.md) §3).
//!
//! **UI 스레드는 절대 네트워크를 타지 않는다**(07-21 SHFileOperation 교훈 계승):
//!
//! ```text
//! 트리 열거(UI) ──▶ cache_get(idx, inner)
//!                     ├ 히트  → 항목 즉시 반환
//!                     └ 미스  → 빈 목록 반환 + 워커 1회 기동
//!                                 └ refresh → API 목록 → 캐시 저장
//!                                    → PostMessage → 재로드(이번엔 히트)
//! ```
//!
//! 캐시는 (연결 인덱스, 내부 경로) 단위. F5·연결 해제 시 무효화한다.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use nexa_core::FileKind;
use nexa_vfs::Entry;

use crate::oauth::{self, Service};

/// 캐시 키 = (연결 인덱스, 내부 경로).
type CacheKey = (usize, String);
type ListCache = HashMap<CacheKey, Vec<Entry>>;

/// 목록 캐시.
static CACHE: Mutex<Option<ListCache>> = Mutex::new(None);
/// 진행 중 요청(중복 기동 방지) — 트리가 같은 폴더를 여러 번 열거해도 워커는 1개.
static INFLIGHT: Mutex<Option<HashSet<CacheKey>>> = Mutex::new(None);

fn with_cache<R>(f: impl FnOnce(&mut ListCache) -> R) -> R {
    let mut g = crate::win::plock(&CACHE);
    f(g.get_or_insert_with(HashMap::new))
}

fn with_inflight<R>(f: impl FnOnce(&mut HashSet<CacheKey>) -> R) -> R {
    let mut g = crate::win::plock(&INFLIGHT);
    f(g.get_or_insert_with(HashSet::new))
}

/// 캐시 조회(UI 스레드 — 즉시 반환). `None` = 미적재.
pub fn cache_get(idx: usize, inner: &str) -> Option<Vec<Entry>> {
    with_cache(|c| c.get(&(idx, inner.to_string())).cloned())
}

/// 캐시 저장(워커 완료).
fn cache_put(idx: usize, inner: &str, entries: Vec<Entry>) {
    with_cache(|c| {
        c.insert((idx, inner.to_string()), entries);
    });
}

/// 연결 1개의 캐시 전부 무효화(F5·연결 해제·재인증).
pub fn invalidate(idx: usize) {
    with_cache(|c| c.retain(|(i, _), _| *i != idx));
}

/// 전체 무효화(연결 목록 재배치 등).
pub fn invalidate_all() {
    with_cache(|c| c.clear());
}

/// 이미 요청 중이면 `false`(중복 기동 금지), 아니면 표시하고 `true`.
fn claim(idx: usize, inner: &str) -> bool {
    with_inflight(|s| s.insert((idx, inner.to_string())))
}

fn release(idx: usize, inner: &str) {
    with_inflight(|s| {
        s.remove(&(idx, inner.to_string()));
    });
}

/// 워커 결과(WM_APP_CLOUD_LIST lparam — 수신 측이 Box 회수).
pub struct ListResult {
    pub idx: usize,
    pub inner: String,
    /// 실패 사유(빈 문자열 = 성공).
    pub err: String,
}

/// 목록 적재 요청 — **비동기**. 이미 캐시에 있거나 진행 중이면 아무것도 하지 않는다.
///
/// # Safety
/// `hwnd_raw`는 유효한 앱 창(파괴 후 통지는 PostMessage 실패로 무해).
pub fn request(hwnd_raw: isize, idx: usize, inner: &str, conn: ConnInfo) {
    if cache_get(idx, inner).is_some() || !claim(idx, inner) {
        return;
    }
    let inner_owned = inner.to_string();
    std::thread::spawn(move || {
        let err = match load_blocking(idx, &inner_owned, &conn) {
            Ok(entries) => {
                cache_put(idx, &inner_owned, entries);
                String::new()
            }
            Err(e) => e,
        };
        release(idx, &inner_owned);
        let boxed = Box::new(ListResult {
            idx,
            inner: inner_owned,
            err,
        });
        crate::win::post_cloud_list(hwnd_raw, Box::into_raw(boxed) as isize);
    });
}

/// 워커에 넘길 연결 정보(State 참조 없이 자족 — 재진입 안전).
#[derive(Clone)]
pub struct ConnInfo {
    pub kind: String,
    pub client_id: String,
    /// DPAPI에서 복호한 refresh 토큰.
    pub refresh: String,
}

/// 블로킹 적재(워커 전용) — refresh로 access 발급 → 서비스별 목록 조회 → Entry 변환.
fn load_blocking(idx: usize, inner: &str, conn: &ConnInfo) -> Result<Vec<Entry>, String> {
    let svc = oauth::service_of(&conn.kind).ok_or("unknown service")?;
    let tokens = oauth::refresh(&svc, &conn.client_id, &conn.refresh)
        .map_err(|e| tr_key(e.key()))?;
    let body = fetch_list(&svc, &tokens.access, inner)?;
    Ok(parse_list(&svc, &body, idx, inner))
}

/// i18n 키를 문구로(워커에서도 안전 — i18n은 읽기 전용 전역).
fn tr_key(key: &str) -> String {
    crate::i18n::tr(key)
}

/// 서비스별 목록 엔드포인트 호출.
fn fetch_list(svc: &Service, access: &str, inner: &str) -> Result<String, String> {
    match svc.kind {
        "onedrive" => {
            // 경로 주소 지정: 루트 = /me/drive/root/children,
            // 하위 = /me/drive/root:/<경로>:/children (Graph 규약)
            let url = if inner.is_empty() {
                "https://graph.microsoft.com/v1.0/me/drive/root/children\
                 ?$select=id,name,size,folder,file,lastModifiedDateTime&$top=500"
                    .to_string()
            } else {
                let enc = inner
                    .trim_start_matches('/')
                    .split('/')
                    .map(oauth::percent)
                    .collect::<Vec<_>>()
                    .join("/");
                format!(
                    "https://graph.microsoft.com/v1.0/me/drive/root:/{enc}:/children\
                     ?$select=id,name,size,folder,file,lastModifiedDateTime&$top=500"
                )
            };
            oauth::http_get(&url, access)
        }
        "googledrive" => {
            // 1차 = 루트만(폴더 재귀는 부모 ID 필요 — 후속). q로 루트 자식 조회.
            let url = "https://www.googleapis.com/drive/v3/files\
                 ?q=%27root%27+in+parents+and+trashed%3Dfalse\
                 &fields=files(id,name,size,mimeType,modifiedTime)&pageSize=500";
            oauth::http_get(url, access)
        }
        "dropbox" => Err("dropbox listing is not implemented yet".into()),
        _ => Err("unsupported service".into()),
    }
}

/// 응답 → [`Entry`] 변환(표시명은 이름, 진입 경로는 `target` 센티널).
fn parse_list(svc: &Service, body: &str, idx: usize, inner: &str) -> Vec<Entry> {
    let (array_key, is_google) = match svc.kind {
        "googledrive" => ("files", true),
        _ => ("value", false),
    };
    oauth::json_objects(body, array_key)
        .iter()
        .filter_map(|o| {
            let name = oauth::json_str(o, "name")?;
            let is_dir = if is_google {
                oauth::json_str(o, "mimeType").as_deref()
                    == Some("application/vnd.google-apps.folder")
            } else {
                oauth::json_has(o, "folder")
            };
            let size = oauth::json_str(o, "size")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            let modified = oauth::json_str(o, if is_google { "modifiedTime" } else { "lastModifiedDateTime" })
                .and_then(|s| parse_rfc3339(&s));
            Some(Entry {
                target: Some(nexa_vfs::cloud_child(idx, inner, &name)),
                name,
                kind: if is_dir { FileKind::Dir } else { FileKind::File },
                size,
                modified,
                attrs: 0,
            })
        })
        .collect()
}

/// RFC3339(`2026-08-01T12:34:56Z`) → SystemTime. 시간대 오프셋은 무시(UTC 가정 —
/// 표시층이 로컬 변환하므로 초 단위 정확도로 충분).
fn parse_rfc3339(s: &str) -> Option<std::time::SystemTime> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let num = |a: usize, z: usize| -> Option<i64> { s.get(a..z)?.parse().ok() };
    let (y, mo, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (h, mi, sec) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    // 1970-01-01 기준 일수(민감하지 않은 표시용 — 그레고리력 공식)
    let a = (14 - mo) / 12;
    let yy = y + 4800 - a;
    let mm = mo + 12 * a - 3;
    let jdn = d + (153 * mm + 2) / 5 + 365 * yy + yy / 4 - yy / 100 + yy / 400 - 32045;
    let days = jdn - 2_440_588; // JDN of 1970-01-01
    let secs = days * 86_400 + h * 3600 + mi * 60 + sec;
    (secs >= 0).then(|| {
        std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_parses_to_epoch() {
        let t = parse_rfc3339("1970-01-01T00:00:00Z").unwrap();
        assert_eq!(t, std::time::UNIX_EPOCH);
        let t = parse_rfc3339("2026-08-01T12:00:00Z").unwrap();
        let secs = t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        // 2026-08-01 12:00 UTC = 1785585600
        assert_eq!(secs, 1_785_585_600, "그레고리력 변환");
        assert!(parse_rfc3339("bad").is_none());
    }

    /// Graph 응답 → Entry: 폴더/파일 판별·크기·진입 경로(target 센티널).
    #[test]
    fn parses_graph_children() {
        let body = r#"{"value":[
          {"id":"1","name":"Documents","size":0,"folder":{"childCount":2},"lastModifiedDateTime":"2026-07-01T09:00:00Z"},
          {"id":"2","name":"note.txt","size":42,"file":{"mimeType":"text/plain"},"lastModifiedDateTime":"2026-07-02T10:30:00Z"}
        ]}"#;
        let v = parse_list(&oauth::ONEDRIVE, body, 0, "");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name, "Documents");
        assert_eq!(v[0].kind, FileKind::Dir);
        assert_eq!(v[0].target.as_deref(), Some("::CLOUD:0::/Documents"));
        assert_eq!(v[1].kind, FileKind::File);
        assert_eq!(v[1].size, 42);
        assert!(v[1].modified.is_some());
    }

    /// 하위 폴더에서도 target이 부모 경로를 이어받아야 한다.
    #[test]
    fn nested_target_paths() {
        let body = r#"{"value":[{"name":"a.txt","size":1,"file":{}}]}"#;
        let v = parse_list(&oauth::ONEDRIVE, body, 2, "/Docs");
        assert_eq!(v[0].target.as_deref(), Some("::CLOUD:2::/Docs/a.txt"));
    }

    /// Google Drive는 mimeType으로 폴더를 판별하고 배열 키가 `files`.
    #[test]
    fn parses_google_files() {
        let body = r#"{"files":[
          {"id":"x","name":"Shared","mimeType":"application/vnd.google-apps.folder"},
          {"id":"y","name":"a.pdf","mimeType":"application/pdf","size":"99"}
        ]}"#;
        let v = parse_list(&oauth::GOOGLEDRIVE, body, 1, "");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].kind, FileKind::Dir);
        assert_eq!(v[1].kind, FileKind::File);
        assert_eq!(v[1].size, 99, "Google size는 문자열이지만 파싱");
    }

    #[test]
    fn cache_roundtrip_and_invalidate() {
        invalidate_all();
        assert!(cache_get(7, "/x").is_none());
        cache_put(7, "/x", vec![]);
        assert!(cache_get(7, "/x").is_some());
        invalidate(7);
        assert!(cache_get(7, "/x").is_none());
    }

    /// 중복 기동 방지 — 같은 키를 두 번 claim할 수 없다.
    #[test]
    fn inflight_claim_is_exclusive() {
        assert!(claim(11, "/a"));
        assert!(!claim(11, "/a"), "진행 중 재요청 차단");
        release(11, "/a");
        assert!(claim(11, "/a"), "해제 후 재요청 가능");
        release(11, "/a");
    }
}
