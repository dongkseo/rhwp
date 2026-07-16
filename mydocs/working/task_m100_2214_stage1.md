# Task M100 #2214 Stage 1 완료보고서 — 연속 입력 표시 불일치 경계 계측

> 2026-07-13 정오표: 이 보고서의 최초 브라우저 통제만으로는 stale `PartialTable` cut과
> warm `cell_units_cache`를 분리하지 못했다. 최신 devel의 cold/warm·cache-only 통제가
> 직접 원인을 warm layout-cache 무효화 누락으로 좁혔다. 아래 §13이 §4~§12의
> “explicit pagination only” 및 “cut 공동 원인” 해석을 대체한다.

## 0. 판정 요약

- **계측 Stage 판정**: 완료
- **후속 구현 판정**: 최신 교차검증 완료, 수행·구현계획 보정 승인 대기
- **HWP**: RED, keyboard 3회 모두 44번째 추가 문자에서 4줄에서 5줄로 전환
- **HWPX**: RED, keyboard 3회 모두 같은 44번째 문자에서 전환
- **최초 불일치 경계**: 모델 text와 `LINE_SEG`는 최신이지만 deferred mutation이 warm
  `cell_units_cache`를 비우지 않아 PageLayerTree, Canvas와 caret이 새 5번째 줄을 잃음
- **통제 결과**: 같은 old cut에서도 cold는 exact, warm은 stale이며 pagination 없는 layout
  cache clear만으로 tree/cursor가 복구됨. full flush는 추가로 cut과 `cellBounds`를 갱신함
- **지연과의 관계**: 직접 연관됨. 새 offset이 stale fragment에 없어서
  `getCursorRectByPathNear()`가 115쪽을 탐색한 뒤 잘못된 첫 run으로 fallback하며 약 2.0초를 사용함
- **production 변경**: 없음

최신 결과에 따라 production 계약은 모든 deferred 셀 편집의 scoped layout-cache coherence와
실제 cell-flow advance가 바뀌는 입력의 pre-cursor 1회 flush로 분리한다. global cache clear를
generic partial invalidation에 넣는 한 줄 수정은 #1949/#2063 캐시를 매 키 전역 폐기하므로
진단 통제로만 유지한다.

## 1. 기준 환경

| 항목 | 값 |
|------|----|
| Git 기준 | `48c3345526d20720e9f0a80743bbfb8dde5813d4` |
| 브랜치 | `issue-2214-page-local-repaint` |
| worktree | `/private/tmp/rhwp-task2214` |
| WASM | `pkg/rhwp_bg.wasm`, 6,651,962 bytes, SHA-256 `f17f274b51ef968fe615ac1191cf87e46819623419609fd1f81094b851ba4148` |
| WASM 빌드 시각 | 2026-07-12 00:33:33 KST |
| Studio | Vite 8.1.4 dev server, `http://127.0.0.1:7714` |
| 런타임 | Node.js 24.15.0, npm 11.12.1 |
| 브라우저 | Google Chrome 150.0.7871.115, headless |
| 화면 | viewport 1280×900, DPR 1, Studio zoom 100% |
| 폰트 | `document.fonts.status=loaded`; HWP는 `대체 글꼴로 보기`, HWPX는 `그대로 보기` 뒤 `대체 글꼴로 보기` |
| 최종 실행 | 2026-07-12 01:08:55–01:10:36 KST |

픽스처:

| 형식 | 크기 | SHA-256 |
|------|-----:|---------|
| `samples/issue1949_giant_cell_nested_tables_perf.hwp` | 303,616 bytes | `ef10261cd29325116028e4f4f3e6be1a72c675eb771bddfd8484e7fe5aa94b4e` |
| `samples/issue1949_giant_cell_nested_tables_perf.hwpx` | 266,523 bytes | `fc6e5f156de470dfbb14aab392389491720ee7fb1bf6f03fe9a018e93b420c65` |

