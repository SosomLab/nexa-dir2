# 20 · 세션 자동 저장 — 디바운스 코얼레싱 설계·구현

> **작성: 2026-07-15** — 사용자 요청("탭 추가/삭제 시 상태 저장 — 변경 폭주 시 쌓아뒀다
> 특정 시간에 1번만 flush, 마지막 변경분만 반영·나머지 무효화") 이행 기록.
> 원본 M2-5 SESS 규율("요청/수행 분리·Tick 코얼레싱")의 dir2 이행이기도 하다.
> 구현 브랜치 `feat/session-autosave`(병합 `16c00b7`).

## 1. 설계 원칙

| 원칙 | 구현 방식 |
| --- | --- |
| **요청/수행 분리** | 변경 지점은 bool 플래그 1개만 세운다(직렬화·I/O 없음 — 비용 0) |
| **코얼레싱(마지막만 반영)** | 저장 요청이 큐에 쌓이는 게 아니라 **디바운스 타이머를 재무장** — 새 변경이 오면 이전 예약이 뒤로 밀리며 자연 폐기(중간 상태 무효화) |
| **스냅샷 1회** | 타이머 만료 시점의 **현재 상태**를 그때 1번 직렬화 — 변경 100번 = 파일 쓰기 1번 |
| **단일 원천** | 자동 저장과 종료 저장이 같은 스냅샷 헬퍼를 사용(형식 불일치 원천 차단) |
| **크래시 안전** | 원자적 쓰기(tmp→rename) — 저장 도중 크래시에도 기존 파일 보존 |

"스택에 쌓아두고 마지막만 반영"이라는 요구를 **큐 없이** 달성한다: 저장할 내용을
쌓는 대신 "저장 필요" 사실만 기억하고, 실제 내용은 flush 시점에 읽는다. 중간 상태는
어디에도 저장되지 않으므로 폐기 비용도 0이다.

## 2. 데이터 흐름

```
[변경 발생]  new_tab / close_tab / move_tab / switch_tab /
             toggle_tab_lock / toggle_tab_pin / apply_source(탐색·재로드)
      │  Panel.session_dirty = true          ← 플래그만 (panel.rs)
      ▼
[수거]      update_status()                  ← 모든 상호작용의 단일 길목 (win.rs)
      │  take_session_dirty() == true 면
      │  SetTimer(TIMER_SESSION_SAVE, 1000)  ← "재무장" = 같은 id 타이머 갱신
      ▼                                         (연속 변경 = 만료 계속 연기)
[flush]     WM_TIMER(TIMER_SESSION_SAVE)     ← 마지막 변경 1초 후 정확히 1회
      │  KillTimer → current_session(st) 스냅샷 → config::save(원자적)
      ▼
data\session.cfg
```

## 3. 구현 상세 (파일·라인)

### 3-1. 변경 표시 — [panel.rs](../crates/nexa-app/src/panel.rs)

- `Panel.session_dirty: bool` 필드(≈L86) — 문서 주석에 규약 명시.
- 마킹 지점 7곳(각 함수 진입부 `self.session_dirty = true;` 한 줄):
  `move_tab`·`toggle_tab_lock`·`toggle_tab_pin`·`close_tab`·`switch_tab`·
  `new_tab`·`apply_source`(탐색/뒤로/앞으로/위로/재로드/복제가 전부 이 길목을
  지나므로 개별 함수에 중복 마킹 불요).
- `take_session_dirty()`(≈L690) = `std::mem::take` — **읽는 순간 리셋되는 1회성**.
  두 번 수거해도 타이머가 한 번만 무장되는 이유.

### 3-2. 수거·디바운스 — [win.rs](../crates/nexa-app/src/win.rs)

- 상수(≈L84): `TIMER_SESSION_SAVE = 8` · `SESSION_SAVE_DEBOUNCE_MS = 1_000`.
- `update_status()` 말미(≈L2416):

  ```rust
  if st.panels[0].take_session_dirty() | st.panels[1].take_session_dirty() {
      SetTimer(Some(hwnd), TIMER_SESSION_SAVE, SESSION_SAVE_DEBOUNCE_MS, None);
  }
  ```

  - `update_status`는 탭/네비/선택 등 **모든 상호작용 흐름이 지나는 단일 길목**이라
    별도 훅 없이 수거가 보장된다.
  - `|`(비트 OR)를 쓴 이유: `||`는 단락 평가라 좌 패널이 더티면 **우 패널 플래그가
    수거되지 않고 남는** 미묘한 결함이 생긴다 — 양쪽 모두 항상 소진해야 한다.
  - **재무장 = 코얼레싱의 핵심**: Win32 `SetTimer`는 같은 (hwnd, id)로 다시 부르면
    기존 타이머를 대체한다. 변경이 계속 오는 동안 만료가 계속 밀리고,
    마지막 변경 후 1초가 조용히 지나야 비로소 1회 발화한다.

### 3-3. flush — win.rs `WM_TIMER`(≈L4319)

```rust
if wparam.0 == TIMER_SESSION_SAVE {
    let _ = KillTimer(Some(hwnd), TIMER_SESSION_SAVE);
    if let Some(st) = state_of(hwnd) {
        let session = current_session(st);
        let _ = config::save(&config::data_dir(), SESSION_FILE, &session.serialize());
    }
    return LRESULT(0);
}
```

- `KillTimer` 선행 — `SetTimer`는 반복 타이머이므로 죽이지 않으면 1초마다 재발화.
- 저장 실패는 무시(`let _`) — 자동 저장은 편의 기능, 종료 저장이 최종 방어선.

### 3-4. 스냅샷 단일 원천 — `current_session(st)`(≈L2991)

탭 경로·활성 인덱스·펼침 집합(F18)·잠금·고정을 `Session` 구조로 수집.
**WM_DESTROY 종료 저장(≈L4477)과 동일 함수를 공유**해 두 경로의 스키마가 항상 일치.

### 3-5. 원자성 — [config.rs](../crates/nexa-app/src/config.rs) `save`(≈L443)

`{name}.tmp`에 전체를 쓴 뒤 `rename` — flush 도중 프로세스가 죽어도
`session.cfg`는 직전 완전본이 유지된다(M2-5 SESS 원자성 계승).

## 4. 성능 특성

| 시나리오 | 비용 |
| --- | --- |
| 변경 1회 | 플래그 쓰기 1 + SetTimer 1 + (1초 후) 직렬화·쓰기 1회 |
| 변경 N회 폭주(탭 드래그 연타 등) | 플래그/SetTimer N회(수 ns~µs) + **쓰기는 여전히 1회** |
| 유휴 | 0 — 타이머 미무장, 폴링 없음(플래그 검사는 상호작용 시에만) |

직렬화 자체도 작다(탭 경로 + 펼침 상한 200/탭 — 수 KB 텍스트). 무거운 것은
디스크 I/O뿐이고 그것이 코얼레싱 대상이다.

## 5. 한계·후속

- 디바운스형이라 **변경이 1초 간격 미만으로 무한 지속되면 flush가 계속 밀린다**
  (이론상 기아). 실사용에선 발생하기 어렵고, 종료 저장이 최종 커버.
  필요 시 "최대 지연 상한(예: 10초)" 하이브리드로 확장 가능.
- 펼침/스크롤만 바뀌는 경우는 마킹하지 않는다(탭 구성 변경이 트리거 기준) —
  이 값들도 flush 시점 스냅샷에는 포함되므로 최신값이 함께 저장된다.
- settings.cfg는 이 경로와 무관(설정 창 즉시 적용 시 저장 + 종료 저장).

## 6. 검증

- 회귀 테스트 `panel::tests::session_dirty_flag_on_tab_ops` — 초기 깨끗·새 탭/전환/
  고정/닫기 각각 더티·수거 후 리셋(1회성).
- 실기: 탭 조작 후 1초 뒤 `data\session.cfg` mtime 갱신, 강제 종료(작업 관리자)
  후 재실행 시 탭 구성 복원.
