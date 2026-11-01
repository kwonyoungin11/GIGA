//! 우측 2D 실측 도판 캔버스 — 방안지·정밀 탁본·단면 합성·레이어 변형·내보내기 미리보기.
//!
//! 원래 4,477줄 단일 파일이던 `sheet_pane.rs` 를 역할별 모듈로 나눈 것이다.
//! 바깥(`crate::sheet_pane::…`)에서 쓰는 공개 API 는 아래 `pub(crate) use` 로 그대로 유지된다.
//!
//! - `config`: 탁본 기법·파라미터, 도면 선 굵기, 용지 규격·방안선, 레이어 변형
//! - `state`: `SheetPaneState` (줌·패닝·레이어 선택·캐시)
//! - `rubbing`: 정밀 탁본 래스터 생성·전개(Unroll)·출처 각인·축척 계산
//! - `widgets`: 탁본 미세 휠, 선 굵기 아이콘, 레이어 뒤집기/회전 컨트롤
//! - `render`: `render_sheet_pane_egui` (실제 egui 도판 패널)
//! - `offscreen`: 창 없이 렌더링하는 작업 공간 PNG 출력

mod config;
mod offscreen;
mod render;
mod rubbing;
mod state;
mod widgets;

pub(crate) use offscreen::*;
pub(crate) use render::*;
pub(crate) use rubbing::*;
pub(crate) use state::*;

#[cfg(test)]
mod tests;
