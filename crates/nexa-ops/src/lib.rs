//! nexa-ops — 파일 조작 엔진(M3-1). **원본 이식**:
//! `app/Nexa.ViewModels/FileOps.cs`(순수 I/O — 청크 진행률·동일 볼륨 fast path·순번 명명) +
//! 원본 docs/33 TRANSFER-ENGINE(`TransferPathsInto` 단일 경로 — 같은 폴더 규칙·충돌 항목만
//! 순차 확인·바이트 진행률·취소·개별 격리).
//!
//! 플랫폼 중립(std 전용) — 전 플랫폼 테스트. 워커 스레드·PostMessage UI 배선은 nexa-app 책임.
//! 원본과의 차이: 취소·오류 시 **부분 복사 파일을 정리**한다(원본은 잔존 — 안전 개선, journal 기록).

pub mod batch_rename;
pub mod history;

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// 전송 연산(docs/33 — 진입점이 달라도 이 두 연산으로 수렴).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Copy,
    Move,
}

/// 이름 충돌 결정(원본: 충돌 항목만 순차 확인 — 예=덮어씀/아니오=건너뜀).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Conflict {
    Overwrite,
    Skip,
}

/// 전송 결과 — `transferred`의 (원본, 최종 대상) 쌍은 Undo(M3-3) 기록용.
#[derive(Default, Debug)]
pub struct Outcome {
    pub transferred: Vec<(PathBuf, PathBuf)>,
    pub skipped: Vec<PathBuf>,
    /// 개별 격리된 실패(항목, 사유) — 한 항목의 실패가 배치를 중단하지 않는다.
    pub errors: Vec<(PathBuf, String)>,
    pub canceled: bool,
}

/// 4MB 청크(원본 CopyBufferSize) — 바이트 진행 보고 단위.
const COPY_BUF: usize = 4 * 1024 * 1024;

/// 경로 끝 구분자를 제거한 잎(파일/폴더) 이름.
pub fn leaf_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 파일 또는 폴더 존재 여부(충돌 판정 공용 — 원본 Exists).
pub fn exists(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

/// 대소문자 무시 경로 동등(Windows 관례 — 원본 PathEquals).
fn path_equals(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches(['\\', '/']))
}

/// `child`가 `ancestor` 자신 또는 그 하위인가(폴더 자기/하위 이동 금지 — 원본 IsSameOrSubPath).
pub fn is_same_or_sub(ancestor: &Path, child: &Path) -> bool {
    let a = ancestor.to_string_lossy();
    let a = a.trim_end_matches(['\\', '/']);
    let c = child.to_string_lossy();
    let c = c.trim_end_matches(['\\', '/']);
    if a.eq_ignore_ascii_case(c) {
        return true;
    }
    let c_low = c.to_lowercase();
    let a_low = a.to_lowercase();
    c_low.starts_with(&format!("{a_low}\\")) || c_low.starts_with(&format!("{a_low}/"))
}

/// 두 경로가 같은 볼륨인가(원본 SameVolume — 루트 비교, 판단 실패 시 true 보수적).
/// Windows = 드라이브/UNC 프리픽스, 그 외 플랫폼 = 단일 루트로 간주.
pub fn same_volume(a: &Path, b: &Path) -> bool {
    fn root(p: &Path) -> Option<String> {
        match p.components().next() {
            Some(Component::Prefix(pr)) => Some(pr.as_os_str().to_string_lossy().to_lowercase()),
            Some(Component::RootDir) => Some("/".into()),
            _ => None, // 상대 경로 — 판단 불가
        }
    }
    match (root(a), root(b)) {
        (Some(ra), Some(rb)) => ra == rb,
        _ => true, // 보수적(원본 동일)
    }
}

/// `dest_dir` 안에서 `name`이 충돌하면 " (2)"…를 부여한 경로(원본 UniqueDest).
/// 폴더는 확장자 분리 안 함(예: "v1.2" 폴더), 파일은 확장자 유지·이름부에 순번.
pub fn unique_dest(dest_dir: &Path, name: &str, is_dir: bool) -> PathBuf {
    let natural = dest_dir.join(name);
    if !exists(&natural) {
        return natural;
    }
    let (stem, ext) = if is_dir {
        (name.to_string(), String::new())
    } else {
        let p = Path::new(name);
        match (p.file_stem(), p.extension()) {
            (Some(s), Some(e)) => (
                s.to_string_lossy().into_owned(),
                format!(".{}", e.to_string_lossy()),
            ),
            _ => (name.to_string(), String::new()),
        }
    };
    for i in 2.. {
        let cand = dest_dir.join(format!("{stem} ({i}){ext}"));
        if !exists(&cand) {
            return cand;
        }
    }
    unreachable!()
}

/// 파일/폴더 총 바이트(폴더 재귀 합계) — 접근 실패는 0으로 개별 격리(원본 SizeOf).
pub fn size_of(path: &Path) -> u64 {
    fn dir_sum(dir: &Path) -> u64 {
        let Ok(rd) = fs::read_dir(dir) else { return 0 };
        rd.flatten()
            .map(|e| {
                let p = e.path();
                match e.file_type() {
                    Ok(t) if t.is_dir() => dir_sum(&p),
                    Ok(t) if t.is_file() => e.metadata().map(|m| m.len()).unwrap_or(0),
                    _ => 0, // 심링크·실패 격리
                }
            })
            .sum()
    }
    match fs::metadata(path) {
        Ok(m) if m.is_dir() => dir_sum(path),
        Ok(m) if m.is_file() => m.len(),
        _ => 0,
    }
}

