# STRATA · 동국유물실측 (`strata-arch3d`) 3단계 상세 구현 및 TDD 테스트 계획서

> [!IMPORTANT]
> 본 문서는 전역 개발 규칙 **3단계(계획: 파일별 작업 순서와 먼저 쓸 테스트를 적는다)** 및 사용자의 지시(*"개발원칙 기록 후 개발 진행하라. 시니어 개발자들이 팀을 이뤄 개발하듯이 모든 것을 검증해가며 하나씩 해나가라"*)에 따라 수립된 실행 계획서입니다.

---

## 1. 작업 격리 환경 및 기준 커밋 정보
- **격리 Git Worktree 경로**: `D:\_dongarch_build\wt\.worktrees\STRATA-RUST` (편의를 위해 `D:\gigamesh\strata-arch3d`와 동기화)
- **작업 브랜치**: `feat/strata-arch3d-rust`
- **기준 커밋 (Base SHA)**: `e1cdd5b35a45b0eeb2a2fe13d727402f6aa118e1`
- **Rust 툴체인**: `rustc 1.97.1 (8bab26f4f 2026-07-14)` / `cargo 1.97.1`

---

## 2. 워크스페이스 크레이트 구조 및 파일별 작업 순서

```text
strata-arch3d/
├── Cargo.toml                        # 워크스페이스 루트 (lints: forbid(unsafe_code), clippy::all)
├── docs/
│   ├── DEV_PRINCIPLES.md             # 시니어 엔지니어링 팀 개발 원칙 및 V1~V8 / X1~X4 하네스 규약
│   └── IMPLEMENTATION_PLAN.md        # 파일별 작업 순서 및 TDD 테스트 계획
└── crates/
    ├── arch3d-core/                  # [Phase 1] 핵심 기하·위상·색상/디자인 토큰·에러(3단 문답)
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── error.rs              # Arch3dError (what_failed / why_it_happened / how_to_fix 3단 구조)
    │       ├── types.rs              # ArtifactKind (완형/편5종/구슬6종/기와/석기/금속), DrillType (Ia/Ib/Ib'/IIb), ScalePreset
    │       ├── mesh.rs               # TriMesh, CompressedCornerTable (opposite_half_edge), OctahedralNormal16
    │       ├── bvh.rs                # Stackless LBVH + SubtreeArea/WeightedNormal 사전 적분 (O(log N) 구형 쿼리)
    │       └── strata_tokens.rs      # STRATA 디자인 기준 PDF 색상 토큰·타이포그래피·창 크기·UI_RULES R1~R8 상수
    ├── arch3d-io/                    # [Phase 1] Artec .obj/.mtl/.jpg, .ply, .stl, Arch3D .ap2, .darch 입출력
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── obj_loader.rs         # 대용량 .obj/.mtl/.jpg 스트리밍 파서 (한글 경로·다각형 삼각분할·법선 복원)
    │       ├── ply_stl.rs            # ASCII/Binary .ply 및 .stl 파서
    │       ├── ap2_codec.rs          # Arch3D Liner .ap2 (Qt QVariantMap [type_id, val...] TransfomMatrix/fc/rc/tc/wcenter) 양방향 코덱
    │       └── darch_codec.rs        # .darch 프로젝트 파일 직렬화/역직렬화 및 히스토리 클램핑
    ├── arch3d-orient/                # [Phase 2] 6대 유물군 정위치 정렬 · 파단면 3분할 · 회전축/천공축/경사각 엔진
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── fracture_seg.rs       # Structure-from-Sherds++ (이면각 >35° + 주곡률 + 법선편차 -> 외벽/내벽/파단면 3분할)
    │       ├── circle_fit.rs         # 소각도 편조각 바이어스 제거 Pratt + Taubin + Huber LM 원호 피팅 & ±σ 신뢰구간
    │       ├── axis_solver.rs        # 완형/편조각 Huber IRLS 법선 공면성 회전축 + 물레속선(Rills) 수평 경사각 복원
    │       ├── defect_scan.rs        # 0°~360° 전 구간 rim_gap_chart & rayon 병렬 angle_defect_score (C++ 버그 #1, #2 완치)
    │       ├── bead_perforation.rs   # 구슬·옥류 관통공 축 ODR 정렬, L/D 비율 분류, 4대 천공 단면(Ia/Ib/Ib'/IIb 교차단차) 자동 판별
    │       ├── tile_lithic_metal.rs  # 기와 와도/보강토 정렬, 석기 관성텐서 PCA 정렬, 부식 금속기 원심/부식층/목질흔 분리
    │       └── refit_baiser.rs       # 다중 편조각 회전축 기반 파단선 디스크립터(BAISER) 가상 접합 후보 탐색
    ├── arch3d-measure/               # [Phase 2] 유물군별 정밀 계측 (0.01mm 구슬 캘리퍼스 ~ 대형 기와/옹기)
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       └── metrics.rs            # 기고·복원구경±σ·잔존율·두께·구슬 상/하공경·최소내경·교차단차·L/D·체적·추정중량
    ├── arch3d-section/               # [Phase 3] 월드 고정 절단틀 슬라이싱 · 미스캔 파선 추정 · Kurbo 3차 베지어 피팅
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── plane_slicer.rs       # 고정 절단면(P_cut) vs 유물 회전(ψ) 교차 루프 체이닝 및 다중 슬롯(fc/rc/tc) 관리
    │       ├── occluded_repair.rs    # 미스캔 내벽 및 구슬 천공 내부 음영 구간 파선(Dash) 자동 보간
    │       └── bezier_fit.rs         # kurbo 기반 3차 베지어 피팅 (오차 < 0.02mm) 및 원본 폴리라인 비교
    ├── arch3d-shade/                 # [Phase 3] 3대 디지털 탁본(정밀 탁본) · 5대 미분기하 특징선 · Unroll 전개 · 포목흔 FFT
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── rubbing_engine.rs        # ① 법선미분+한지 Handprint, ② 16-Scale MSII 적분불변량, ③ 양개도/음개도 이치화
    │       ├── feature_lines.rs      # Silhouette, Suggestive Contours, Crest Lines, Apparent Ridges, Demarcating Curves
    │       ├── unroll.rs             # 원통(Cylinder)·원뿔(Cone)·구면(Sphere) 360° 무왜곡 전개도
    │       └── hemp_fft.rs           # 기와 내면 포목흔 1×1 cm 2D 공간주파수 분석 기반 마포 올수(N×M 올/cm²) 자동 계측
    ├── arch3d-export/                # [Phase 4] 8종 표준 보고서 도판 조판 · SVG(1mm=5px 앵커) · ret/ · 분할인쇄 PDF · DXF · 치수표
    │   ├── Cargo.toml
    │   └── src/
    │       ├── lib.rs
    │       ├── plate_composer.rs     # 도법 ①~⑧ 표준 보고서 도판 자동 조판 (완형 D5, 편 3단 병렬, 구슬 4대 천공, 기와, 석기, 금속 3층)
    │       ├── svg_illustrator.rs    # 유물제도선.ai 레이어 분리 SVG + cutline_rc0.svg 1mm=5px & 4코너 1x1px 투명 앵커
    │       ├── ret_exporter.rs       # Arch3D 호환 ret/ 폴더 패키지 (cutline_*.svg, rubbing_*.png/.svg/.txt) 출력
    │       ├── pdf_tiled_print.rs    # 한 장 인쇄 & 대형 유물 1:1 다중 용지 분할 인쇄 (10mm 겹침·자르는 점선·장 번호·5cm 확인선)
    │       └── dxf_table_export.rs   # AutoCAD DXF R12 & 보고서 치수표 (.csv / 마크다운 / 한글 표 복사 포맷)
    └── arch3d-studio/                # [Phase 5] STRATA · 디자인 기준 & UI_RULES R1~R8 네이티브 데스크톱 스튜디오 + 하네스 러너
        ├── Cargo.toml
        └── src/
            ├── main.rs               # GUI 실행 및 --verify-all / --export-golden / --screenshot-tour CLI 하네스 진입점
            ├── app.rs                # StrataStudioApp 상태 머신 (스플래시 -> 홈 -> ①바닥 -> ②자르기 -> ③편집 -> 내보내기)
            ├── splash.rs             # 700×400 px 3.0초 지층 & 유물 실측선 드로잉 스플래시 화면
            ├── layout_r1.rs          # 5단 고정 레이아웃 (탭 34px · 리본 98px 50×50 타일 · ctxBar 36px · 176px+27%+73% · 상태줄 30px)
            ├── view3d_pane.rs        # 3D 보조창 (고정 절단틀, 동심원 스코프, 파단면 분리색상, X-Ray 천공/원심 뷰, 0°~360° 오차 띠)
            ├── sheet_pane.rs         # 우측 73% (Tab 100%) 실측 도판 캔버스 (도법 ①~⑧ 실시간 렌더링, R 반전, 방안 눈금, 치수 대장 띠)
            ├── dialogs.rs            # STRATA 5종 표준 창 (S 420–460 3단 오류·닫기확인 / 진행 480 / M 520 / L 860×720 검수·860×560 분할인쇄 / XL 1280×860 편집판)
            └── harness_runner.rs     # V1~V8 결정론적 검증 및 헤드리스 도판/UI 스냅샷 렌더러
```