두 형식 모두 실제 앱 로드 뒤 `sourceFormat`, 115쪽, 활성 InputHandler, page 0 Canvas,
font ready를 단언했다. HWPX validation에서는 자동 보정을 선택하지 않아 모델 변형을 피했다.

### 1.1 최신 devel 교차검증 환경

| 항목 | 값 |
|------|----|
| Git 기준 | `c7864c62f3aea359d1a25ecc704af037c33e4a58` |
| worktree | `/private/tmp/rhwp-task2214-latest` (detached) |
| WASM | 6,662,474 bytes, SHA-256 `41d675bebe3c981903ef7c0ab67b0e38393c379a215f12693901d57e73f2cb92` |
| Studio | `http://127.0.0.1:7724`, HWP/HWPX 각 3회 |
| 산출물 | `output/poc/task2214/crosscheck-c7864c62/` (ignored) |
| production 변경 | 없음 |

## 2. 재현·계측 방법

### 2.1 실제 앱과 사용자 입력 경로

기존 E2E의 직접 `wasm.loadDocument()` helper를 사용하지 않고 다음 실제 앱 계약을 사용했다.

1. `open-document-bytes`에 bytes, file name, `skipUnsavedGuard: true`, 고유 `requestId`를 전달
2. 같은 `requestId`의 `open-document-bytes:done` 대기
3. validation 및 local font modal을 명시적으로 처리
4. font ready와 2 rAF 뒤 115쪽, InputHandler, Canvas를 단언

캐럿 위치는 다음 full cell path로 고정했다.

```text
sectionIndex=0, paragraphIndex=5, charOffset=130
parentParaIndex=0, controlIndex=2, cellIndex=2, cellParaIndex=5
cellPath=[{ controlIndex=2, cellIndex=2, cellParaIndex=5 }]
```

`clearSelection()` → `moveTo()` → `resetPreferredX()` → `updateCaret()` → `focus()` 순서를
사용했다.

### 2.2 결정성 및 시간축

- 형식마다 독립 reload 후 `page.keyboard.type('1')`로 한 글자씩 입력해 4줄에서 5줄로 바뀌는
  문자 수 `N`을 세 번 탐색했다.
- 상세 시간축은 다시 reload한 뒤 keyboard로 `N-1`자까지 입력하고, `N`번째 문자는 hidden
  textarea에 동기 `InputEvent('input')`을 dispatch했다.
- 동기 dispatch 안에서 handler 시간, model length/text, cursor offset, `LINE_SEG`를 읽었다.
- full 구조 및 합성 화면은 2 rAF, 100ms, 850ms, 1.6초, `N+1`, `N+2`에 수집했다.
- caret blink는 합성 crop에서 숨기고 DOM geometry를 별도 기록했다.

`sync-after-boundary`라는 full checkpoint는 CDP 호출 사이에서 rAF가 실행될 수 있으므로
“첫 rAF 전 Canvas” 권위로 쓰지 않았다. 동기 권위는 같은 dispatch task 안에서 반환한
model/line/cursor 값이고, 화면 권위는 명시적인 2 rAF 이후와 독립 통제군이다.

### 2.3 독립 통제군

다음 두 통제는 각각 깨끗한 reload에서 `N-1`과 `N`을 다시 입력해 수행했다.

1. **page full-layer only**: `{ pageIndex: 0, reason: 'unknown' }` invalidation으로
   `allowStaticOverlayReuse=false` 렌더. pagination은 하지 않음.
2. **explicit pagination**: `inputHandler.flushDeferredPaginationIfNeeded('issue-2214-e2e', false)`
   한 번 뒤 `document-view-changed`와 2 rAF.

## 3. 형식별 결정성

