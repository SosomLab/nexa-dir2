//! 플랫폼 중립 입력 이벤트 — nexa-app이 WM_*를 이 타입으로 번역해 위젯에 라우팅한다.

/// Win32 `WHEEL_DELTA` — 휠 1노치의 delta 단위.
pub const WHEEL_DELTA: i32 = 120;

/// 네비게이션 키(키보드 우선 DR-5).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    /// 캐럿 행 인라인 펼침, 펼침 상태면 첫 자식으로(원본 docs/07 §8).
    Right,
    /// 캐럿 행 접힘, 접힘 상태면 부모로.
    Left,
    /// 캐럿 행 선택 토글(원본 docs/32 §7 결정 1 — 타입어헤드에서 제외).
    Space,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputEvent {
    /// 세로 휠 원시 delta(`WHEEL_DELTA` 단위 — 트랙패드는 분수 노치). 양수 = 위로.
    Wheel {
        delta: i32,
    },
    /// 가로 휠(Shift+휠 포함). 양수 = 오른쪽으로 스크롤.
    HWheel {
        delta: i32,
    },
    /// 키 입력 + 수식키. `shift` = 범위 선택, `ctrl` = 선택 없이 캐럿만 이동(탐색기 규약).
    Key {
        key: Key,
        shift: bool,
        ctrl: bool,
    },
    /// 인쇄 가능 문자(타입어헤드 — docs/32). `'\u{8}'` = Backspace(접두사 축소).
    /// `now_ms` = 단조 시각(밀리초) 주입 — 버퍼 타임아웃 판정용(테스트 가능성).
    Char {
        c: char,
        now_ms: u64,
    },
    /// 현재 가시 노드 전체 선택(Ctrl+A — 원본 docs/07 §8).
    SelectAll,
    /// 마우스 좌클릭(클라이언트 좌표). 헤더 = 정렬/리사이즈, 본문 = 선택/펼침/러버밴드.
    /// `shift` = 범위 선택·다중 정렬(docs/23 COL-3), `ctrl` = 비연속 토글(docs/07 §1-2).
    MouseDown {
        x: i32,
        y: i32,
        shift: bool,
        ctrl: bool,
    },
    /// 마우스 우클릭 — 경로 바 편집 모드 진입(docs/27) 등.
    RightDown {
        x: i32,
        y: i32,
    },
    /// 마우스 이동(버튼 상태 무관 — 위젯이 드래그 상태를 보유).
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseUp {
        x: i32,
        y: i32,
    },
}

/// 분수 노치 휠 누적기 — M0-7 스파이크 로직의 이식(트랙패드 분수 delta를 잔여 누적).
#[derive(Clone, Copy, Default, Debug)]
pub struct WheelAccum {
    accum: i32,
}

impl WheelAccum {
    /// delta를 누적하고 이번에 스크롤할 행 수를 반환(양수 = 위로). 잔여는 다음 호출로 이월.
    pub fn add(&mut self, delta: i32, lines_per_notch: i32) -> i32 {
        self.accum += delta;
        let lines = self.accum * lines_per_notch / WHEEL_DELTA;
        if lines != 0 {
            self.accum -= lines * WHEEL_DELTA / lines_per_notch;
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_notch_scrolls_lines_per_notch() {
        let mut w = WheelAccum::default();
        assert_eq!(w.add(WHEEL_DELTA, 3), 3);
        assert_eq!(w.add(-WHEEL_DELTA, 3), -3);
    }

    #[test]
    fn fractional_notches_accumulate() {
        // 트랙패드: 40 + 40 = 노치 2/3 → 아직 3행/노치 기준 2행
        let mut w = WheelAccum::default();
        assert_eq!(w.add(40, 3), 1);
        assert_eq!(w.add(40, 3), 1);
        assert_eq!(w.add(40, 3), 1); // 총 120 = 정확히 3행
    }

    #[test]
    fn remainder_carries_over_without_loss() {
        let mut w = WheelAccum::default();
        let mut total = 0;
        for _ in 0..12 {
            total += w.add(30, 3); // 30×12 = 노치 3개
        }
        assert_eq!(total, 9);
    }
}
