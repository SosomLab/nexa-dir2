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

fn copy_dir_with_progress(
    src: &Path,
    dest: &Path,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for e in fs::read_dir(src)?.flatten() {
        check_cancel(cancel)?;
        let p = e.path();
        let d = dest.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_with_progress(&p, &d, on_bytes, cancel)?;
        } else {
            // 원본 규약: 디렉터리 재귀 내부는 overwrite 복사
            copy_file_with_progress(&p, &d, true, on_bytes, cancel)?;
        }
    }
    Ok(())
}

/// 원본을 정확히 `dest`로 복사(순번 부여 없음) — overwrite면 기존 대상 대체(폴더는 삭제 후 재귀).
pub fn copy_onto_with_progress(
    src: &Path,
    dest: &Path,
    overwrite: bool,
    on_bytes: &mut dyn FnMut(u64),
    cancel: &AtomicBool,
) -> io::Result<()> {
    if src.is_dir() {
        if overwrite && dest.is_dir() {
            fs::remove_dir_all(dest)?;
        }
        copy_dir_with_progress(src, dest, on_bytes, cancel)
    } else {
        copy_file_with_progress(src, dest, overwrite, on_bytes, cancel)
    }
}

/// 원본을 정확히 `dest`로 이동 — 같은 볼륨=rename(전체 크기 1회 보고), 다른 볼륨=복사 후 원본 삭제.
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
    if overwrite {
        if dest.is_dir() {
            fs::remove_dir_all(dest)?;
        } else if dest.is_file() {
            fs::remove_file(dest)?;
        }
    }
    if same_volume(src, dest) {
        fs::rename(src, dest)?;
        on_bytes(size_of(dest)); // 메타데이터 이동(즉시) — 전체 크기 1회 보고
        Ok(())
    } else {
        copy_onto_with_progress(src, dest, true, on_bytes, cancel)?;
        if is_dir {
            fs::remove_dir_all(src)
        } else {
            fs::remove_file(src)
        }
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
}