| 형식 | 실행 | `N` | keyboard p50 | p95 | 경계 입력 | 판정 |
|------|-----:|----:|-------------:|----:|----------:|------|
| HWP | 1 | 44 | 35.70ms | 37.46ms | 2,051.06ms | RED |
| HWP | 2 | 44 | 36.00ms | 37.44ms | 2,052.75ms | RED |
| HWP | 3 | 44 | 35.48ms | 36.87ms | 2,019.97ms | RED |
| HWPX | 1 | 44 | 34.29ms | 35.88ms | 1,996.10ms | RED |
| HWPX | 2 | 44 | 34.23ms | 36.25ms | 2,001.31ms | RED |
| HWPX | 3 | 44 | 33.79ms | 35.46ms | 2,014.45ms | RED |

`N`, 최초 stale 경계, 구조 값과 합성 crop hash가 형식 간에도 같았다. 이 증상은 HWP parser와
HWPX parser의 차이가 아니라 두 형식이 공통으로 들어가는 deferred edit/pagination 경계의 문제다.

## 4. 시간축 상태

아래 값은 최종 HWP 상세 실행이다. HWPX도 같은 구조 값을 보였고 실제 checkpoint 시간만
수십 ms 이내에서 달랐다.

| 체크포인트 | 실제 Δt | model len | 줄 시작 | cursor offset | PageLayerTree len | page layout len | 화면 hash |
|-----------|--------:|----------:|---------|--------------:|------------------:|----------------:|----------|
| 전환 직전 (`N-1`) | 22.9ms | 173 | `[0,44,84,122]` | 173 | 173 | 173 | `339b0be2…` |
| `N` 후 2 rAF | 2,134.7ms | 174 | `[0,44,84,122,129]` | 174 | 129 | 129 | `32315fda…` |
| 100ms checkpoint | 2,283.1ms | 174 | 동일 | 174 | 129 | 129 | `32315fda…` |
| 850ms checkpoint | 3,095.5ms | 174 | 동일 | 174 | 129 | 129 | `32315fda…` |
| 1.6초 checkpoint | 3,906.1ms | 174 | 동일 | 174 | 129 | 129 | `32315fda…` |
| `N+1` 후 2 rAF | 5,984.5ms | 175 | 동일 | 175 | 129 | 129 | `32315fda…` |
| `N+2` 후 2 rAF | 8,017.5ms | 176 | 동일 | 176 | 129 | 129 | `32315fda…` |
| explicit flush 후 | 별도 실행 | 174 | `[0,44,84,122,129]` | 174 | 174 | 174 | `20835c0e…` |

핵심 상태 전이는 다음과 같다.

```text
N-1:
  model 173 / LINE_SEG 4줄 / page fragment 173 / 화면에 43자 표시

N:
  model 174 / LINE_SEG 5줄(새 줄 시작 129)
  page fragment 및 두 page-tree 조회는 정확히 129에서 끝남
  새 5번째 줄 129..174가 page tree, Canvas, caret 좌표에 없음

explicit pagination:
  model과 LINE_SEG는 그대로 유지
  page fragment 및 page tree가 174까지 확장
  새 줄, 44자와 올바른 caret이 표시됨
```

따라서 모델 데이터는 손실되지 않았다. 표시 계층이 임의로 글자를 누락한 것도 아니다.
새 tree는 만들어졌지만 stale `PartialTable` slice basis가 새 줄 시작점 129에서 tree를 잘랐다.

### 4.1 caret 상태

| 상태 | cursor rect | DOM caret | cell bounds 높이 |
|------|-------------|-----------|------------------:|
| `N-1` | page 0, `(663.7, 315.5)` | `(683.85, 325.5)` | 945.9 |
| `N` stale fallback | page 0, `(84.1, 238.7)` | `(104.25, 248.7)` | 945.9 |
| explicit flush | page 0, `(569.7, 341.1)` | `(589.85, 351.1)` | 971.5 |

`cellOverflowed`는 모든 상태에서 false였다. 큰 셀의 기존 가시 높이 안에서 줄이 늘어났으므로
현재 overflow 기반 즉시 flush guard는 이 전환을 감지하지 못한다.

