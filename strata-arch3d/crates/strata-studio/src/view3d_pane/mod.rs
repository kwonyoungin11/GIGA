//! 3D 뷰포트(`27%`) — 상단 직상방 동심원 절단 뷰 + 하단 수평 정면 입면 뷰(5휠 조작).
//!
//! 원래 3,600줄 단일 파일이던 `view3d_pane.rs` 를 역할별 모듈로 나눈 것이다.
//! 바깥(`crate::view3d_pane::…`)에서 쓰는 공개 API 는 아래 `pub(crate) use` 로 그대로 유지된다.
//!
//! - [`state`]: 셰이딩·드래그 모드 열거형, `View3DPaneState`, 캐시·다이얼·자세 조작
//! - [`geometry`]: 5휠 월드 변환, 절단면 교차 선분, 사용자 단면선, 직상방 평면 절단
//! - [`raster`]: 소프트웨어 Z-버퍼/탑다운 래스터, 방안선 배경, 정투영 투영식
//! - `ui` 모듈: `render_view3d_pane_egui` (실제 egui 패널)

mod geometry;
mod raster;
mod state;
mod ui;

pub(crate) use geometry::*;
pub(crate) use raster::*;
pub(crate) use state::*;
pub(crate) use ui::*;

#[cfg(test)]
mod tests;