/// 취소 신호를 io::Error(Interrupted)로 변환 — 엔진이 취소로 판정.
fn check_cancel(cancel: &AtomicBool) -> io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(io::Error::new(io::ErrorKind::Interrupted, "canceled"))
    } else {
        Ok(())
    }
}

/// 파일을 청크로 복사하며 증분 바이트를 보고(원본 CopyFileWithProgress).
/// 취소/실패 시 부분 대상 파일을 제거한다(원본 대비 안전 개선).
pub fn copy_file_with_progress(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    let mut input = fs::File::open(src)?;
    let mut output = if overwrite {
        fs::File::create(dest)?
    } else {
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(dest)?
    };
    let mut buf = vec![0u8; COPY_BUF];
    let run = (|| -> io::Result<()> {
        loop {
            check_cancel(cancel)?;
            let n = input.read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            output.write_all(&buf[..n])?;
            on_bytes(n as u64);
        }
    })();
    if run.is_err() {
        drop(output);
        let _ = fs::remove_file(dest); // 부분 파일 정리
    }
    run
}

/// 폴더 재귀 복사. 열거 중 `Err` 엔트리는 **오류로 전파**한다(점검 2차 A14 — 종전 `flatten()`은
/// SMB 일시 오류·권한 등으로 빠진 항목을 조용히 버리고 Ok를 돌려줘, 교차 볼륨 이동이 원본을
/// 통째로 지우면 그 항목이 사본에도 원본에도 남지 않았다).
/// `copied`가 있으면 복사를 마친 원본 경로를 **후위 순서**(자식 → 부모)로 기록한다 — 교차 볼륨
/// 이동이 복사한 항목만 골라 지우는 데 쓴다.
fn copy_dir_with_progress(
    src: &Path,
    dest: &Path,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
    mut copied: Option<&mut Vec<PathBuf>>,
) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for e in fs::read_dir(src)? {
        check_cancel(cancel)?;
        let e = e?;
        let p = e.path();
        let d = dest.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_with_progress(&p, &d, on_bytes, cancel, copied.as_deref_mut())?;
        } else {
            // 원본 규약: 디렉터리 재귀 내부는 overwrite 복사
            copy_file_with_progress(&p, &d, true, on_bytes, cancel)?;
        }
        if let Some(c) = copied.as_deref_mut() {
            c.push(p);
        }
    }
    Ok(())
}

/// 교차 볼륨 이동의 원본 정리 — `copied`(후위 순서)에 적힌 항목만 지우고 마지막에 빈 `src`를
/// `remove_dir`한다(탐색기 방식). 열거에서 빠졌거나 복사 중 새로 생긴 항목은 남고, 그 때문에 폴더가
/// 비지 않으면 오류로 보고한다 — 사본은 완성돼 있으므로 데이터는 양쪽 어디에도 유실되지 않는다.
fn remove_copied(src: &Path, copied: &[PathBuf]) -> io::Result<()> {
    let mut first_err: Option<io::Error> = None;
    let mut note = |r: io::Result<()>| {
        if let Err(e) = r {
            first_err.get_or_insert(e);
        }
    };
    for p in copied {
        // 파일(심링크 포함) 우선, 실패하면 폴더(빈 폴더만 — 남은 항목이 있으면 그대로 둔다)
        let r = fs::remove_file(p).or_else(|e| {
            if p.is_dir() {
                remove_empty_dir(p)
            } else {
                Err(e)
            }
        });
        note(r);
    }
    note(remove_empty_dir(src));
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// 빈 폴더 삭제 — 비어 있지 않으면 사용자 메시지에 사유를 담아 반환.
fn remove_empty_dir(dir: &Path) -> io::Result<()> {
    fs::remove_dir(dir).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!(
                "원본 폴더를 비울 수 없어 남겨 둠(복사되지 않은 항목이 있을 수 있음): {}: {e}",
                leaf_name(dir)
            ),
        )
    })
}

/// `copy_onto_with_progress`의 본체 — `copied`는 교차 볼륨 이동 전용(복사한 원본 경로 기록).
fn copy_onto_inner(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
    copied: Option<&mut Vec<PathBuf>>,
) -> io::Result<()> {
    let replacing = overwrite && dest.exists();
    if src.is_dir() {
        if !replacing {
            return copy_dir_with_progress(src, dest, on_bytes, cancel, copied);
        }
        let staged = staging_path(dest, "tmp");
        let run = copy_dir_with_progress(src, &staged, on_bytes, cancel, copied)
            .and_then(|()| commit_replace(&staged, dest));
        if run.is_err() {
            let _ = fs::remove_dir_all(&staged);
        }
        run
    } else if replacing {
        let staged = staging_path(dest, "tmp");
        // copy_file_with_progress가 실패 시 staged를 정리한다
        copy_file_with_progress(src, &staged, true, on_bytes, cancel)?;
        commit_replace(&staged, dest).inspect_err(|_| {
            let _ = fs::remove_file(&staged);
        })
    } else {
        copy_file_with_progress(src, dest, overwrite, on_bytes, cancel)
    }
}

