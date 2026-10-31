# STRATA · 동국유물실측 (DongArch3D) 시니어 엔지니어링 팀 개발 원칙 및 하네스 규약 (v2.0)

> **적용 범위**: `strata-arch3d` Rust 워크스페이스 전 크레이트 (`arch3d-core`, `arch3d-io`, `arch3d-orient`, `arch3d-measure`, `arch3d-section`, `arch3d-shade`, `arch3d-export`, `arch3d-studio`)
> **기준 설계도**: [`docs/DESIGN_SPEC.md`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/docs/DESIGN_SPEC.md) (v2.0), [`docs/IMPLEMENTATION_PLAN.md`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/docs/IMPLEMENTATION_PLAN.md) (v2.0), [`docs/ui/UI_RULES.md`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/docs/ui/UI_RULES.md)

---

## 1. 3대 절대 수칙 (어기면 즉시 중단)

1. **금지어 영문 축약어 사용 절대 금지 — '정밀 탁본' (`rubbing` / `Rubbing`) 표준화**:
   - 코드·주석·UI 텍스트·설계 문서·테스트 이름 어디에도 금지어 축약어를 절대 사용하지 않는다.
   - 반드시 한국 고고학 표준 용어인 **'정밀 탁본'**과 영문 식별자 **`rubbing` / `Rubbing`**만 사용한다.
