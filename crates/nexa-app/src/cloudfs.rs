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
/// 항목 ID 캐시 — 키 = (연결, 항목 내부 경로) → 서비스 파일 ID(X-37 3차 다운로드).
/// Google Drive는 경로 주소 지정이 없어 ID가 필수라 목록 시점에 함께 적재한다.
static IDS: Mutex<Option<HashMap<CacheKey, String>>> = Mutex::new(None);
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

/// 항목 ID 조회·저장(다운로드 시 Google Drive가 요구).
fn id_put(idx: usize, item_inner: &str, id: &str) {
    let mut g = crate::win::plock(&IDS);
    g.get_or_insert_with(HashMap::new)
        .insert((idx, item_inner.to_string()), id.to_string());
}
fn id_get(idx: usize, item_inner: &str) -> Option<String> {
    let mut g = crate::win::plock(&IDS);
    g.get_or_insert_with(HashMap::new)
        .get(&(idx, item_inner.to_string()))
        .cloned()
}

/// 연결 1개의 캐시 전부 무효화(F5·연결 해제·재인증).
pub fn invalidate(idx: usize) {
    with_cache(|c| c.retain(|(i, _), _| *i != idx));
    let mut g = crate::win::plock(&IDS);
    g.get_or_insert_with(HashMap::new)
        .retain(|(i, _), _| *i != idx);
}

/// 전체 무효화(연결 목록 재배치 등).
pub fn invalidate_all() {
    with_cache(|c| c.clear());
    let mut g = crate::win::plock(&IDS);
    g.get_or_insert_with(HashMap::new).clear();
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
    /// Google 전용 — 빈 문자열이면 전송하지 않는다(MS·Dropbox).
    pub client_secret: String,
    /// DPAPI에서 복호한 refresh 토큰.
    pub refresh: String,
}

/// 블로킹 적재(워커 전용) — refresh로 access 발급 → 서비스별 목록 조회 → Entry 변환.
fn load_blocking(idx: usize, inner: &str, conn: &ConnInfo) -> Result<Vec<Entry>, String> {
    let svc = oauth::service_of(&conn.kind).ok_or("unknown service")?;
    let tokens = oauth::refresh(&svc, &conn.client_id, &conn.client_secret, &conn.refresh)
        .map_err(|e| tr_key(e.key()))?;
    let body = fetch_list(&svc, &tokens.access, idx, inner)?;
    Ok(parse_list(&svc, &body, idx, inner))
}

/// i18n 키를 문구로(워커에서도 안전 — i18n은 읽기 전용 전역).
fn tr_key(key: &str) -> String {
    crate::i18n::tr(key)
}

/// 서비스별 목록 엔드포인트 호출.
fn fetch_list(svc: &Service, access: &str, idx: usize, inner: &str) -> Result<String, String> {
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
            // 부모 ID 기준 조회 — 루트는 예약어 `root`, 하위는 목록 시점에 캐시한 ID.
            let parent = if inner.is_empty() {
                "root".to_string()
            } else {
                id_get(idx, inner).ok_or("folder id unknown — 상위 폴더를 새로 고치세요")?
            };
            let url = format!(
                "https://www.googleapis.com/drive/v3/files\
                 ?q={}+in+parents+and+trashed%3Dfalse\
                 &fields=files(id,name,size,mimeType,modifiedTime)&pageSize=500",
                oauth::percent(&format!("'{parent}'"))
            );
            oauth::http_get(&url, access)
        }
        "dropbox" => {
            // Dropbox는 전부 POST + JSON. 루트는 빈 문자열, 하위는 `/경로`.
            let body = format!(
                "{{\"path\":\"{}\",\"limit\":500}}",
                json_escape(inner) // 루트 = "" (Dropbox 규약)
            );
            oauth::http_send(
                "https://api.dropboxapi.com/2/files/list_folder",
                "POST",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
        }
        _ => Err("unsupported service".into()),
    }
}

