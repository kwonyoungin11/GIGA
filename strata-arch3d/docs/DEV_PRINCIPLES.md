# STRATA · 동국유물실측 (DongArch3D) 시니어 엔지니어링 팀 개발 원칙 및 하네스 규약

> **적용 범위**: `strata-arch3d` Rust 워크스페이스 전 크레이트 (`arch3d-core`, `arch3d-io`, `arch3d-orient`, `arch3d-measure`, `arch3d-section`, `arch3d-shade`, `arch3d-export`, `arch3d-studio`)
> **기준 설계도**: `strata_dongarch3d_rust_master_design_spec.md` (`DESIGN_SPEC.md`) 및 `docs/ui/UI_RULES.md`, `STRATA · 디자인 기준 (6p PDF)`
> **필수 스킬 체계 (Superpowers Skills 상시 가동)**:
> - `superpowers:using-superpowers` (`C:\Users\kyi25\.gemini\config\plugins\superpowers\skills\using-superpowers\SKILL.md`)
> - `superpowers:using-git-worktrees` (격리된 워크트리 `D:\_dongarch_build\wt\.worktrees\STRATA-RUST` 및 브랜치 `feat/strata-arch3d-rust`)
> - `superpowers:writing-plans` & `superpowers:executing-plans` & `superpowers:subagent-driven-development` (`.superpowers/sdd/strata-arch3d/progress.md` 원장 + `task.md` 체크리스트 실시간 기록)
> - `superpowers:test-driven-development` (**Iron Law**: 실패하는 테스트(`RED`) 먼저 확인 후 최소 구현(`GREEN`) 및 `clippy -D warnings` 정돈(`REFACTOR`))
> - `superpowers:requesting-code-review` & `superpowers:receiving-code-review` (독립 `research` 서브에이전트 `git diff` 감사 `X1~X4`)
> - `superpowers:verification-before-completion` (**Iron Law**: 방금 실행한 검증 명령의 `exit 0` 증거 없이는 완료 주장 절대 금지)

---

## 1. 시니어 엔지니어링 팀 4대 분담 및 상호 검증 체계

본 프로젝트는 4명의 시니어 도메인·시스템 엔지니어가 한 팀으로 협업하고 각 단계마다 **하네스 검증기(`V1~V8`)**와 **독립 리뷰어(`X1~X4`)**의 감리를 통과하는 방식으로 개발한다.

| 역할 (Role) | 담당 크레이트 | 핵심 책임 및 완료 게이트 |
|---|---|---|
| **Role 1. 수석 기하·위상 시스템 아키텍트**<br>(Principal Geometry & IO Architect) | `arch3d-core`<br>`arch3d-io` | • `Compressed Corner Table` (`opposite_half_edge: Vec<u32>`) 및 `Octahedral 16B GpuVertex`<br>• `Stackless LBVH` (`subtree_area` 및 면적가중 법선 사전 적분)<br>• `E:\` 실측 `.obj`/`.mtl`/`.jpg` (한글 경로·대용량), `.ply`, `.stl`, `.ap2` (`QVariantMap` JSON `[81, ...]` 행렬·슬롯), `.darch` 무손실 입출력<br>• **검증 게이트**: `V1` (컴파일/클리피 경고 0), `V2` (`E:\` 실제 파일 Golden-Master 정합) |
| **Role 2. 수석 계산고고학·정밀계측 엔지니어**<br>(Computational Archaeology & Metrology Lead) | `arch3d-orient`<br>`arch3d-measure` | • 구 C++ 5대 결함 완치 (`0°~360°` 구연 오차, `rayon` 병렬 결함 점수, `95mm #6b4fc0` 하드코딩 제거)<br>• **편조각 5종**: `Structure-from-Sherds++` 파단면 3분할(파단면 원 피팅 혼입 0%), `Pratt+Taubin+Huber` 소각도 호 피팅 및 신뢰구간($\hat{D}\pm\sigma$), 물레속선(`Rills`) 경사각, 특수부위(파수 삽입촉 파선·뚜껑 정위치·고배 투창 열린 면), `BAISER` 가상 접합<br>• **구슬·옥류 6종**: $L/D$ 비율 자동 분류(`환옥/소옥/절구옥/관옥/곡옥/다면옥`) + **4대 천공 단면(`Ⅰa·Ⅰb·Ⅰb'·Ⅱb` 교차단차 $\Delta e$)** 판별 + `0.01 mm` 정밀도·초확대 축척(`2:1~10:1`)<br>• **기와·석기·금속기**: 막새 접합 파선·보강토, 석기 박리 선후 관계(`Diacritical`), 부식 금속기 3층 분리(원심·부식층·목질흔)<br>• **검증 게이트**: `V3`, `V4`, `V5` |
| **Role 3. 수석 미분기하 단면·탁본·출판 엔지니어**<br>(Section, Shading & Publishing Lead) | `arch3d-section`<br>`arch3d-shade`<br>`arch3d-export` | • 월드 고정 절단틀(`P_cut` 고정·유물 회전) 슬라이싱 + 내벽/천공 미스캔 구간 파선 추정 + `kurbo` 3차 베지어 피팅(`< 0.02 mm`)<br>• `16-Scale MSII` · 법선 미분 + 한지 손탁본(`Handprint`) · 개도(`Openness`) 이치화 · **5대 특징선(`Silhouette·Suggestive·Crest·Apparent Ridge·Demarcating Curves`)** · `Unroll` 전개 · 기와 포목흔 `1㎠` 2D FFT 올수 계측<br>• 8종 표준 보고서 도판 조판기 + Illustrator 레이어 분리 SVG(`1 mm = 5 px` + 4코너 `1×1 px` 투명 앵커) + `Arch3D ret/` 호환 세트 + 분할 인쇄 PDF + `DXF R12` + `.hwpx`/`.csv` 치수표<br>• **검증 게이트**: `V6`, `V7` |
| **Role 4. 수석 STRATA UI/UX & 하네스 감리관**<br>(STRATA Studio UI & Harness Supervisor) | `arch3d-studio`<br>(GUI & CLI Verifier) | • `STRATA · 디자인 기준` (6p PDF) & `UI_RULES.md` R1~R8 100% 구현<br>• `700×400` 3.0초 지층 스플래시, 5단 고정 레이아웃(`34 / 98 / 36 / [176 + 27% + 73% (Tab 100%)] / 30 px`), `50×50` 리본 타일, 5종 창 크기(`S 420–460`, `진행 480`, `M 520`, `L 860`, `XL 1280×860`), 3단 오류 창(`무엇이 안 됐나 / 왜 그런가 / 어떻게 하나`)<br>• **검증 게이트**: `V8` + 독립 서브에이전트 교차 감사(`X1~X4`) |