2. **기존 기능 삭제·축약 절대 금지 및 C++ 원본 앱 1:1 계승**:
   - 이미 구현된 8개 크레이트의 알고리즘과 단위 테스트를 삭제하거나 축약하지 않는다.
   - 현행 C++ 원본 앱([`View3D.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/View3D.cpp), [`MeshRaster.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshRaster.cpp), [`SheetView.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/SheetView.cpp), [`Plate.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Plate.cpp), [`Rubbing.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Rubbing.cpp), [`Ribbon.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/Ribbon.cpp), [`MeshIO.cpp`](file:///D:/_dongarch_build/wt/.worktrees/JOB-010/studio/src/MeshIO.cpp))의 실제 Z-버퍼 3D 렌더링·2D 도판 합성·리본 벡터 아이콘·`.mtl`/`.ap2` 자동 연동 기능을 정밀하게 계승한다.
3. **1단계씩 실제 실행 창(`arch3d-studio.exe`) 캡처로 검증 후 다음 단계 진행**:
   - 한 번에 모든 단계를 코딩하지 않는다.
   - 가짜 오프스크린 사각형 드로잉 함수로 만든 이미지를 검증 증거로 제출하는 행위를 전면 금지한다.
   - 반드시 각 단계(`Step 1` ~ `Step 5`)마다 실제 실행된 `arch3d-studio.exe` 네이티브 창의 스왑체인 프레임버퍼(`egui::ViewportCommand::Screenshot`)를 캡처하여 사용자 확인을 받은 뒤 다음 단계로 넘어간다.

---

## 2. 시니어 엔지니어링 팀 4대 분담 및 상호 검증 체계

| 역할 (Role) | 담당 크레이트 | 핵심 책임 및 완료 게이트 |
|---|---|---|
| **Role 1. 수석 기하·위상 및 입출력 아키텍트** | `arch3d-core`<br>`arch3d-io` | • `TriMesh` 정점·법선·UV(`vertex_uvs`, `triangle_uvs`)·실사 텍스처(`MeshTextureRgba`) 및 `Compressed Corner Table`, `Stackless LBVH`<br>• `E:\034(7-실4-4)\7지구 9호-7.obj`(완형 세트) 및 `E:\1\1.obj`(편조각 세트) 로드 시 동반 `.mtl`/`.jpg` 텍스처 디코딩 및 동명 `.ap2`(`TransfomMatrix` + `rc.PlanePosition0 = 18.0 mm`) 자동 적용 |
| **Role 2. 수석 계산고고학·정밀계측 엔지니어** | `arch3d-orient`<br>`arch3d-measure` | • 구 C++ 5대 결함 완치 (`0°~360°` 구연 오차, `rayon` 병렬 결함 점수, `95mm #6b4fc0` 하드코딩 제거)<br>• 편조각 5종 `Structure-from-Sherds++` 파단면 3분할(`외면/내면/파단면`), `Pratt+Taubin+Huber` 원호 피팅, 물레속선(`Rills`) 경사각, 구슬 4대 천공 단면(`Ⅰa·Ⅰb·Ⅰb'·Ⅱb`) 및 `0.01 mm` 계측 |
| **Role 3. 수석 단면·정밀 탁본·출판 엔지니어** | `arch3d-section`<br>`arch3d-shade`<br>`arch3d-export` | • 고정 절단틀 및 `.ap2` 지정 절단면(`18.0 mm`) 닫힌 루프 추출, 미스캔 파선 보정, `kurbo` 3차 베지어 피팅<br>• 외면·내면·상면·외저면 고해상도 정밀 탁본(`Rubbing`) 및 실사 텍스처 정사투영 래스터 생성, Ear-Clipping 삼각분할 `#141413` 먹물 솔리드 단면, 다중 뷰 도판 조판 및 SVG/PDF/DXF/`ret/` 출력 |
| **Role 4. 수석 STRATA UI/UX & 하네스 감리관** | `arch3d-studio` | • `UI_RULES.md` R1~R8 5단 고정 레이아웃(`top_down` 수직 레이아웃 격리로 우측 73% 백지 버그 원천 차단)<br>• 상단 리본 `50×50` 타일 `1.75px` 흑연(`#141413`) + 테라코타(`#B5573A`) 전용 벡터 아이콘<br>• 좌측 27% `View3DPane` 멀티스레드 Z-버퍼 3D 렌더러 + 우측 73% `SheetPane` 정밀 탁본 래스터 & `#141413` 솔리드 단면 뷰어 + 실제 실행 창 프레임버퍼 캡처 검증 |

---

## 3. 전역 7단계 개발 순서 및 Rust Core Rules

1. **설계도 및 원칙 대조**: 매 단계 착수 전 [`docs/DESIGN_SPEC.md`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/docs/DESIGN_SPEC.md)와 [`docs/IMPLEMENTATION_PLAN.md`](file:///D:/_dongarch_build/wt/.worktrees/STRATA-RUST/strata-arch3d/docs/IMPLEMENTATION_PLAN.md)의 해당 단계 명세를 대조한다.
2. **실패하는 테스트 먼저 작성 (`Red` 단계)**: 구현 전 단위/통합 테스트(`#[test]`)를 먼저 작성하여 실패를 확인한다.
3. **최소·정밀 구현 (`Green` & `Refactor` 단계)**: 기존 기능을 훼손하지 않고 해당 단계 요구사항을 정확히 구현한다.
4. **Rust Core Rules 3대 강제 규칙**:
   - **규칙 R-1**: 소유권·차용 원칙 준수, 불필요한 `clone()` 지양, `#![forbid(unsafe_code)]` 유지.
   - **규칙 R-2**: 프로덕션 코드에서 `unwrap()`/`expect()` 사용을 금지하고 `Result<T, Arch3dError>`와 `?` 연산자로 에러를 안전하게 처리한다.
   - **규칙 R-3**: 모든 단계에서 `cargo check --workspace` 및 `cargo clippy --workspace --all-targets -- -D warnings` 경고가 **0건**이어야 한다.
5. **실제 실행 창(`arch3d-studio.exe`) 캡처 검증**: 각 단계 구현 후 실제 네이티브 창을 실행·캡처하여 사용자 승인을 받는다.
6. **독립 서브에이전트 Diff 자기 검토 (`X1 ~ X4`)**: 구현 대화와 분리된 `research` 서브에이전트로 `git diff`를 감사한다.
7. **Git 격리 및 데이터 보존**: 격리된 워크트리(`D:\_dongarch_build\wt\.worktrees\STRATA-RUST\strata-arch3d`)에서만 작업하며, 기존 C++ 워크트리와 `E:\` 원본 파일은 읽기 전용으로만 참조한다.
