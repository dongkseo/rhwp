# Task M100 #2124 - 프론트 baseline manifest

- 이슈: #2124
- 작성일: 2026-07-10
- 측정일: 2026-07-11 KST
- metrics generatedAt: `2026-07-10T15:16:40.284Z`
- 기준 브랜치: `upstream/devel`
- 기준 커밋: `ebf052685e0927b60ab06f27defdfa484f717e79`
- 작업 브랜치: `task2124-frontend-baseline`
- 공식 snapshot:
  - `mydocs/metrics/frontend/2026-07-11/metrics.json`
  - `mydocs/metrics/frontend/2026-07-11/summary.md`
- metrics scope: `mydocs/tech/task_m100_2124_frontend_metrics_scope.md`

## 1. 목적

이 문서는 #2124 Phase 0 frontend baseline freeze의 공식 측정 환경을 고정한다. 이후 Phase A/B
리팩터링은 이 manifest와 schema v2 metrics snapshot을 기준으로 전후 차이를 비교한다.

이 baseline은 advisory snapshot이다. #2124 리뷰 승인 전에는 CC>25, CC>100 또는 SOLID 평가를
PR fail gate로 사용하지 않는다.

## 2. Git 기준과 측정 상태

| 항목 | 값 |
|------|----|
| `HEAD` | `ebf052685e0927b60ab06f27defdfa484f717e79` |
| `upstream/devel` | `ebf052685e0927b60ab06f27defdfa484f717e79` |
| 기준 상태 | `HEAD`와 `upstream/devel` 일치 |
| 전체 worktree | #2124 문서·도구 작성으로 dirty |
| 측정 대상 소스 | clean (`measuredSourceDirtyPaths: []`) |

전체 dirty path 목록은 `metrics.json`의 `git.dirtyPaths`에 보존한다. 측정 결과를 바꾸는 프론트
소스 변경은 없었으며, 측정 도구와 문서 변경만 존재하는 상태에서 snapshot을 생성했다.

## 3. 실행 환경과 재현성

| 항목 | 값 |
|------|----|
| OS release | Darwin `25.3.0`, arm64 |
| Node.js | `v24.15.0` |
| npm | `11.12.1` |
| TypeScript | `6.0.3` |
| ESLint | `10.6.0` |
| eslint-plugin-sonarjs | `4.1.0` |
| @typescript-eslint/parser | `8.63.0` |
| Docker context | `colima`, Linux arm64 |
| Docker client / server | `29.4.0` / `29.2.1` |
| Docker Compose | `5.1.3` |
| metrics script SHA-256 | `f18376cd8662c8822a9c5d3bc56de72fa40d97336153d866ac22758378172864` |
| Studio package-lock SHA-256 | `448dc90d0d9a4d1d45809d25d9c496c120677fa6a671fb03aab6fc7e03662aee` |

도구 의존성은 `rhwp-studio` devDependencies에 고정했다. 루트 `package.json`은 없다. commit,
dirty 상태, 플랫폼, 도구 버전과 입력 파일 해시는 `metrics.json`의 `git` 및 `tools`에 함께 보존한다.

## 4. 브라우저 후보

| 브라우저 | 버전 |
|----------|------|
| Google Chrome | `149.0.7827.201` |
| Mozilla Firefox | `152.0.4` |
| Safari | `26.3.1` |
| Safari build | `21623.2.7.111.2` |

이 버전은 수동 smoke 후보 환경 기록이다. 자동 build·dist 계약과 수동 브라우저 smoke 결과는
`mydocs/tech/task_m100_2124_smoke_manifest.md`에서 별도로 추적한다.

## 5. 폰트 경로

| 항목 | 값 |
|------|----|
| canonical 현재 위치 | `web/fonts` |
| studio public 경로 | `rhwp-studio/public/fonts -> ../../web/fonts` |
| WOFF2 파일 수 | 36 |
| WOFF2 총 크기 | 22,630,940 bytes |
| license/index 문서 | `web/fonts/FONTS.md`, `THIRD_PARTY_LICENSES.md`, `web/fonts/SourceHanSerifK-OFL.txt` |

