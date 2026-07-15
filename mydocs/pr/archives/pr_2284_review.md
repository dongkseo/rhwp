# PR #2284 검토 - #2279 footer 계측과 layout 정합 변경

- 검토일: 2026-07-15
- 작성자: planet6897
- 대상: [PR #2284](https://github.com/edwardkim/rhwp/pull/2284), [Issue #2279](https://github.com/edwardkim/rhwp/issues/2279)
- base / head: `devel` `f260475e3` / `ea57d1560`
- 규모: 36 files, +888/-77 (HWPX fixture 25개, renderer/typeset 5개 파일, Python 도구 4개)
- mergeable: `MERGEABLE` (검토 시점 참고값, merge 직전 최신 head 재확인 필요)
- 리뷰어: `jangster77` 지정 완료

## PR 본문과 후속 코멘트

본문은 footer 흡수/분할 임계의 계측과 측정/렌더 폭 진단만 추가한 **동작 불변** PR이라고 설명한다. 그러나 후속 코멘트 [#4977799850](https://github.com/edwardkim/rhwp/pull/2284#issuecomment-4977799850)는 다음 네 가지 renderer/typeset 동작 변경을 추가했다고 명시한다.

1. 1x1 중첩 셀의 nested table, 빈 문단 line box, 셀 말미 line spacing 유닛화
2. 본문 `NO_LS` 문단의 CharShapeRef 재분할과 재래핑
3. 재래핑 줄별 line pitch 재계산
4. RowBreak float의 선언 이월 증거를 구역 전역 flag가 아닌 host 문단 `saved_span`으로 판정

코드도 이를 확인한다. `table_layout.rs`의 `cell_units` 계산은 비어 있던 문단을 더 이상 건너뛰지 않고 nested table/빈 줄 높이를 추가하며, `composer.rs`와 `typeset.rs`는 본문 래핑과 RowBreak 이월 조건을 변경한다. 따라서 본문과 제목을 최종 head의 실제 변경 범위, 회귀 범위, 남은 제한으로 갱신해야 한다. 초기 계측-only 설명을 그대로 두면 merge 판단과 후속 추적의 근거가 달라진다.

`#2279`는 열린 상태이며, PR 코멘트가 보류한 footer/table/font 잔여 축도 남아 있다. 이 PR에는 `Closes #2279`가 없으므로 이슈를 자동 close하면 안 된다.

## 검증

- merge simulation: `upstream/devel` + PR head clean
- 로컬 build: `CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=target/pr-review-2284 cargo build` 통과
- focused regression: `cargo test --profile release-test --test issue_1891 --test issue_1921_59043_pagination_pin` 통과 (`86712=65`, `80168=157`, `76076=82`, `80250=17`, `59043=41`)
- 전체 회귀: `cargo test --profile release-test --tests` 통과
- `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `python3 -m py_compile tools/task2279/*.py` 통과
- `wasm-pack build --target web --out-dir pkg` 통과
- 최신 PR head의 GitHub Actions `Build & Test`, `Native Skia tests`, `CodeQL`, `Canvas visual diff` 통과. `WASM Build`와 `Frontend package gates`는 workflow 조건으로 skipped다.

## Visual Sweep

`samples/86712_regulatory_analysis.hwp`와 `pdf/issue1921/86712_regulatory_analysis-2024.pdf`로 PR head와 `devel`을 각각 비교했다. 두 빌드 모두 65쪽이고 structural flag는 0개였다.

- page 10: PR `pixel match 87.48438%`, `ink/proxy 5.46072%`; base `88.23770%`, `5.97180%`
- page 65: PR/base 모두 `pixel match 92.65181%`, `ink/proxy 12.12548%`; SVG는 byte-identical
- 대표 검증 asset: `mydocs/pr/assets/pr_2284/86712_p10_review.png` (SHA-256 `63b3916b08a37c460e16f63a2b21e1f71a5971a232c90793670eabf53a6600f3`), `mydocs/pr/assets/pr_2284/86712_p65_review.png` (SHA-256 `ae5fc7ae2f57605ab75fe6493d0d97674868004e769596e277ce79fcee599fa9`)

현재 macOS 검토 환경은 `Haansoft Batang`/`HY견명조`를 fontconfig에서 찾지 못하고 `Verdana`으로 fallback한다. 따라서 page 10 수치 하락만으로 코드 회귀를 단정하지는 않는다. 다만 이 환경에서는 PR 코멘트의 "기준선 상회" 시각 정합 주장을 독립적으로 재현하지 못했으므로, merge 근거로 사용하려면 저자 실행 환경의 기준 PDF, 폰트 조건, 전후 sweep 산출물을 함께 남겨야 한다.

## Findings

### P1 - 동작 변경 네 건에 대한 직접 회귀 oracle이 없다

기존 `issue_1891`의 네 문서 페이지 수 pin은 유지됐지만, 이번 변경은 page count만으로 구분되지 않는 셀 높이, CharShape 분할, 줄별 pitch, RowBreak 분할 정책을 바꾼다. 새 `tests/` 파일이나 기존 test 수정은 없고, `t172_gate.py`도 성공/실패를 판정하지 않는다. 적어도 86712의 pi=172 cell units, pi=20 줄별 pitch/CharShape, pi=30 RowBreak page-head 구조를 직접 검증하는 test를 추가해야 한다. 그래야 후속 renderer 변경이 같은 page count를 유지한 채 네 규칙을 되돌리는 것을 막을 수 있다.

### P2 - `code_slack_probe.py`의 문서상 기본 실행이 macOS/Linux에서 실패한다

후속 코멘트 [#4977851254](https://github.com/edwardkim/rhwp/pull/2284#issuecomment-4977851254)는 `python tools/task2279/code_slack_probe.py`만으로 재현된다고 하지만, 기본 `--exe` 값이 Windows 경로 `target\\debug\\rhwp.exe`다. macOS에서는 `FileNotFoundError`가 발생한다. OS별 default를 자동 선택하거나 `Path` 기반의 portable 경로를 사용하고, Windows 전용이면 코멘트/도구 사용법을 그에 맞게 제한해야 한다.

### P2 - `t172_gate.py`는 현재 gate가 아니라 진단 출력이다

이 도구는 `pages=65`와 한글 목표 `cut_sum=2282.6`, `r27=1401.9`를 출력하지만 assertion이나 non-zero exit가 없다. 검토 실행에서도 `cut_sum=2343.5`, `r27=1373.2`처럼 목표와 차이가 있어도 성공 종료했다. 이름과 PR의 "knife-edge gate" 주장에 맞게 pages/row height 허용 오차를 검사해 실패시 non-zero로 종료하거나, 단순 진단 도구로 이름과 설명을 낮춰야 한다.

### P2 - TSV가 `git diff --check`를 통과하지 않는다

`tools/task2279/flip_results_20260715.tsv`의 25개 데이터 행 끝에 trailing tab이 있어 `git diff --check`가 실패한다. 빈 컬럼을 유지해야 해도 행 끝 공백 없이 구조화된 TSV로 정리해야 한다.

## 최종 권고

**Request changes / merge 보류.** P1 회귀 oracle을 추가하고 PR 본문을 최종 head의 실제 동작 변경으로 갱신한 뒤, P2의 portable 실행·gate 의미·TSV 형식을 보완해야 한다. 갱신 head에서 focused test, 전체 회귀, 최신 CI, 기준 환경의 visual sweep을 다시 확인한다. `#2279`는 이 PR 병합 후에도 open으로 유지한다.