---

## 2. 전역 7단계 개발 순서 및 TDD 철칙

1. **설계도 및 원칙 대조 ( 매 Phase 시작 전 )**:
   - 각 Phase 착수 전 반드시 마스터 설계서(`strata_dongarch3d_rust_master_design_spec.md`)의 해당 장절과 완료 기준(`AC-1 ~ AC-8`)을 먼저 대조한다.
2. **실패하는 테스트 먼저 작성 (`Red` 단계)**:
   - 구현 코드를 먼저 쓰지 않고, 해당 모듈의 단위/통합 테스트(`#[test]`)를 먼저 작성하여 실패 또는 미구현 상태를 확인한다.
3. **최소·정밀 구현 (`Green` & `Refactor` 단계)**:
   - 요청하지 않은 장식 기능이나 불필요한 추상화를 배제하고, 테스트와 설계서 명세를 정확히 만족하는 코드를 작성한다.
4. **Rust Core Rules 3대 강제 규칙**:
   - **규칙 R-1**: 소유권·차용 원칙을 준수하고 불필요한 `clone()` 및 `unsafe` 블록 사용을 금지한다 (`#![forbid(unsafe_code)]` 기본 적용).
   - **규칙 R-2**: 프로덕션 라이브러리·앱 코드에서 `unwrap()` / `expect()` 남발을 금지하고, 모든 실패 가능 연산은 `Result<T, Arch3dError>`와 `?` 연산자로 전파하여 UI의 3단 오류 안내(`무엇이 안 됐나 / 왜 그런가 / 어떻게 하나`)로 연결한다.
   - **규칙 R-3**: 모든 단계에서 `cargo check --workspace` 및 `cargo clippy --workspace --all-targets -- -D warnings` 경고가 **0건**이어야 한다.
5. **하네스 결정론적 검증 (`V1 ~ V8`)**:
   - 실행하지 않은 검증은 절대 통과로 기록하지 않으며, 실제 명령·종료 코드·출력 수치·로그 경로를 기록한다.
6. **독립 서브에이전트 Diff 자기 검토 (`X1 ~ X4`)**:
   - 구현한 대화와 분리된 새 `research` 서브에이전트를 호출해 `git diff`만을 대상으로 안전성(`X1`), 수학/기하(`X2`), UI/디자인 토큰(`X3`), 경로/보안(`X4`) 위반을 감사한다.
7. **Git 격리 및 데이터 보존**:
   - 모든 작업은 격리된 워크트리(`D:\_dongarch_build\wt\.worktrees\STRATA-RUST`, 브랜치 `feat/strata-arch3d-rust`)에서만 수행하며, 기존 C++ 워크트리와 `E:\` 원본 파일은 읽기 전용으로만 참조한다.