/// 교체용 스테이징 경로 — 대상과 **같은 부모**(같은 볼륨이라 rename이 원자적). 숨김 규약 `.<name>.nexa-<tag>-<pid>-<seq>`.
fn staging_path(dest: &Path, tag: &str) -> PathBuf {
    use std::sync::atomic::AtomicU64;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    dest.with_file_name(format!(
        ".{name}.nexa-{tag}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

/// 스테이징 → 대상 **커밋 교체**(점검 1차 #1 — 옛 대상은 새 내용이 완성된 뒤에만 사라진다).
/// 파일: `rename`(std = MOVEFILE_REPLACE_EXISTING, 원자) · 폴더: 옛 폴더를 옆으로 치우고 rename,
/// 실패 시 원복 · 대상이 파일인데 폴더로 교체(형 불일치)면 파일 제거 후 rename.
fn commit_replace(staged: &Path, dest: &Path) -> io::Result<()> {
    if dest.is_dir() {
        let old = staging_path(dest, "old");
        fs::rename(dest, &old)?;
        if let Err(e) = fs::rename(staged, dest) {
            let _ = fs::rename(&old, dest); // 원복 — 옛 대상 보존
            return Err(e);
        }
        let _ = fs::remove_dir_all(&old); // 새 대상은 이미 커밋됨 — 잔여는 정리 대상일 뿐
        return Ok(());
    }
    if staged.is_dir() && dest.is_file() {
        fs::remove_file(dest)?;
    }
    fs::rename(staged, dest)
}

/// 원본을 정확히 `dest`로 복사(순번 부여 없음). overwrite면 **스테이징 후 교체** — 취소/실패 시
/// 옛 대상이 그대로 남고 스테이징만 제거된다(점검 1차 #1: 종전은 폴더를 먼저 지워 취소 시 소실).
pub fn copy_onto_with_progress(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    copy_onto_inner(src, dest, overwrite, on_bytes, cancel, None)
}

/// 교차 볼륨 이동 = 복사 후 **복사한 항목만** 원본에서 제거(폴더는 `remove_copied`, 파일은 `remove_file`).
/// 종전은 `remove_dir_all(src)`로 통째로 지워 열거에서 빠진 항목·복사 중 생긴 항목이 유실됐다.
fn move_by_copy(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    if src.is_dir() {
        let mut copied = Vec::new();
        copy_onto_inner(src, dest, overwrite, on_bytes, cancel, Some(&mut copied))?; // 스테이징 교체
        remove_copied(src, &copied)
    } else {
        copy_onto_inner(src, dest, overwrite, on_bytes, cancel, None)?;
        fs::remove_file(src)
    }
}

/// 원본을 정확히 `dest`로 이동 — 같은 볼륨=rename(전체 크기 1회 보고), 다른 볼륨=복사 후 **복사한 항목만** 원본에서 삭제.
/// 자기 자신/하위로의 폴더 이동은 오류(원본 cycleMove).
pub fn move_onto_with_progress(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    let is_dir = src.is_dir();
    if is_dir && is_same_or_sub(src, dest) {
        return Err(io::Error::other("자기 자신/하위 폴더로는 이동할 수 없음"));
    }
    if same_volume(src, dest) {
        // 옛 대상은 새 대상이 자리를 잡은 뒤에만 사라진다(점검 1차 #1 — 종전은 선삭제 후 rename).
        if overwrite && dest.is_dir() {
            let old = staging_path(dest, "old");
            fs::rename(dest, &old)?;
            if let Err(e) = fs::rename(src, dest) {
                let _ = fs::rename(&old, dest); // 원복
                return Err(e);
            }
            let _ = fs::remove_dir_all(&old);
        } else if overwrite && is_dir && dest.is_file() {
            fs::remove_file(dest)?; // 형 불일치(파일 위에 폴더) — 새 내용은 원본에 온전히 있음
            fs::rename(src, dest)?;
        } else {
            fs::rename(src, dest)?; // 파일 위 파일 = std rename(REPLACE_EXISTING) 원자 교체
        }
        on_bytes(size_of(dest)); // 메타데이터 이동(즉시) — 전체 크기 1회 보고
        Ok(())
    } else {
        move_by_copy(src, dest, overwrite, on_bytes, cancel)
    }
}

/// 완전 삭제(휴지통 아님, 폴더 재귀) — 없으면 무동작(원본 DeletePermanent).
/// 휴지통 삭제는 셸 API가 필요해 앱 계층(win.rs) 담당.
pub fn delete_permanent(path: &Path) -> io::Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)
    } else if exists(path) {
        fs::remove_file(path)
    } else {
        Ok(())
    }
}

/// 제자리 이름변경(원본 B-6 인라인 리네임 — 같은 폴더 내 rename). 반환: 새 경로.
/// 규칙: 공백 트림·빈 이름/구분자 포함 = 오류·동일 이름 = 무동작·기존 이름과 충돌 = 오류.
pub fn rename(path: &Path, new_name: &str) -> io::Result<PathBuf> {
    let new_name = new_name.trim();
    if new_name.is_empty() || new_name.contains(['\\', '/']) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "잘못된 이름"));
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "루트는 이름변경 불가"))?;
    let dest = parent.join(new_name);
    if path_equals(path, &dest) {
        return Ok(path.to_path_buf()); // 동일 이름 = 무동작
    }
    if exists(&dest) {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "같은 이름이 이미 있음",
        ));
    }
    fs::rename(path, &dest)?;
    Ok(dest)
}

/// 새 폴더 생성 — 충돌 없는 이름("base"·"base (2)"…, 원본 UniqueChildPath). 반환: 생성 경로.
pub fn create_new_dir(dir: &Path, base: &str) -> io::Result<PathBuf> {
    let dest = unique_dest(dir, base, true);
    fs::create_dir(&dest)?;
    Ok(dest)
}

