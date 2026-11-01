# STRATA · 동국유물실측 v3.0 — 사용자 피드백 7대 문제증상 해결 Todo List 및 TDD 계획서

작성일: 2026-11-01
대상 브랜치: `feat/strata-strata-rust` (`D:\_dongarch_build\wt\.worktrees\STRATA-RUST\strata-strata`)

---

## 1. 사용자 피드백(이미지 주석 0~6) 분석 및 Todo List

| 순서 | Todo 항목 | 출처 주석 | 핵심 해결 내용 | TDD 검증 테스트 이름 |
|---|---|---|---|---|
| **Todo 1** | **성능 병목 제거 및 3D 뷰포트 고해상도화** | Image 6 ("화질이 너무 떨어지고 버벅거린다 병목지점을 확인해야한다. 전체앱이 버벅거린다") | ① `View3DPaneState` 더티 캐시(`View3DRenderCacheKey` + `egui::TextureHandle` 재사용) 도입으로 정지 프레임 렌더링 비용 `0ms` 달성<br>② `SheetPaneState` 텍스처 핸들 영구 캐싱(매 프레임 `load_texture` 재할당 제거)<br>③ 슬라이더/다이얼 조작 시 `rebuild_plate`에서 고비용 `segment_sherd_three_regions` 재호출 제거<br>④ 3D Z-버퍼 해상도 향상(최대 `640×640`) 및 `rayon` 멀티코어 스캔라인 병렬화 | `test_tdd1_view3d_dirty_cache_skips_redundant_rerender_and_supports_hd_resolution` |
| **Todo 2** | **좌측 슬롯창 제거·작업창 확대 및 상단 4단계 탭 단순화** | Image 1 ("복잡하다 이것은 사용자가 판단할문제이다. 창을 더크게하고 없애라") & Image 5 ("왜있는지 모르겠다?") | ① 좌측 `176px` 슬롯 패널 및 가짜 프리셋 5종 제거 → `3D 뷰포트 + 2D 도판`이 전체 화면 가로폭(`100%`) 사용<br>② 모호한 상단 `①~④` 단계 탭 제거 → 단일 통합 상단 바 + 국가유산청 도면 양식 리본으로 일원화 | `test_tdd2_full_width_workspace_without_left_slot_panel_or_step_tabs` |
| **Todo 3** | **국가유산청(구 문화재청) 발굴조사보고서 도면 작성양식 10종 세분화** | Image 4 ("더세밀하게 나눠서 문화재청 보고서 도면 작성양식으로 만들어야한다") | 기존 4개 버튼을 국가유산청 『발굴조사보고서 작성 표준』 기반 **10종 세부 도면 양식(`ChsReportTemplate`)**으로 세분화 (`토기·완형 표준도`, `토기·구연/저부 복원도`, `토기·동체편 3단도`, `토기·파수/주구/굽`, `기와·막새`, `기와·평기와`, `석기·타제 박리도`, `석기·마제 실측도`, `금속기·부식분리도`, `옥류·장신구 확대도`) | `test_tdd3_chs_10_archaeological_report_drawing_templates` |
| **Todo 4** | **손으로 유물을 상하좌우 돌리는 3D 핸드 다이얼(트랙볼/조그 링)** | Image 0 ("유물 상하좌우로 돌리는 다이얼을 만들어서 직관적으로 유물을 손으로 돌리듯") | 추상적인 슬라이더 4개를 대체하여, 손으로 유물을 잡고 상하좌우(`Pitch / Yaw / Roll`)로 자유롭게 굴리는 **원형 3D 핸드 다이얼(구면 트랙볼 패드 + 외곽 회전 링 + 절단면 깊이 조절)** 구현 | `test_tdd4_hand_dial_spherical_rotation_updates_pitch_yaw_roll` |
| **Todo 5** | **화면 전체(Full-Bleed) 실제 실측 방안지 구현** | Image 3 ("화면전체를 방안지를 만들고 방안지를 실제방안지와 똑같이 표현되어야한다") | 우측 `SheetPane`의 회색 책상 여백을 없애고 **캔버스 전체(100% Full-Bleed)**를 실제 고고학 실측 방안지(`1mm` 세선 + `5mm` 중선 + `10mm` 주선 + 상단·좌측 `cm/mm` 눈금자)로 렌더링 | `test_tdd5_full_bleed_archaeological_millimeter_graph_paper_grid` |
| **Todo 6** | **논문 기반 고정밀 디지털 탁본(MSII + DoG + Sigmoid Ink Transfer) 엔진** | Image 2 후반 ("디지털 탁본은 표현의 방법을 더 논문을 보고해야한다") | Mara & Krömker (2010) *다중스케일 MSII* 다중 스케일 곡률 + 다중 반경 DoG(Difference of Gaussians) 음각/양각 분리 + 비선형 시그모이드 먹물 전이 곡선(Sigmoid Ink Transfer) + 주변 차폐(Cavity Ambient Occlusion) 기반 학술 디지털 탁본 구현 | `test_tdd6_academic_msii_dog_sigmoid_digital_rubbing_contrast` |
| **Todo 7** | **드래그 영역 지정 부분 탁본창(ROI Rubbing Studio) 및 농도 개별 제어** | Image 2 전반 ("실제 탁본할위치를 드레그를 하면 그부분만 탁본이 나오게 하는 탁본창을 만들어서 농도를 조정할수있게하라") | 도판/유물 위에서 마우스 드래그로 **탁본할 영역(ROI)**을 지정하면 해당 영역만 부분 탁본으로 렌더링되고, 전용 **『부분 정밀 탁본 창(ROI Rubbing Window)』**에서 먹물 농도·곡률 감도·대비를 실시간 조절하여 도판에 반영 | `test_tdd7_drag_roi_partial_rubbing_window_and_density_control` |

