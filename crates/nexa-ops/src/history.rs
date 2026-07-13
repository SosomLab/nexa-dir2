//! 파일 작업 undo/redo 히스토리(M3-3). **원본 이식**: `app/Nexa.ViewModels/OperationHistory.cs`
//! (B-13u — docs/33 §Undo/Redo) + `OperationHistoryTests.cs`.
//!
//! 스택 2개(undo/redo)·새 작업 push 시 redo 무효화·세션 한정(영속 X). 연산 실행 중 오류가
//! 나면 그 연산은 스택에서 제거된 채 전파된다(부분 실패 상태의 재실행은 더 위험 —
//! 호출자는 상태바 알림, docs/33 "무결성 우선"). 오류 문구는 i18n이 앱 책임이므로
//! [`OpError`]로 구조화해 반환한다(원본은 Localizer 직접 호출).
//!
//! 사본/생성물 삭제 수단은 주입(앱=휴지통·테스트=완전삭제 — 원본 CopyBatchOp 동일).
//! 휴지통 삭제 배치(DeleteBatchOp)는 셸 COM 의존이라 앱 계층 구현(원본 RecycleBin.cs 동일 배치).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::{copy_onto_with_progress, exists, leaf_name, move_onto_with_progress};

/// 연산 실패의 구조화 사유 — 앱이 i18n 키로 변환(history.failedItems 등).
#[derive(Debug, PartialEq, Eq)]
pub enum OpError {
    /// 배치 중 n건 실패(소실·충돌·I/O — 나머지는 최선 수행 후 집계, 원본 IOException 집계).
    Failed(usize),
    /// 원본 소실(외부 변경) — 잎 이름.
    MissingSource(String),
    /// 대상 이름 충돌 — 잎 이름.
    NameExists(String),
}

/// 되돌릴 수 있는 파일 작업 1건(배치=1 트랜잭션) — 원본 IReversibleOp.
pub trait ReversibleOp {
    /// 상태바 표기용 설명(예: "이동 3개", "이름 변경: a → b") — push 시점에 i18n 확정.
    fn description(&self) -> &str;
    /// 작업을 되돌린다. 일부 항목 실패 시 나머지는 최선 수행 후 집계 오류.
    fn undo(&mut self) -> Result<(), OpError>;
    /// 되돌린 작업을 다시 수행한다.
    fn redo(&mut self) -> Result<(), OpError>;
}

/// 파일 작업 undo/redo 히스토리(탐색기 Ctrl+Z/Y) — 원본 OperationHistory.
pub struct OperationHistory {
    undo: Vec<Box<dyn ReversibleOp>>,
    redo: Vec<Box<dyn ReversibleOp>>,
    capacity: usize,
}

impl Default for OperationHistory {
    fn default() -> Self {
        Self::with_capacity(100) // 원본 기본 상한
    }
}

impl OperationHistory {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            capacity: capacity.max(1),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_description(&self) -> Option<&str> {
        self.undo.last().map(|op| op.description())
    }

    pub fn redo_description(&self) -> Option<&str> {
        self.redo.last().map(|op| op.description())
    }

