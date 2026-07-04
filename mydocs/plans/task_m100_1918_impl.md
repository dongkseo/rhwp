# 구현 계획서 - Task #1918

**이슈**: #1918 표 입력/삭제 시 워터마크·정적 이미지 레이어 포함 페이지가 매 입력마다 고비용 재렌더링됨  
**브랜치**: `local/task1918`  
**마일스톤**: M100 / v1.0.0 editing foundation stage  
**수행계획서**: `mydocs/plans/task_m100_1918.md`  
**작성일**: 2026-07-05  

---

## 1. 설계 요지

이번 작업의 1차 목표는 표 입력/삭제 후 발생하는 page-local refresh에서 **변하지 않은 정적 이미지 레이어를
반복 렌더링하지 않도록 하는 것**이다. 표 텍스트 편집 자체는 빠르며, 병목은 편집 후 같은 페이지의
정적 이미지성 요소가 매 입력마다 다시 렌더링되는 경로에 있다.

Stage 1 계측 결과, 문제 샘플은 두 부류로 나뉜다.

- `samples/복학원서.hwp`: 같은 text edit refresh에서 `flow/background/behind/front`가 모두 반복 렌더링되는
  overlay 재생성 병목.
- `samples/253E164F57A1BC6934-empty.hwp`, `samples/143E433F503322BD33.hwp`: 별도 behind/front overlay는
  없지만 flow plane 내부의 정적 이미지/OLE·RawSvg 비용이 커지는 병목.

따라서 Stage 2-3은 overlay 반복 렌더링 제거에 집중하고, Stage 4에서는 이미지 decode 재렌더 안전망과 함께
flow 내부 정적 이미지 비용을 완화하는 별도 경로를 검토한다.

수정은 `rhwp-studio` 렌더 갱신 경로를 중심으로 봉인한다. Rust 렌더러의 이미지 해석이나 HWP 파서
동작은 변경하지 않는다. 다만 이미 존재하는 경량 API인 `getPageOverlayImages`가 page layer summary를
위해 활용 가능한지 검증하고, 부족하면 summary 전용 확장을 최소 범위로 추가한다.

핵심 방향:

- `document-page-invalidated`의 `reason: 'text-edit'`를 page-local text edit fast path로 사용한다.
- `PageRenderer`에 정적 overlay canvas 재사용 경로를 추가한다.
- overlay 재사용 여부는 보수적인 cache key로 판단한다.
- flow canvas는 계속 다시 렌더링하되, background/behind/front overlay canvas는 변하지 않으면 유지한다.
- overlay summary는 replay-plane 기준으로 계산해 부모 layer의 `TextWrap`으로 분류되는 이미지까지 포함한다.
- 이미지 디코드 재렌더 안전망은 유지하되, 동일 이미지 count에서 overlay까지 반복 재렌더링하지 않도록 분리한다.
- overlay가 없는 문서의 flow 내부 이미지/OLE 비용은 Stage 4에서 별도 완화 대상으로 다룬다.

## 2. 현재 코드 기준

라인 번호는 `local/task1918` 생성 시점의 `054be69c` 기준이다.

| 경로 | 현재 동작 |
|------|-----------|
| `rhwp-studio/src/engine/input-handler.ts:2234` | `afterPageLocalEdit`가 `document-page-invalidated`를 `reason: 'text-edit'`와 함께 emit |
| `rhwp-studio/src/view/canvas-view.ts:274` | `refreshInvalidatedPage`가 기존 canvas에 `renderCanvas`를 호출 |
| `rhwp-studio/src/view/canvas-view.ts:156` | `renderCanvas`가 page 위치/scale을 설정하고 `PageRenderer.renderPage` 호출 |
| `rhwp-studio/src/view/page-renderer.ts:25` | `renderPage`가 flow를 렌더한 뒤 `applyOverlays` 수행 |
| `rhwp-studio/src/view/page-renderer.ts:97` | `applyOverlays` 시작 시 기존 overlay canvas를 무조건 제거 |
| `rhwp-studio/src/view/page-renderer.ts:162` | `createFilteredCanvasLayer`가 background/behind/front layer마다 WASM 렌더 호출 |
| `rhwp-studio/src/view/page-renderer.ts:227` | `getLayerPlaneSummary`가 전체 `PageLayerTree` JSON을 받아 parse |
| `rhwp-studio/src/view/page-renderer.ts:344` | 이미지 decode 안전망 재렌더가 flow와 overlay를 모두 다시 렌더 |
| `src/document_core/queries/rendering.rs:761` | `get_page_layer_tree_native`는 전체 tree JSON 반환 |
| `src/document_core/queries/rendering.rs:774` | `get_page_overlay_images_native`는 behind/front image 목록과 imageCount를 반환하는 경량 API 후보 |

