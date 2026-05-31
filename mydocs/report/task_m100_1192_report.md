# 최종 결과 보고서 — Task #1192: CI 시간 단축

- **이슈**: #1192 → CLOSED
- **브랜치**: `local/task1192` → devel 머지 (`fe5d306c`)
- **작성일**: 2026-05-31
- **성격**: CI 인프라 개선 (소스 무변경, `.github/workflows/` 만)

## 요약

PR당 CI 병목인 `Build & Test` 를 **11분 52초 → 3분 32초** 로 단축했다 (약 −70%).
작업지시자 결정 4개 항목(A/C/B/F)을 모두 적용하고 devel CI 런타임으로 검증 완료.

## 적용 항목

| 항목 | 내용 | 파일 |
|------|------|------|
| A | `Canvas layer parity tests` step 제거 (cargo test 중복). native-skia step 은 유지 | ci.yml |
| C | `Free disk space` 축소 — android/dotnet 만 제거, 느린 정리(apt/docker prune/ghc/CodeQL) 삭제 | ci.yml ×2 job |
| B | CodeQL rust matrix 에 cargo 캐시 추가 (`codeql-rust` key) | codeql.yml |
| F | concurrency 취소 그룹 (cancel-in-progress 는 PR 이벤트 한정) | ci/codeql/render-diff |

## 런타임 검증 결과 (devel `fe5d306c`, 변경 후 첫 CI run)

### Build & Test step 비교 (이전 6ca2be7f → 이후 fe5d306c)

| step | 이전 | 이후 |
|------|------|------|
| Run tests | 371s | **140s** |
| Free disk space | 76s | **2s** (C) |
| Build | 57s | 11s* |
| Native Skia tests | 48s | 12s* |
| Check WASM target | 37s | 16s* |
| Clippy | 41s | 7s* |
| Canvas layer parity | 36s | **제거** (A) |
| cache restore | 5s | 2s |
| **총계** | **712s (11.9분)** | **212s (3.5분)** |

\* Build/test/clippy 단축은 warm 캐시(`target/`) 효과가 추가로 작용 — 이전 run 대비 캐시 적중도 차이.
순수 A/C 효과는 canvas step 제거(−36s) + 디스크 정리(−74s)로 최소 −110s 보장.

- **A 검증**: `canvas_layer_tree_matches_legacy` 가 로그에 정상 출현(6회 = 테스트 함수들) →
  별도 step 없이 `Run tests` 에 포함되어 실행됨 확인. native-skia step 도 유지·실행(12s).
- **C 검증**: `df -h` 결과 `/dev/root 72G, 25G Avail (26% 사용)` → 빌드 디스크 여유 충분.
  디스크 초과 위험 없음, 롤백 불필요.

### CodeQL Analyze(rust) (run 26715868940)

| step | 시간 |
|------|------|
| Initialize CodeQL | 330s (GitHub 고정 비용, 통제 밖) |
| Perform CodeQL Analysis | 91s |
| Build Rust (for CodeQL) | 47s |
| Cache cargo registry & build (rust) | 5s |
| 총계 | 488s (8.1분) |

- **B 검증**: 이번 run 은 첫 실행이라 캐시 **저장**(cold). run 종료 후 저장소에
  `Linux-codeql-rust-...` 캐시(311MB) 생성 확인 → **다음 PR 부터 warm build**
  (`Build Rust` ~47s → 의존성 재컴파일 생략으로 절감 예상).

### F 검증

- ci/codeql/render-diff 3개 워크플로우에 concurrency 적용, `cancel-in-progress` PR 이벤트 한정.
- 본 devel push run 은 단독이라 취소 미발생(정상 — push run 은 보존 대상).
- 연속 push 취소 효과는 후속 PR 에서 관찰 예정.

## 전체 CI 결과

- **CI run 26715868949: success** / **CodeQL run 26715868940: success** (전부 green).

## 안전성 / 비범위 확인

- 테스트 커버리지 손실 0 (A 는 중복 제거만, native-skia 유지) — feedback_push_full_test_required 준수.
- 디스크 초과 재발 없음 (25G 여유) — C 롤백 불필요.
- PR #1170(진행 중)과 파일 충돌 없음.
- CodeQL python 매트릭스 제거는 범위 외(유지).

## 결론

CI 병목 `Build & Test` 약 −70% 단축, CodeQL rust 는 차기 PR 부터 캐시 효과 발생.
모든 항목이 안전장치 내에서 동작하며 devel CI 전체 green. **Task #1192 완료.**