## 5. 호출 trace와 800ms 가설

상세 시간축의 형식별 호출 수는 동일했다.

| 구간 | pagination flush | page invalidation | filtered render | `PageRenderer` | `document-changed` |
|------|-----------------:|------------------:|----------------:|---------------:|-------------------:|
| `N`, `N+1`, `N+2` | 0 | 3 | 3 (`flow`) | 3 | 0 |
| page full-layer 통제 | 0 | 2 | 2 (`flow`) | 2 | 0 |
| explicit 통제 전 | 0 | 1 | 1 (`flow`) | 1 | 0 |
| explicit 통제 후 누계 | 1 | 1 | 3 (`flow`, visible page 0·1) | 3 | 0 |

text-edit render context는 `{ reason:'text-edit', allowStaticOverlayReuse:true }`, page full-layer
통제는 `{ reason:'unknown', allowStaticOverlayReuse:false }`였다.

- 모든 text-edit render의 `needsTextEditStaticLayerVerification`은 false였다.
- 800ms verification timer는 한 번도 예약되지 않았다.
- 2 rAF부터 1.6초까지 page tree와 화면 hash가 변하지 않았다.

따라서 “첫 화면은 최신인데 800ms 검증이 오래된 화면을 덮는다”는 가설은 기각한다.

## 6. 통제 대조

### 6.1 pagination 없는 page full-layer render

- `wasm.flushDeferredPagination`: 0회
- static overlay reuse: 비활성
- PageLayerTree/page layout 길이: 129 그대로
- 합성 crop hash: `32315fda…` 그대로

fresh full-layer render도 `PartialTable` cut과 cell-unit 입력을 새로 조판하지 않는다. 따라서 static
layer reuse, Canvas rAF coalescing 또는 단순 page invalidation 누락은 최초 원인이 아니다.

### 6.2 explicit pagination 후 full refresh

- 통제 전 flush: 0회
- 의도한 raw `wasm.flushDeferredPagination`: 정확히 1회
- raw pagination: HWP 872.7ms, HWPX 895.3ms
- cursor 재배치와 view refresh 요청까지: HWP 945.6ms, HWPX 972.2ms
- PageLayerTree/page layout 길이: 129에서 174로 복구
- caret: `(84.1,238.7)` fallback에서 `(569.7,341.1)`로 복구

stale 화면과 flush 정답 crop의 비교는 두 형식에서 동일했다.

| 비교 | changed pixels | diff ratio | raw diff bbox | max channel delta |
|------|---------------:|-----------:|---------------|------------------:|
| 2 rAF vs flush | 8,644 | 4.5418% | `(85,140)–(670,229)` | 255 |
| page full-layer vs flush | 8,644 | 4.5418% | `(85,140)–(670,229)` | 255 |

두 비교가 완전히 같다는 점도 page full-layer render가 상태를 바꾸지 못했음을 보여준다.

## 7. 약 2초 입력 지연의 원인과 표시 버그와의 관계

### 7.1 브라우저 세분 계측

| 형식/입력 | 전체 handler | deferred insert | `getCursorRectByPathNear` | page render |
|-----------|-------------:|----------------:|--------------------------:|------------:|
| HWP `N` | 1,994.3ms | 0.2ms | 1,993.4ms | 14.6ms |
| HWP `N+1` | 1,990.2ms | 1.7ms | 1,988.0ms | 14.6ms |
| HWP `N+2` | 1,964.9ms | 1.4ms | 1,963.1ms | 14.2ms |
| HWPX `N` | 2,050.3ms | 0.1ms | 2,049.5ms | 14.9ms |
| HWPX `N+1` | 2,050.0ms | 1.3ms | 2,047.5ms | 15.3ms |
| HWPX `N+2` | 2,052.2ms | 1.5ms | 2,050.2ms | 15.3ms |