    /// 완료된 작업 기록 — redo 스택은 비워진다(표준 undo 모델). 상한 초과 시 가장 오래된 것 제거.
    pub fn push(&mut self, op: Box<dyn ReversibleOp>) {
        self.undo.push(op);
        if self.undo.len() > self.capacity {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// 마지막 작업 되돌리기. 없으면 `None`. 오류 시 해당 연산은 양쪽 스택에서 제외된 채
    /// 전파(재시도 불가 — 호출자 알림). 설명은 호출 전 [`Self::undo_description`]으로 확보.
    pub fn undo(&mut self) -> Option<Result<(), OpError>> {
        let mut op = self.undo.pop()?;
        match op.undo() {
            Ok(()) => {
                self.redo.push(op);
                Some(Ok(()))
            }
            Err(e) => Some(Err(e)), // op 소실(무결성 우선)
        }
    }

    /// 마지막 되돌리기를 재수행. 없으면 `None`. 오류 규약은 [`Self::undo`]와 동일.
    pub fn redo(&mut self) -> Option<Result<(), OpError>> {
        let mut op = self.redo.pop()?;
        match op.redo() {
            Ok(()) => {
                self.undo.push(op);
                Some(Ok(()))
            }
            Err(e) => Some(Err(e)),
        }
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

/// 진행·취소 없는 단순 이동(히스토리 전용) — 다른 볼륨은 복사 후 삭제(원본 FileOps.MoveOnto).
fn move_plain(from: &Path, to: &Path) -> io::Result<()> {
    let cancel = AtomicBool::new(false);
    move_onto_with_progress(from, to, false, &mut |_| {}, &cancel)
}

fn copy_plain(from: &Path, to: &Path) -> io::Result<()> {
    let cancel = AtomicBool::new(false);
    copy_onto_with_progress(from, to, false, &mut |_| {}, &cancel)
}

/// 이동 배치 — undo: dest→src 역이동 / redo: src→dest 재이동. 소실·충돌 항목은 건너뛰고 집계 오류.
pub struct MoveBatchOp {
    pairs: Vec<(PathBuf, PathBuf)>,
    description: String,
}

impl MoveBatchOp {
    pub fn new(pairs: Vec<(PathBuf, PathBuf)>, description: String) -> Self {
        Self { pairs, description }
    }

    fn move_all(&self, reverse: bool) -> Result<(), OpError> {
        let mut failed = 0usize;
        for (src, dest) in &self.pairs {
            let (from, to) = if reverse { (dest, src) } else { (src, dest) };
            if !exists(from) || exists(to) {
                failed += 1; // 원본 소실(외부 변경) 또는 대상 충돌 → 건너뜀(무결성 — 덮어쓰지 않음)
                continue;
            }
            if move_plain(from, to).is_err() {
                failed += 1;
            }
        }
        if failed > 0 {
            Err(OpError::Failed(failed))
        } else {
            Ok(())
        }
    }
}

impl ReversibleOp for MoveBatchOp {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&mut self) -> Result<(), OpError> {
        self.move_all(true)
    }

    fn redo(&mut self) -> Result<(), OpError> {
        self.move_all(false)
    }
}

/// 사본/생성물 제거 방법(앱=휴지통 · 테스트=완전삭제) — Windows 전용 API 격리(원본 Action<string>).
pub type DeleteFn = Box<dyn FnMut(&Path) -> io::Result<()>>;

/// 복사 배치 — undo: 사본 삭제(주입) / redo: 재복사. 소실·충돌은 건너뛰고 집계 오류.
pub struct CopyBatchOp {
    pairs: Vec<(PathBuf, PathBuf)>,
    description: String,
    delete_copy: DeleteFn,
}

impl CopyBatchOp {
    pub fn new(pairs: Vec<(PathBuf, PathBuf)>, description: String, delete_copy: DeleteFn) -> Self {
        Self {
            pairs,
            description,
            delete_copy,
        }
    }
}

impl ReversibleOp for CopyBatchOp {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&mut self) -> Result<(), OpError> {
        let mut failed = 0usize;
        for (_, dest) in &self.pairs {
            if exists(dest) && (self.delete_copy)(dest).is_err() {
                failed += 1;
            }
        }
        if failed > 0 {
            Err(OpError::Failed(failed))
        } else {
            Ok(())
        }
    }

    fn redo(&mut self) -> Result<(), OpError> {
        let mut failed = 0usize;
        for (src, dest) in &self.pairs {
            if !exists(src) || exists(dest) {
                failed += 1; // 원본 소실/대상 충돌 → 건너뜀
                continue;
            }
            if copy_plain(src, dest).is_err() {
                failed += 1;
            }
        }
        if failed > 0 {
            Err(OpError::Failed(failed))
        } else {
            Ok(())
        }
    }
}

/// 이름 변경 — undo: new→old / redo: old→new. 소실·충돌은 오류.
pub struct RenameOp {
    old_path: PathBuf,
    new_path: PathBuf,
    description: String,
}

impl RenameOp {
    pub fn new(old_path: PathBuf, new_path: PathBuf, description: String) -> Self {
        Self {
            old_path,
            new_path,
            description,
        }
    }

    fn rename(from: &Path, to: &Path) -> Result<(), OpError> {
        if !exists(from) {
            return Err(OpError::MissingSource(leaf_name(from)));
        }
        if exists(to) {
            return Err(OpError::NameExists(leaf_name(to)));
        }
        move_plain(from, to).map_err(|_| OpError::Failed(1))
    }
}

impl ReversibleOp for RenameOp {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&mut self) -> Result<(), OpError> {
        Self::rename(&self.new_path, &self.old_path)
    }

    fn redo(&mut self) -> Result<(), OpError> {
        Self::rename(&self.old_path, &self.new_path)
    }
}

/// 재생성 방법(폴더/빈 파일 등) — 원본 CreateOp의 recreate 델리게이트.
pub type RecreateFn = Box<dyn FnMut() -> io::Result<()>>;

/// 새로 만들기(폴더/파일) — undo: 생성물 삭제(주입 — 앱=휴지통) / redo: 재생성(주입).
pub struct CreateOp {
    path: PathBuf,
    description: String,
    delete: DeleteFn,
    recreate: RecreateFn,
}

impl CreateOp {
    pub fn new(path: PathBuf, description: String, delete: DeleteFn, recreate: RecreateFn) -> Self {
        Self {
            path,
            description,
            delete,
            recreate,
        }
    }
}

impl ReversibleOp for CreateOp {
    fn description(&self) -> &str {
        &self.description
    }