/// 응답 → [`Entry`] 변환(표시명은 이름, 진입 경로는 `target` 센티널).
fn parse_list(svc: &Service, body: &str, idx: usize, inner: &str) -> Vec<Entry> {
    let array_key = match svc.kind {
        "googledrive" => "files",
        "dropbox" => "entries",
        _ => "value",
    };
    oauth::json_objects(body, array_key)
        .iter()
        .filter_map(|o| {
            let name = oauth::json_str(o, "name")?;
            let is_dir = match svc.kind {
                "googledrive" => {
                    oauth::json_str(o, "mimeType").as_deref()
                        == Some("application/vnd.google-apps.folder")
                }
                // Dropbox는 `.tag`가 "folder"/"file"
                "dropbox" => oauth::json_str(o, ".tag").as_deref() == Some("folder"),
                _ => oauth::json_has(o, "folder"),
            };
            let size = oauth::json_str(o, "size")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);
            // 수정 시각 키가 서비스마다 다르다
            let mod_key = match svc.kind {
                "googledrive" => "modifiedTime",
                "dropbox" => "server_modified",
                _ => "lastModifiedDateTime",
            };
            let modified = oauth::json_str(o, mod_key).and_then(|s| parse_rfc3339(&s));
            // 항목 ID 적재 — Google은 목록/다운로드에 필수, Dropbox는 `id`(file/folder id).
            // OneDrive는 경로 주소 지정이 되지만 함께 담아 둔다.
            if let Some(id) = oauth::json_str(o, "id") {
                id_put(idx, &format!("{inner}/{name}"), &id);
            }
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

// ── 다운로드(X-37 3차 — 읽기 scope로 가능) ────────────────────────────────────

/// 단일 파일 다운로드 상한(메모리 적재 방식 — 스트리밍은 후속).
const DOWNLOAD_LIMIT: usize = 512 * 1024 * 1024;

/// 다운로드 작업 1건 — 클라우드 항목 → 로컬 대상 경로.
#[derive(Clone)]
pub struct DownloadItem {
    pub inner: String,
    pub dest: std::path::PathBuf,
    /// 폴더면 워커가 하위를 재귀 전개한다(파일이면 그대로 받는다).
    pub is_dir: bool,
}

/// 다운로드 완료 통지(WM_APP_CLOUD_DOWNLOAD lparam — 수신 측이 Box 회수).
pub struct DownloadResult {
    /// 성공한 로컬 경로들(열기·재로드 대상).
    pub done: Vec<std::path::PathBuf>,
    /// 실패 사유(빈 문자열 = 전건 성공).
    pub err: String,
    /// 완료 후 그 파일을 **연결 프로그램으로 열지** 여부(더블클릭 경로).
    pub open_after: bool,
}

/// 다운로드 시작(비동기 — 워커). 완료 시 `WM_APP_CLOUD_DOWNLOAD` 통지.
pub fn start_download(
    hwnd_raw: isize,
    idx: usize,
    items: Vec<DownloadItem>,
    conn: ConnInfo,
    open_after: bool,
) {
    std::thread::spawn(move || {
        let mut done = Vec::new();
        let mut err = String::new();
        match token_for(&conn) {
            Err(e) => err = e,
            Ok(access) => {
                // 폴더는 먼저 재귀 전개해 파일 목록으로 평탄화(X-37 5차)
                let mut flat: Vec<DownloadItem> = Vec::new();
                let svc = oauth::service_of(&conn.kind);
                for it in &items {
                    if it.is_dir {
                        let Some(svc) = svc.as_ref() else { continue };
                        if let Err(e) = std::fs::create_dir_all(&it.dest) {
                            err = e.to_string();
                            break;
                        }
                        if let Err(e) =
                            expand_tree(&access, svc, idx, &it.inner, &it.dest, &mut flat)
                        {
                            err = e;
                            break;
                        }
                    } else {
                        flat.push(it.clone());
                    }
                }
                if err.is_empty() {
                    for it in &flat {
                        match download_one(idx, &conn, &access, it) {
                            Ok(()) => done.push(it.dest.clone()),
                            Err(e) => {
                                err = e;
                                break; // 첫 실패에서 중단(부분 결과는 done에 남는다)
                            }
                        }
                    }
                }
            }
        }
        let boxed = Box::new(DownloadResult {
            done,
            err,
            open_after,
        });
        crate::win::post_cloud_download(hwnd_raw, Box::into_raw(boxed) as isize);
    });
}

/// refresh → access 토큰(워커 전용).
fn token_for(conn: &ConnInfo) -> Result<String, String> {
    let svc = oauth::service_of(&conn.kind).ok_or("unknown service")?;
    oauth::refresh(&svc, &conn.client_id, &conn.client_secret, &conn.refresh)
        .map(|t| t.access)
        .map_err(|e| tr_key(e.key()))
}

/// 항목 1개 다운로드 → 로컬 파일 기록(원자적: 임시 파일 → rename).
fn download_one(
    idx: usize,
    conn: &ConnInfo,
    access: &str,
    it: &DownloadItem,
) -> Result<(), String> {
    let bytes = match conn.kind.as_str() {
        "onedrive" => {
            // 사전 인증 downloadUrl을 먼저 받고, 그 URL에는 **인증 헤더를 붙이지 않는다**
            let enc = it
                .inner
                .trim_start_matches('/')
                .split('/')
                .map(oauth::percent)
                .collect::<Vec<_>>()
                .join("/");
            let meta = oauth::http_get(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/drive/root:/{enc}\
                     ?$select=@microsoft.graph.downloadUrl"
                ),
                access,
            )?;
            let url = oauth::json_str(&meta, "@microsoft.graph.downloadUrl")
                .ok_or("no downloadUrl")?;
            oauth::http_get_bytes(&url, None, DOWNLOAD_LIMIT)?
        }
        "googledrive" => {
            let id = id_get(idx, &it.inner).ok_or("file id unknown — 목록을 새로 고치세요")?;
            oauth::http_get_bytes(
                &format!("https://www.googleapis.com/drive/v3/files/{id}?alt=media"),
                Some(access),
                DOWNLOAD_LIMIT,
            )?
        }
        "dropbox" => {
            // content API — 인자는 **헤더**로 전달하고 본문은 비운다(Dropbox 규약).
            let arg = format!("Dropbox-API-Arg: {{\"path\":\"{}\"}}\r\n", json_escape(&it.inner));
            oauth::http_send_bytes(
                "https://content.dropboxapi.com/2/files/download",
                "POST",
                None,
                None,
                &arg,
                Some(access),
                DOWNLOAD_LIMIT,
            )?
        }
        _ => return Err("download not supported for this service".into()),
    };
    if let Some(dir) = it.dest.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = it.dest.with_extension("nexadl.part");
    std::fs::write(&tmp, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, &it.dest).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

// ── 쓰기(X-37 4차 — Files.ReadWrite scope 필요·재로그인 전제) ─────────────────

/// 업로드 세션 청크 크기 — Graph 규약상 **320 KiB의 배수**여야 한다(약 5MB).
const CHUNK: usize = 320 * 1024 * 16;
/// 단순 PUT 상한 — 이보다 크면 업로드 세션(청크 스트리밍)으로 전환.
const SIMPLE_PUT_MAX: u64 = 4 * 1024 * 1024;

/// 쓰기 작업 종류(한 워커가 배치로 처리).
#[derive(Clone)]
pub enum WriteOp {
    /// 로컬 파일 → 클라우드 업로드(대상 폴더 내부 경로 + 로컬 원본).
    Upload { src: std::path::PathBuf, dest_inner: String },
    /// 클라우드 항목 삭제.
    Delete { inner: String },
    /// 이름 변경(대상 항목 + 새 이름).
    Rename { inner: String, new_name: String },
    /// 새 폴더(부모 내부 경로 + 이름).
    NewFolder { parent_inner: String, name: String },
    /// 로컬 폴더 **재귀 업로드**(하위 폴더 생성 + 전 파일 업로드).
    UploadTree { src: std::path::PathBuf, dest_inner: String },
    /// 같은 연결 내 **서버 사이드 복사**(다운로드/재업로드 없음 — Graph copy API).
    CopyWithin { inner: String, dest_parent_inner: String },
    /// 같은 연결 내 이동(부모 변경 — PATCH parentReference).
    MoveWithin { inner: String, dest_parent_inner: String },
}

/// 쓰기 완료 통지(WM_APP_CLOUD_WRITE lparam — 수신 측이 Box 회수).
pub struct WriteResult {
    pub idx: usize,
    /// 성공 건수.
    pub done: usize,
    /// 실패 사유(빈 문자열 = 전건 성공).
    pub err: String,
}

/// 쓰기 시작(비동기 — 워커). 완료 시 `WM_APP_CLOUD_WRITE` 통지 + 캐시 무효화.
pub fn start_write(hwnd_raw: isize, idx: usize, ops: Vec<WriteOp>, conn: ConnInfo) {
    std::thread::spawn(move || {
        let mut done = 0usize;
        let mut err = String::new();
        match token_for(&conn) {
            Err(e) => err = e,
            Ok(access) => {
                for op in &ops {
                    match apply_write(idx, &conn, &access, op) {
                        Ok(()) => done += 1,
                        Err(e) => {
                            err = e;
                            break;
                        }
                    }
                }
            }
        }
        invalidate(idx); // 목록이 바뀌었다 — 다음 열거에서 재조회
        let boxed = Box::new(WriteResult { idx, done, err });
        crate::win::post_cloud_write(hwnd_raw, Box::into_raw(boxed) as isize);
    });
}

/// Graph 경로 주소 지정용 인코딩(`/a/b` → `a/b` 각 세그먼트 퍼센트 인코딩).
fn enc_path(inner: &str) -> String {
    inner
        .trim_start_matches('/')
        .split('/')
        .map(oauth::percent)
        .collect::<Vec<_>>()
        .join("/")
}

/// 쓰기 1건 수행.
fn apply_write(idx: usize, conn: &ConnInfo, access: &str, op: &WriteOp) -> Result<(), String> {
    match conn.kind.as_str() {
        "dropbox" => return apply_write_dropbox(access, op),
        "googledrive" => return apply_write_google(idx, access, op),
        _ => {}
    }
    match op {
        WriteOp::Upload { src, dest_inner } => upload_onedrive(access, src, dest_inner),
        WriteOp::Delete { inner } => oauth::http_send(
            &format!(
                "https://graph.microsoft.com/v1.0/me/drive/root:/{}",
                enc_path(inner)
            ),
            "DELETE",
            None,
            None,
            "",
            Some(access),
        )
        .map(|_| ()),
        WriteOp::Rename { inner, new_name } => {
            let body = format!("{{\"name\":\"{}\"}}", json_escape(new_name));
            oauth::http_send(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/drive/root:/{}",
                    enc_path(inner)
                ),
                "PATCH",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::UploadTree { src, dest_inner } => upload_tree(access, src, dest_inner),
        WriteOp::CopyWithin {
            inner,
            dest_parent_inner,
        } => {
            // Graph copy는 **비동기 202**(Location 모니터) — 요청 수락까지만 확인한다.
            let name = inner.rsplit('/').next().unwrap_or("item");
            let parent = if dest_parent_inner.is_empty() {
                "\"path\":\"/drive/root:\"".to_string()
            } else {
                format!("\"path\":\"/drive/root:{}\"", json_escape(dest_parent_inner))
            };
            let body = format!(
                "{{\"parentReference\":{{{parent}}},\"name\":\"{}\"}}",
                json_escape(name)
            );
            oauth::http_send(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/drive/root:/{}:/copy",
                    enc_path(inner)
                ),
                "POST",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::MoveWithin {
            inner,
            dest_parent_inner,
        } => {
            let parent = if dest_parent_inner.is_empty() {
                "\"path\":\"/drive/root:\"".to_string()
            } else {
                format!("\"path\":\"/drive/root:{}\"", json_escape(dest_parent_inner))
            };
            let body = format!("{{\"parentReference\":{{{parent}}}}}");
            oauth::http_send(
                &format!(
                    "https://graph.microsoft.com/v1.0/me/drive/root:/{}",
                    enc_path(inner)
                ),
                "PATCH",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::NewFolder { parent_inner, name } => {
            let url = if parent_inner.is_empty() {
                "https://graph.microsoft.com/v1.0/me/drive/root/children".to_string()
            } else {
                format!(
                    "https://graph.microsoft.com/v1.0/me/drive/root:/{}:/children",
                    enc_path(parent_inner)
                )
            };
            let body = format!(
                "{{\"name\":\"{}\",\"folder\":{{}},\
                 \"@microsoft.graph.conflictBehavior\":\"rename\"}}",
                json_escape(name)
            );
            oauth::http_send(
                &url,
                "POST",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
    }
}

/// Dropbox 쓰기 — 전부 POST + JSON(경로 기반이라 ID 불요).
/// 업로드는 150MB 이하 단순 `files/upload`, 초과는 세션 청크.
fn apply_write_dropbox(access: &str, op: &WriteOp) -> Result<(), String> {
    let post = |url: &str, body: String| -> Result<(), String> {
        oauth::http_send(
            url,
            "POST",
            Some(body.as_bytes()),
            Some("application/json"),
            "",
            Some(access),
        )
        .map(|_| ())
    };
    match op {
        WriteOp::Upload { src, dest_inner } => upload_dropbox(access, src, dest_inner),
        WriteOp::UploadTree { src, dest_inner } => {
            let _ = post(
                "https://api.dropboxapi.com/2/files/create_folder_v2",
                format!("{{\"path\":\"{}\"}}", json_escape(dest_inner)),
            ); // 이미 존재 = 무시(멱등)
            for ent in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
                let child = ent.path();
                let cname = ent.file_name().to_string_lossy().into_owned();
                let cdest = format!("{dest_inner}/{cname}");
                match ent.file_type() {
                    Ok(t) if t.is_dir() => {
                        apply_write_dropbox(access, &WriteOp::UploadTree { src: child, dest_inner: cdest })?
                    }
                    Ok(t) if t.is_file() => upload_dropbox(access, &child, &cdest)?,
                    _ => {}
                }
            }
            Ok(())
        }
        WriteOp::Delete { inner } => post(
            "https://api.dropboxapi.com/2/files/delete_v2",
            format!("{{\"path\":\"{}\"}}", json_escape(inner)),
        ),
        WriteOp::Rename { inner, new_name } => {
            let parent = inner.rfind('/').map(|i| &inner[..i]).unwrap_or("");
            post(
                "https://api.dropboxapi.com/2/files/move_v2",
                format!(
                    "{{\"from_path\":\"{}\",\"to_path\":\"{}/{}\"}}",
                    json_escape(inner),
                    json_escape(parent),
                    json_escape(new_name)
                ),
            )
        }
        WriteOp::NewFolder { parent_inner, name } => post(
            "https://api.dropboxapi.com/2/files/create_folder_v2",
            format!(
                "{{\"path\":\"{}/{}\",\"autorename\":true}}",
                json_escape(parent_inner),
                json_escape(name)
            ),
        ),
        WriteOp::CopyWithin { inner, dest_parent_inner } | WriteOp::MoveWithin { inner, dest_parent_inner } => {
            let name = inner.rsplit('/').next().unwrap_or("item");
            let url = if matches!(op, WriteOp::CopyWithin { .. }) {
                "https://api.dropboxapi.com/2/files/copy_v2"
            } else {
                "https://api.dropboxapi.com/2/files/move_v2"
            };
            post(
                url,
                format!(
                    "{{\"from_path\":\"{}\",\"to_path\":\"{}/{}\",\"autorename\":true}}",
                    json_escape(inner),
                    json_escape(dest_parent_inner),
                    json_escape(name)
                ),
            )
        }
    }
}

/// Dropbox 업로드 — 150MB 이하 단순, 초과는 세션(8MB 청크 append → finish).
fn upload_dropbox(access: &str, src: &std::path::Path, dest: &str) -> Result<(), String> {
    use std::io::Read;
    const SIMPLE_MAX: u64 = 140 * 1024 * 1024;
    const DBX_CHUNK: usize = 8 * 1024 * 1024;
    let total = std::fs::metadata(src).map_err(|e| e.to_string())?.len();
    let arg = |json: String| format!("Dropbox-API-Arg: {json}\r\n");
    if total <= SIMPLE_MAX {
        let bytes = std::fs::read(src).map_err(|e| e.to_string())?;
        return oauth::http_send(
            "https://content.dropboxapi.com/2/files/upload",
            "POST",
            Some(&bytes),
            Some("application/octet-stream"),
            &arg(format!(
                "{{\"path\":\"{}\",\"mode\":\"overwrite\"}}",
                json_escape(dest)
            )),
            Some(access),
        )
        .map(|_| ());
    }
    let mut f = std::fs::File::open(src).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; DBX_CHUNK];
    let mut off: u64 = 0;
    let mut session = String::new();
    while off < total {
        let want = DBX_CHUNK.min((total - off) as usize);
        f.read_exact(&mut buf[..want]).map_err(|e| e.to_string())?;
        if session.is_empty() {
            let r = oauth::http_send(
                "https://content.dropboxapi.com/2/files/upload_session/start",
                "POST",
                Some(&buf[..want]),
                Some("application/octet-stream"),
                &arg("{\"close\":false}".into()),
                Some(access),
            )?;
            session = oauth::json_str(&r, "session_id").ok_or("no session_id")?;
        } else {
            oauth::http_send(
                "https://content.dropboxapi.com/2/files/upload_session/append_v2",
                "POST",
                Some(&buf[..want]),
                Some("application/octet-stream"),
                &arg(format!(
                    "{{\"cursor\":{{\"session_id\":\"{session}\",\"offset\":{off}}}}}"
                )),
                Some(access),
            )?;
        }
        off += want as u64;
    }
    oauth::http_send(
        "https://content.dropboxapi.com/2/files/upload_session/finish",
        "POST",
        Some(&[]),
        Some("application/octet-stream"),
        &arg(format!(
            "{{\"cursor\":{{\"session_id\":\"{session}\",\"offset\":{total}}},\
              \"commit\":{{\"path\":\"{}\",\"mode\":\"overwrite\"}}}}",
            json_escape(dest)
        )),
        Some(access),
    )
    .map(|_| ())
}

/// Google Drive 쓰기 — ID 기반(경로 주소 지정 없음). 업로드는 multipart.
fn apply_write_google(idx: usize, access: &str, op: &WriteOp) -> Result<(), String> {
    // 부모/항목 ID — 루트는 예약어, 그 외는 목록 시점에 적재한 캐시에서 조회.
    let parent_id = |inner: &str| -> Result<String, String> {
        if inner.is_empty() {
            Ok("root".into())
        } else {
            id_get(idx, inner)
                .ok_or_else(|| format!("folder id unknown for {inner} — 상위 폴더를 새로 고치세요"))
        }
    };
    let item_id = |inner: &str| -> Result<String, String> {
        id_get(idx, inner)
            .ok_or_else(|| format!("item id unknown for {inner} — 목록을 새로 고치세요"))
    };
    match op {
        WriteOp::Upload { src, dest_inner } => {
            let (parent, name) = match dest_inner.rfind('/') {
                Some(i) => (&dest_inner[..i], &dest_inner[i + 1..]),
                None => ("", dest_inner.as_str()),
            };
            let pid = parent_id(parent)?;
            let bytes = std::fs::read(src).map_err(|e| e.to_string())?;
            // multipart/related — 메타데이터 파트 + 본문 파트
            let bound = "nexadirBOUNDARY7f3a";
            let meta = format!(
                "{{\"name\":\"{}\",\"parents\":[\"{pid}\"]}}",
                json_escape(name)
            );
            let mut body = Vec::new();
            body.extend_from_slice(
                format!(
                    "--{bound}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n\
                     --{bound}\r\nContent-Type: application/octet-stream\r\n\r\n"
                )
                .as_bytes(),
            );
            body.extend_from_slice(&bytes);
            body.extend_from_slice(format!("\r\n--{bound}--\r\n").as_bytes());
            oauth::http_send(
                "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart",
                "POST",
                Some(&body),
                Some(&format!("multipart/related; boundary={bound}")),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::NewFolder { parent_inner, name } => {
            let pid = parent_id(parent_inner)?;
            let body = format!(
                "{{\"name\":\"{}\",\"mimeType\":\"application/vnd.google-apps.folder\",\
                  \"parents\":[\"{pid}\"]}}",
                json_escape(name)
            );
            oauth::http_send(
                "https://www.googleapis.com/drive/v3/files",
                "POST",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::Delete { inner } => oauth::http_send(
            &format!("https://www.googleapis.com/drive/v3/files/{}", item_id(inner)?),
            "DELETE",
            None,
            None,
            "",
            Some(access),
        )
        .map(|_| ()),
        WriteOp::Rename { inner, new_name } => {
            let body = format!("{{\"name\":\"{}\"}}", json_escape(new_name));
            oauth::http_send(
                &format!("https://www.googleapis.com/drive/v3/files/{}", item_id(inner)?),
                "PATCH",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::CopyWithin {
            inner,
            dest_parent_inner,
        } => {
            let body = format!("{{\"parents\":[\"{}\"]}}", parent_id(dest_parent_inner)?);
            oauth::http_send(
                &format!(
                    "https://www.googleapis.com/drive/v3/files/{}/copy",
                    item_id(inner)?
                ),
                "POST",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::MoveWithin {
            inner,
            dest_parent_inner,
        } => {
            // 이동 = 부모 교체(addParents/removeParents 쿼리 — 본문 없음)
            let old_parent = inner.rfind('/').map(|i| &inner[..i]).unwrap_or("");
            oauth::http_send(
                &format!(
                    "https://www.googleapis.com/drive/v3/files/{}?addParents={}&removeParents={}",
                    item_id(inner)?,
                    oauth::percent(&parent_id(dest_parent_inner)?),
                    oauth::percent(&parent_id(old_parent)?)
                ),
                "PATCH",
                Some(b"{}"),
                Some("application/json"),
                "",
                Some(access),
            )
            .map(|_| ())
        }
        WriteOp::UploadTree { src, dest_inner } => {
            // 폴더 생성 후 하위를 재귀 — 생성 응답의 id를 캐시에 심어 자식이 부모를 찾는다
            let (parent, name) = match dest_inner.rfind('/') {
                Some(i) => (&dest_inner[..i], &dest_inner[i + 1..]),
                None => ("", dest_inner.as_str()),
            };
            let pid = parent_id(parent)?;
            let body = format!(
                "{{\"name\":\"{}\",\"mimeType\":\"application/vnd.google-apps.folder\",\
                  \"parents\":[\"{pid}\"]}}",
                json_escape(name)
            );
            let r = oauth::http_send(
                "https://www.googleapis.com/drive/v3/files",
                "POST",
                Some(body.as_bytes()),
                Some("application/json"),
                "",
                Some(access),
            )?;
            if let Some(new_id) = oauth::json_str(&r, "id") {
                id_put(idx, dest_inner, &new_id);
            }
            for ent in std::fs::read_dir(src).map_err(|e| e.to_string())?.flatten() {
                let child = ent.path();
                let cname = ent.file_name().to_string_lossy().into_owned();
                let cdest = format!("{dest_inner}/{cname}");
                match ent.file_type() {
                    Ok(t) if t.is_dir() => apply_write_google(
                        idx,
                        access,
                        &WriteOp::UploadTree {
                            src: child,
                            dest_inner: cdest,
                        },
                    )?,
                    Ok(t) if t.is_file() => apply_write_google(
                        idx,
                        access,
                        &WriteOp::Upload {
                            src: child,
                            dest_inner: cdest,
                        },
                    )?,
                    _ => {}
                }
            }
            Ok(())
        }
    }
}

/// JSON 문자열 값 이스케이프(최소 — crate 0).
fn json_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// 로컬 폴더 **재귀 업로드** — 하위 폴더를 만들고 전 파일을 올린다.
/// 폴더 생성은 `conflictBehavior=fail`로 두고 이미 있으면 무시(멱등).
fn upload_tree(access: &str, src: &std::path::Path, dest_inner: &str) -> Result<(), String> {
    // 대상 폴더 먼저 생성(이미 있으면 오류를 삼킨다)
    let (parent, name) = match dest_inner.rfind('/') {
        Some(i) => (&dest_inner[..i], &dest_inner[i + 1..]),
        None => ("", dest_inner),
    };
    let url = if parent.is_empty() {
        "https://graph.microsoft.com/v1.0/me/drive/root/children".to_string()
    } else {
        format!(
            "https://graph.microsoft.com/v1.0/me/drive/root:/{}:/children",
            enc_path(parent)
        )
    };
    let body = format!(
        "{{\"name\":\"{}\",\"folder\":{{}},\
         \"@microsoft.graph.conflictBehavior\":\"replace\"}}",
        json_escape(name)
    );
    let _ = oauth::http_send(
        &url,
        "POST",
        Some(body.as_bytes()),
        Some("application/json"),
        "",
        Some(access),
    ); // 이미 존재 = 무시(멱등)
    let rd = std::fs::read_dir(src).map_err(|e| e.to_string())?;
    for ent in rd.flatten() {
        let child = ent.path();
        let cname = ent.file_name().to_string_lossy().into_owned();
        let child_dest = format!("{dest_inner}/{cname}");
        match ent.file_type() {
            Ok(t) if t.is_dir() => upload_tree(access, &child, &child_dest)?,
            Ok(t) if t.is_file() => upload_onedrive(access, &child, &child_dest)?,
            _ => {} // 심볼릭 링크 등은 건너뜀
        }
    }
    Ok(())
}

/// 클라우드 폴더 **재귀 다운로드** 대상 전개 — (내부 경로, 로컬 대상) 목록으로 평탄화.
/// 목록 API를 폴더마다 호출하므로 워커에서만 쓴다.
fn expand_tree(
    access: &str,
    svc: &Service,
    idx: usize,
    inner: &str,
    dest_dir: &std::path::Path,
    out: &mut Vec<DownloadItem>,
) -> Result<(), String> {
    let body = fetch_list(svc, access, idx, inner)?;
    for e in parse_list(svc, &body, idx, inner) {
        let child_inner = format!("{inner}/{}", e.name);
        let child_dest = dest_dir.join(&e.name);
        if e.kind == FileKind::Dir {
            std::fs::create_dir_all(&child_dest).map_err(|x| x.to_string())?;
            expand_tree(access, svc, idx, &child_inner, &child_dest, out)?;
        } else {
            out.push(DownloadItem {
                inner: child_inner,
                dest: child_dest,
                is_dir: false, // 전개 결과는 항상 파일
            });
        }
    }
    Ok(())
}

/// OneDrive 업로드 — 작은 파일은 단순 PUT, 큰 파일은 **업로드 세션 청크 스트리밍**
/// (파일 전체를 메모리에 올리지 않는다).
fn upload_onedrive(access: &str, src: &std::path::Path, dest_inner: &str) -> Result<(), String> {
    use std::io::{Read, Seek, SeekFrom};
    let meta = std::fs::metadata(src).map_err(|e| e.to_string())?;
    let total = meta.len();
    let enc = enc_path(dest_inner);
    if total <= SIMPLE_PUT_MAX {
        let bytes = std::fs::read(src).map_err(|e| e.to_string())?;
        return oauth::http_send(
            &format!("https://graph.microsoft.com/v1.0/me/drive/root:/{enc}:/content"),
            "PUT",
            Some(&bytes),
            Some("application/octet-stream"),
            "",
            Some(access),
        )
        .map(|_| ());
    }
    // 업로드 세션 생성 → 청크 PUT(세션 URL은 사전 인증이라 Authorization 불요)
    let sess = oauth::http_send(
        &format!("https://graph.microsoft.com/v1.0/me/drive/root:/{enc}:/createUploadSession"),
        "POST",
        Some(b"{\"item\":{\"@microsoft.graph.conflictBehavior\":\"replace\"}}"),
        Some("application/json"),
        "",
        Some(access),
    )?;
    let url = oauth::json_str(&sess, "uploadUrl").ok_or("no uploadUrl")?;
    let mut f = std::fs::File::open(src).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; CHUNK];
    let mut off: u64 = 0;
    while off < total {
        let want = CHUNK.min((total - off) as usize);
        f.seek(SeekFrom::Start(off)).map_err(|e| e.to_string())?;
        f.read_exact(&mut buf[..want]).map_err(|e| e.to_string())?;
        let range = format!(
            "Content-Range: bytes {}-{}/{}\r\n",
            off,
            off + want as u64 - 1,
            total
        );
        oauth::http_send(
            &url,
            "PUT",
            Some(&buf[..want]),
            Some("application/octet-stream"),
            &range,
            None,
        )?;
        off += want as u64;
    }
    Ok(())
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

    /// Dropbox는 배열 키 `entries`·`.tag`로 폴더 판별·수정일 `server_modified`.
    #[test]
    fn parses_dropbox_entries() {
        let body = r#"{"entries":[
          {".tag":"folder","name":"Docs","id":"id:aaa","path_lower":"/docs"},
          {".tag":"file","name":"b.txt","id":"id:bbb","size":7,
           "server_modified":"2026-05-01T08:00:00Z","path_lower":"/b.txt"}
        ],"has_more":false}"#;
        let v = parse_list(&oauth::DROPBOX, body, 3, "");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].kind, FileKind::Dir);
        assert_eq!(v[0].target.as_deref(), Some("::CLOUD:3::/Docs"));
        assert_eq!(v[1].kind, FileKind::File);
        assert_eq!(v[1].size, 7);
        assert!(v[1].modified.is_some(), "server_modified 파싱");
    }

    /// JSON 이스케이프 — 경로에 따옴표·역슬래시가 있어도 본문이 깨지지 않아야 한다.
    #[test]
    fn json_escape_protects_body() {
        assert_eq!(json_escape(r#"a"b\c"#), r#"a\"b\\c"#);
        assert_eq!(json_escape("tab\there"), "tab\\there");
        assert!(!json_escape("새\u{1}폴더").contains('\u{1}'), "제어문자 이스케이프");
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