지연은 글자 삽입, 새 `LINE_SEG` 계산 또는 Canvas render가 아니다. 거의 전부가 mutation 뒤
`cursor.moveTo()`에서 호출되는 `getCursorRectByPathNear()`에 있다.

### 7.2 코드 경로

1. `src/document_core/commands/text_editing.rs`의 deferred insert는 live 모델 text, `LINE_SEG`와
   vpos를 갱신한 뒤 `invalidate_page_tree_cache_from(0)`만 호출한다.
2. 이 부분 무효화는 page-tree entry만 비운다. full invalidate와 달리
   `layout_engine.clear_layout_caches()`를 부르지 않아 포인터 키 `cell_units_cache`가 4줄 기준을
   유지하고, `paginate_immediately=false`라 이전 `PageItem::PartialTable.start_cut/end_cut`도 남는다.
3. 다음 조회는 이전 tree를 재사용하지 않고 새 tree를 만든다. 하지만 이전 pagination의
   PageContent와 `PartialTable` cut, stale cell units를 slice 입력으로 사용하므로 live 5줄을 compose한
   뒤에도 가시 범위를 129에서 자른다.
4. `rhwp-studio/src/engine/input-handler.ts`의 operation router가 새 offset으로 cursor를 옮긴다.
5. `rhwp-studio/src/engine/cursor.ts`는 직전 page 0을 hint로
   `getCursorRectByPathNear(sec, parent, path, offset, 0)`을 호출한다.
6. `src/document_core/queries/cursor_rect.rs`는 hint page와 ±1을 먼저 본 뒤 host paragraph가 걸친
   나머지 페이지를 모두 순회하며 정확한 offset을 포함하는 TextRun을 찾는다.
7. stale slice는 129에서 끝나므로 offset 174가 어느 페이지에도 없다. 정확 검색 전체가 실패한
   뒤 두 번째 fallback 순회가 page 0의 첫 matching run `(84.1,238.7)`을 반환한다.

기존 ignored #2021 probe도 이 샘플의 host paragraph가 page 0부터 114까지 **115쪽 전체에 걸쳐
있음**을 확인했다. native release-test에서 page 0 한 장 cold build는 약 28ms였다. 브라우저에서는
정확 offset을 못 찾는 동안 115쪽 후보를 순회하고, 각 입력이 cache를 다시 무효화하므로 이 비용이
매번 약 2초로 반복된다.

따라서 이 재현의 성능 버그는 별개의 Canvas 병목이 아니다. 정확성 결함과 성능 결함은 같은
stale `PartialTable` slice를 원인 상태로 가지며, exact cursor miss의 115쪽 탐색이 지연을 증폭한다.

```text
stale pagination fragment가 새 줄을 누락
  ├─ page tree와 Canvas가 새 문자를 그릴 수 없음
  └─ cursor offset을 어느 fragment에서도 못 찾음
       → 115쪽 exact search
       → 첫 run fallback
       → 약 2초 입력 지연 + 잘못된 caret
```

단, 이 결론은 줄 경계 이후의 단일 글자 지연에 대한 것이다. #2193의 문서 로드, 일반 입력,
pagination, page-tree 및 Canvas 전체 성능 범위를 이 Stage에서 모두 해결했다는 뜻은 아니다.

## 8. 원인 판정

Stage 1 브라우저 통제 당시 선택한 판정표 행은 다음이었다. 최신 정오표 §13은 이 관찰을
**warm cell-unit cache가 직접 원인이고 old cut은 충분조건이 아님**으로 더 좁힌다.

> 모델/LINE_SEG는 최신이고 page layout/tree가 stale이며 page full-layer render로 복구되지 않고
> explicit pagination만 복구한다 — stale pagination fragment.

확정한 사실:

