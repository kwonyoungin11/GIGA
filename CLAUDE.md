# CLAUDE.md — DongArch3D 작업 규칙 (2026-10-17)

고고학 유물 3D 스캔(PLY/OBJ/STL/WRL, mm)을 정위치 → 단면·외곽·치수 → 1:1 mm 실측 도판(SVG/DXF/PDF)으로 만드는 Windows 앱. 모든 문서·대화·보고는 **한국어**. 코드 식별자·커밋 제목은 영문 허용.

## 1. 정본과 결정 (바꾸려면 먼저 사용자 승인)

- **UI 정본**: `docs/ui/UI_RULES.md`(R1~R8) + `docs/ui/mockup/`. 목업과 코드가 다르면 코드가 틀린 것이다. 구형 시안(`docs/legacy/`, 삭제된 `docs/dongarch3d/mockups/`)은 참조하지 않는다.
- **앱 본체**: `studio/`(Qt 6) + `GigaMesh/gui/src/dongarch/karch/`(K-Arch 엔진). `GigaMesh/`의 Qt 5 GUI(`gui/src/dongarch/*`)는 알고리즘 참조·CLI용이며 새 기능을 넣지 않는다.
- **라이선스**: 저장소 전체 GPL-3.0-or-later. `GigaMesh/external/triangle`은 비상업 조건이라 GigaMesh 기반 실행파일은 상용 배포 불가 — Studio/K-Arch는 triangle을 쓰지 않는다. Artec SDK·Studio 바이너리·번역·분석 결과는 절대 커밋하지 않는다.
- **토기 도판 좌/우(D5, 2026-10-17 확정)**: 외면 왼쪽 · 단면 오른쪽(국내 보고서 표준), `R` 로 좌/우 반전. 근거: `factory/project-profile.md` D5.
- 결정 기록: `factory/project-profile.md` §결정.

## 2. 좌표·단위 규약 (코드와 문서 모두 이 규약으로)

- 내부 단위 **mm(double)**, 수직축 **+Z**, 회전축 **+Z**, 도면 좌표 `(x, z)` → SVG 출력 시 y 반전. GigaMesh 코어(Y-up)·Artec 입력(임의)과의 경계에는 명시적 변환 계층을 둔다.
- SVG: `width/height="…mm"` + viewBox 1 user unit = 1 mm, 선 두께 mm, 최상위 `<g id>` 레이어. DXF: R12, `$INSUNITS=4`(mm), 레이어 11종(`01_AXIS_CENTER` … `11_SCALE_BAR`).
- 단위 자동 환산은 **모호하면 사용자에게 묻는다**(조용히 ×10/×1000 금지). 추정값(내벽 추정 등)은 실측값과 항상 구분해 표기한다.

