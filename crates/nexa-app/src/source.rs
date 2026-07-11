//! nexa-tree → nexa-gui 배선(M1-3): 가시 노드 평면 스트림을 `RowSource`로 투영.
//! M1-4: 컬럼 셀 값(확장자·크기·수정한 날짜·종류)·헤더 정렬(`set_sort`) 배선 — 원본 docs/23.
//! 플랫폼 중립(비-Windows에서도 테스트) — 창/렌더와 무관한 순수 어댑터.

use nexa_core::FileKind;
use nexa_gui::widgets::{Marker, RowItem, RowSource};
use nexa_tree::{SortKey, SortSpec, Tree};

/// 컬럼 key(Column.key ↔ 이 상수). 0 = 트리 컬럼 관례(nexa-gui).
pub const COL_NAME: u32 = 0;
pub const COL_EXT: u32 = 1;
pub const COL_SIZE: u32 = 2;
pub const COL_MODIFIED: u32 = 3;
pub const COL_KIND: u32 = 4;

/// 트리 한 그루를 행 스트림으로 노출. 클릭 토글 = 펼침/접힘(캐럿·선택은 M1-5).
pub struct TreeSource {
    tree: Tree,
    /// 수정한 날짜 표시용 로컬 타임존 오프셋(분, UTC 기준 동쪽 양수).
    tz_offset_min: i32,
}

impl TreeSource {
    pub fn new(tree: Tree, tz_offset_min: i32) -> Self {
        TreeSource {
            tree,
            tz_offset_min,
        }
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

impl RowSource for TreeSource {
    fn len(&self) -> usize {
        self.tree.visible_len()
    }

    fn row(&self, index: usize) -> RowItem {
        match self.tree.row(index) {
            Some(r) => RowItem {
                text: r.name,
                depth: r.depth,
                marker: if r.has_children {
                    if r.expanded {
                        Marker::Expanded
                    } else {
                        Marker::Collapsed
                    }
                } else {
                    Marker::None
                },
            },
            // 페인트 중 행 수가 바뀌는 일은 없지만(단일 스레드) 방어적 빈 행
            None => RowItem {
                text: String::new(),
                depth: 0,
                marker: Marker::None,
            },
        }
    }

    fn cell(&self, index: usize, key: u32) -> String {
        let Some(r) = self.tree.row(index) else {
            return String::new();
        };
        let is_dir = r.kind == FileKind::Dir;
        match key {
            COL_EXT => {
                if is_dir {
                    String::new()
                } else {
                    ext_of(&r.name).to_string()
                }
            }
            COL_SIZE => {
                if is_dir {
                    String::new() // 폴더 크기는 OS 잡음값(코어 정렬도 0 정규화)
                } else {
                    human_size(r.size)
                }
            }
            COL_MODIFIED => fmt_datetime(r.modified_unix_ms, self.tz_offset_min),
            COL_KIND => match r.kind {
                FileKind::Dir => "폴더".to_string(),
                FileKind::Symlink => "바로가기".to_string(),
                FileKind::File => {
                    let ext = ext_of(&r.name);
                    if ext.is_empty() {
                        "파일".to_string()
                    } else {
                        format!("{} 파일", ext.to_uppercase())
                    }
                }
            },
            _ => String::new(),
        }
    }

    fn toggle(&mut self, index: usize) -> bool {
        let Some(id) = self.tree.visible_id(index) else {
            return false;
        };
        match self.tree.is_expanded(id) {
            Some(true) => self.tree.collapse(id).removed > 0,
            Some(false) => match self.tree.expand(id) {
                // 빈 폴더 펼침은 가시 변화 0이지만 마커(▸→▾)는 바뀜 — 다시 그린다
                Ok(_) => self.tree.is_expanded(id) == Some(true),
                Err(_) => false, // 접근 불가 폴더 — 조용히 무시(M3 watcher에서 UX 개선)
            },
            None => false,
        }
    }

    fn set_sort(&mut self, keys: &[(u32, bool)]) -> bool {
        let mapped: Vec<(SortKey, bool)> = keys
            .iter()
            .filter_map(|&(k, desc)| {
                let key = match k {
                    COL_NAME => SortKey::Name,
                    COL_EXT => SortKey::Ext,
                    COL_SIZE => SortKey::Size,
                    COL_MODIFIED => SortKey::Modified,
                    COL_KIND => SortKey::Kind,
                    _ => return None,
                };
                Some((key, desc))
            })
            .collect();
        // 빈 목록 = 열거 순서(폴더 우선은 탐색기 규약으로 유지 — 원본 docs/23 §4 "없음")
        self.tree.set_sort(SortSpec {
            keys: mapped,
            folders_first: true,
        });
        true
    }
}

/// 파일명의 확장자(마지막 `.` 뒤). 선행 `.`만 있는 dotfile은 확장자 없음("").
fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i + 1..],
        _ => "",
    }
}