## 3. 단계 (5단계, stage-gated)

### Stage 1 - 재현 계측과 fast path 경계 고정

목표: 구현 전후를 비교할 기준과 "어떤 refresh에서 overlay 재사용을 허용할지"를 고정한다.

작업:

- `samples/복학원서.hwp`, `samples/253E164F57A1BC6934-empty.hwp`,
  `samples/143E433F503322BD33.hwp`, `samples/table-001.hwp`의 현재 벤치 값을 재확인한다.
- `rhwp-studio`에 임시 계측 또는 개발자 콘솔 계측 지점을 두어 다음 호출 횟수와 시간을 확인한다.
  - `renderPageToCanvasFiltered(..., 'flow')`
  - `renderPageToCanvasFiltered(..., 'background' | 'behind' | 'front')`
  - `getPageLayerTree`
  - `getPageOverlayImages`
- `refreshInvalidatedPage` payload의 `reason === 'text-edit'`만 fast path 대상으로 삼는 설계를 확정한다.
- 표 편집이 page count를 바꾸거나 page index가 유효하지 않을 때는 기존 full refresh로 유지한다.

산출:

- Stage 1 보고서: 기준 벤치, 호출 횟수, fast path 적용 조건.
- 소스 변경이 필요하면 계측 코드는 최종 구현에 남기지 않는다.

검증:

- `target/debug/rhwp bench ... -n 5`
- 수동 또는 E2E 기반 입력 계측 1회.

### Stage 2 - overlay summary 경량화

목표: 매 page-local text edit마다 전체 `PageLayerTree` JSON을 parse하지 않도록 한다.

작업:

- `WasmBridge.getPageOverlayImages(pageNum)`를 `PageRenderer`에서 사용할 수 있도록 summary parser를 추가한다.
- 기존 `LayerPlaneSummary`를 다음 형태로 확장한다.
  - `hasBehind`
  - `hasFront`
  - `imageCount`
  - `rawSvgCount`
  - `signature`
- `getPageOverlayImages`가 제공하는 `hasBehind`, `hasFront`, `imageCount`, `rawSvgCount`를 먼저 사용한다.
- Rust 쪽 `getPageOverlayImages`는 이미지 자체의 `text_wrap`뿐 아니라 부모 `RenderLayerInfo`를 반영한
  replay-plane 기준으로 `hasBehind`/`hasFront`를 계산한다.
- `rawSvgCount` 또는 page background 여부가 부족하면 다음 중 보수적인 쪽을 선택한다.
  - 전체 `PageLayerTree` fallback을 유지한다.
  - Rust에 summary 전용 API를 최소 추가한다.
- summary 실패 시 기존 `getPageLayerTree` 경로로 fallback한다.

가드레일:

- summary API가 부모 layer 기반 behind/front 분류를 빠뜨리지 않는지 `samples/복학원서.hwp`로 확인한다.
- `samples/253E164F57A1BC6934-empty.hwp`는 overlay가 아니라 flow 내부 정적 이미지 비용이 핵심일 수 있으므로,
  Stage 2 완료 판단에서 이 샘플의 지연을 overlay 개선만으로 해결했다고 보지 않는다.
- `Picture` 워터마크만 기준으로 분기하지 않는다.
- RawSvg/OLE는 이미지 decode 재렌더 트리거에 계속 포함한다.

테스트:

- `rhwp-studio/tests/render-backend.test.ts`에 정적 검사 추가 후보:
  - `PageRenderer`가 page-local overlay summary에 `getPageOverlayImages`를 우선 사용한다.
  - 실패 시 `getPageLayerTree` fallback이 남아 있다.
- Rust API를 변경하는 경우 `src/wasm_api/tests.rs` 또는 rendering query 테스트 추가.

검증:

- `cd rhwp-studio && npm test -- render-backend`
- Rust API 변경 시 관련 `cargo test` subset.

### Stage 3 - text edit 전용 overlay canvas 재사용

