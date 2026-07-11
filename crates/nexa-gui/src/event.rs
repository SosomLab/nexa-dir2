//! 플랫폼 중립 입력 이벤트 — nexa-app이 WM_*를 이 타입으로 번역해 위젯에 라우팅한다.

/// Win32 `WHEEL_DELTA` — 휠 1노치의 delta 단위.
pub const WHEEL_DELTA: i32 = 120;

/// 네비게이션 키(키보드 우선 DR-5). 문자 입력·수식키 조합은 타입어헤드(M1-6)에서 확장.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputEvent {
    /// 세로 휠 원시 delta(`WHEEL_DELTA` 단위 — 트랙패드는 분수 노치). 양수 = 위로.
    Wheel {
        delta: i32,
    },
    Key(Key),
    /// 마우스 좌클릭(클라이언트 좌표). 캐럿/선택(M1-5) 전까지 행 활성화(펼침 토글)에 사용.
    MouseDown {
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