---

## 3. 단계별(Phase 1 ~ Phase 5) 먼저 작성할 실패 테스트(`Red`) 및 하네스 검증 명세

| 단계 | 먼저 작성할 핵심 테스트 이름 (`#[test]`) | 검증할 내용 및 통과 기준 (`V1~V8`) |
|---|---|---|
| **Phase 1**<br>(`arch3d-core`, `arch3d-io`) | • `test_corner_table_and_lbvh_subtree_area`<br>• `test_strata_tokens_and_3part_error_contract`<br>• `test_load_real_e_drive_1obj_and_7_9_7_obj`<br>• `test_ap2_qvariant_roundtrip_7_9_7` | • `E:\1\1.obj` (`149,047` 정점 / `298,090` 면) 및 `E:\034(7-실4-4)\7지구 9호-7.obj` (`115,610` 정점) 실제 로드 성공<br>• `7지구 9호-7.ap2`의 `TransfomMatrix [81, ...]` 16개 부동소수점 및 `BBLengthX/Y/Z` (`125.174 × 123.935 × 63.133 mm`), `fc/rc/tc` 5슬롯, `wcenter` 100% 정합 복원 (`V1`, `V2`) |
| **Phase 2**<br>(`arch3d-orient`, `arch3d-measure`) | • `test_rim_gap_chart_covers_full_360_degrees`<br>• `test_parallel_angle_defect_score_varies_by_angle`<br>• `test_sherd_fracture_3way_segmentation_excludes_fracture`<br>• `test_small_arc_circle_fit_unbiased_with_confidence`<br>• `test_body_sherd_rills_stance_recovery`<br>• `test_bead_ld_classification_and_4_drill_types`<br>• `test_corroded_metal_3layer_and_lithic_diacritical` | • 구연 오차 차트가 `112°`에서 끊기지 않고 `0°~360°`(180빈) 전체를 반환(`V3`)<br>• 파단면 정점이 원 피팅에서 100% 배제되어 $\Delta\theta = 25^\circ$ 소형 구연편에서도 반지름 오차 `< 0.25 mm` 달성(`V4`)<br>• 구슬 $L/D$ 분류(`환옥/소옥/절구옥/관옥/곡옥`) 및 천공 `Ⅰa / Ⅰb / Ⅰb' / Ⅱb`(중앙 교차단차 $\Delta e$) 판별, `0.01 mm` 정밀도 검증(`V5`) |
| **Phase 3**<br>(`arch3d-section`, `arch3d-shade`) | • `test_world_locked_plane_slice_on_real_1obj`<br>• `test_occluded_inner_wall_and_drill_dashed_repair`<br>• `test_kurbo_bezier_fit_sub_002mm_accuracy`<br>• `test_탁본_3_algorithms_and_5_feature_lines`<br>• `test_cylindrical_unroll_and_hemp_fft_thread_count` | • `1.obj` 및 `7지구 9호-7.obj` 실제 메쉬 절단 프로파일 추출 및 `kurbo` 3차 베지어 오차 `< 0.02 mm`<br>• `16-Scale MSII`, 법선 미분 + 한지 `Handprint`, `Demarcating Curves` 문양 단차선, 기와 포목흔 `1×1 cm` 올수(`8×9올` 등) 정확 계측(`V6`) |
| **Phase 4**<br>(`arch3d-export`) | • `test_plate_composer_all_8_archaeological_modes`<br>• `test_svg_1mm_5px_and_4corner_anchors_match_ret`<br>• `test_tiled_1to1_pdf_and_dxf_r12_export` | • 도법 ①~⑧ 도판 조판 결과에 `H=95.0mm #6b4fc0` 하드코딩 0건<br>• 출력 SVG가 `E:\034(7-실4-4)\ret\cutline_rc0.svg`와 동일하게 `1 mm = 5 px` 스케일 및 네 모서리 `1×1 px` 투명 정렬 앵커를 포함하고 `유물제도선.ai` 레이어 구조를 준수(`V7`) |
| **Phase 5**<br>(`arch3d-studio`) | • `test_ui_rules_r1_to_r8_and_strata_dialog_metrics`<br>• `test_splash_700x400_and_3part_error_dialog`<br>• `test_headless_golden_export_and_plate_snapshots` | • `UI_RULES.md` R1(`34/98/36/176/27%/73%/30px`), R2(지정 토큰 외 임의 색상 0건), R5(`D5` 외면 좌·단면 우 및 `R` 반전), `STRATA` 5종 창 크기(`420~460 / 480 / 520 / 860 / 1280×860`), `700×400` 지층 스플래시 검증(`V8`)<br>• 독립 서브에이전트 `X1~X4` 교차 감사 결함 0건 |
