# STRATA · 동국유물실측 (DongArch3D) Rust 마스터 디자인 설계서 및 엔진 명세서 (v2.0 전면 개정판)

> [!IMPORTANT]
> **문서 지위 및 절대 수칙 (어기면 즉시 중단)**:
> 1. **용어 철칙**: 문서·코드·UI·주석·식별자 어디에도 금지어 영문 축약어를 절대 쓰지 않으며, 반드시 한국 고고학 표준 용어인 **'정밀 탁본' (`rubbing` / `Rubbing`)**만 사용합니다.
> 2. **기존 기능 보존 및 C++ 1:1 계승 철칙**: 이미 구현된 8개 크레이트(`arch3d-core`, `arch3d-io`, `arch3d-orient`, `arch3d-measure`, `arch3d-section`, `arch3d-shade`, `arch3d-export`, `arch3d-studio`)의 기하 알고리즘과 테스트를 삭제·축약하지 않으며, 현행 C++ 원본 앱([`MainWindow.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MainWindow.cpp), [`View3D.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/View3D.cpp), [`MeshRaster.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshRaster.cpp), [`SheetView.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/SheetView.cpp), [`Plate.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Plate.cpp), [`Rubbing.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Rubbing.cpp), [`Ribbon.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Ribbon.cpp), [`MeshIO.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshIO.cpp))의 실제 렌더링·UI 파이프라인을 1:1로 계승합니다.
> 3. **단계별 실제 실행 창(`arch3d-studio.exe`) 캡처 검증 철칙**: 가짜 오프스크린 사각형 드로잉 함수로 검증을 대체하는 행위를 전면 금지합니다. 반드시 **1단계씩 실제 네이티브 실행 창(`arch3d-studio.exe`)의 화면을 캡처**하여 사용자 확인을 받은 뒤 다음 단계로 넘어갑니다.

---

## 0. 요청 정리 및 이전 실패 원인 진단

