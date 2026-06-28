# PR #1619/#1621/#1625/#1628 통합 리뷰 기록

## 메타

| 항목 | 내용 |
|------|------|
| 원 PR | #1619, #1621, #1625, #1628 |
| 작성자 | planet6897 |
| 통합 브랜치 | `integrate/planet6897-1619-1628` |
| base | `devel` |
| 관련 이슈 | #1618, #1620, #1624, #1627 |
| 원 PR 상태 | 문서 작성 시점 참고값: 모두 `OPEN`, `MERGEABLE` / `BEHIND` |
| 통합 방식 | 최신 `upstream/devel` 위에 원 PR head 커밋 4개를 cherry-pick |
| 통합 규모 | 문서 작성 시점 참고값: 19 files, +876 / -42 |
| 작성 시각 | 2026-06-28 17:35 KST |

## 시리즈 판단

네 PR은 모두 같은 작성자와 같은 날짜의 PR 묶음이지만, 하나의 브랜치 ancestry로 쌓인 stack 은 아니다. 각 PR head 는 같은 과거 `devel` 지점에서 분기한 squash commit 이다.

- #1619와 #1625는 `-1쪽` / footer pagination 흐름의 같은 문제 계열이다.
- #1621은 `clear_initial_field_texts` 패닉 수정으로 본문에서 독립 PR이라고 명시했다.
- #1628은 HWPX serializer bookmark 순서 보존 수정으로 본문에서 독립 PR이라고 명시했다.

이번 처리는 GitHub PR 4개를 개별 merge 하지 않고, 원 작성자 commit 4개를 보존 cherry-pick 한 통합 PR 로 준비한다.

## 변경 범위

| 원 PR | 관련 이슈 | 핵심 변경 |
|-------|-----------|-----------|
| #1619 | #1618 | LINE_SEG `vpos` reset 기반 페이지 예측과 표 row-split 가설을 전수 분석하고, `vpos_reset_analyze` 진단 example 과 보고 문서를 추가한다. |
| #1621 | #1620 | `clear_initial_field_texts` 의 다중 removal 처리에서 현재 텍스트 길이 기준 범위 가드를 추가해 빈 문단 slice panic 을 막는다. |
| #1625 | #1624 | footer `Page+Bottom` vpos 동기화를 본문 흐름과 plausibly 연결된 경우로 제한해 footer over-push +1쪽 부작용을 줄인다. |
| #1628 | #1627 | empty-text 객체-only 문단에서 bookmark 를 문단 시작으로 끌어올리지 않고 `para.controls` 순서대로 방출한다. |

통합 중 #1628 테스트 코드에서 최신 clippy `box_default` 경고가 발생해 maintainer 보정 commit `a9575dad1` 을 추가했다. 런타임 동작 변경 없이 `Box::new(Table::default())` 를 `Box::default()` 로 정리하고 불필요 import 를 제거한 변경이다.

## 커밋

| 통합 커밋 | 원 커밋 | 출처 |
|-----------|---------|------|
| `020e01c87` | `5d113e170e8689e1b329fb646974a1f254822ad1` | PR #1619 |
| `1a9ca5126` | `ef03b5d2102b4cb886a5d937d0bc0502f2c37a63` | PR #1621 |
| `d35548a00` | `cc2a1179af4cb3954f6d06e8e7de3139e0349b50` | PR #1625 |
| `95ac3230b` | `ae23562c79347861f802941aca471faf3428c4d4` | PR #1628 |
| `a9575dad1` | maintainer 보정 | #1628 테스트 clippy 경고 정리 |

## 로컬 검증

- `git diff --check upstream/devel...HEAD`: 통과
- `cargo fmt --check`: 통과
- `cargo build --release`: 통과
- `cargo test --release --lib`: 통과, 1980 passed / 7 ignored
- `cargo test --profile release-test --tests`: 통과
- `cargo clippy --all-targets -- -D warnings`: 최초 #1628 테스트 코드의 `box_default` 경고로 실패, `a9575dad1` 보정 후 재실행 통과
- `cargo test --doc`: 통과, 0 passed / 1 ignored
- `wasm-pack build --target web --out-dir pkg`: 통과. 이 플랫폼용 prebuilt `wasm-bindgen` 미제공으로 cargo install fallback 경고가 있었음.
- `cd rhwp-studio && npx tsc --noEmit`: 통과
- `cd rhwp-studio && npm test`: 통과, 147 passed
- `cargo test --test svg_snapshot`: 통과, 8 passed

## 리스크

- #1619는 결론형 분석 PR 이며 런타임 파이프라인 변경은 없다. 다만 새 진단 example 이 추가되므로 `clippy --all-targets` 검증이 필요했다.
- #1625는 pagination 동작 변경이다. 통합 테스트와 `svg_snapshot`은 통과했지만, merge 전 원격 `Build & Test` / Render Diff 결과를 최신 head 기준으로 확인해야 한다.
- #1628은 serializer 순서 변경이다. 기존에 보류된 char_shape 오프셋 문제는 본 PR 범위 밖이며 문서에 별도 보류로 남아 있다.
- 원 PR 4개는 통합 PR merge 후 superseded 처리 코멘트와 close 가 필요하다. 관련 이슈 #1618/#1620/#1624/#1627 도 자동 close 실패 가능성이 있으므로 merge 후 상태를 확인한다.

## 최종 권고

통합 PR 을 원본 저장소 `edwardkim/rhwp` 의 `devel` 대상으로 생성한다. PR 본문에는 `Closes #1618`, `Closes #1620`, `Closes #1624`, `Closes #1627` 을 포함한다. merge 전 조건은 통합 PR head 최신 커밋 기준 GitHub Actions 통과와 작업지시자 승인이다.