/// 사람이 읽는 크기 — B/KB/MB/…, 100 미만은 소수 1자리.
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    const UNITS: [&str; 5] = ["KB", "MB", "GB", "TB", "PB"];
    let mut v = bytes as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if v >= 100.0 {
        format!("{v:.0} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Unix epoch 일수 → (년, 월, 일) — Howard Hinnant civil_from_days(공용 알고리즘).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Unix ms → "yyyy-MM-dd HH:mm"(오프셋 반영). 없음(-1 등 음수 관례값)이면 빈 값.
fn fmt_datetime(unix_ms: i64, tz_offset_min: i32) -> String {
    if unix_ms < 0 {
        return String::new();
    }
    let secs = unix_ms.div_euclid(1000) + i64::from(tz_offset_min) * 60;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}",
        rem / 3600,
        (rem % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    /// base/{dirA/{x.txt,y.txt}, empty/, file1.txt}
    fn fixture(tag: &str) -> PathBuf {
        let base =
            std::env::temp_dir().join(format!("nexa_app_src_{}_{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("dirA")).unwrap();
        fs::create_dir_all(base.join("empty")).unwrap();
        fs::write(base.join("dirA/x.txt"), b"x").unwrap();
        fs::write(base.join("dirA/y.txt"), b"y").unwrap();
        fs::write(base.join("file1.txt"), b"f").unwrap();
        base
    }

    #[test]
    fn rows_project_marker_and_depth() {
        let base = fixture("proj");
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        // 기본 정렬: [dirA, empty, file1.txt]
        assert_eq!(s.len(), 3);
        assert_eq!(s.row(0).marker, Marker::Collapsed);
        assert_eq!(s.row(2).marker, Marker::None);

        assert!(s.toggle(0)); // dirA 펼침 → x.txt·y.txt 삽입
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(s.len(), 5);
        assert_eq!(s.row(0).marker, Marker::Expanded);
        assert_eq!(s.row(1).text, "x.txt");
        assert_eq!(s.row(1).depth, 1);

        assert!(s.toggle(0)); // 접기
        assert_eq!(s.len(), 3);
    }

    #[test]
    fn toggle_file_is_noop_and_empty_dir_still_repaints() {
        let base = fixture("noop");
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        assert!(!s.toggle(2)); // file1.txt — 무변화
        assert!(s.toggle(1)); // empty 폴더 — 행 수 불변이지만 마커 갱신 필요 → true
        fs::remove_dir_all(&base).unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s.row(1).marker, Marker::Expanded);
        assert!(!s.toggle(99)); // 범위 밖
    }

    #[test]
    fn cells_project_ext_size_kind_and_dirs_are_blank() {
        let base = fixture("cells");
        let s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        // [dirA, empty, file1.txt]
        assert_eq!(s.cell(0, COL_SIZE), ""); // 폴더 크기 없음
        assert_eq!(s.cell(0, COL_KIND), "폴더");
        assert_eq!(s.cell(0, COL_EXT), "");
        assert_eq!(s.cell(2, COL_SIZE), "1 B");
        assert_eq!(s.cell(2, COL_EXT), "txt");
        assert_eq!(s.cell(2, COL_KIND), "TXT 파일");
        assert!(!s.cell(2, COL_MODIFIED).is_empty()); // 방금 만든 파일 — 날짜 표시
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn set_sort_reorders_visible_rows() {
        let base = fixture("sort");
        fs::write(base.join("big.bin"), vec![0u8; 2048]).unwrap();
        let mut s = TreeSource::new(Tree::open(&base).unwrap(), 0);
        // 기본: [dirA, empty, big.bin, file1.txt] (폴더 우선·이름 오름)
        assert!(s.set_sort(&[(COL_SIZE, true)])); // 크기 내림
        assert_eq!(s.row(2).text, "big.bin"); // 폴더 2개 뒤 가장 큰 파일
        assert!(s.set_sort(&[])); // 없음 = 열거 순서(폴더 우선 유지)
        assert_eq!(s.len(), 4);
        fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn human_size_units_and_precision() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.0 KB");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(150 * 1024), "150 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn fmt_datetime_applies_offset_and_epoch_math() {
        assert_eq!(fmt_datetime(0, 0), "1970-01-01 00:00");
        assert_eq!(fmt_datetime(0, 540), "1970-01-01 09:00"); // KST
                                                              // 2026-07-12 00:00:00 UTC = 20646일 × 86400s = 1_783_814_400_000 ms
        assert_eq!(fmt_datetime(1_783_814_400_000, 0), "2026-07-12 00:00");
        assert_eq!(fmt_datetime(-1, 540), ""); // 없음
    }
}