36개 파일과 세 문서의 정확한 byte 수 및 SHA-256은 `metrics.json`의 `fontAssets`에 보존한다.
Phase A #2125에서 canonical 위치 이전을 검토하며, #2124에서는 경로·파일·fallback을 변경하지 않는다.

## 6. 공식 모집단과 제외군

공식 모집단과 제외군은 `mydocs/tech/task_m100_2124_frontend_metrics_scope.md`를 기준으로 한다.

포함 group:

- `studio-runtime`
- `chrome-extension`
- `firefox-extension`
- `safari-extension`
- `shared-frontend`
- `vscode-extension`
- `npm-editor`
- `legacy-web`

주요 제외군:

- `node_modules/`, `dist/`, `pkg/`
- generated WASM glue/declaration: `web/rhwp.js`, `web/rhwp.d.ts`, `web/rhwp_bg.wasm.d.ts`
- `*.min.js`, vendored/generated data
- 모든 `test`, `tests`, `e2e` 디렉터리 및 `*.test.*`, `*.spec.*`, `web/clipboard_test.html`
- font binary와 `web/fonts/`, 향후 `assets/fonts/`
- icons, `_locales`, certs, snapshot, `output/`, cache 파일

테스트 코드는 제품 복잡도 총량을 왜곡하지 않도록 전 group에서 제외한다. 제외된 추적 파일은
`metrics.json`의 `excludedTrackedFilesConsidered`로 확인할 수 있다.

## 7. 공식 snapshot 요약

| Group | Files | Lines | Functions | Total CC | Top 20 sum | CC>25 | CC>25 sum | CC>100 | Max CC | any | as any | this:any | exports |
|------|------:|------:|----------:|---------:|-----------:|------:|----------:|-------:|-------:|----:|-------:|---------:|--------:|
| Studio runtime | 145 | 59,663 | 3,949 | 9,474 | 2,523 | 46 | 3,315 | 6 | 453 | 455 | 241 | 82 | 595 |
| Chrome extension | 15 | 2,330 | 130 | 444 | 295 | 4 | 132 | 0 | 40 | 0 | 0 | 0 | 31 |
| Firefox extension | 15 | 2,325 | 137 | 444 | 295 | 4 | 132 | 0 | 40 | 0 | 0 | 0 | 31 |
| Safari extension | 3 | 1,305 | 107 | 299 | 259 | 3 | 115 | 0 | 43 | 0 | 0 | 0 | 0 |
| Shared frontend | 9 | 864 | 40 | 107 | 98 | 0 | 0 | 0 | 14 | 0 | 0 | 0 | 19 |
| VS Code extension | 4 | 1,387 | 86 | 159 | 126 | 0 | 0 | 0 | 23 | 8 | 4 | 0 | 5 |
| npm editor wrapper | 2 | 266 | 25 | 19 | 19 | 0 | 0 | 0 | 8 | 0 | 0 | 0 | 6 |
| legacy `/web` | 10 | 6,592 | 251 | 828 | 442 | 4 | 207 | 0 | 86 | 0 | 0 | 0 | 6 |

전체 official snapshot:

- 포함 파일: 203
- lines / code lines: 74,732 / 67,037
- functions / reported CC functions: 4,725 / 2,280
- Total CC: 11,774
- 전체 Top 20 CC 합: 2,581
- CC>25: 61개, 합계 3,901
- CC>100: 6개, 합계 1,732
- Max CC: 453
- ESLint parse/fatal diagnostics: 0

group의 `Top 20 sum`은 각 group 안의 상위 20개 합이다. 따라서 group 합을 더한 값은 전체 모집단의
상위 20개 합 2,581과 같은 의미가 아니다.

Phase 0 baseline의 승인된 complexity 예외는 0건이다. 후속 PR의 일시적 CC>25 증가는 자동 허용하지
않으며 `task_m100_2124_frontend_metrics_scope.md`의 예외 심사 조건과 기록 양식을 따른다.

## 8. 상위 cognitive complexity