목표: `reason: 'text-edit'`로 들어온 page-local refresh에서 정적 overlay canvas를 재사용한다.

작업:

- `CanvasView.refreshInvalidatedPage`에서 payload reason을 파싱해 `renderCanvas`에 refresh reason을 전달한다.
- `CanvasView.renderCanvas`와 `PageRenderer.renderPage`에 선택적 render context를 추가한다.
  - 예: `{ reason?: 'text-edit' | 'unknown'; allowStaticOverlayReuse?: boolean }`
- `PageRenderer`에 page/layer 단위 cache key를 둔다.
  - page index
  - render scale
  - canvas physical width/height
  - layer kind
  - summary signature
  - render profile
  - backend
- `applyOverlays`가 기존 overlay canvas를 무조건 제거하지 않고, text edit fast path에서 key가 같으면 유지한다.
- flow canvas는 항상 다시 렌더링하고 margin guide도 다시 그린다.
- background/behind/front overlay는 key miss 또는 non-text refresh에서 기존처럼 새로 렌더링한다.

가드레일:

- zoom, DPR, page size, page left/top/transform이 달라지면 overlay box는 반드시 갱신한다.
- overlay canvas를 재사용하더라도 `applyPageLayerBox`는 매번 호출해 위치와 CSS 크기를 최신화한다.
- page count 변화, pageIndex 불일치, canvas pool release, full `document-changed`에서는 기존처럼 overlay를 제거한다.
- CanvasKit backend는 우선 대상에서 제외하고 기존 경로를 유지한다.

테스트:

- `rhwp-studio/tests/render-backend.test.ts` 또는 신규 `page-renderer-static-overlay.test.ts`:
  - text edit refresh에서 `createFilteredCanvasLayer` 계열 또는 `renderPageToCanvasFiltered(..., 'behind')`가 재호출되지 않는 구조 검증.
  - scale/signature 변경 시 overlay 재렌더가 발생하는 구조 검증.
  - non-text refresh에서는 기존처럼 overlay를 재생성하는 구조 검증.

검증:

- `cd rhwp-studio && npm test`
- 대표 샘플 수동 입력 확인.

### Stage 4 - flow 내부 정적 이미지 비용과 decode 재렌더 안전망 분리

목표: Task #1154/#1456 계열의 이미지 decode 안전망은 유지하되, text edit마다 overlay 또는 flow 내부 정적
이미지/OLE까지 반복 고비용 처리되는 일을 줄인다.

작업:

- `scheduleReRender`와 `reRenderPageCanvases`에 render context 또는 overlay reuse policy를 전달한다.
- text edit fast path에서 이미지 count/signature가 기존과 같으면 지연 재렌더가 flow 중심으로만 동작하도록 분리한다.
- 최초 렌더, imageCount 변화, rawSvgCount 변화, overlay signature 변화 시에는 기존처럼 overlay까지 재렌더한다.
- overlay가 없는 페이지는 `samples/253E164F57A1BC6934-empty.hwp`, `samples/143E433F503322BD33.hwp` 기준으로
  flow 내부 정적 이미지/OLE가 매 입력마다 다시 decode·draw 되는지 확인하고, 가능한 경우 정적 이미지 summary/cache
  또는 image prefetch 경로로 비용을 낮춘다.
- `prefetchLayerImages`가 전체 `PageLayerTree` JSON을 정규식으로 훑는 비용을 유지해야 하는지 재검토한다.
  - 가능한 경우 Stage 2의 경량 overlay JSON을 사용한다.
  - rawSvg data URL 추출이 필요하면 기존 fallback을 유지한다.

가드레일:

- 이미지가 최초 로드되지 않아 빈 상태로 남는 #1154/#1456 회귀를 만들지 않는다.
- OLE/rawSvg가 있는 `samples/143E433F503322BD33.hwp`는 반드시 확인한다.
- behind/front overlay가 없는 페이지에서는 추가 비용이 생기지 않아야 한다.

테스트:

- 기존 `rhwp-studio/e2e/issue-1456-chart-rerender.test.mjs`를 유지 실행한다.
- 필요 시 renderer contract 테스트에 `scheduleReRender`가 overlay 재사용 정책을 유지한다는 정적 검사 추가.

검증:

- `cd rhwp-studio && npm test`
- `cd rhwp-studio && npm run e2e:renderer-contract`
- 필요 시 `node e2e/issue-1456-chart-rerender.test.mjs`.

