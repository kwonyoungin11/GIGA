# STRATA · 동국유물실측 (`strata-strata`) 상세 구현 및 단계별 실제 창 캡처 검증 계획서 (v2.0)

> [!IMPORTANT]
> 본 문서는 전역 개발 규칙 **3단계(계획: 파일별 작업 순서와 먼저 쓸 테스트를 적는다)** 및 마스터 설계서([`DESIGN_SPEC.md`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/docs/DESIGN_SPEC.md) v2.0)에 따라 수립된 실행 계획서입니다.
> - **절대 수칙 1**: 문서·코드·UI 어디에도 금지어를 절대 쓰지 않으며, 반드시 **'정밀 탁본' (`rubbing` / `Rubbing`)**만 사용합니다.
> - **절대 수칙 2**: 기존 8개 크레이트의 구현된 기하 알고리즘과 테스트를 삭제하거나 축약하지 않습니다.
> - **절대 수칙 3**: 가짜 오프스크린 함수로 만든 이미지가 아니라, **1단계씩 실제 실행된 `strata-studio.exe` 네이티브 창의 스왑체인 프레임버퍼를 캡처(`egui::ViewportCommand::Screenshot`)하여 사용자 확인을 받은 뒤 다음 단계로 넘어갑니다.**

---

## 1. 작업 격리 환경 및 기준 정보
- **격리 Git Worktree 경로**: `D:\_dongarch_build\wt\.worktrees\STRATA-RUST\strata-strata`
- **작업 브랜치**: `feat/strata-strata-rust`
- **기준 커밋 (Base SHA)**: `237d425`
- **참조 C++ 원본 앱 경로**: `D:\_dongarch_build\wt\.worktrees\JOB-010\studio\src\` ([`MainWindow.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MainWindow.cpp), [`View3D.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/View3D.cpp), [`MeshRaster.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshRaster.cpp), [`SheetView.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/SheetView.cpp), [`Plate.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Plate.cpp), [`Rubbing.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Rubbing.cpp), [`Ribbon.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Ribbon.cpp), [`MeshIO.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshIO.cpp))
- **실제 테스트 유물 데이터 세트**:
  1. 완형 세트: [`E:\034(7-실4-4)\7지구 9호-7.obj`](file:///e:/034(7-실4-4)/7지구%209호-7.obj), [`7지구 9호-7.mtl`](file:///e:/034(7-실4-4)/7지구%209호-7.mtl), [`7지구 9호-7_1.jpg`](file:///e:/034(7-실4-4)/7지구%209호-7_1.jpg), [`7지구 9호-7.ap2`](file:///e:/034(7-실4-4)/7지구%209호-7.ap2), [`E:\034(7-실4-4)\ret\`](file:///e:/034(7-실4-4)/ret)
  2. 편조각 세트: [`E:\1\1.obj`](file:///e:/1/1.obj), [`E:\1\1.mtl`](file:///e:/1/1.mtl), [`E:\1\1_1.jpg`](file:///e:/1/1_1.jpg)

---

## 2. 워크스페이스 크레이트 구조 및 파일별 보강 명세

```text
strata-strata/
├── Cargo.toml                        # 워크스페이스 루트 (lints: forbid(unsafe_code), clippy::all)
├── docs/
│   ├── DESIGN_SPEC.md                # 마스터 디자인 설계서 및 엔진 명세서 (v2.0)
│   ├── DEV_PRINCIPLES.md             # 시니어 엔지니어링 팀 개발 원칙 및 실제 창 캡처 규약
│   └── IMPLEMENTATION_PLAN.md        # 5단계 점진적 작업 순서 및 TDD 테스트 계획
└── crates/
    ├── strata-core/                  # 핵심 기하·위상·색상/디자인 토큰·에러(3단 문답)
    │   └── src/
    │       ├── error.rs              # StrataError (what_failed / why_it_happened / how_to_fix 3단 구조)
    │       ├── types.rs              # ArtifactKind, DrillType (Ia/Ib/Ib'/IIb), ScalePreset
    │       ├── mesh.rs               # TriMesh (+ vertex_uvs, triangle_uvs, vertex_colors, texture_image: Option<Arc<MeshTextureRgba>>)
    │       ├── bvh.rs                # Stackless LBVH + SubtreeArea/WeightedNormal 사전 적분
    │       └── strata_tokens.rs      # STRATA 디자인 기준 PDF 색상 토큰·타이포그래피·창 크기·UI_RULES R1~R8 상수
    ├── strata-io/                    # Artec .obj/.mtl/.jpg, .ply, .stl, STRATA .ap2, .darch 입출력
    │   └── src/
    │       ├── obj_loader.rs         # f v/vt/vn UV 파싱 + .mtl map_Kd (.jpg/.png) 텍스처 디코딩 + 동명 .ap2 정위치·절단면(18.0mm) 자동 적용
    │       ├── ply_stl.rs            # ASCII/Binary .ply 및 .stl 파서
    │       ├── ap2_codec.rs          # STRATA .ap2 (TransfomMatrix, rc.PlanePosition0, clip.PlanePosition, fc/rc/tc, wcenter) 양방향 코덱
    │       └── darch_codec.rs        # .darch 프로젝트 파일 직렬화/역직렬화
    ├── strata-orient/                # 6대 유물군 정위치 정렬 · 파단면 3분할 · 회전축/천공축/경사각 엔진
    │   └── src/
    │       ├── fracture_seg.rs       # Structure-from-Sherds++ (외면/내면/파단면 3분할)
    │       ├── circle_fit.rs         # 소각도 편조각 Pratt + Taubin + Huber LM 원호 피팅 & ±σ 신뢰구간
    │       ├── axis_solver.rs        # Huber IRLS 회전축 + 물레속선(Rills) 수평 경사각 복원
    │       ├── defect_scan.rs        # 0°~360° 전 구간 rim_gap_chart & rayon 병렬 angle_defect_score
    │       ├── bead_perforation.rs   # 구슬·옥류 관통공 축 ODR 정렬, L/D 분류, 4대 천공 단면(Ia/Ib/Ib'/IIb) 판별
    │       ├── tile_lithic_metal.rs  # 기와 와도/보강토, 석기 PCA 정렬, 부식 금속기 3층 분리
    │       └── refit_baiser.rs       # 다중 편조각 회전축 기반 파단선 디스크립터(BAISER) 가상 접합
    ├── strata-measure/               # 유물군별 정밀 계측 (0.01mm 구슬 캘리퍼스 ~ 완형/편조각/기와)
    │   └── src/metrics.rs            # 기고·복원구경±σ·잔존율·두께·구슬 상/하공경·최소내경·교차단차·L/D·체적·추정중량
    ├── strata-section/               # 월드 고정 절단틀 슬라이싱 · 미스캔 파선 추정 · Kurbo 3차 베지어 피팅
    │   └── src/
    │       ├── plane_slicer.rs       # 임의 평면(offset_mm + angle_deg) 교차 닫힌 단면 루프 체이닝 및 다중 슬롯(fc/rc/tc) 관리
    │       ├── occluded_repair.rs    # 미스캔 내벽 및 구슬 천공 내부 음영 구간 파선(Dash) 자동 보간
    │       └── bezier_fit.rs         # kurbo 기반 3차 베지어 피팅 (오차 < 0.02mm)
    ├── strata-shade/                 # 3대 정밀 탁본(Rubbing) · 5대 미분기하 특징선 · Unroll 전개 · 포목흔 FFT
    │   └── src/
    │       ├── rubbing_engine.rs     # ① 법선미분+한지 Handprint, ② 16-Scale MSII 적분불변량, ③ 양개도/음개도 이치화 + 다중 뷰 정사투영 래스터 생성기
    │       ├── feature_lines.rs      # Silhouette, Suggestive Contours, Crest Lines, Apparent Ridges, Demarcating Curves
    │       ├── unroll.rs             # 원통·원뿔·구면 360° 무왜곡 전개도
    │       └── hemp_fft.rs           # 기와 내면 포목흔 1×1 cm 2D 공간주파수 마포 올수 자동 계측
    ├── strata-export/                # 8종 표준 보고서 도판 조판(래스터+솔리드단면) · SVG(1mm=5px 앵커) · ret/ · PDF · DXF
    │   └── src/
    │       ├── plate_composer.rs     # 도법 ①~⑧ 도판 조판 (외면/내면/저면 정밀 탁본 & 실사 래스터 레이어 + #141413 솔리드 단면 폴리곤 + 중심축/수평 연결선 + 다중 뷰 + 축척자)
    │       ├── svg_illustrator.rs    # 유물제도선.ai 레이어 분리 SVG + cutline_rc0.svg 1mm=5px & 4코너 1x1px 투명 앵커
    │       ├── ret_exporter.rs       # STRATA 호환 ret/ 폴더 패키지 출력
    │       ├── pdf_tiled_print.rs    # 한 장 인쇄 & 대형 유물 1:1 다중 용지 분할 인쇄 PDF
    │       └── dxf_table_export.rs   # AutoCAD DXF R12 & 보고서 치수표 (.csv / 마크다운 / 한글 표 복사 포맷)
    └── strata-studio/                # STRATA · 디자인 기준 & UI_RULES R1~R8 네이티브 데스크톱 스튜디오
        └── src/
            ├── main.rs               # 네이티브 GUI 실행(top_down 레이아웃 격리) 및 --capture-window 실제 창 프레임버퍼 캡처
            ├── splash.rs             # 700×400 px 3.0초 지층 & 유물 실측선 드로잉 스플래시 화면
            ├── layout_r1.rs          # 5단 고정 레이아웃 + 50×50 리본 타일 1.75px(#141413/#B5573A) 전용 벡터 아이콘 렌더러
            ├── view3d_pane.rs        # 좌측 27% 진짜 3D Z-버퍼 렌더러 (솔리드 + UV .jpg 텍스처 + 실시간 정밀 탁본 + 3분류 파단면 + 상단 PlanView + Z=0 격자 + #FF0000 단면선 + 하단 0~360° 오차 띠)
            ├── sheet_pane.rs         # 우측 73% 실측 도판 캔버스 (정밀 탁본/실사 TextureHandle 합성 + Ear-Clipping #141413 먹물 솔리드 단면 + 중심축/수평 연결선 + 다중 뷰 + 축척자 + 마우스 줌/팬/두께 풍선)
            ├── dialogs.rs            # STRATA 5종 표준 창 (S 420–460 / 진행 480 / M 520 / L 860 / XL 1280×860)
            └── harness_runner.rs     # V1~V8 결정론적 검증 러너
```

---

## 3. 5단계 점진적 구현 순서 및 단계별 TDD 테스트 · 실제 창 캡처 계획

각 단계는 **① 실패하는 테스트(`#[test]`) 먼저 작성 → ② 최소·정밀 구현 → ③ `cargo test` 통과 → ④ 실제 `strata-studio.exe` 창 실행 및 화면 캡처(`ViewportCommand::Screenshot`) → ⑤ 사용자 확인 후 다음 단계 진입** 순서로 엄격히 진행합니다.

| 단계 | 작업 파일 순서 | 먼저 작성할 핵심 테스트 (`#[test]`) | 검증 명령 및 사용자 확인 기준 |
|---|---|---|---|
| **Step 1**<br>우측 73% 백지 버그 수정 & 리본 `50×50` 벡터 아이콘 | 1. [`crates/strata-studio/src/layout_r1.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/layout_r1.rs)<br>2. [`crates/strata-studio/src/main.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/main.rs)<br>3. [`crates/strata-studio/src/sheet_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/sheet_pane.rs)<br>4. [`crates/strata-studio/src/view3d_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/view3d_pane.rs) | • `test_sheet_pane_and_view3d_vertical_stack_stays_inside_window`<br>• `test_all_ribbon_tiles_have_distinct_vector_icon_kind` | • `cargo test -p strata-studio`<br>• `cargo run --release -p strata-studio -- --capture-window step1_window.png`<br>• **사용자 확인**: 실제 실행 창 캡처에서 우측 73% 도판이 백지 없이 정상 렌더링되고 상단 리본 `50×50` 타일에 `1.75px` 벡터 아이콘이 표시되는지 확인 |
| **Step 2**<br>`.obj` + `.mtl`/`.jpg` UV 텍스처 & `.ap2` 자동 연동 | 1. [`crates/strata-core/src/mesh.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-core/src/mesh.rs)<br>2. [`crates/strata-io/src/ap2_codec.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-io/src/ap2_codec.rs)<br>3. [`crates/strata-io/src/obj_loader.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-io/src/obj_loader.rs)<br>4. [`crates/strata-studio/src/main.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/main.rs) | • `test_obj_loader_parses_vt_uvs_and_mtl_jpg_texture`<br>• `test_obj_loader_auto_applies_companion_ap2_transform_and_cut_plane_18mm` | • `cargo test -p strata-core -p strata-io`<br>• `cargo run --release -p strata-studio -- --capture-window step2_window.png`<br>• **사용자 확인**: `7지구 9호-7.obj`(완형 세트)와 `1.obj`(편조각 세트)가 `.mtl`/`.jpg` 텍스처 및 `.ap2` 정위치(`125.2 × 123.9 × 63.1 mm`, 절단면 `18.0 mm`)와 함께 로드된 실제 창 캡처 확인 |
| **Step 3**<br>좌측 27% `View3DPane` 진짜 3D Z-버퍼 렌더러 | 1. [`crates/strata-studio/src/view3d_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/view3d_pane.rs)<br>2. [`crates/strata-studio/src/main.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/main.rs) | • `test_zbuffer_3d_rasterizer_produces_opaque_shaded_and_textured_pixels`<br>• `test_view3d_plan_view_and_3d_cut_plane_intersection` | • `cargo test -p strata-studio`<br>• `cargo run --release -p strata-studio -- --capture-window step3_window.png`<br>• **사용자 확인**: 좌측 27% 3D 뷰포트에 가짜 선분이 아닌 진짜 Z-버퍼 솔리드 음영, 실사 UV `.jpg` 텍스처, 실시간 정밀 탁본 셰이딩, 3분류 파단면, 상단 `PlanView` 절단선 핸들, `#FF0000` 단면선이 렌더링된 실제 창 캡처 확인 |
| **Step 4**<br>우측 73% `SheetPane` 정밀 탁본 래스터 + `#141413` 솔리드 단면 합성 | 1. [`crates/strata-shade/src/rubbing_engine.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-shade/src/rubbing_engine.rs)<br>2. [`crates/strata-export/src/plate_composer.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-export/src/plate_composer.rs)<br>3. [`crates/strata-studio/src/sheet_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/sheet_pane.rs) | • `test_ear_clipping_concave_section_polygon_triangulation`<br>• `test_plate_composer_generates_multiview_rubbing_rasters_and_solid_ink_sections` | • `cargo test -p strata-shade -p strata-export -p strata-studio`<br>• `cargo run --release -p strata-studio -- --capture-window step4_window.png`<br>• **사용자 확인**: 우측 73% 도판 용지 위에 외면·내면·저면 정밀 탁본 래스터 영상, `#141413` 먹물 솔리드 채움 단면 폴리곤, 중심축·수평 연결선, 다중 뷰, 교차 흑백 축척자가 합성된 실제 창 캡처 확인 |
| **Step 5**<br>출력 패키지 정합 및 최종 전수 검증 | 1. [`crates/strata-export/src/svg_illustrator.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-export/src/svg_illustrator.rs)<br>2. [`crates/strata-export/src/ret_exporter.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-export/src/ret_exporter.rs)<br>3. [`crates/strata-studio/src/harness_runner.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-strata/crates/strata-studio/src/harness_runner.rs) | • `test_ret_cutline_rc0_svg_matches_ap2_18mm_section`<br>• `test_zero_forbidden_term_across_workspace` | • `cargo clippy --workspace --all-targets -- -D warnings`<br>• `cargo test --workspace`<br>• 독립 서브에이전트 `git diff` 교차 감사(`X1~X4`) 및 전역 규칙 6장 보고 |
