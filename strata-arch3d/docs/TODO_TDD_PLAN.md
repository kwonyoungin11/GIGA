# STRATA · 동국유물실측 v3.0 — 사용자 피드백 7대 문제증상 해결 Todo List 및 TDD 계획서

작성일: 2026-11-01
대상 브랜치: `feat/strata-arch3d-rust` (`D:\_dongarch_build\wt\.worktrees\STRATA-RUST\strata-arch3d`)

---

## 1. 사용자 피드백(이미지 주석 0~6) 분석 및 Todo List

| 순서 | Todo 항목 | 출처 주석 | 핵심 해결 내용 | TDD 검증 테스트 이름 |
|---|---|---|---|---|
| **Todo 1** | **성능 병목 제거 및 3D 뷰포트 고해상도화** | Image 6 ("화질이 너무 떨어지고 버벅거린다 병목지점을 확인해야한다. 전체앱이 버벅거린다") | ① `View3DPaneState` 더티 캐시(`View3DRenderCacheKey` + `egui::TextureHandle` 재사용) 도입으로 정지 프레임 렌더링 비용 `0ms` 달성<br>② `SheetPaneState` 텍스처 핸들 영구 캐싱(매 프레임 `load_texture` 재할당 제거)<br>③ 슬라이더/다이얼 조작 시 `rebuild_plate`에서 고비용 `segment_sherd_three_regions` 재호출 제거<br>④ 3D Z-버퍼 해상도 향상(최대 `640×640`) 및 `rayon` 멀티코어 스캔라인 병렬화 | `test_tdd1_view3d_dirty_cache_skips_redundant_rerender_and_supports_hd_resolution` |
| **Todo 2** | **좌측 슬롯창 제거·작업창 확대 및 상단 4단계 탭 단순화** | Image 1 ("복잡하다 이것은 사용자가 판단할문제이다. 창을 더크게하고 없애라") & Image 5 ("왜있는지 모르겠다?") | ① 좌측 `176px` 슬롯 패널 및 가짜 프리셋 5종 제거 → `3D 뷰포트 + 2D 도판`이 전체 화면 가로폭(`100%`) 사용<br>② 모호한 상단 `①~④` 단계 탭 제거 → 단일 통합 상단 바 + 국가유산청 도면 양식 리본으로 일원화 | `test_tdd2_full_width_workspace_without_left_slot_panel_or_step_tabs` |
| **Todo 3** | **국가유산청(구 문화재청) 발굴조사보고서 도면 작성양식 10종 세분화** | Image 4 ("더세밀하게 나눠서 문화재청 보고서 도면 작성양식으로 만들어야한다") | 기존 4개 버튼을 국가유산청 『발굴조사보고서 작성 표준』 기반 **10종 세부 도면 양식(`ChsReportTemplate`)**으로 세분화 (`토기·완형 표준도`, `토기·구연/저부 복원도`, `토기·동체편 3단도`, `토기·파수/주구/굽`, `기와·막새`, `기와·평기와`, `석기·타제 박리도`, `석기·마제 실측도`, `금속기·부식분리도`, `옥류·장신구 확대도`) | `test_tdd3_chs_10_archaeological_report_drawing_templates` |
| **Todo 4** | **손으로 유물을 상하좌우 돌리는 3D 핸드 다이얼(트랙볼/조그 링)** | Image 0 ("유물 상하좌우로 돌리는 다이얼을 만들어서 직관적으로 유물을 손으로 돌리듯") | 추상적인 슬라이더 4개를 대체하여, 손으로 유물을 잡고 상하좌우(`Pitch / Yaw / Roll`)로 자유롭게 굴리는 **원형 3D 핸드 다이얼(구면 트랙볼 패드 + 외곽 회전 링 + 절단면 깊이 조절)** 구현 | `test_tdd4_hand_dial_spherical_rotation_updates_pitch_yaw_roll` |
| **Todo 5** | **화면 전체(Full-Bleed) 실제 실측 방안지 구현** | Image 3 ("화면전체를 방안지를 만들고 방안지를 실제방안지와 똑같이 표현되어야한다") | 우측 `SheetPane`의 회색 책상 여백을 없애고 **캔버스 전체(100% Full-Bleed)**를 실제 고고학 실측 방안지(`1mm` 세선 + `5mm` 중선 + `10mm` 주선 + 상단·좌측 `cm/mm` 눈금자)로 렌더링 | `test_tdd5_full_bleed_archaeological_millimeter_graph_paper_grid` |
| **Todo 6** | **논문 기반 고정밀 디지털 탁본(MSII + DoG + Sigmoid Ink Transfer) 엔진** | Image 2 후반 ("디지털 탁본은 표현의 방법을 더 논문을 보고해야한다") | Mara & Krömker (2010) *GigaMesh MSII* 다중 스케일 곡률 + 다중 반경 DoG(Difference of Gaussians) 음각/양각 분리 + 비선형 시그모이드 먹물 전이 곡선(Sigmoid Ink Transfer) + 주변 차폐(Cavity Ambient Occlusion) 기반 학술 디지털 탁본 구현 | `test_tdd6_academic_msii_dog_sigmoid_digital_rubbing_contrast` |
| **Todo 7** | **드래그 영역 지정 부분 탁본창(ROI Rubbing Studio) 및 농도 개별 제어** | Image 2 전반 ("실제 탁본할위치를 드레그를 하면 그부분만 탁본이 나오게 하는 탁본창을 만들어서 농도를 조정할수있게하라") | 도판/유물 위에서 마우스 드래그로 **탁본할 영역(ROI)**을 지정하면 해당 영역만 부분 탁본으로 렌더링되고, 전용 **『부분 정밀 탁본 창(ROI Rubbing Window)』**에서 먹물 농도·곡률 감도·대비를 실시간 조절하여 도판에 반영 | `test_tdd7_drag_roi_partial_rubbing_window_and_density_control` |

---

## 2. TDD 실행 원칙 (`/test-driven-development`)
1. 각 Todo 항목마다 **실패하는 단위 테스트(RED)**를 먼저 작성하고 `cargo test`로 실패를 확인한다.
2. 테스트를 통과시키는 **최소한의 명확한 프로덕션 코드(GREEN)**를 구현하고 `cargo test` 통과를 확인한다.
3. 중복 제거 및 리팩토링(REFACTOR) 후 실제 실행 창(`--capture-window`) 캡처로 시각 검증을 수행한다.