---

## 2. TDD 실행 원칙 (`/test-driven-development`)
1. 각 Todo 항목마다 **실패하는 단위 테스트(RED)**를 먼저 작성하고 `cargo test`로 실패를 확인한다.
2. 테스트를 통과시키는 **최소한의 명확한 프로덕션 코드(GREEN)**를 구현하고 `cargo test` 통과를 확인한다.
3. 중복 제거 및 리팩토링(REFACTOR) 후 실제 실행 창(`--capture-window`) 캡처로 시각 검증을 수행한다.


---

## 3. v6.0 추가 사용자 피드백(이미지 주석 0~15 + 수평 정합·5휠·고해상도) 명세 및 계획

| 순서 | Todo 항목 | 출처 주석 | 핵심 완료 기준 및 구현 명세 | TDD 검증 테스트 이름 |
|---|---|---|---|---|
| **Todo 8** | **원본 스캐너급 고해상도(`tri_stride=1` + 4K Bilinear UV + Rayon 병렬화)** | Image 0, Image 9, `<USER_REQUEST>` | ① `obj_loader.rs`: 3D 스캐너 텍스처 해상도 상한을 `1024` → `4096`으로 확장 및 `MeshTextureRgba::sample_uv_rgb`에 쌍선형(Bilinear) 보간 적용<br>② `view3d_pane.rs` & `sheet_pane.rs`: 드래그·정지 불문 `tri_stride = 1`(100% 전체 삼각형, 점·핀홀 0개) 강제 적용 및 `rayon` 멀티코어 병렬 투영 + `560×560` 이상 고밀도 탁본 버퍼 적용 | `test_tdd12_native_scanner_resolution_zero_pinholes_and_bilinear_uv` |
| **Todo 9** | **상·하·좌·우 4방향 휠 기능·설명 위치 재정의 + 내부 제5 수평정합 휠 추가** | Image 1, Image 2, Image 3, Image 4, `<USER_REQUEST>` | ① 상단 가로 휠: 유물 시계/반시계 회전(`ψ`, `0.1°` 단위), 휠 **아래**에 설명 배치<br>② 하단 가로 휠: 유물·단면선 앞뒤 미세조정(`ΔY`, `0.01mm` 단위), 휠 **아래**에 설명 배치<br>③ 우측 세로 휠: 유물 좌우 기울기 미세조정(`roll_deg`, `0.1°` 단위), 휠 **옆**에 설명 배치<br>④ 좌측 세로 휠: 유물 상하 회전 미세조정(`tilt_deg`, 올리면 위로 `+0.1°`, 내리면 아래로 `-0.1°`), 휠 **옆**에 설명 배치<br>⑤ 상하좌우 휠 안쪽에 **제5 수평·단면 미세정합 휠(`InnerLevel`)**을 추가하고, 3D 뷰와 2D 단면 슬라이서가 100% 동일한 월드 변환 정점(`transform_local_to_world_full`)을 공유하도록 통합 | `test_tdd13_five_wheel_cardinal_and_inner_level_exact_section_sync` |
| **Todo 10** | **직상방 뷰 중앙 고정 단면선 + 유물 이동·회전 + 하단 전체 방안선 + 상하 단면 점선 연결** | Image 5, Image 6, Image 7, Image 8 | ① 상단 직상방 동심원 방안지에서 동심원은 항상 `1:1` 정원(둥근 원)을 유지하고 단면선은 중앙(`Y = 0`)에 고정하며, 유물만 수평 이동(`artifact_offset_x_mm`, `cut_y_mm`) 및 회전(`ψ`)되도록 구현<br>② 하단 3D 정면 입면 뷰 전체 배경에도 `2mm / 10mm` 실측 방안선을 풀블리드로 렌더링<br>③ 상단 직상방 뷰와 하단 정면 입면 뷰의 가로 스케일(`mm → px`)을 일치시키고, 위 단면선과 아래 단면선의 좌·중·우 위치를 얇은 점선으로 수직 연결 | `test_tdd14_topdown_fixed_center_section_and_vertical_dashed_link` |
| **Todo 11** | **수평 횡단면 상단 이동 + 단면 외곽선 전용(검은 채움 제거) + 선 굵기(`mm`)·색상 즉시 조정** | Image 10, Image 11, Image 12, Image 15 | ① `수평 횡단면`을 유물 정면도 **위쪽**으로 이동 배치<br>② 우측 수직 단면을 검은색 채움 대신 **깨끗한 외곽선(선 제도)**으로 기본 렌더링<br>③ 도면 바로 위·옆에서 모든 선(단면선·외곽선·횡단면선·치수선)의 **굵기(`0.1mm ~ 1.2mm`)와 색상**을 즉시 변경할 수 있는 직관적 인라인 컨트롤 제공 | `test_tdd15_horizontal_section_above_and_outline_section_with_mm_color_controls` |
| **Todo 12** | **부분 탁본 선택 드래그 → 1:1 크기 탁본 표시 + 탁본 바로 밑 농도 미세조정 다중 휠 + 밀착 치수선** | Image 13, Image 14, Image 15 | ① `[⬚ 부분탁본 선택]` 버튼을 누르고 유물 위를 드래그하면 드래그한 실제 크기(`1:1 mm` 스케일) 그대로 도판 우측 탁본 영역에 정밀 탁본이 표시됨<br>② 학술 탁본 알고리즘(4-Scale Integral-Image DoG + Weingarten 주곡률 + 한지 접촉 포락선) 고도화 및 **탁본 이미지 바로 밑에 4개의 농도·대비·선명도·여백 미세조정 휠** 배치<br>③ 축척 막대(`1:2`)와 치수선(`기고`, `구경`)이 유물 크기에 맞춰 도면 바로 옆·아래에 밀착되어 함께 이동 | `test_tdd16_roi_1to1_rubbing_with_under_image_wheels_and_tight_dimensions` |