/// 새 빈 파일 생성 — `base_with_ext`(예: "새 파일.txt")로 충돌 없는 이름. 반환: 생성 경로.
pub fn create_new_file(dir: &Path, base_with_ext: &str) -> io::Result<PathBuf> {
    let dest = unique_dest(dir, base_with_ext, false);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&dest)?;
    Ok(dest)
}

/// 전송 진행 스냅샷 — UI 표시용.
#[derive(Clone, Copy, Debug)]
pub struct Progress {
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub item_index: usize,
    pub item_count: usize,
}

/// 항목 종결 상태(진행 창 세그먼트 바 — 07-21).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemStatus {
    Done,
    Skipped,
    Failed,
}

/// 전송 이벤트(07-21 — 세그먼트 진행 바·전송 중 대상 잠금을 위해 바이트 진행에서 확장).
#[derive(Debug)]
pub enum Event<'a> {
    /// 전송 시작 전 계획 — 항목별 크기(세그먼트 비율)·총 바이트. 정확히 1회.
    Plan { sizes: &'a [u64], total_bytes: u64 },
    /// 항목 쓰기 시작 — 실제 대상 경로(완료 전 열기/이동 차단용).
    ItemStart { index: usize, dest: &'a Path },
    /// 바이트 진행(4MB 청크 단위 — `done_bytes`는 전체 누적).
    Bytes(Progress),
    /// 항목 종결(성공/건너뜀/실패·취소).
    ItemEnd { index: usize, status: ItemStatus },
}

/// **전송 단일 경로**(원본 TransferPathsInto 이식) — 모든 복사/이동 진입점이 이 함수로 수렴.
///
/// 보장(docs/33): 같은 폴더 규칙(이동=무동작·복사=순번 복제) · 다른 폴더 충돌은 `resolve`로
/// **충돌 항목만 순차** 확인(Overwrite/Skip) · 이벤트 통지(`on_event` — 계획/항목/바이트) ·
/// 취소(`cancel`) · 항목 실패 개별 격리. 호출 측(워커 스레드)이 완료 후 재로드를 수행한다.
pub fn transfer(
    sources: &[PathBuf],
    dest_dir: &Path,
    op: Op,
    resolve: &mut dyn FnMut(&Path) -> Conflict,
    on_event: &mut dyn FnMut(Event),
    cancel: &AtomicBool,
) -> Outcome {
    let mut out = Outcome::default();
    let sizes: Vec<u64> = sources.iter().map(|p| size_of(p)).collect();
    let total_bytes: u64 = sizes.iter().sum();
    on_event(Event::Plan {
        sizes: &sizes,
        total_bytes,
    });
    let item_count = sources.len();
    let mut done = 0u64;
    for (i, src) in sources.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            out.canceled = true;
            break;
        }
        let is_dir = src.is_dir();
        let same_folder = src
            .parent()
            .is_some_and(|parent| path_equals(parent, dest_dir));

        let result: io::Result<Option<PathBuf>> = (|| {
            if same_folder {
                return match op {
                    Op::Move => Ok(None), // 제자리 이동 = 무동작(원본 규칙)
                    Op::Copy => {
                        // 같은 폴더 복사 = 순번 복제(" (2)"…)
                        let dest = unique_dest(dest_dir, &leaf_name(src), is_dir);
                        on_event(Event::ItemStart {
                            index: i,
                            dest: &dest,
                        });
                        copy_onto_with_progress(
                            src,
                            &dest,
                            false,
                            &mut |d| {
                                done += d;
                                on_event(Event::Bytes(Progress {
                                    done_bytes: done,
                                    total_bytes,
                                    item_index: i,
                                    item_count,
                                }));
                            },
                            cancel,
                        )?;
                        Ok(Some(dest))
                    }
                };
            }
            if is_dir && op == Op::Move && is_same_or_sub(src, dest_dir) {
                return Err(io::Error::other("자기 자신/하위 폴더로는 이동할 수 없음"));
            }
            let natural = dest_dir.join(leaf_name(src));
            let overwrite = if exists(&natural) {
                match resolve(&natural) {
                    Conflict::Skip => return Ok(None),
                    Conflict::Overwrite => true,
                }
            } else {
                false
            };
            on_event(Event::ItemStart {
                index: i,
                dest: &natural,
            });
            let mut bytes = |d: u64| {
                done += d;
                on_event(Event::Bytes(Progress {
                    done_bytes: done,
                    total_bytes,
                    item_index: i,
                    item_count,
                }));
            };
            match op {
                Op::Copy => copy_onto_with_progress(src, &natural, overwrite, &mut bytes, cancel)?,
                Op::Move => move_onto_with_progress(src, &natural, overwrite, &mut bytes, cancel)?,
            }
            Ok(Some(natural))
        })();

        let status = match &result {
            Ok(Some(_)) => ItemStatus::Done,
            Ok(None) => ItemStatus::Skipped,
            Err(_) => ItemStatus::Failed,
        };
        on_event(Event::ItemEnd { index: i, status });
        match result {
            Ok(Some(dest)) => out.transferred.push((src.clone(), dest)),
            Ok(None) => out.skipped.push(src.clone()),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {
                out.canceled = true;
                break;
            }
            Err(e) => out.errors.push((src.clone(), e.to_string())),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nexa_ops_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn no_conflict(_: &Path) -> Conflict {
        panic!("충돌이 없어야 함")
    }

    fn run(
        sources: &[PathBuf],
        dest: &Path,
        op: Op,
        resolve: &mut dyn FnMut(&Path) -> Conflict,
    ) -> Outcome {
        let cancel = AtomicBool::new(false);
        transfer(sources, dest, op, resolve, &mut |_| {}, &cancel)
    }

    #[test]
    fn unique_dest_numbering_file_and_dir() {
        let d = fixture("uniq");
        fs::write(d.join("a.txt"), "x").unwrap();
        assert_eq!(unique_dest(&d, "a.txt", false), d.join("a (2).txt"));
        fs::write(d.join("a (2).txt"), "x").unwrap();
        assert_eq!(unique_dest(&d, "a.txt", false), d.join("a (3).txt"));
        fs::create_dir(d.join("v1.2")).unwrap();
        assert_eq!(
            unique_dest(&d, "v1.2", true),
            d.join("v1.2 (2)"),
            "폴더는 확장자 분리 안 함"
        );
        assert_eq!(
            unique_dest(&d, "new.txt", false),
            d.join("new.txt"),
            "무충돌 = 자연 경로"
        );
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn same_folder_rules_move_noop_copy_duplicates() {
        let d = fixture("samefolder");
        fs::write(d.join("f.txt"), "내용").unwrap();
        let srcs = vec![d.join("f.txt")];

        let out = run(&srcs, &d, Op::Move, &mut no_conflict);
        assert!(
            out.transferred.is_empty() && out.errors.is_empty(),
            "제자리 이동 = 무동작"
        );
        assert!(d.join("f.txt").exists());

        let out = run(&srcs, &d, Op::Copy, &mut no_conflict);
        assert_eq!(
            out.transferred[0].1,
            d.join("f (2).txt"),
            "같은 폴더 복사 = 순번 복제"
        );
        assert_eq!(fs::read_to_string(d.join("f (2).txt")).unwrap(), "내용");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn cross_folder_copy_move_and_dir_recursive() {
        let d = fixture("cross");
        let (a, b) = (d.join("a"), d.join("b"));
        fs::create_dir_all(a.join("sub")).unwrap();
        fs::create_dir(&b).unwrap();
        fs::write(a.join("f.txt"), "1").unwrap();
        fs::write(a.join("sub/g.txt"), "22").unwrap();

        // 폴더 복사(재귀)
        let out = run(std::slice::from_ref(&a), &b, Op::Copy, &mut no_conflict);
        assert_eq!(out.transferred.len(), 1);
        assert_eq!(fs::read_to_string(b.join("a/sub/g.txt")).unwrap(), "22");

        // 파일 이동(동일 볼륨 fast path) — 원본 소멸
        let out = run(&[a.join("f.txt")], &b, Op::Move, &mut no_conflict);
        assert_eq!(out.transferred[0].1, b.join("f.txt"));
        assert!(!a.join("f.txt").exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn conflict_skip_and_overwrite_sequential() {
        let d = fixture("conflict");
        let (a, b) = (d.join("a"), d.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("f.txt"), "새값").unwrap();
        fs::write(a.join("g.txt"), "새값g").unwrap();
        fs::write(b.join("f.txt"), "옛값").unwrap();
        fs::write(b.join("g.txt"), "옛값g").unwrap();

        let mut asked = Vec::new();
        let out = run(
            &[a.join("f.txt"), a.join("g.txt")],
            &b,
            Op::Copy,
            &mut |p| {
                asked.push(leaf_name(p));
                if p.ends_with("f.txt") {
                    Conflict::Overwrite
                } else {
                    Conflict::Skip
                }
            },
        );
        assert_eq!(asked, vec!["f.txt", "g.txt"], "충돌 항목만 순차 확인");
        assert_eq!(
            fs::read_to_string(b.join("f.txt")).unwrap(),
            "새값",
            "덮어씀"
        );
        assert_eq!(
            fs::read_to_string(b.join("g.txt")).unwrap(),
            "옛값g",
            "건너뜀"
        );
        assert_eq!(out.transferred.len(), 1);
        assert_eq!(out.skipped, vec![a.join("g.txt")]);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn cycle_move_is_isolated_error() {
        let d = fixture("cycle");
        let outer = d.join("outer");
        fs::create_dir_all(outer.join("inner")).unwrap();
        fs::write(d.join("ok.txt"), "x").unwrap();
        let out = run(
            &[outer.clone(), d.join("ok.txt")],
            &outer.join("inner"),
            Op::Move,
            &mut no_conflict,
        );
        assert_eq!(out.errors.len(), 1, "순환 이동은 오류");
        assert_eq!(out.errors[0].0, outer);
        assert_eq!(out.transferred.len(), 1, "다른 항목은 개별 격리로 계속");
        assert!(outer.join("inner/ok.txt").exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn progress_reports_bytes_up_to_total() {
        let d = fixture("progress");
        let (a, b) = (d.join("a"), d.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("f.bin"), vec![7u8; 100_000]).unwrap();
        let cancel = AtomicBool::new(false);
        let mut last = None;
        let mut plan: Option<(Vec<u64>, u64)> = None;
        let mut starts: Vec<(usize, PathBuf)> = Vec::new();
        let mut ends: Vec<(usize, ItemStatus)> = Vec::new();
        let out = transfer(
            &[a.join("f.bin")],
            &b,
            Op::Copy,
            &mut no_conflict,
            &mut |ev| match ev {
                Event::Plan { sizes, total_bytes } => plan = Some((sizes.to_vec(), total_bytes)),
                Event::ItemStart { index, dest } => starts.push((index, dest.to_path_buf())),
                Event::Bytes(p) => last = Some(p),
                Event::ItemEnd { index, status } => ends.push((index, status)),
            },
            &cancel,
        );
        assert_eq!(out.transferred.len(), 1);
        let p = last.unwrap();
        assert_eq!(p.done_bytes, 100_000);
        assert_eq!(p.total_bytes, 100_000);
        assert_eq!((p.item_index, p.item_count), (0, 1));
        // 이벤트 프로토콜(07-21): 계획(항목 크기·총합) → 시작(대상 경로) → 종결(Done)
        assert_eq!(plan, Some((vec![100_000], 100_000)), "Plan 1회·크기 정확");
        assert_eq!(starts, vec![(0, b.join("f.bin"))], "ItemStart = 실제 대상");
        assert_eq!(ends, vec![(0, ItemStatus::Done)]);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn item_events_report_skip_per_item() {
        let d = fixture("itemevents");
        let (a, b) = (d.join("a"), d.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("f.txt"), "새값").unwrap();
        fs::write(b.join("f.txt"), "옛값").unwrap();
        fs::write(a.join("g.txt"), "g").unwrap();
        let cancel = AtomicBool::new(false);
        let mut ends = Vec::new();
        let out = transfer(
            &[a.join("f.txt"), a.join("g.txt")],
            &b,
            Op::Copy,
            &mut |_| Conflict::Skip,
            &mut |ev| {
                if let Event::ItemEnd { index, status } = ev {
                    ends.push((index, status));
                }
            },
            &cancel,
        );
        assert_eq!(out.skipped.len(), 1);
        assert_eq!(
            ends,
            vec![(0, ItemStatus::Skipped), (1, ItemStatus::Done)],
            "건너뜀/성공이 항목별로 보고"
        );
        fs::remove_dir_all(&d).unwrap();
    }

    /// 점검 1차 #1: 덮어쓰기 중 취소해도 **옛 대상이 보존**되고 스테이징 잔여가 없어야 한다.
    #[test]
    fn overwrite_keeps_old_dest_when_canceled_and_replaces_on_success() {
        let d = fixture("overwrite_stage");
        let (a, b) = (d.join("a"), d.join("b"));
        fs::create_dir_all(a.join("x")).unwrap();
        fs::create_dir_all(b.join("x")).unwrap();
        fs::write(a.join("x").join("big.bin"), vec![1u8; 9 * 1024 * 1024]).unwrap();
        fs::write(b.join("x").join("keep.txt"), "옛 폴더 내용").unwrap();
        fs::write(a.join("f.bin"), vec![2u8; 9 * 1024 * 1024]).unwrap();
        fs::write(b.join("f.bin"), "옛 파일").unwrap();
        let leftovers = |dir: &Path| {
            fs::read_dir(dir)
                .unwrap()
                .flatten()
                .filter(|e| e.file_name().to_string_lossy().contains(".nexa-"))
                .count()
        };
        // ① 폴더 덮어쓰기 중 취소 → 옛 폴더 그대로
        let cancel = AtomicBool::new(false);
        let out = transfer(
            &[a.join("x")],
            &b,
            Op::Copy,
            &mut |_| Conflict::Overwrite,
            &mut |ev| {
                if matches!(ev, Event::Bytes(_)) {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &cancel,
        );
        assert!(out.canceled);
        assert_eq!(
            fs::read_to_string(b.join("x").join("keep.txt")).unwrap(),
            "옛 폴더 내용"
        );
        assert!(!b.join("x").join("big.bin").exists());
        assert_eq!(leftovers(&b), 0, "스테이징 잔여 없음");
        // ② 파일 덮어쓰기 중 취소 → 옛 파일 그대로
        let cancel = AtomicBool::new(false);
        let out = transfer(
            &[a.join("f.bin")],
            &b,
            Op::Copy,
            &mut |_| Conflict::Overwrite,
            &mut |ev| {
                if matches!(ev, Event::Bytes(_)) {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &cancel,
        );
        assert!(out.canceled);
        assert_eq!(fs::read_to_string(b.join("f.bin")).unwrap(), "옛 파일");
        assert_eq!(leftovers(&b), 0);
        // ③ 성공 시 교체 완료(폴더·파일) — 옛 내용 소거·잔여 없음
        fs::write(a.join("x").join("big.bin"), b"new").unwrap();
        fs::write(a.join("f.bin"), b"newf").unwrap();
        let out = run(&[a.join("x"), a.join("f.bin")], &b, Op::Copy, &mut |_| {
            Conflict::Overwrite
        });
        assert_eq!(out.transferred.len(), 2, "{:?}", out.errors);
        assert!(
            !b.join("x").join("keep.txt").exists(),
            "옛 폴더 내용 교체됨"
        );
        assert_eq!(fs::read(b.join("x").join("big.bin")).unwrap(), b"new");
        assert_eq!(fs::read(b.join("f.bin")).unwrap(), b"newf");
        assert_eq!(leftovers(&b), 0);
        // ④ 같은 볼륨 이동 덮어쓰기(폴더) — 옛 폴더 옆으로 치운 뒤 rename·정리
        fs::create_dir_all(a.join("y")).unwrap();
        fs::write(a.join("y").join("n.txt"), "새").unwrap();
        fs::create_dir_all(b.join("y")).unwrap();
        fs::write(b.join("y").join("o.txt"), "옛").unwrap();
        let out = run(&[a.join("y")], &b, Op::Move, &mut |_| Conflict::Overwrite);
        assert_eq!(out.transferred.len(), 1, "{:?}", out.errors);
        assert!(b.join("y").join("n.txt").exists() && !b.join("y").join("o.txt").exists());
        assert!(!a.join("y").exists());
        assert_eq!(leftovers(&b), 0);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn cancel_mid_copy_cleans_partial_and_reports() {
        let d = fixture("cancel");
        let (a, b) = (d.join("a"), d.join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        // 4MB 청크 2개 이상이 되도록 9MB — 첫 청크 보고에서 취소
        fs::write(a.join("big.bin"), vec![1u8; 9 * 1024 * 1024]).unwrap();
        let cancel = AtomicBool::new(false);
        let out = transfer(
            &[a.join("big.bin")],
            &b,
            Op::Copy,
            &mut no_conflict,
            &mut |ev| {
                if matches!(ev, Event::Bytes(_)) {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
            &cancel,
        );
        assert!(out.canceled);
        assert!(out.transferred.is_empty());
        assert!(!b.join("big.bin").exists(), "부분 파일 정리");
        assert!(a.join("big.bin").exists(), "원본 무손상");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rename_rules() {
        let d = fixture("rename");
        fs::write(d.join("a.txt"), "x").unwrap();
        fs::write(d.join("b.txt"), "y").unwrap();
        assert_eq!(rename(&d.join("a.txt"), "c.txt").unwrap(), d.join("c.txt"));
        assert!(!d.join("a.txt").exists() && d.join("c.txt").exists());
        assert_eq!(
            rename(&d.join("c.txt"), "c.txt").unwrap(),
            d.join("c.txt"),
            "동일 이름 = 무동작"
        );
        assert_eq!(
            rename(&d.join("c.txt"), "b.txt").unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(rename(&d.join("c.txt"), "  ").is_err(), "빈 이름");
        assert!(rename(&d.join("c.txt"), "x/y").is_err(), "구분자 금지");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn create_new_numbering_and_delete_permanent() {
        let d = fixture("createnew");
        assert_eq!(create_new_dir(&d, "새 폴더").unwrap(), d.join("새 폴더"));
        assert_eq!(
            create_new_dir(&d, "새 폴더").unwrap(),
            d.join("새 폴더 (2)")
        );
        assert_eq!(
            create_new_file(&d, "새 파일.txt").unwrap(),
            d.join("새 파일.txt")
        );
        assert_eq!(
            create_new_file(&d, "새 파일.txt").unwrap(),
            d.join("새 파일 (2).txt"),
            "확장자 앞 순번(원본 규약)"
        );
        fs::write(d.join("새 폴더/x.txt"), "z").unwrap();
        delete_permanent(&d.join("새 폴더")).unwrap(); // 폴더 재귀
        assert!(!d.join("새 폴더").exists());
        delete_permanent(&d.join("없는 경로")).unwrap(); // 무동작
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn size_of_recursive_and_same_volume() {
        let d = fixture("size");
        fs::create_dir_all(d.join("s/t")).unwrap();
        fs::write(d.join("s/a.bin"), vec![0u8; 10]).unwrap();
        fs::write(d.join("s/t/b.bin"), vec![0u8; 32]).unwrap();
        assert_eq!(size_of(&d.join("s")), 42);
        assert_eq!(size_of(&d.join("없음")), 0, "실패 격리 = 0");
        assert!(same_volume(&d, &d.join("s")), "같은 루트");
        fs::remove_dir_all(&d).unwrap();
    }
    /// A14(점검 2차 G8 누락1): 교차 볼륨 폴더 이동은 **복사한 항목만** 지운다 — 열거에서 빠진 항목이
    /// 원본에 남고 폴더는 비지 않아 오류로 보고된다(종전 `remove_dir_all`은 통째로 삭제 = 유실).
    #[test]
    fn move_by_copy_removes_only_copied_items_and_keeps_strays() {
        let d = fixture("a14_strays");
        let (src, dest) = (d.join("src"), d.join("dest"));
        fs::create_dir_all(src.join("sub/deep")).unwrap();
        fs::write(src.join("a.txt"), "a").unwrap();
        fs::write(src.join("sub/b.txt"), "b").unwrap();
        fs::write(src.join("sub/deep/c.txt"), "c").unwrap();

        // 복사 기록 = 후위 순서(자식 → 부모)
        let mut copied = Vec::new();
        let cancel = AtomicBool::new(false);
        copy_dir_with_progress(&src, &dest, &mut |_| {}, &cancel, Some(&mut copied)).unwrap();
        assert_eq!(copied.len(), 5, "파일 3 + 폴더 2");
        let pos = |p: &Path| copied.iter().position(|c| c == p).unwrap();
        assert!(pos(&src.join("sub/deep/c.txt")) < pos(&src.join("sub/deep")));
        assert!(pos(&src.join("sub/deep")) < pos(&src.join("sub")));
        assert!(pos(&src.join("sub/b.txt")) < pos(&src.join("sub")));

        // 열거에서 빠진(=기록에 없는) 항목이 있는 상태로 원본 정리
        fs::write(src.join("sub/stray.txt"), "열거 오류로 빠진 항목").unwrap();
        let r = remove_copied(&src, &copied);
        assert!(r.is_err(), "폴더가 비지 않으면 오류 보고: {r:?}");
        assert!(!src.join("a.txt").exists());
        assert!(!src.join("sub/b.txt").exists());
        assert!(!src.join("sub/deep").exists(), "비워진 하위 폴더는 삭제");
        assert_eq!(
            fs::read_to_string(src.join("sub/stray.txt")).unwrap(),
            "열거 오류로 빠진 항목",
            "복사되지 않은 항목은 원본에 잔존"
        );
        assert!(src.is_dir(), "원본 폴더 잔존");
        assert_eq!(
            fs::read_to_string(dest.join("sub/deep/c.txt")).unwrap(),
            "c"
        );
        fs::remove_dir_all(&d).unwrap();
    }

    /// 정상 이동 결과 불변: 전부 복사되면 원본은 폴더까지 사라지고 사본이 온전하다.
    #[test]
    fn move_by_copy_full_success_removes_source_tree() {
        let d = fixture("a14_ok");
        let (src, dest) = (d.join("src"), d.join("dest"));
        fs::create_dir_all(src.join("sub/deep")).unwrap();
        fs::write(src.join("a.txt"), "a").unwrap();
        fs::write(src.join("sub/deep/c.txt"), "c").unwrap();
        let mut bytes = 0u64;
        move_by_copy(
            &src,
            &dest,
            false,
            &mut |n| bytes += n,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(bytes, 2);
        assert!(!src.exists(), "원본 소멸");
        assert_eq!(fs::read_to_string(dest.join("a.txt")).unwrap(), "a");
        assert_eq!(
            fs::read_to_string(dest.join("sub/deep/c.txt")).unwrap(),
            "c"
        );

        // 덮어쓰기(스테이징 교체) 경로도 동일
        let src2 = d.join("src2");
        fs::create_dir_all(&src2).unwrap();
        fs::write(src2.join("new.txt"), "new").unwrap();
        move_by_copy(&src2, &dest, true, &mut |_| {}, &AtomicBool::new(false)).unwrap();
        assert!(!src2.exists());
        assert_eq!(fs::read_to_string(dest.join("new.txt")).unwrap(), "new");
        assert!(!dest.join("a.txt").exists(), "옛 대상은 교체됨");
        fs::remove_dir_all(&d).unwrap();
    }

    /// 열리지 않는 파일(다른 프로세스 독점 열기 = 공유 위반)이 든 폴더 이동 → 오류, 원본 전부 잔존.
    #[cfg(windows)]
    #[test]
    fn move_by_copy_locked_file_keeps_source_intact() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = fixture("a14_locked");
        let (src, dest) = (d.join("src"), d.join("dest"));
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("a.txt"), "a").unwrap();
        fs::write(src.join("sub/locked.txt"), "잠김").unwrap();
        let _hold = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(src.join("sub/locked.txt"))
            .unwrap();
        let r = move_by_copy(&src, &dest, false, &mut |_| {}, &AtomicBool::new(false));
        assert!(r.is_err(), "{r:?}");
        assert_eq!(fs::read_to_string(src.join("a.txt")).unwrap(), "a");
        drop(_hold); // 독점 열기 해제 후 내용 확인(share_mode(0)은 자기 자신의 재열기도 거부)
        assert_eq!(
            fs::read_to_string(src.join("sub/locked.txt")).unwrap(),
            "잠김"
        );
        fs::remove_dir_all(&d).unwrap();
    }

    /// 실제 교차 볼륨(임시 폴더 ≠ target 볼륨일 때만 — 로컬 C:/D:·Windows CI) 재현: 복사 중 원본에
    /// 생긴 파일은 열거에서 빠질 수 있다. 종전엔 `remove_dir_all`로 함께 사라졌고(10-02 재현 캡처),
    /// 이제는 원본 또는 사본 어느 한쪽에 반드시 남는다.
    #[test]
    fn cross_volume_move_never_loses_item_missed_by_enumeration() {
        let src_root = fixture("a14_xvol");
        let dst_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/nexa_ops_a14_xvol");
        if same_volume(&src_root, &dst_root) {
            fs::remove_dir_all(&src_root).unwrap();
            return; // 둘째 볼륨 없음 — 건너뜀
        }
        let _ = fs::remove_dir_all(&dst_root);
        fs::create_dir_all(&dst_root).unwrap();
        let src = src_root.join("folder");
        fs::create_dir_all(src.join("sub")).unwrap();
        fs::write(src.join("sub/big.bin"), vec![7u8; 9 * 1024 * 1024]).unwrap();
        let late = src.join("sub/0late.txt"); // NTFS 이름순 열거에서 big.bin 뒤에 재개되면 빠진다
        let dest = dst_root.join("folder");
        let mut fired = false;
        let r = move_onto_with_progress(
            &src,
            &dest,
            false,
            &mut |_| {
                if !fired {
                    fired = true;
                    fs::write(&late, "late").unwrap();
                }
            },
            &AtomicBool::new(false),
        );
        assert!(fired);
        assert!(dest.join("sub/big.bin").exists(), "사본 완성");
        let in_src = late.exists();
        let in_dest = dest.join("sub/0late.txt").exists();
        assert!(
            in_src || in_dest,
            "늦게 생긴 항목이 양쪽 모두에서 사라짐(유실) — {r:?}"
        );
        if in_src {
            assert!(r.is_err(), "원본에 남은 항목이 있으면 오류 보고");
            assert!(
                !src.join("sub/big.bin").exists(),
                "복사한 항목은 원본에서 제거"
            );
        } else {
            assert!(r.is_ok(), "{r:?}");
            assert!(!src.exists());
        }
        let _ = fs::remove_dir_all(&src_root);
        let _ = fs::remove_dir_all(&dst_root);
    }
}