| CC | 함수 LOC | 함수 | 위치 |
|---:|---------:|------|------|
| 453 | 995 | `onClick` | `rhwp-studio/src/engine/input-handler-mouse.ts:262` |
| 444 | 909 | `onKeyDown` | `rhwp-studio/src/engine/input-handler-keyboard.ts:371` |
| 348 | 381 | `handleOk` | `rhwp-studio/src/ui/picture-props-dialog.ts:1930` |
| 212 | 286 | `populateFromProps` | `rhwp-studio/src/ui/picture-props-dialog.ts:2316` |
| 142 | 354 | `finishResizeDrag` | `rhwp-studio/src/engine/input-handler-table.ts:708` |
| 133 | 328 | `onMouseMove` | `rhwp-studio/src/engine/input-handler-mouse.ts:1405` |
| 88 | 326 | `fillSnapshotFromWasm` | `rhwp-studio/src/compare/diff-engine.ts:929` |
| 86 | 206 | `<anonymous>` | `web/editor.js:187` |
| 73 | 129 | `findPictureAtClick` | `rhwp-studio/src/engine/input-handler-picture.ts:135` |
| 73 | 124 | `renderPictureObjectSelection` | `rhwp-studio/src/engine/input-handler-picture.ts:302` |

## 9. 상위 함수 LOC

| LOC | 함수 | 위치 |
|----:|------|------|
| 995 | `onClick` | `rhwp-studio/src/engine/input-handler-mouse.ts:262` |
| 909 | `onKeyDown` | `rhwp-studio/src/engine/input-handler-keyboard.ts:371` |
| 706 | `<anonymous>` | `web/font_substitution.js:10` |
| 625 | `<anonymous>` | `rhwp-firefox/content-script.js:4` |
| 610 | `<anonymous>` | `rhwp-safari/src/content-script.js:5` |
| 606 | `<anonymous>` | `rhwp-chrome/content-script.js:4` |
| 381 | `handleOk` | `rhwp-studio/src/ui/picture-props-dialog.ts:1930` |
| 354 | `finishResizeDrag` | `rhwp-studio/src/engine/input-handler-table.ts:708` |
| 329 | `getHtml` | `rhwp-vscode/src/hwp-editor-provider.ts:172` |
| 328 | `onMouseMove` | `rhwp-studio/src/engine/input-handler-mouse.ts:1405` |

## 10. 검증

실행한 검증:

```bash
npm --prefix rhwp-studio run metrics:frontend -- --out mydocs/metrics/frontend/2026-07-11/metrics.json --summary mydocs/metrics/frontend/2026-07-11/summary.md
node -e "JSON.parse(require('fs').readFileSync('mydocs/metrics/frontend/2026-07-11/metrics.json','utf8')); console.log('ok')"
node scripts/frontend-metrics.mjs --compare mydocs/metrics/frontend/2026-07-11/metrics.json --out /tmp/task2124-metrics-compare.json --summary /tmp/task2124-metrics-compare.md
docker-compose --env-file .env.docker run --rm wasm
node --test scripts/frontend-wasm-bindings.test.mjs scripts/frontend-editor-embed.test.mjs
npm --prefix rhwp-studio run build
npm --prefix rhwp-studio run test
npm --prefix rhwp-vscode run compile
npm --prefix rhwp-chrome run build
npm --prefix rhwp-firefox run build
node --test scripts/frontend-extension-dist.test.mjs
git rev-parse HEAD upstream/devel
```

검증 결과:

- JSON parse: `ok`
- `HEAD`와 `upstream/devel`: 동일 commit
- 측정 대상 소스: clean
- schema v2 자체 비교: aggregate delta 0, function diff 0
- repository Docker service fresh WASM build: 통과
- Rust explicit export/generated declaration 및 editor contract: 2 tests 통과
- Studio build/185 unit tests, VS Code compile: 통과
- Chrome/Firefox build와 3개 extension dist contract tests: 통과

## 11. 다음 단계

Stage 3 계약 snapshot과 Stage 4 local gate를 완료했다. maintainer/collaborator 리뷰 승인이 확인되기
전에는 Stage 5 완료, #2124 close 또는 #2125 착수를 선언하지 않는다.