### 0.1 무엇을 왜 하는가
1. **목표**: 현행 C++ 기반 [`DongArch3DStudio`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MainWindow.cpp)의 검증된 작업 흐름([`UI_RULES.md`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/docs/ui/UI_RULES.md) R1~R8)과 **`STRATA · 디자인 기준` (`Strata_UI·UX_개선안.pdf`)**의 시각 체계를 100% 계승하고, 실제 발굴 유물 데이터 세트인 **완형 세트([`7지구 9호-7.obj`](file:///e:/034(7-실4-4)/7지구%209호-7.obj), [`.mtl`](file:///e:/034(7-실4-4)/7지구%209호-7.mtl), [`.ap2`](file:///e:/034(7-실4-4)/7지구%209호-7.ap2), [`ret/`](file:///e:/034(7-실4-4)/ret))** 및 **편조각 세트([`1.obj`](file:///e:/1/1.obj), [`.mtl`](file:///e:/1/1.mtl), [`1_1.jpg`](file:///e:/1/1_1.jpg))**가 3D 뷰포트와 2D 실측 도판에서 완벽히 로드·렌더링·출판되는 **고고학 유물 실측 전용 Rust 네이티브 애플리케이션(`STRATA · 동국유물실측`)**을 완성합니다.
2. **이전 구현이 실패했던 5대 근본 원인과 본 설계서의 해결책**:

| 번호 | 이전 구현의 치명적 결함 | 발생 원인 (코드 및 구 설계도 결함) | 본 개정 설계서(v2.0)의 아키텍처 해결책 |
|---|---|---|---|
| **1** | **우측 73% 도판 영역(`SheetPane`) 백지 버그** | [`main.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/main.rs)의 `ui.horizontal_top` 내부에서 `ui.vertical` 없이 `allocate_ui`를 호출하여, [`sheet_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/sheet_pane.rs)의 헤더·치수띠·도판 캔버스가 가로(`left_to_right`)로 배치되면서 메인 도판이 화면 오른쪽 밖(`x > window_width`)으로 밀려남 | `View3DPane`(27%)과 `SheetPane`(73%) 컨테이너 모두 `ui.allocate_ui_with_layout(..., egui::Layout::top_down(egui::Align::Min), ...)`으로 강제하여 상단 헤더(`32px`) → 치수띠(`28px`) → 도판 캔버스(남은 높이 100%)가 수직으로 정확히 배치되도록 명세 (§3.3, §4.3) |
| **2** | **좌측 27% `View3DPane`이 점/선 1,600개만 찍는 가짜 와이어프레임** | 구 설계도에 3D 래스터라이저 명세가 빠져 [`view3d_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/view3d_pane.rs)에서 `mesh.triangles.len() / 1600`으로 건너뛰며 선분 2개씩만 그림 | C++ [`MeshRaster.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshRaster.cpp) 및 [`View3D.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/View3D.cpp)를 1:1 계승한 **멀티스레드(`rayon`) Z-버퍼 솔리드 래스터라이저(실사 UV `.jpg` 텍스처 + 양면 조명 + 실시간 정밀 탁본 셰이딩 + X-Ray + 3분류 파단면 컬러 + 상단 `PlanView` 절단선 핸들 + `Z=0` 격자 + `#FF0000` 3D 단면선)** 명세 (§4.2) |
| **3** | **우측 2D 도판(`SheetPane`)에 정밀 탁본 래스터 영상과 `#141413` 먹물 채움 단면 누락** | 구 설계도가 도판을 단순 선분(`PlatePathElement`) 나열로만 정의하여 래스터 합성 및 오목 다각형 삼각분할 채움이 빠짐 | C++ [`SheetView.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/SheetView.cpp) · [`Plate.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Plate.cpp) 및 [`ret/`](file:///e:/034(7-실4-4)/ret) 정답처럼 **① 외면·내면·상/하면 정밀 탁본 & 실사 텍스처 래스터 이미지(`egui::TextureHandle`) 합성, ② Ear-Clipping 삼각분할 기반 `#141413` 먹물 솔리드 채움 단면 폴리곤, ③ 중심축·수평 연결선(Bridge line), ④ 다중 뷰, ⑤ 정밀 축척자 및 줌/팬/두께 풍선** 명세 (§4.3) |
| **4** | **`.obj` 로드 시 동반 `.mtl`/`.jpg`(UV 텍스처) 및 `.ap2`(정위치·절단면) 자동 연동 누락** | [`obj_loader.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-io/src/obj_loader.rs)가 `f v/vt/vn`에서 정점 좌표만 읽고 `vt`와 `.mtl` 이미지를 버렸으며, 동명 `.ap2` 자동 적용 파이프라인이 없었음 | `.obj` 로드 시 `vt` UV 인덱스 및 `.mtl` `map_Kd` 텍스처([`7지구 9호-7_1.jpg`](file:///e:/034(7-실4-4)/7지구%209호-7_1.jpg), [`1_1.jpg`](file:///e:/1/1_1.jpg))를 자동 디코딩·샘플링하고, 동명의 [`.ap2`](file:///e:/034(7-실4-4)/7지구%209호-7.ap2)가 있으면 `TransfomMatrix`와 저장된 절단면(`rc.PlanePosition0 = 18.0 mm` 등)을 즉시 자동 적용하도록 명세 (§4.1) |
| **5** | **상단 리본(`TileRibbon`) `50×50` 타일에 벡터 아이콘 누락** | [`layout_r1.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/layout_r1.rs)에서 타일 안에 알파벳 약자(`O`, `S` 등)만 출력함 | `Strata_UI·UX_개선안.pdf` 및 C++ [`Ribbon.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Ribbon.cpp) 기준대로 모든 `50×50` 타일 안에 **`1.75px` 흑연(`#141413`) 벡터 아이콘 + `#B5573A` 테라코타 포인트 + 그룹 구분선(`1px #DEDCD1`) + 하단 묶음 이름(`11px`)**을 그리도록 전 타일 아이콘 도형 명세 (§3.4) |

---

## Part 1. 기준 데이터셋(`E:\`) 및 현행 C++ `DongArch3DStudio` 계승 명세

### 1.1 필수 완벽 지원 실측 유물 데이터 세트 (`E:\` Golden-Master Datasets)

본 애플리케이션은 시작 시 및 파일 열기 시 아래 두 가지 실제 발굴 유물 세트를 오류 없이 즉시 로드하고, 정답 파일들과 시각적·수치적으로 1:1 정합되어야 합니다.

| 구분 | 파일 구성 및 실제 스펙 | 정위치 · 절단면 · 도판 정답 기준 |
|---|---|---|
| **① 완형 토기·뚜껑 세트**<br>(`E:\034(7-실4-4)\`) | • 메쉬: [`7지구 9호-7.obj`](file:///e:/034(7-실4-4)/7지구%209호-7.obj) (`23.2 MB`, 정점 `115,610`개, 삼각형 `231,216`개, `vt` UV 좌표 포함)<br>• 재질·텍스처: [`7지구 9호-7.mtl`](file:///e:/034(7-실4-4)/7지구%209호-7.mtl) (`map_Kd 7지구 9호-7_1.jpg`) + [`7지구 9호-7_1.jpg`](file:///e:/034(7-실4-4)/7지구%209호-7_1.jpg) (`566 KB` 실사 UV 텍스처)<br>• 프로젝트 메타: [`7지구 9호-7.ap2`](file:///e:/034(7-실4-4)/7지구%209호-7.ap2) (`Qt QVariantMap` JSON)<br>• 정답 출력폴더: [`E:\034(7-실4-4)\ret\`](file:///e:/034(7-실4-4)/ret) | • `.ap2` 내 `TransfomMatrix` (`[81, -0.7850..., 0.1456..., -0.6020..., 0, -0.2586..., -0.9602..., -0.1045..., 0, -0.5628..., 0.2379..., 0.7915..., 0, -7.8328, 30.8966, -152.9765, 1]`) 자동 적용 시 바운딩 박스 **`125.174 × 123.935 × 63.133 mm`** 정위치 복원<br>• `.ap2` 내 활성 절단면: `rc.IsSaved0 = true`, **`rc.PlanePosition0 = 18.0 mm`**, `clip.PlanePosition = 18.0 mm`, `rc.CanvasRect = [0, 0, 625.871, 619.675]`, `rc.CanvasRect_CutLine = [0, 0, 619.675, 315.665]`<br>• [`ret/cutline_rc0.svg`](file:///e:/034(7-실4-4)/ret/cutline_rc0.svg) (`625.87 × 315.665 pt`, `1 mm = 5 px`, `#FF0000` 3차 베지어 단면선 + 4코너 `1×1 px` 투명 정렬 앵커) 및 `ret/` 내 정면·저면 정밀 탁본 PNG 영상과 1:1 일치 |
| **② 편조각 세트**<br>(`E:\1\`) | • 메쉬: [`1.obj`](file:///e:/1/1.obj) (`20.4 MB`, 정점 `149,047`개, 삼각형 `298,090`개, `vt` UV 좌표 포함)<br>• 재질·텍스처: [`1.mtl`](file:///e:/1/1.mtl) (`map_Kd 1_1.jpg`) + [`1_1.jpg`](file:///e:/1/1_1.jpg) (`47.0 MB` 초고해상도 실사 UV 텍스처) | • `Structure-from-Sherds++` 이면각(`>35°`) 및 곡률 거칠기 기반 **3분류(`외면` / `내면` / `파단면`)** 자동 분리<br>• 파단면 정점을 원 피팅 및 축 계산에서 100% 배제하고, 원래 기벽 경사각(Stance) 정렬 후 **외면 정밀 탁본 · `#141413` 솔리드 회전단면 · 내면 정밀 탁본** 및 실사 UV 텍스처 렌더링 |

---

### 1.2 현행 C++ `DongArch3DStudio`에서 반드시 고쳐 계승하는 5대 알고리즘 개선점

| 번호 | 현행 C++ 코드 위치 | 현행 C++ 한계점 | Rust 엔진(`strata-arch3d`) 해결 완료 사항 |
|---|---|---|---|
| **1** | [`ArtifactDoc.cpp:2721`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/ArtifactDoc.cpp#L2721) | `rimGapChart()`가 `112°`까지만 하드코딩되어 112°~360° 구연 접지 오차가 잘림 | `0°~360°` 전 구간(2° 간격 180빈) 스캔 + 편조각 잔존 호 구간 $[\theta_{\min}, \theta_{\max}]$ 표시 (`arch3d-orient::defect_scan`) |
| **2** | [`ArtifactDoc.cpp:2798-2810`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/ArtifactDoc.cpp#L2798-L2810) | `angleDefectScore()`가 각도와 무관한 고정 분산을 계산하고 72개 각도를 단일 스레드로 순회 | `rayon` 병렬 슬라이싱으로 각도별 실제 구연/바닥 도달 결손·스캔 공백률·두께 연속성을 평가하여 추천 각도 산출 |
| **3** | [`SheetView.cpp:243-256`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/SheetView.cpp#L243-L256) | 유물 크기와 무관하게 높이 `H = 95.0 mm`에 보라색 점(`#6b4fc0`)을 찍는 하드코딩 | 하드코딩 완전 제거, 실제 최대경 높이 또는 사용자 지정 계측 높이에서만 `STRATA` 표준 토큰(`#B5573A`)으로 표시 |
| **4** | [`MeshSlicer.h`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/GigaMesh/gui/src/dongarch/karch/MeshSlicer.h) | 편조각 파단면과 원래 내·외벽을 분리하지 않아 원 피팅 시 복원구경이 왜곡됨 | 이면각($>35^\circ$) 능선 + 곡률 거칠기 기반 **3분류(`외면` / `내면` / `파단면`)** 및 `Pratt + Taubin + Huber IRLS` 원호 피팅 |
| **5** | [`Plate.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Plate.cpp) | 구슬·옥류(`Ⅰa·Ⅰb·Ⅰb'·Ⅱb` 4대 천공, `2:1~10:1` 초확대 축척) 및 동체편 3단 병렬 도판 미지원 | **6대 유물군 전용 기하 엔진 + 8종 표준 보고서 도판 조판기** 탑재 |

---

## Part 2. 전 유물군(완형·편조각 5종·구슬/옥류 4종·기와·석기·금속) 기하·도판 엔진 명세

### 2.1 편조각(Potsherds) 파단면 3분할 및 복원 엔진 (`arch3d-orient` & `arch3d-section`)
1. **파단면(Fracture Surface) 3분할 (`fracture_seg.rs`)**:
   - 인접 삼각형 법선 이면각($> 35^\circ$) 능선 그래프와 정점 곡률 거칠기($\sigma_\kappa$), 두께 방향 법선 부호를 결합하여 모든 메쉬 삼각형과 정점을 **`외면(Outer)`**, **`내면(Inner)`**, **`파단면(Fracture)`** 3개 영역으로 자동 분할합니다.
   - 3D 뷰포트(`View3DPane`)에서 `파단면 표시`를 켜면 외면(청록/점토), 내면(황토), 파단면(테라코타 적갈색 `#B5573A`)이 즉시 구분 렌더링되며, 회전축 및 복원구경 계산 시 파단면은 100% 배제됩니다.
2. **소각도 원호 피팅(`circle_fit.rs`) 및 물레속선 경사각 복원(`axis_solver.rs`)**:
   - 잔존 호 $\Delta\theta \ge 45^\circ$(완전 복원 도판), $20^\circ \le \Delta\theta < 45^\circ$(추정 파선 복원 도판), $\Delta\theta < 20^\circ$ 또는 동체부편(물레속선 `Rills` 수평 정렬 후 **『외면 정밀 탁본 ── `#141413` 회전단면 ── 내면 정밀 탁본』 3단 병렬 도판**)으로 자동 조판합니다.

### 2.2 구슬·옥류(Beads & Jade) $L/D$ 분류 및 4대 천공 단면(`Ⅰa·Ⅰb·Ⅰb'·Ⅱb`) 엔진
1. **치수비율($L/D$) 세분류**: `환옥`($L/D \ge 0.95$), `소옥`($0.46 \le L/D \le 0.94$), `절구옥`($L/D \le 0.45$), `관옥`($L/D \ge 1.5$), `곡옥`(C자 비대칭), `다면옥`(다각 연마면).
2. **한국 고고학 표준 4대 천공 단면(`Ⅰa` 원통형 일방향, `Ⅰb` 원추형 일방향, `Ⅰb'` 미관통 맹공, `Ⅱb` 양방향 맞조르기 모래시계형 + 중앙 교차 단차 $\Delta e$ mm)** 자동 판별 및 `2:1 ~ 10:1` 초확대 축척(`0.01 mm` 단위) 지원.

### 2.3 기와·막새(포목흔 FFT), 석기(박리 선후 관계), 부식 금속기(3층 분리)
1. **기와·막새**: 드림새 접합 경계 파선·보강토 단차, 배면 타날문 및 내면 포목흔 원통 전개(`Unroll`), `1×1 cm` 2D FFT 마포 올수(`N×M 올/cm²`) 자동 계측.
2. **석기**: 외곽선(`0.35mm`) · 박리 능선(`0.20mm`) · 타격 파동(`0.10mm`) 3단계 위계 및 능선 직전 단절(`Gap`) 기반 박리 선후 관계(`Diacritical Diagram`).
3. **부식 금속기**: ① 본래 금속 원심(실선·먹물 채움), ② 부식 팽창층(파선), ③ 고착 목질흔·직물흔(세선) 3층 분리 제도.

---

## Part 3. `STRATA · 디자인 기준` & `UI_RULES.md` R1~R8 초정밀 UI/UX 규격

### 3.1 색상 토큰 체계 (`strata_tokens.rs` — 지정 토큰 외 임의 색상 사용 금지)

| 토큰 이름 | 식별자 | HEX | 적용 위치 및 규칙 |
|---|---|---|---|
| **종이** | `GROUND` | `#FAF9F5` | 화면 전체 바탕, 창 머리, 대화상자 하단 버튼 줄(`58px`), 스플래시 바탕, 3D 뷰 배경 상단 |
| **옅은 종이** | `WASH` | `#F5F4ED` | 좌측 슬롯 판(`176px`), 지금 도구 줄(`ctxBar 36px`), 도판 상단 치수 대장 띠(`28px`), 3D `PlanView` 배경 |
| **눌린 면** | `DESK` | `#F0EEE6` | 리본 타일(`50×50`) 기본/올림 면, 보조 컨트롤 바탕 |
| **흰 면** | `SURFACE` | `#FFFFFF` | **실측 도판 용지(A4/A3)**, 탭 줄(`34px`), 리본 바(`98px`), 대화상자 본문 |
| **먹** | `INK` | `#141413` | 본문 주 글자, **리본 타일 `1.75px` 벡터 아이콘 주 선**, **단면 폴리곤 솔리드 채움(`#141413`)**, 도판 외곽선 |
| **보조 글자** | `MUTED` | `#5E5D59` | 설명 문구, 리본 하단 묶음 이름(`11px`), 보조 안내 |
| **흐린 글자** | `FAINT` | `#73726C` | 칸 이름, 수치 뒤 단위(`mm`, `°`, `%`), 비활성 요소 |
| **구분선 / 테두리** | `LINE` / `EDGE` | `#DEDCD1` / `#C2C0B6` | 영역·리본 그룹 세로 구분선(`1px #DEDCD1`) / 버튼·도판 용지 테두리(`1px #C2C0B6`) |
| **짙은 흙 (큰 행동)** | `ACTION` | `#B5573A` | **화면에 단 하나만 있는 큰 행동 타일/버튼**, 리본 벡터 아이콘 강조 포인트, 3D 절단선 핸들 |
| **흙빛 글자 / 켜진 타일** | `CLAY_INK` / `CLAY_WASH` | `#9C4A2F` / `#F4E3DA` | 켜진(Active/On) 리본 타일 아이콘·글자(`#9C4A2F`) 및 타일 바탕(`#F4E3DA`) |
| **손에 든 도구** | `HAND` | `#30302E` | 현재 마우스 커서에 들려 있는 단일 인터랙티브 도구 타일 바탕(글자 `#FAF9F5`) |
| **실측 단면선** | `PROFILE_RED` | `#FF0000` | 3D 뷰 절단 교차선 및 2D 도판 실측 단면 윤곽선 (`cutline_rc0.svg` 호환 순적색) |
| **도형 선택** | `SELECT_BLUE` | `#2A78D6` | 선택된 제어점·보조선 및 치수선 강조 |
| **방안 눈금** | `GRID_1MM` / `GRID_10MM` | `#EDEBE2` / `#D5D2C6` | 도판 용지 내부 1 mm 세선(`0.75px`) / 10 mm 주선(`1.0px`) |

---

### 3.2 글꼴(Typography) 위계 규격 (`Malgun Gothic` · `Batang` · `Consolas`)
- **T1 화면 큰 제목 (`36px` 명조)**: 시작 화면 메인 타이틀
- **T2 유물·카드 제목 (`21–26px` 명조)**: 도판 상단 유물 제목
- **T3 창 안 핵심 문장 (`15px Bold` 고딕)**: 대화상자 본문 첫 줄 핵심 질문·상태
- **T4 본문·버튼·리본 타일 이름 (`13px` 고딕)**: 모든 버튼 글자, `50×50` 리본 타일 아래 이름
- **T5 보조 글·칩·맥락 안내 (`12px` 고딕)**: 상태 칩(`22px`), `ctxBar` 안내문
- **T6 리본 묶음 이름·칸 이름 (`11px Bold` 고딕, `#5E5D59`)**: 리본 그룹 하단 묶음 제목(`파일`, `유물 종류 · 정위치` 등)
- **T7 슬롯·레이어 목록 (`10–12px` 고딕)**: 좌측 `176px` 단면 슬롯 목록
- **T8 좌표·축척·치수·단축키 (`12px` 고정폭 `Consolas`)**: 치수 대장 띠 수치, 상태줄 `R / H / 두께`, 각도

---

### 3.3 전체 화면 5단 고정 구조 (`UI_RULES.md` R1~R8 + `egui` 레이아웃 불변 규칙)

```text
+==================================================================================================================+
| [영역 1: 탭 줄 (34px 고정, #FFFFFF)]                                                                             |
|  ❖ 동국유물실측  |  파일   홈   ① 바닥 정하기   ② 막으로 자르기   ③ 도면 편집   내보내기          [↶ 되돌리기] [↷ 다시] |
+------------------------------------------------------------------------------------------------------------------+
| [영역 2: 타일 리본 (98px 고정, #FFFFFF, 그룹 사이 1px #DEDCD1 세로선, 50×50 타일 내 1.75px 벡터 아이콘 + 13px 이름)] |
|  +---------------+---------------------------+----------------------------+--------------------+---------------+ |
|  | [열기] [저장]  | [유물종류] [자동세우기]    | [단면선] [외곽선] [단면추가] | [정밀탁본][실사텍스처]| [도면·내보내기]| |
|  |               | [바닥수평] [구연수평]      | [좌우반전]                  | [파단면]  [방안눈금] | (#B5573A 큰행동| |
|  |     파일      |   유물 종류 · 정위치 맞춤  |      실측선 · 다듬기       |   도판 · 3D 표시   |    내보내기   | |
|  +---------------+---------------------------+----------------------------+--------------------+---------------+ |
+------------------------------------------------------------------------------------------------------------------+
| [영역 3: 지금 도구 줄 ctxBar (36px 고정, #F5F4ED Wash — 활성 도구 맥락 컨트롤 바)]                                  |
|  ✂ 막으로 자르기  |  [고정절단틀 · 유물 회전]  [−1°] [  0.0° ] [+1°]  [★추천1] [추천2] [훑기]  [지금 각도 고정 Enter] |
+------------------+-----------------------------------+-----------------------------------------------------------+
| [영역 4: 작업 공간 (남은 높이 100% — 내부 3개 열 모두 반드시 egui::Layout::top_down 수직 레이아웃으로 격리!)]         |
|                  |                                   |                                                           |
| [슬롯 목록 176px]| [좌측 3D 뷰포트 (View3DPane) 27%] | [우측 실측 도판 (SheetPane) 73% / Tab 누르면 100%]         |
| (#F5F4ED Wash)   | +-------------------------------+ | +-------------------------------------------------------+ |
|                  | | [상단 평면도 PlanView (124px)] | | | [도판 머리 32px] 7지구 9호-7 (완형/뚜껑) [✓ 정위치 (.ap2)]| |
| 유물 · 단면 목록 | |  • 위(+Z)에서 본 유물 윤곽     | | +-------------------------------------------------------+ |
|                  | |  • A–A' 절단선 회전 핸들       | | | [치수 대장 띠 28px (#F5F4ED Wash, 고정폭 12px)]        | |
| ● 7지구 9호-7    | +-------------------------------+ | | 기고 63.1 mm · 구경 125.2 mm · 저경 123.9 mm · 두께 5.9 | |
|   (완형 세트)    | | [진짜 3D Z-버퍼 뷰포트]        | | +-------------------------------------------------------+ |
| ○ 1.obj          | |  • Z-버퍼 삼각형 솔리드 음영   | | | [실측 도판 용지 (#FFFFFF Surface + 1mm/10mm 방안 눈금)]| |
|   (편조각 세트)  | |  • .mtl/.jpg 실사 UV 텍스처    | | |                                                       | |
|                  | |  • 실시간 정밀 탁본 셰이딩     | | |  ① 외면·내면·저면 정밀 탁본(Rubbing) 래스터 영상 합성  | |
| ---------------- | |  • 3분류 파단면 컬러 오버레이  | | |  ② #141413 먹물 솔리드 채움 단면 폴리곤 (삼각분할)     | |
| ● 종단면 rc0     | |  • Z=0 바닥 동심원/직교 격자   | | |  ③ 수직 중심축선 + 구연/저부 수평 연결선(Bridge Line)  | |
|   (18.0mm · .ap2)| |  • A–A' 절단면 & #FF0000 단면선| | |  ④ 정면도(좌 외면탁본│우 먹물단면) + 하단 외저면 탁본   | |
| ○ 종단면 B–B′    | +-------------------------------+ | |  ⑤ 우하단 5cm 교차 흑백 축척자 & 커서 기벽두께 풍선    | |
| ○ 외저면 탁본    | | [0°~360° 접지 오차 띠 (36px)]  | | |                                                       | |
+------------------+-----------------------------------+-----------------------------------------------------------+
| [영역 5: 상태줄 (30px 고정, #FAF9F5 Ground, 상단 1px #DEDCD1)]                                                   |
|  다음: 마우스 휠로 도판을 확대하거나 3D 뷰를 드래그해 회전하세요  |  R 62.6 mm  H 63.1 mm  두께 5.9 mm  |  축척 1:2  |  ✓ 정상 |
+==================================================================================================================+
```

> [!CAUTION]
> **`egui` 수평/수직 레이아웃 버그 방지 강제 규칙**:
> `egui`에서 `ui.horizontal_top(|ui| { ... })` 내부의 자식 `ui.allocate_ui(size, |ui| { ... })`는 부모의 `left_to_right` 레이아웃을 그대로 상속합니다. 따라서 자식 내부에서 `ui.allocate_exact_size(vec2(width, 32.0), ...)`를 호출하면 다음 위젯이 아래(`y + 32`)로 내려가지 않고 오른쪽(`x + width`)으로 밀려나 **우측 73% 도판 전체가 화면 오른쪽 밖으로 사라지는 치명적 백지 버그**가 발생합니다.
> 이를 원천 차단하기 위해 `main.rs`, `view3d_pane.rs`, `sheet_pane.rs`의 모든 패널 컨테이너는 반드시 **`ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| { ... })`**을 사용하여 위에서 아래로 쌓이도록 구현해야 합니다.

---

### 3.4 상단 리본(`TileRibbon`) `50×50` 타일 전용 벡터 아이콘 규격 (`Ribbon.cpp` & PDF 기준)

모든 리본 타일은 **`50 × 50 px` 정사각형(모서리 둥글기 `12px`)**이며, 타일 내부에 텍스트 알파벳 약자를 넣는 것을 금지하고 아래 **`1.75px` 흑연(`#141413`) + `#B5573A` 테라코타 포인트 전용 벡터 아이콘**을 `egui::Painter`로 직접 그립니다:

| 아이콘 종류 (`TileIconKind`) | 적용 타일 | 벡터 드로잉 기하 명세 (`1.75px #141413` 주 선 + `#B5573A` 강조) |
|---|---|---|
| **`FolderOpen`** | `열기`, `폴더 일괄실측` | 열린 폴더 탭 윤곽(`#141413`) + 폴더 위로 올라오는 테라코타 화살표(`#B5573A`) |
| **`SaveDisk`** | `저장`, `Arch3D 호환 저장` | 플로피/문서 외곽 사각형 + 상단 라벨 셔터(`#141413`) + 하단 완료 체크/원점(`#B5573A`) |
| **`ArtifactKindCube`** | `유물 종류`, `완형/편/구슬` | 토기 단면 실루엣과 편조각 격자 외곽(`#141413`) + 중심 회전축 수직 점선(`#B5573A`) |
| **`AutoUprightAxis`** | `자동 세우기`, `중심축 맞춤` | 수평 바닥 기준선(`#141413`) + 수직 중심축 화살표(`#B5573A`) + 좌우 대칭 유물 윤곽선 |
| **`RimOrBaseLevel`** | `바닥 수평`, `구연 수평`, `3점 찍기` | 유물 구연/바닥 수평선(`#141413`) + 수평선 위의 3개 정합 기준점 원(`#B5573A` 솔리드 반경 `2.5px`) |
| **`FineTiltRotate`** | `90° 회전`, `뒤집기`, `0.1° 기울기` | 원호 회전 궤적 화살표(`#141413`) + 각도 기준선 및 중심 피벗 점(`#B5573A`) |
| **`SectionCutPlane`** | `단면선`, `Y/X/Z 막`, `단면 추가` | 3D 절단 평면 사각형(`#141413`) + 그 중앙을 가르는 순적/테라코타 절단 프로파일 곡선(`#B5573A`) |
| **`SilhouetteOutline`** | `외곽선`, `좌/우 반전(R)` | 수직 중심축(`#141413` 세선) 좌측의 유물 외곽 곡선(`#141413` 굵은 선) + 우측 반전 화살표(`#B5573A`) |
| **`RubbingTexture`** | `정밀 탁본`, `유물 음영`, `전개도` | 유물 표면 곡면 격자 + 먹물 농담 점묘/해칭 스트로크(`#141413`) + 스캔 광선 포인트(`#B5573A`) |
| **`FractureSegment`** | `파단면 분리`, `내벽 파선 보정` | 내·외벽 실선 사이에 꺾인 톱니형 파단 경계선(`#B5573A`)과 영역 분할선(`#141413`) |
| **`GridAndRuler`** | `방안 눈금`, `축척·치수표` | 직교 방안 눈금 십자선(`#141413`) + 눈금자 틱 마크 및 코너 기준점(`#B5573A`) |
| **`ExportPackage`** | `도면·내보내기`, `이 자세로 확정`, `인쇄·PDF` | 도판 용지 사각형 위로 나가는 내보내기 화살표 (주 행동 `#B5573A` 타일 위에서는 `#FAF9F5` 밝은 종이색 `2.0px` 벡터 선으로 렌더링) |

---

## Part 4. 핵심 I/O · 3D Z-버퍼 뷰포트 · 2D 실측 도판 상세 기술 명세

### 4.1 `.obj` + `.mtl`/`.jpg`(UV 텍스처) 및 `.ap2`(정위치·절단면) 자동 연동 파이프라인 (`arch3d-io`)

1. **UV 좌표(`vt`) 및 `.mtl` 실사 텍스처(`.jpg`/`.png`) 자동 로더 (`obj_loader.rs`)**:
   - `.obj` 파싱 시 `v x y z`, `vt u v`, `vn nx ny nz`와 면 정의 `f v1/vt1/vn1 v2/vt2/vn2 v3/vt3/vn3`를 모두 파싱하여 `TriMesh`에 다음 데이터를 저장합니다:
     - `vertex_uvs: Vec<[f32; 2]>` (정점별 대표 UV 좌표, $v$축 상하 반전 $1.0 - v$ 적용)
     - `triangle_uvs: Vec<[[f32; 2]; 3]>` (UV 심(Seam) 경계에서도 텍스처가 깨지지 않도록 삼각형 코너별 정확한 UV 좌표 보관)
     - `texture_image: Option<Arc<MeshTextureRgba>>` (`width`, `height`, `rgba: Vec<u8>`)
     - `vertex_colors: Vec<[u8; 3]>` (텍스처 이미지에서 샘플링한 정점별 기본 RGB 컬러)
   - `.obj` 파일 내부의 `mtllib <파일명.mtl>` 또는 같은 폴더의 동명 `.mtl` 파일을 읽어 `map_Kd`에 지정된 이미지 파일([`7지구 9호-7_1.jpg`](file:///e:/034(7-실4-4)/7지구%209호-7_1.jpg), [`1_1.jpg`](file:///e:/1/1_1.jpg))을 자동 로드합니다.
2. **동명 `.ap2` 파일 자동 감지 및 정위치·절단면 즉시 적용 (`ap2_codec.rs` + `obj_loader.rs`)**:
   - `.obj`를 로드할 때 같은 디렉터리에 동일한 파일 줄기(stem)의 `.ap2` 파일(예: [`7지구 9호-7.ap2`](file:///e:/034(7-실4-4)/7지구%209호-7.ap2))이 존재하면 자동으로 함께 파싱합니다:
     - `TransfomMatrix` (`4×4` 행렬)가 단위행렬이 아니면 메쉬의 모든 정점과 법선에 즉시 적용(`apply_ap2_transform`)하여 C++ 원본 및 `ret/` 정답과 **소수점 넷째 자리까지 동일한 정위치 좌표계(`125.174 × 123.935 × 63.133 mm`)**로 변환합니다.
     - `.ap2`에 저장된 `rc.IsSaved0`~`4`, `rc.PlanePosition0` (`18.0 mm`), `clip.PlanePosition` (`18.0 mm`), `fc/rc/tc` 슬롯 정보 및 `wcenter`를 함께 반환하여, 사용자가 별도 조작을 하지 않아도 첫 화면부터 정답 절단면(`18.0 mm`)이 적용되도록 합니다.

---

### 4.2 좌측 27% `View3DPane` — 월드 고정 레이저 절단틀 · 1/4 유물 절개(Cutaway) · 턴테이블 회전 & Z-버퍼 3D 렌더러 (`view3d_pane.rs`)

사용자가 지정한 [`archaeo3d_rs_gigamesh_arch3d_rust_gate_1 (1).html`](file:///c:/Users/kyi25/Downloads/archaeo3d_rs_gigamesh_arch3d_rust_gate_1%20(1).html)의 **3D 단면 생성 방식 · 유물 턴테이블 회전 방식 · 유물 절개(Cutaway) 방식**과 기존 C++ [`MeshRaster.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshRaster.cpp)의 Z-버퍼/UV 텍스처/정밀 탁본 셰이딩을 결합하여 구현합니다. (전체 화면 디자인과 5단 레이아웃은 원래 계획인 `STRATA · 디자인 기준`을 그대로 유지합니다.)

1. **월드 고정 레이저 절단틀(`P_cut: Y = ΔY`) + 유물 턴테이블 회전(`ψ`) + 미세 기울기(`tilt`) 좌표 변환 (`local_to_world`)**:
   - 절단 평면(`P_cut: Y = cut_y_mm`)은 월드 공간에 **고정**되어 있고, 유물 메쉬 정점 $(l_x, l_y, l_z)$만 수직 회전축 둘레로 $\psi \in [0^\circ, 359^\circ]$ 턴테이블 회전하며 바닥/천공축 기울기 $\text{tilt} \in [-25.0^\circ, +25.0^\circ]$ (`0.1°` 단위)가 적용됩니다:
     $$r_x = l_x \cos\psi - l_y \sin\psi, \quad r_y = l_x \sin\psi + l_y \cos\psi, \quad r_z = l_z - z_{\text{mid}}$$
     $$w_x = r_x \cos(\text{tilt}) - r_z \sin(\text{tilt}), \quad w_y = r_y, \quad w_z = r_x \sin(\text{tilt}) + r_z \cos(\text{tilt}) + z_{\text{mid}}$$
2. **유물 전면-좌측 1/4 절개(Cutaway) 렌더링 (`c_wy > y_cut && c_wx < -1.5`)**:
   - 고정 절단면(`w_y > y_cut`) 앞쪽이면서 단면이 위치하는 좌측 영역(`w_x < -1.5 mm`, `X-Ray` 모드 제외 및 `절개 보기` 활성 시)의 삼각형을 **실시간 절개(Cutaway 컬링)**합니다.
   - 이를 통해 유물의 앞쪽 좌측 사분면이 실제로 칼로 도려낸 것처럼 열려서, **유물 내부 내벽(내저원각)과 절단면에 맞닿은 기벽 두께 단면이 3D 뷰포트에서 직접 입체적으로 보이도록** 렌더링합니다.
3. **고정 레이저 절단틀 쿼드 + 중심축 점선 + 실시간 수직 종단면(`vertical`) · 수평 횡단면(`horizontal`) 교차선**:
   - **고정 레이저 절단틀**: 월드 좌표 `(-hw, y_cut, -4)` ~ `(hw, y_cut, hh)`에 반투명 청록색 절단틀 사각형(`rgba(56, 189, 248, 0.12)` 채움 + `#38BDF8` `1.9px` 테두리)과 수직 중심축 점선(`#F59E0B`, `dash 5, 4`)을 그립니다.
   - **실시간 삼각형-평면 교차 단면선 (`slice_active_mesh_with_plane`)**:
     - **수직 종단면(`vertical`, $d = w_y - y_{\text{cut}}$)**: 부호가 바뀌는 삼각형 두 변의 선형 보간 교점 선분들을 추출하여 절개된 단면 경계 위에 **`#FEF08A` / `#FF0000` `2.4px` 하이라이트 단면선**으로 선명하게 그립니다.
     - **수평 횡단면(`horizontal`, $d = w_z - z_{\text{cut}}$)**: `Top Cut Z` 높이 평면과 교차하는 3D 수평 링 선분들을 추출하여 유물 둘레에 **`#F59E0B` `1.8px` 수평 절단선**으로 표시하고, 우측 2D 도판의 `수평 횡단면 (Top Cut Z)` 보조 뷰에 실시간으로 투영합니다.
4. **3D 뷰포트 내부 UI 구성 (HTML 메커니즘 + STRATA 디자인 통합)**:
   - **좌상단 단계별 핵심 액션 & 4종 셰이더 스위치 (`실사 텍스처` · `3분류 파단면` · `X-Ray` · `실시간 정밀 탁본` + `절개 ON/OFF`)**:
     - 멀티스레드(`rayon`) Z-버퍼 삼각형 래스터라이저로 `.mtl`/`.jpg` 실사 UV 텍스처, 양면 조명 솔리드, 실시간 정밀 탁본, X-Ray, 3분류 파단면 컬러를 렌더링합니다.
   - **우상단 `Cross-Plane · 축 스코프` (`126px` 동심원 타깃 카드)**:
     - 동심원 타깃 위에 구연 외경 링(`#38BDF8`), 굽/저부 링(`#10B981`), 현재 유물 회전각 $\psi$를 가리키는 회전 바늘(`#EF4444` 선 + 끝점 원)과 `축 정합 0.0°` 상태 라벨을 표시하며, 스코프 클릭/드래그로도 $\psi$ 회전이 가능합니다.
   - **하단 3종 드래그 모드 스위치 & 실시간 좌표 배지**:
     - **`[드래그 = 유물만 회전 (절단틀 고정)]`** (`turntable`, 기본 ON) / **`[드래그 = 카메라 회전]`** (`camera`) / **`[드래그 = 바닥 기울기(0.1°)]`** (`tilt`) 스위치와 `고정절단면 Y=0.0mm | 유물회전 ψ=0° | 기울기 0.0°` 배지를 표시합니다.
   - **하단 4대 정밀 스크럽 컨트롤 (`ψ` 유물 회전 `0~359°`, `Cutline ΔY` 종단면 오프셋, `Top Cut Z` 횡단면 높이, `0.1°` 기울기 + `0°~360°` 접지 오차 띠)**:
     - 슬라이더 또는 마우스 드래그 조작 시 좌측 3D 절개 뷰와 우측 2D 도판이 실시간 동기화됩니다.

---

### 4.3 우측 73% `SheetPane` — 정밀 탁본 래스터 영상 + `#141413` 먹물 솔리드 채움 단면 + 다중 뷰 도판 (`sheet_pane.rs` & `plate_composer.rs`)

기존 C++ [`SheetView.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/SheetView.cpp), [`Plate.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Plate.cpp), [`Rubbing.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Rubbing.cpp) 및 [`E:\034(7-실4-4)\ret\`](file:///e:/034(7-실4-4)/ret) 정답 파일들과 동일하게, 우측 73% 도판 용지 위에 **래스터 영상과 솔리드 채움 단면, 제도 보조선, 다중 뷰, 축척자**를 정밀 합성합니다.

1. **도판 영역 3단 수직 구성 (`top_down` 레이아웃)**:
   - **① 도판 머리줄 (`32px`, `#FAF9F5`)**: 유물 번호·명칭(`7지구 9호-7` / `1.obj`), 유물 종류 배지, 정위치/절단면 기준 배지(`[✓ .ap2 정위치 · 절단 18.0mm]`), 뷰 전환 버튼(`[도판 맞춤]`, `[좌/우 반전 R]`, `[음영: 정밀 탁본 / 실사 / 단면만]`).
   - **② 치수 대장 띠 (`28px`, `#F5F4ED Wash`, 고정폭 `12px`)**: `기고` · `복원구경(또는 잔존폭)` · `저경` · `평균두께` · `절단 위치` 및 `[치수 복사]` 버튼.
   - **③ 실측 도판 캔버스 (남은 높이 100%, `#ECEAE2` 책상 배경 위 `#FFFFFF` A4/A3 용지 + 그림자 + `1mm`/`10mm` 방안 눈금)**.
2. **용지 위에 합성되는 5대 핵심 그래픽 요소**:
   - **(a) 외면·내면·상/하면 정밀 탁본(`Rubbing`) 및 실사 텍스처 래스터 이미지 합성 (`egui::TextureHandle`)**:
     - 실제 3D 메쉬를 각 뷰 방향(정면 외면, 배면/내면, 상면 구연/평면, 하면 외저면)에서 고해상도(`1 mm = 5 px` 이상) Z-버퍼로 정사투영하여 **정밀 탁본 래스터 이미지(`RGBA8` — 외곽 밖 픽셀은 완전 투명 `alpha = 0`, 내부는 법선 미분 + 다중스케일 곡률 요철 + 한지 손탁본 먹물 농담)**와 **실사 UV 텍스처 정사투영 이미지**를 생성합니다.
     - 생성된 래스터 이미지를 `egui::Context::load_texture`로 GPU 텍스처 핸들(`egui::TextureHandle`)에 캐싱하고, 도판 용지 위의 정확한 밀리미터 좌표 영역(`Rect`)에 `painter.image(...)`로 선명하게 합성합니다.
   - **(b) `#141413` 먹물 솔리드 채움 단면 폴리곤 (`triangulate_ear_clipping` + `egui::Mesh`)**:
     - 메쉬를 절단 평면(완형 `7지구 9호-7`의 경우 `.ap2`의 `PlanePosition = 18.0 mm`, 편조각 `1.obj`의 경우 중앙 수직 회전단면)으로 자른 **실제 닫힌 기벽 단면 폴리곤(Closed Section Loop)**을 추출합니다.
     - 오목 다각형인 기벽 단면 루프를 **귀 자르기(Ear-Clipping) 삼각분할 알고리즘**으로 삼각형 메쉬(`egui::epaint::Mesh`)로 변환하여, 내부를 **`#141413` 먹물 솔리드 색상**으로 빈틈없이 채우고 외곽에 `#141413` 또는 `#FF0000` 실측 단면 윤곽선을 그립니다 (`45° 해칭` 및 `윤곽선만` 모드 전환 지원).
   - **(c) 수직 중심축선 · 구연/저부 수평 연결선(Bridge Line) · 인출 보조선**:
     - 완형/회전체 도판에서는 수직 중심축선(`#141413`, `0.25mm`), 상단 구연 수평 연결선(Rim bridge line), 하단 저부 수평 연결선(Base bridge line), 좌측 외면 정밀 탁본과 우측 먹물 채움 단면의 경계 정합을 정확히 렌더링합니다.
     - 편조각 3단 병렬 도판에서는 `[외면 정밀 탁본] ── [#141413 회전단면] ── [내면 정밀 탁본]`의 상·하단을 수평 인출 보조선(`0.13mm`)으로 1:1 연결합니다.
   - **(d) 상/하/정/측 다중 뷰(Multi-View) 자동 배치**:
     - **완형 세트 (`7지구 9호-7.obj`)**: 상단 중앙에 **정면 실측도**(중심축 기준 좌측 = 외면 정밀 탁본/실사 래스터 + 외곽선, 우측 = `#141413` 먹물 솔리드 채움 단면 + `#FF0000` 절단선 [`cutline_rc0.svg`](file:///e:/034(7-실4-4)/ret/cutline_rc0.svg) 정합)를 배치하고, 그 아래에 [`ret/`](file:///e:/034(7-실4-4)/ret) 정답과 동일하게 **하단 외저면(Bottom) 정밀 탁본 뷰**를 수직 정렬하여 배치하며, 우측에 **보조 횡단면 뷰**를 함께 배치합니다.
     - **편조각 세트 (`1.obj`)**: **① 외면 정사투영(정밀 탁본 / `1_1.jpg` 실사 텍스처) ── ② 수직 회전 단면도(`#141413` 먹물 솔리드 채움 + 파단면 경계 표시) ── ③ 내면 정사투영(내면 정밀 탁본 / 물레속선)** 3단 병렬 뷰와 상단 평면(구연 원호) 뷰를 배치합니다.
   - **(e) 정밀 축척자(Scale Bar) 및 줌/팬/기벽두께 풍선 인터랙션**:
     - 용지 우하단에 `5 cm` (10 mm 단위 흑백 교차 막대 + `축척 1:2` 등 라벨) 정밀 축척자를 그립니다.
     - 도판 위에서 마우스 휠로 확대/축소(`zoom`), 드래그로 이동(`pan`), 더블클릭으로 용지 맞춤을 지원하며, 단면 위에 마우스를 올리면 해당 높이($H$), 반경($R$), **기벽 두께(`두께 5.9 mm`) 풍선**을 실시간 표시합니다.

---

## Part 5. 5단계 점진적 구현 및 실제 실행 창(`arch3d-studio.exe`) 캡처 검증 게이트

> [!IMPORTANT]
> **한 번에 다 짜지 말고 1단계씩 실제 실행 창 캡처로 검증받을 것**:
> 코드는 아래 5단계로 나누어 진행하며, **각 단계가 끝날 때마다 실제 `arch3d-studio.exe` 네이티브 창을 실행하여 스왑체인 프레임버퍼(`egui::ViewportCommand::Screenshot`)를 캡처한 실제 창 이미지를 사용자에게 제시하고 승인을 받은 뒤에만 다음 단계로 진행**합니다.

| 단계 | 구현 대상 및 핵심 작업 내용 | 완료 기준 및 검증 방법 (실제 창 캡처 필수) |
|---|---|---|
| **Step 1** | **우측 73% `SheetPane` 백지 버그 해결 + 상단 리본 `50×50` 벡터 아이콘 구현**<br>• [`main.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/main.rs), [`view3d_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/view3d_pane.rs), [`sheet_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/sheet_pane.rs)의 패널 컨테이너를 `Layout::top_down`으로 수정하여 우측 73% 도판 정상 표시<br>• [`layout_r1.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/layout_r1.rs)의 모든 `50×50` 리본 타일에 `1.75px` `#141413` 흑연 + `#B5573A` 테라코타 전용 벡터 아이콘 및 그룹 구분선/묶음명 구현<br>• `arch3d-studio.exe` 실제 네이티브 창 프레임버퍼 캡처 기능(`--capture-window <path.png>`) 연결 | • `cargo test -p arch3d-studio`<br>• **실제 실행된 `arch3d-studio.exe` 창 캡처 이미지에서 ① 우측 73% 도판 용지가 정상 표시되고 ② 상단 리본 `50×50` 타일에 벡터 아이콘이 그려진 것을 사용자에게 확인받음** |
| **Step 2** | **`.obj` + `.mtl`/`.jpg` (UV 텍스처) 및 `.ap2` (정위치·절단면) 자동 연동**<br>• [`mesh.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-core/src/mesh.rs) 및 [`obj_loader.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-io/src/obj_loader.rs)에서 `vt` UV 좌표와 `.mtl` `map_Kd` 이미지(`7지구 9호-7_1.jpg`, `1_1.jpg`) 로드 및 UV 컬러 샘플링<br>• 동명 `.ap2`(`7지구 9호-7.ap2`) 자동 감지, `TransfomMatrix` 정위치 변환 및 `rc.PlanePosition0 = 18.0 mm` 절단면 자동 적용<br>• 스튜디오 시작 시 슬롯 1에 `7지구 9호-7.obj`(완형 세트), 슬롯 2에 `1.obj`(편조각 세트) 자동 로드 | • `cargo test -p arch3d-io`<br>• **실제 실행된 `arch3d-studio.exe` 창 캡처 이미지에서 `7지구 9호-7`과 `1.obj`의 `.ap2` 정위치 치수(`125.2 × 123.9 × 63.1 mm`) 및 `.jpg` 텍스처/절단면(`18.0 mm`) 연동 상태를 사용자에게 확인받음** |
| **Step 3** | **좌측 27% `View3DPane` 진짜 3D Z-버퍼 렌더러 구현**<br>• [`view3d_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/view3d_pane.rs)의 가짜 선분 샘플링을 제거하고 `rayon` 멀티스레드 Z-버퍼 삼각형 래스터라이저 구현<br>• ① 실사 UV `.jpg` 텍스처 매핑, ② 솔리드 양면 조명, ③ 실시간 정밀 탁본 셰이딩, ④ X-Ray, ⑤ 3분류 파단면 컬러 오버레이<br>• 상단 `PlanView` 절단선 핸들 + `Z=0` 바닥 격자 + 3D `A–A'` 절단면 및 `#FF0000` 단면선 + 마우스 회전/줌/팬 | • `cargo test -p arch3d-studio`<br>• **실제 실행된 `arch3d-studio.exe` 창 캡처 이미지에서 좌측 27% 3D 뷰포트에 진짜 Z-버퍼 솔리드/UV 텍스처/실시간 정밀 탁본/파단면 3분류가 렌더링된 것을 사용자에게 확인받음** |
| **Step 4** | **우측 73% `SheetPane` 정밀 탁본 래스터 영상 + `#141413` 먹물 솔리드 채움 단면 + 다중 뷰 합성**<br>• [`plate_composer.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-export/src/plate_composer.rs) 및 [`sheet_pane.rs`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/crates/arch3d-studio/src/sheet_pane.rs)에서 외면·내면·외저면 고해상도 정밀 탁본(`Rubbing`) 및 실사 텍스처 래스터 이미지를 생성하여 도판 용지 위에 `TextureHandle`로 합성<br>• 닫힌 단면 루프 Ear-Clipping 삼각분할 기반 **`#141413` 먹물 솔리드 채움 단면 폴리곤** 렌더링<br>• 중심축선·구연/저부 수평 연결선(Bridge line)·다중 뷰(`ret/` 호환 정면+저면 / 편조각 3단 병렬)·교차 흑백 축척자·마우스 줌/팬/두께 풍선 구현 | • `cargo test -p arch3d-export -p arch3d-studio`<br>• **실제 실행된 `arch3d-studio.exe` 창 캡처 이미지에서 우측 73% 도판 용지 위에 정밀 탁본 래스터 영상과 `#141413` 먹물 채움 단면, 중심축/연결선, 다중 뷰, 축척자가 합성된 것을 사용자에게 확인받음** |
| **Step 5** | **SVG / PDF / DXF / `ret/` 출력 정합 및 전체 하네스·독립 Diff 감사**<br>• `ret/cutline_rc0.svg` 호환 3차 베지어 SVG·래스터 출력 정합 확인<br>• `cargo clippy --workspace --all-targets -- -D warnings` (경고 0건) 및 `cargo test --workspace` 전수 통과<br>• 독립 `research` 서브에이전트 `git diff` 교차 검토(`X1~X4`) 및 최종 보고 | • `cargo clippy --workspace --all-targets -- -D warnings` (종료 코드 `0`)<br>• `cargo test --workspace` (종료 코드 `0`)<br>• 금지어 영문 축약어 전수 검사 **0건** |