    fn undo(&mut self) -> Result<(), OpError> {
        if exists(&self.path) && (self.delete)(&self.path).is_err() {
            return Err(OpError::Failed(1));
        }
        Ok(())
    }

    fn redo(&mut self) -> Result<(), OpError> {
        if !exists(&self.path) && (self.recreate)().is_err() {
            return Err(OpError::Failed(1));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delete_permanent;
    use std::cell::Cell;
    use std::fs;
    use std::rc::Rc;

    fn fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nexa_history_{}_{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 호출 횟수만 세는 가짜 연산(스택 규약 검증용 — 원본 FakeOp).
    struct FakeOp {
        undone: Rc<Cell<u32>>,
        redone: Rc<Cell<u32>>,
    }

    fn fake() -> (Box<FakeOp>, Rc<Cell<u32>>, Rc<Cell<u32>>) {
        let (u, r) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        (
            Box::new(FakeOp {
                undone: u.clone(),
                redone: r.clone(),
            }),
            u,
            r,
        )
    }

    impl ReversibleOp for FakeOp {
        fn description(&self) -> &str {
            "fake"
        }
        fn undo(&mut self) -> Result<(), OpError> {
            self.undone.set(self.undone.get() + 1);
            Ok(())
        }
        fn redo(&mut self) -> Result<(), OpError> {
            self.redone.set(self.redone.get() + 1);
            Ok(())
        }
    }

    fn perm_delete() -> DeleteFn {
        Box::new(delete_permanent)
    }

    // ── 스택 규약 ────────────────────────────────────────────────

    #[test]
    fn push_then_undo_then_redo_round_trip() {
        let mut h = OperationHistory::default();
        let (op, undone, redone) = fake();
        h.push(op);
        assert!(h.can_undo() && !h.can_redo());
        assert_eq!(h.undo_description(), Some("fake"));

        assert!(h.undo().unwrap().is_ok());
        assert_eq!(undone.get(), 1);
        assert!(!h.can_undo() && h.can_redo());

        assert!(h.redo().unwrap().is_ok());
        assert_eq!(redone.get(), 1);
        assert!(h.can_undo() && !h.can_redo());
    }

    #[test]
    fn push_clears_redo_stack() {
        let mut h = OperationHistory::default();
        h.push(fake().0);
        h.undo();
        assert!(h.can_redo());
        h.push(fake().0); // 새 작업 → redo 무효화(표준 모델)
        assert!(!h.can_redo());
    }

    #[test]
    fn undo_empty_returns_none() {
        let mut h = OperationHistory::default();
        assert!(h.undo().is_none());
        assert!(h.redo().is_none());
    }

    #[test]
    fn capacity_drops_oldest() {
        let mut h = OperationHistory::with_capacity(2);
        let (first, first_undone, _) = fake();
        h.push(first);
        h.push(fake().0);
        h.push(fake().0);
        h.undo();
        h.undo();
        assert!(!h.can_undo(), "first는 상한으로 제거됨");
        assert_eq!(first_undone.get(), 0);
    }

    #[test]
    fn failing_undo_drops_op_and_propagates() {
        let d = fixture("failundo");
        let mut h = OperationHistory::default();
        // 소실 상태의 rename → 오류
        h.push(Box::new(RenameOp::new(
            d.join("no.txt"),
            d.join("gone.txt"),
            "이름 변경".into(),
        )));
        assert_eq!(
            h.undo().unwrap().unwrap_err(),
            OpError::MissingSource("gone.txt".into())
        );
        assert!(
            !h.can_undo() && !h.can_redo(),
            "실패한 연산은 소실(무결성 우선)"
        );
        fs::remove_dir_all(&d).unwrap();
    }

    // ── 연산 왕복 ────────────────────────────────────────────────

    #[test]
    fn move_batch_undo_moves_back_and_redo_moves_again() {
        let d = fixture("move");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        let f = src.join("a.txt");
        let moved = dst.join("a.txt");
        fs::write(&f, "hello").unwrap();
        fs::rename(&f, &moved).unwrap(); // 원 작업(이동)이 이미 수행된 상태를 기록

        let mut op = MoveBatchOp::new(vec![(f.clone(), moved.clone())], "이동 1개".into());
        op.undo().unwrap();
        assert!(f.exists() && !moved.exists());

        op.redo().unwrap();
        assert!(!f.exists() && moved.exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn move_batch_undo_skips_conflict_and_reports() {
        let d = fixture("moveconflict");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        let f = src.join("a.txt");
        let moved = dst.join("a.txt");
        fs::write(&f, "x").unwrap();
        fs::rename(&f, &moved).unwrap();
        fs::write(&f, "새로 생긴 충돌").unwrap(); // undo 목적지에 외부 변경으로 파일 생김

        let mut op = MoveBatchOp::new(vec![(f, moved.clone())], "이동 1개".into());
        assert_eq!(op.undo().unwrap_err(), OpError::Failed(1));
        assert!(moved.exists(), "덮어쓰지 않음(무결성)");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn copy_batch_undo_deletes_copy_and_redo_recopies() {
        let d = fixture("copy");
        let (src, dst) = (d.join("src"), d.join("dst"));
        fs::create_dir_all(&src).unwrap();
        fs::create_dir_all(&dst).unwrap();
        let f = src.join("a.txt");
        let copied = dst.join("a.txt");
        fs::write(&f, "hello").unwrap();
        fs::copy(&f, &copied).unwrap();

        let mut op = CopyBatchOp::new(
            vec![(f.clone(), copied.clone())],
            "복사 1개".into(),
            perm_delete(),
        );
        op.undo().unwrap();
        assert!(f.exists(), "원본 유지");
        assert!(!copied.exists(), "사본 제거");

        op.redo().unwrap();
        assert_eq!(fs::read_to_string(&copied).unwrap(), "hello");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn rename_op_round_trip() {
        let d = fixture("renameop");
        let a = d.join("a.txt");
        let b = d.join("b.txt");
        fs::write(&a, "v").unwrap();
        fs::rename(&a, &b).unwrap(); // 원 작업(이름 변경) 수행됨

        let mut op = RenameOp::new(a.clone(), b.clone(), "이름 변경: a.txt → b.txt".into());
        op.undo().unwrap();
        assert!(a.exists());
        op.redo().unwrap();
        assert!(b.exists() && !a.exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn create_op_undo_deletes_and_redo_recreates() {
        let d = fixture("createop");
        let created = d.join("새 폴더");
        fs::create_dir(&created).unwrap();

        let recreate_path = created.clone();
        let mut op = CreateOp::new(
            created.clone(),
            "새 폴더".into(),
            perm_delete(),
            Box::new(move || fs::create_dir(&recreate_path)),
        );
        op.undo().unwrap();
        assert!(!created.exists());
        op.redo().unwrap();
        assert!(created.exists());
        fs::remove_dir_all(&d).unwrap();
    }
}
