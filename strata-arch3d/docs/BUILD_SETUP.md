# STRATA · Rust 빌드 최적 설정 (2026-11-01)

> 대상: `strata-arch3d/` Rust 워크스페이스(8 크레이트, `strata-studio` = eframe/egui 데스크톱 앱)
> 원칙: **런타임 동작·의존성 버전을 바꾸지 않고 컴파일·도구 설정만 조정**한다.

## 1. 프로파일 (`Cargo.toml`)

| 프로파일 | 설정 | 이유 |
|---|---|---|
| `release` | `opt-level=3`, `lto="fat"`, `codegen-units=1`, `panic="abort"`, `strip="symbols"` | 배포용 앱의 실행 속도·바이너리 크기 최우선. 워크스페이스 전체에 `catch_unwind`/`panic::*` 사용처가 없어(2026-11-01 grep 확인) `panic="abort"` 가능 |
| `dev` | `opt-level=1`, `debug=1`, `incremental=true` | 증분 빌드 속도를 유지하면서 실행은 debug 기본(0)보다 빠르게 |
| `dev.package."*"` | `opt-level=2` | **의존성 크레이트만** 최적화. 워크스페이스 자체 크레이트는 `opt-level=1` 로 두어 증분 빌드를 해치지 않는다 |
| `test` | `dev` 상속 | 실패 메시지·백트레이스를 그대로 보기 위해 별도 최적화 없음 |

`panic="abort"` 때문에 **`cargo test --release` 는 실행할 수 없다**(테스트 하네스는 unwind 필요). 테스트는 기본(dev) 프로파일로 돌린다.

### 머신 종속 플래그는 넣지 않는다
`-C target-cpu=native` 같은 플래그는 이 파일에 넣지 않았다(다른 PC·CI 러너에서 빌드가 깨지거나 CPU 별로 산출물이 달라짐). 그 PC 에서만 최대 성능이 필요하면 셸에서 일회성으로:

```powershell
$env:RUSTFLAGS = "-C target-cpu=native"
cargo build --release -p strata-studio
```

## 2. 도구 체인 고정 (`rust-toolchain.toml`)

```toml
[toolchain]
channel = "1.91.1"
profile = "minimal"
components = ["rustfmt", "clippy"]
```

- rustup 이 설치된 PC·CI 에서는 이 파일만으로 1.91.1 + rustfmt + clippy 가 자동 설치된다.
- **이 개발 PC 에는 rustup 실행 파일이 없다**(`~/.cargo/bin` 없음, `~/.rustup` 에는 toolchain 만 존재). 그래서 `rustup component add` 가 동작하지 않았고, 검증은 `static.rust-lang.org` 의 1.91.1 배포본 컴포넌트(rustfmt 1.8.0 / clippy 0.1.91, rustc 커밋 일치)를 임시 폴더에 풀어 PATH 로만 사용했다.
- 영구 사용하려면 [rustup-init](https://rustup.rs) 설치를 권장한다. 그 뒤에는 위 파일이 나머지를 자동으로 맞춘다.

## 3. 별칭 (`.cargo/config.toml`)

| 별칭 | 명령 | 용도 |
|---|---|---|
| `cargo verify-v1` | `clippy --workspace --all-targets -- -D warnings` | CI 와 같은 엄격 lint 게이트 |
| `cargo verify-all` | `test --workspace -- --nocapture` | 전체 테스트 |
| `cargo studio` | `run --release -p strata-studio --` | 앱 실행 |
| `cargo format` | `fmt --all` | rustfmt 일괄 적용 |
| `cargo fmt-check` | `fmt --all -- --check` | 포맷 차이 검사 |

- `fmt = "fmt --all"` 은 cargo 1.91.1 에서 `unresolvable recursive definition` 으로 실패한다(별칭 확장 결과의 첫 토큰이 다시 같은 별칭). 그래서 적용용 이름을 `format` 으로 둔다.
- `[net] retry = 5` — 현장 PC·불안정 회선 대비.

## 4. 실측 효과 (이 PC, 2026-11-01)

| 항목 | 변경 전 | 변경 후 |
|---|---|---|
| 워크스페이스 테스트 실행 시간(75종, 캐시 warm) | 3.08 s | **0.37 s** (`dev.package."*" = opt-level 2` 효과, 약 8.3배) |
| `cargo test --workspace` warm 전체 | 4.52 s | 1.80 s |
| `cargo build --release -p strata-studio` | — | 158.67 s (LTO fat·codegen-units=1, `target/release/strata-studio.exe` 8,113,152 B) |
| `cargo clippy --workspace --all-targets` | — | 33.57 s |

## 5. CI (`.github/workflows/strata-rust-ci.yml`)

- 툴체인을 1.91.1 로 고정(`dtolnay/rust-toolchain@master` + `rust-toolchain.toml`), `Swatinem/rust-cache` 키에 `Cargo.toml`·`.cargo/config.toml` 해시를 넣어 프로파일 변경 시 캐시가 어긋나지 않게 했다.
- 게이트: `cargo check --locked` → `cargo clippy -- -D warnings` → `cargo test --workspace` → `--verify-all --screenshot-tour` 캡처 검증.
- 이전 워크플로는 존재하지 않는 패키지 이름(`arch3d-studio`)을 호출해 V8 단계가 항상 실패했다. `strata-studio` 로 고쳤다.

## 6. 포맷 정리 (2026-11-01 완료)

- `cargo fmt --all` 을 **한 번 일괄 적용**했다(43개 파일, 942+/692-). 이전에는 `cargo fmt --check` 가 50개 파일 219 hunks 로 실패했는데, 저장소 코드가 rustfmt 로 정리된 적이 없었기 때문이다.
- 이후 `cargo fmt --all -- --check` 는 통과하며, CI 게이트에도 같은 검사를 넣었다.
- 새 코드를 작성할 때는 `cargo format`(별칭)으로 적용하고 `cargo fmt-check` 로 확인한다.