### Stage 5 - 성능 검증, 회귀 확인, 보고

목표: 샘플별 체감 지연과 렌더 호출이 실제로 줄었는지 확인하고 문서화한다.

작업:

- 문제 샘플 3종에서 표 입력/삭제 후 다음 항목을 before/after로 기록한다.
  - flow 렌더 횟수
  - background/behind/front 렌더 횟수
  - `getPageLayerTree`/`getPageOverlayImages` 호출 횟수
  - 1회 입력 후 refresh 소요 시간
- `samples/table-001.hwp`에서 일반 표 편집 무회귀 확인.
- 시각 확인:
  - 워터마크/배경 이미지가 편집 전후 유지되는지 확인
  - 커서/선택 표시가 overlay 뒤/앞에 가려지지 않는지 확인
  - zoom 변경 후 overlay 위치가 틀어지지 않는지 확인
- 단계별 보고서와 최종 보고서 작성.

검증 명령:

```bash
cargo test
```

```bash
cd rhwp-studio
npm test
npm run build
```

필요 시:

```bash
cd rhwp-studio
npm run e2e:renderer-contract
node e2e/issue-1456-chart-rerender.test.mjs
```

## 4. 캐시 키 초안

초기 구현은 보수적인 문자열 signature를 사용한다.

```text
page={pageIdx}
scale={renderScale}
width={canvas.width}
height={canvas.height}
profile={renderProfile}
backend={backend}
hasBehind={summary.hasBehind}
hasFront={summary.hasFront}
imageCount={summary.imageCount}
rawSvgCount={summary.rawSvgCount}
behindSig={behind image bbox/wrap/effect/brightness/contrast/opacity/crop/transform/base64 length or hash}
frontSig={front image bbox/wrap/effect/brightness/contrast/opacity/crop/transform/base64 length or hash}
```

주의:

- base64 전체를 key에 넣지 않는다. 길이와 짧은 hash를 사용한다.
- key 계산이 이미지 재렌더보다 비싸지 않게 한다.
- Rust summary가 hash를 제공하지 않으면 Stage 2에서는 JSON 문자열 자체의 간단한 hash를 사용하고,
  Stage 3 이후 필요할 때 Rust summary hash를 추가한다.

## 5. 회귀 리스크

| 리스크 | 대응 |
|--------|------|
| overlay stale로 워터마크/앞쪽 객체 위치가 틀어짐 | scale/page size/transform/signature mismatch 시 재렌더 |
| 편집으로 페이지 구조가 바뀌었는데 overlay 재사용 | pageCount/pageIndex 유효성 검사 실패 시 기존 full refresh |
| `BorderFill` 이미지 채움 누락 | `253E164F57A1BC6934-empty.hwp`를 Stage 2/5 필수 검증 샘플로 사용 |
| OLE/rawSvg 재렌더 회귀 | `143E433F503322BD33.hwp`와 `issue-1456-chart-rerender` 확인 |
| 이미지 decode 안전망 회귀 | 최초 렌더와 imageCount/signature 변화 시 기존 재렌더 유지 |
| 메모리 증가 | page 단위 overlay cache만 사용하고 `releaseAllRenderedPages`/`removePageLayers`/`dispose`에서 해제 |
| CanvasKit 경로 회귀 | 이번 단계에서는 Canvas2D overlay 경로에 한정, CanvasKit은 기존 동작 유지 |

## 6. 커밋 계획

단계별 완료보고서와 함께 stage 단위로 커밋한다.

| Stage | 커밋 메시지 |
|-------|-------------|
| 1 | `Task #1918: Stage 1 - 정적 이미지 레이어 편집 지연 계측` |
| 2 | `Task #1918: Stage 2 - overlay summary 경량화` |
| 3 | `Task #1918: Stage 3 - text edit overlay canvas 재사용` |
| 4 | `Task #1918: Stage 4 - flow 정적 이미지 재렌더 정책 분리` |
| 5 | `Task #1918: Stage 5 - 성능 검증 및 보고` |

기능 변경과 문서/보고서 변경은 stage 단위 안에서만 함께 묶고, 무관한 포맷 변경은 포함하지 않는다.

## 7. 승인 요청

위 5단계 구현계획으로 Stage 1을 시작해도 되는지 확인 부탁드립니다.