- 모델 text는 `N`, `N+1`, `N+2`를 모두 정확히 보존한다.
- `LINE_SEG`는 `N`에서 즉시 5줄 `[0,44,84,122,129]`로 바뀐다.
- warm `cell_units_cache`를 사용하는 tree는 새 line 129..174를 포함하지 않는다.
- uncached page text layout과 cached PageLayerTree가 모두 같은 129 cutoff를 보인다.
- full-layer Canvas rebuild만으로는 복구되지 않는다.
- Stage 1 브라우저 통제에서는 explicit pagination만 fragment, Canvas, caret을 함께 복구했다.
  최신 native 통제에서는 pagination 없는 cache clear도 tree/cursor를 복구했다.
- HWP/HWPX가 같은 경계를 보인다.

기각한 후보:

- document-core text mutation 또는 `LINE_SEG` 데이터 손실
- HWP 전용 import semantic
- 잘못된 invalidated page 하나만의 문제
- Canvas static layer reuse 단독 문제
- 800ms delayed verification/render generation 문제
- caret DOM만의 문제

## 9. 검증 결과

| 검증 | 결과 |
|------|------|
| Docker WASM build | 통과 |
| WASM SHA-256 고정 | 통과 |
| `npm test` | 186/186 통과 |
| E2E `node --check` | 통과 |
| HWP keyboard 결정성 3회 | `N=[44,44,44]`, 통과 |
| HWPX keyboard 결정성 3회 | `N=[44,44,44]`, 통과 |
| HWP/HWPX 상세 시간축 및 독립 통제 | 통과, 두 형식 RED |
| explicit control 전 full pagination 0회 | 통과 |
| page full-layer 통제의 full pagination 0회 | 통과 |
| explicit control raw pagination 1회 | 통과 |
| #2021 ignored native probe | 1 passed, host paragraph 115쪽 확인 |

최종 진단 명령:

```bash
cd /private/tmp/rhwp-task2214/rhwp-studio
VITE_URL=http://127.0.0.1:7714 \
CHROME_PATH="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
CHROME_EXTRA_ARGS="--force-device-scale-factor=1" \
node e2e/issue-2214-page-local-repaint.test.mjs \
  --mode=headless --diagnose --runs=3
```

## 10. 변경·비변경 범위

Tracked Stage 1 산출물:

- `rhwp-studio/e2e/issue-2214-page-local-repaint.test.mjs`
- `mydocs/working/task_m100_2214_stage1.md`
- 승인된 수행·구현계획서와 날짜별 할일 문서

변경하지 않은 범위:

- `src/` production Rust
- `rhwp-studio/src/` production TypeScript
- pagination 정책, renderer, cursor implementation

raw JSON/PNG, diff, WASM, `node_modules`, Cargo `target`은 각각 ignored 임시 산출물이며 커밋하지 않는다.

```text
output/poc/task2214/stage1/
  summary.json
  hwp-diagnostic.json
  hwpx-diagnostic.json
  hwp/{timeline,full-layer-control,flush-control,diff}/
  hwpx/{timeline,full-layer-control,flush-control,diff}/
```

## 11. 완료·중단 조건 판정

완료한 조건:

- 실제 앱과 keyboard 경로에서 HWP 3회 같은 RED
- HWPX 3회 반복 가능한 RED 분류
- 형식별 `N`과 최초 불일치 경계 안정
- explicit 대조 전 pagination flush 0회
- 두 통제군을 독립 reload로 실행
- production 변경 없이 단일 원인으로 축소

당시 중단 조건:

- pagination 없는 full-layer render는 복구하지 못함
- 시험한 브라우저 대조군 중 explicit pagination만 노출된 복구 수단이었음
- 승인된 “입력 중 전체 pagination 0회” 조건만으로 production 해결안을 선택할 수 없음

따라서 Stage 1 계측 뒤 후속 구현을 중단했다. §13의 추가 통제로 해당 중단 사유는
“zero-pagination 복구 불가”에서 “scoped coherence와 flow-boundary flush 계약 재승인 필요”로
대체됐다.

## 12. 다음 단계와 계획 보정안