## 3. 검증된 명령 (이 PC: VS2022 BuildTools, Qt6 `D:/OSGeo4W/apps/Qt6`, Qt5 `C:/Qt/5.15.2/msvc2019_64`)

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File factory/scripts/verify.ps1                      # Studio: configure+build+ctest(11)+캡처, 짧은 경로 D:\_dongarch_build\verify\studio
powershell -NoProfile -ExecutionPolicy Bypass -File factory/scripts/verify.ps1 -Target gigamesh      # GigaMesh Qt5: prepare → VS2022 configure(/utf-8) → build(≈7분) → ctest
powershell -NoProfile -ExecutionPolicy Bypass -File factory/scripts/prepare-gigamesh.ps1             # gitignore된 생성 파일 17개(analyticsConfig.h, .qm, png, .tex) 생성
powershell -NoProfile -ExecutionPolicy Bypass -File factory/scripts/factory-status.ps1               # 작업 상태표
```
- 빌드 폴더는 **짧은 경로**(긴 경로 → MSBuild FTK1011). `ctest`는 빌드 폴더로 `cd` 후 실행(`--test-dir` 금지). 소스는 git 체크아웃 안에 있어야 configure가 된다.
- 로컬 `python`은 OSGeo4W 것이 깨져 있으니 `C:\Users\kyi25\AppData\Local\Programs\Python\Python312\python.exe`를 쓴다.
- 빌드 산출물·로그는 저장소 밖(`D:\_dongarch_build\`) 또는 `factory/jobs/<ID>/artifacts/`(gitignore)에만.

## 4. 절대 규칙

1. `GigaMesh/core/`, `GigaMesh/external/`는 수정하지 않는다(알고리즘이 필요하면 코어 밖에서 재구현). 예외는 사용자 승인 후 별도 작업으로.
2. 새 UI 문자열은 100 % 한국어, 약어 노출 금지(예: MSII → 문양 살리기). 색상은 `studio/src/Theme.h` 토큰만.
3. 구현은 항상 **별도 브랜치 `factory/JOB-NNN` + worktree**(`.worktrees/`, gitignore)에서. `main`·`factory/setup`에 직접 커밋하지 않는다. 동시 구현 작업은 1개.
4. **모든 작업은 사용자 승인(`/factory approve`) 전까지 병합·push하지 않는다.** AI 검토 통과(review=pass)는 승인이 아니다. push는 사용자가 명시적으로 지시한 경우에만.
5. 완료 증거는 마커 파일이 아니라 **빌드 로그·ctest 결과·캡처**다(`factory/jobs/<ID>/verify.md`). 실패는 실패라고 쓴다.
6. 검토자 역할(code/correctness/security/ui-reviewer)은 제품 코드를 수정하지 않는다. 구현은 builder만.
7. 5 MB 초과 바이너리, 스캔 데이터(`*.ply/*.obj/*.stl` — `GigaMesh/testdata/`의 5개 소형 파일 제외), 독점 자료는 커밋하지 않는다.
8. 메인 체크아웃 `D:\gigamesh`의 미커밋 변경과 `feat/dongarch-studio` 브랜치(다른 도구가 커밋 중)는 건드리지 않는다. 병합 기준 SHA는 `factory/project-profile.md`에 고정한다.
9. **기능 추가는 반드시 기능별 브랜치를 새로 만들어 그 위에서만 한다**(2026-11-01 사용자 지시). 기준은 현재 개발선 `feat/strata-arch3d-rust`. 브랜치 이름은 `feat/<주제>-<짧은 영문 슬러그>`(예: `feat/hsec-fixed-view`), 작업 단위는 **기능 하나 = 브랜치 하나 = 커밋 묶음 하나**. `feat/strata-arch3d-rust` 에 기능 커밋을 직접 쌓지 않는다. 검증(빌드·테스트·캡처) 통과 후 사용자 지시가 있을 때만 병합한다. 리팩터링·문서·설정 변경도 각각 `refactor/…`, `docs/…`, `chore/…` 브랜치에서 하는 것을 권장한다.
10. **CI(GitHub Actions)는 실행하지 않는다**(2026-11-01 사용자 지시). 계정 결제 잠금으로 잡이 시작되지 않아 `strata-rust-ci.yml` 의 push·pull_request 자동 트리거를 꺼 두었다(수동 `workflow_dispatch` 만 남김). 검증은 로컬에서 `cargo fmt --all -- --check` · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo test --workspace` · 실제 창 캡처로 수행한다. CI 재개는 사용자가 명시적으로 지시할 때만 한다.

## 5. `/factory` 사용법 (상세: `.claude/skills/factory/SKILL.md`)

`/factory <개발 요청>` 등록·명세 → `/factory next` 구현·검증·검토 → `/factory status` 상태표 → `/factory rework <ID>` 재작업 → `/factory approve <ID>` 사용자 승인·병합. 상태: `queued → spec → build → verify → review → (rework | needs-human) → approved → merged`.

## 6. 커밋 규칙

- 제목: `<범주>(<영역>): <한국어 요약>` (범주 feat/fix/docs/chore/test/ci), 본문 한국어로 이유·검증 결과·관련 JOB ID.
- 마지막 줄: `🤖 Generated with Claude Code` / `Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>`.
- 커밋 메시지·PR 본문에 모델 식별자 외 비밀값·절대경로 비밀을 넣지 않는다.

## 7. 참고 위치

- 조사 보고서(2026-10-17): 프로젝트 라이브러리 `DongArch3D_1단계_조사보고서_및_준비계획*.md` — 빌드 함정·모듈 위험·정본 모순의 근거.
- 업스트림 GigaMesh 클론: `C:\Users\kyi25\OneDrive\Dokumen\GigaMesh`(gitlab fcgl 23d8cf4, testdata 26개).
- Artec 매뉴얼: `C:\Program Files\Artec\Artec Studio 20 Trial\Documentation\Artec_Studio_Manual_EN.pdf`(내보내기 9.5, 단위 13.1.10).
- 하위 `GigaMesh/CLAUDE.md`는 GigaMesh 원본 빌드 참고용이며 이 파일과 충돌하면 **이 파일이 우선**한다.