---

## 4. v7.0 추가 사용자 피드백 (`v6_vessel_roi_1to1.png` 주석 0~5 + 방향키 이동) 명세 및 계획

| 순서 | Todo 항목 | 출처 주석 | 핵심 완료 기준 및 구현 명세 | TDD 검증 테스트 이름 |
|---|---|---|---|---|
| **Todo 13** | **제5 안쪽 휠을 '수평횡단면 높이(`Z`) 조정 휠'로 전환 + 좌측 상단 옆 '수평횡단면 전용 고정 유물 뷰' 추가 + 최상단 단면선 표시** | `Image 0`, `Image 1`, `Image 4`, `A1` | ① 안쪽 가로 휠(`InnerHorizSectionZ`)을 돌리거나 드래그하면 유물은 고정된 채 **수평횡단면 절단선(`top_cut_z_mm`)만 상·하로 이동(`0.1mm/step`)**하고, 우측 2D 도면의 `수평 횡단면 (Z=...)` 도면이 실시간 갱신됨<br>② 좌측 상단 `직상방 동심원` 뷰 바로 옆(우측)에 **'수평횡단면 전용 고정 유물 뷰(유물 고정 · 수평절단선만 상하 드래그/휠 이동)'**를 나란히 배치<br>③ 하단 정면 입면 뷰 및 2D 도면에서도 수평·수직 단면선이 유물이나 탁본 이미지에 가려지지 않도록 **최상단 레이어**에 선명하게 렌더링 | `test_tdd17_inner_wheel_controls_horizontal_section_z_and_updates_plate` |
| **Todo 14** | **방향키(`←/→/↑/↓`)를 통한 유물 및 수평횡단면 정밀 이동** | `<USER_REQUEST>` ("이동은 방향키로도 움직일수있게해") | ① 키보드 방향키(`←`, `→`, `↑`, `↓`) 입력 시 직상방 뷰의 유물 수평 위치(`artifact_offset_x_mm`, `cut_y_mm`)가 `0.1mm` 단위(`Shift` 조합 시 `1.0mm` 단위)로 정밀 이동됨<br>② `Alt` 또는 수평횡단면 뷰 포커스 시 방향키(`↑`, `↓`)로 수평횡단면 높이(`top_cut_z_mm`)를 `0.2mm` 단위로 상·하 이동 가능 | `test_tdd18_arrow_keys_nudge_artifact_and_horizontal_section` |
| **Todo 15** | **직상방 뷰 좌측 상단 `[+ 단면추가]` 버튼 + 사용자 자유 단면선 드래그 작도 + 끝점 개별 이동 + 우측 2D 추가 단면도 생성** | `Image 5` | ① 직상방 동심원 뷰 좌측 상단에 `[+ 단면추가]` 버튼 배치<br>② 버튼을 누르고 직상방 유물 위에 선(`p0 -> p1`)을 그으면 임의 각도·위치의 3D 메쉬 절단 단면(`slice_arbitrary_line_2d`)이 즉시 추출되어 우측 2D 도면에 **추가 단면도(`단면 A-A'` 등)**로 그려짐<br>③ 그어진 단면선의 **양 끝지점(`p0` 또는 `p1`)을 마우스로 잡고 이동하면 반대쪽 끝점은 고정된 채 잡은 한쪽 끝점만 이동**하며 실시간으로 우측 2D 단면도가 갱신됨 | `test_tdd19_custom_section_line_single_endpoint_move_and_2d_profile` |
| **Todo 16** | **2D 출판 조판 용지 설정(`A4`/`A3`/`B4`/`B5`) + 용지 내부 전용 방안선(`5cm`마다 얇은 빨간선) + 선 조정 위젯 중복 제거 및 유물 연동 이동** | `Image 1`, `Image 2`, `Image 3`, `A2` | ① 용지 규격 선택(`PaperSizePreset`: `A4 세로`, `A4 가로`, `A3 가로`, `B4 세로`, `B5 세로`)을 추가하고, **용지 영역 안쪽에만 방안선**을 그리며 **`5cm(50mm)`마다 얇은 빨간색 격자선**을 렌더링<br>② 상단 툴바의 중복 선 조정 바를 제거하고, 각 선(`단면선`, `외곽선`, `횡단면`, `치수선`)의 굵기·색상 조정 위젯을 해당 도면 요소 옆 **고정 오프셋(`mm` 좌표)**에 겹치지 않게 1곳씩만 배치하여 유물 이동·줌인/줌아웃 시 유물과 함께 이동하며 서로 겹치지 않도록 보장 | `test_tdd20_paper_sheet_5cm_red_grid_and_non_overlapping_line_widgets` |