최초 제안의 방향인 “경계 1회 flush로 geometry 정확성을 우선 복구”는 유지하되, 이를 모든
cache coherence 문제의 대체재로 사용하지 않는다. 최신 결과에 따라 다음 순서로 보정한다.

1. cold/warm/cache-only/full-flush 네 상태와 cell-flow signal을 회귀 계약으로 고정한다.
2. 모든 deferred 셀 편집에서 편집 cell과 소유 table cache만 scoped eviction한다.
3. 단순 line-count가 아니라 상대 flow advance 변화인 `cellFlowChanged`를 반환한다.
4. `cellFlowChanged=true`일 때만 cursor 조회 전에 full flush를 정확히 1회 수행한다.
5. 안정 입력은 flush 0회, 44번째 경계는 1회, 50자 누계는 1회로 고정한다.
6. 남는 약 0.95초 경계 flush의 partial paginator 대체는 #2193 후속으로 분리한다.

## 13. 최신 devel 원인 격리 정오표

### 13.1 native cold/warm matrix

HWP/HWPX, batch/sequential, direct/path-near의 30-case matrix 결과는 같았다.

| 상태 | model/tree max | cursor | query | `end_cut` |
|------|----------------|--------|------:|----------:|
| cold sequential 44 | 174/174 | `(569.7,341.9)` exact | 약 27ms | 37 |
| prewarm sequential 44 | 174/129 | `(84.1,238.7)` fallback | 약 1.95s | 37 |
| cold sequential 50 | 180/180 | `(629.7,341.9)` exact | 약 27ms | 37 |
| 30자 warm + 20자 | 180/129 | `(84.1,238.7)` fallback | 약 1.94~1.97s | 37 |

같은 final model과 cut에서 cold만 정확하므로 old `PartialTable` cut은 직접 원인의 충분조건이
아니다. query API나 입력 단위도 판정을 바꾸지 않았다.

### 13.2 pagination 없는 cache-only 격리

warm 44자 상태에서 pagination 없이 full cache invalidation만 수행한 crate-internal 통제는
두 형식에서 동일했다.

| 상태 | tree max | cursor | cut | `cellBounds.h` |
|------|---------:|--------|----:|---------------:|
| stale warm | 129 | fallback | 37 | 945.9 |
| cache clear only | 174 | exact | 37 | 945.9 |
| explicit full flush | 174 | exact | 38 | 971.5 |

이 통제로 visible tree/cursor의 직접 원인은 warm `cell_units_cache`임을 확정했다. 동시에
cache-only는 geometry 전체를 확정하지 않으므로 production은 scoped eviction과 flow-boundary
flush를 분리해야 한다.

### 13.3 최신 Studio와 성능 관계

최신 WASM에서도 HWP/HWPX 각 3회 모두 `N=44` RED였다. 일반 입력 p50은 약 35~36ms,
경계 handler는 HWP 약 1.97초, HWPX 약 2.00~2.02초였다. mutation 자체는 약 0.2ms이고
지연 대부분은 stale tree에서 새 offset을 찾지 못한 path-near 115쪽 scan이다. full flush는
약 0.93~0.95초였다.

따라서 표시 결함과 약 2초 입력 지연은 같은 cache coherence 결함이다. #2214에서는 정확한
tree/cursor와 경계 1회 flush를 해결하고, 그 1회 비용을 bounded/partial paginator로 줄이는
작업은 #2193 후속 범위로 남긴다.

### 13.4 산출물

- native matrix: `output/poc/task2214/crosscheck-c7864c62/native-matrix.json`
- Studio summary: `output/poc/task2214/crosscheck-c7864c62/studio/summary.json`
- HWP/HWPX diagnostics: 같은 경로의 `studio/{hwp,hwpx}-diagnostic.json`
- 위 산출물과 latest worktree test-only probe는 ignored 진단 자료이며 production 변경이 아니다.
