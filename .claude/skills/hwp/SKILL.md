---
name: hwp
description: "Use this skill any time a HWP document (한글 문서) is the primary input or output. This means any task where the user wants to: open, read, edit, or fix an existing .hwp file (e.g., replacing text, filling templates, editing tables, applying formatting, inserting images); create a new .hwp from scratch or from other content; or extract text/tables/structure from a .hwp. Trigger especially when the user references a .hwp file by name or path — even casually (like \"내려받은 한글 파일\", \"the hwp in my downloads\") — and wants something done to it or produced from it. The deliverable must be a .hwp file or its contents. Do NOT trigger when the primary deliverable is a Word/Excel/PDF file or an HTML report; when the task is CLI-based rendering, layout debugging, or IR diffing (use the rhwp-cli skill); or when converting exam papers into HWPX (use the rhwp-exam-ingest skill)."
license: 저장소 라이선스(MIT)를 따른다.
---

# 산출물 요구사항

## 모든 HWP 파일

### 손상 0 (ZERO corruption)
- 모든 산출 `.hwp`는 **한컴오피스에서 경고 없이 열려야** 한다. xlsx가 "수식 오류 0"을 강제하듯, HWP는 "손상 0"을 강제한다.
- 저장 전 반드시 `exportHwpVerify()` 게이트를 통과시킨다. 이 검증은 선택이 아니다.

### 일관된 글꼴
- 별도 지시가 없으면 한컴 표준 글꼴(함초롬바탕/함초롬돋움)을 일관되게 사용한다.

### 기존 템플릿 보존 (편집 시)
- 기존 `.hwp`를 수정할 때는 원본의 서식·스타일·관례를 **그대로 유지**하고 표준 서식을 강요하지 않는다.
- 기존 템플릿의 관례가 이 가이드보다 **항상 우선**한다.
- HWP 출처 문서는 내보내기 시 어댑터가 no-op이므로 원본 구조가 보존된다.

# HWP 생성 · 편집 · 분석

## Overview

사용자가 `.hwp` 파일을 만들거나, 편집하거나, 내용을 추출하도록 돕는다. 모든 작업은 node에서 rhwp WASM 모듈의 `HwpDocument` 클래스로 수행한다.

기본 흐름은 항상 같다: **열기(또는 생성) → 편집 → 검증 → 저장**.

## 중요 전제 조건

이 스킬은 rhwp 를 WebAssembly 로 쓴다. **설치는 npm 한 줄이다** — Rust 툴체인도 Docker 도
필요 없다.

```bash
npm i github:dongkseo/rhwp
```

차트를 만들 때만 래스터라이저가 하나 더 필요하다 (플랫폼별 prebuilt 제공):

```bash
npm i @resvg/resvg-js
```

그 외 기능은 node 표준 모듈만 쓴다.

`scripts/hwp/loader.mjs` 가 설치된 패키지를 알아서 찾는다. 못 찾으면 설치 명령을 알려주며
멈춘다. 설치 위치가 특이하면 `RHWP_PKG=/path/to/pkg` 로 지정한다.

## CRITICAL: 검증 없이 저장하지 마라

`exportHwp()`는 바이트를 돌려줄 뿐, 그 결과가 한컴에서 열리는지는 보장하지 않는다. openpyxl이 계산되지 않은 수식을 담은 채 저장되는 것과 같다. **반드시 `exportHwpVerify()`로 자기 재로드 검증을 통과시킨 뒤 저장한다.**

### ❌ 잘못된 방법 — 검증 없이 저장
```js
doc.insertText(0, 0, 0, '내용');
writeFileSync('out.hwp', Buffer.from(doc.exportHwp()));  // 손상 여부 미확정
```

### ✅ 올바른 방법 — 검증 후 저장
```js
doc.insertText(0, 0, 0, '내용');
const v = JSON.parse(doc.exportHwpVerify());
if (!v.recovered || v.pageCountBefore !== v.pageCountAfter) throw new Error(JSON.stringify(v));
writeFileSync('out.hwp', Buffer.from(doc.exportHwp()));
```

`scripts/hwp/loader.mjs`의 `saveHwp()`가 이 게이트를 강제한다. **직접 `exportHwp()`를 쓰지 말고 `saveHwp()`를 써라.**

## CRITICAL: 원본을 덮어쓰지 마라

편집 결과는 **항상 새 경로로 저장**한다. 원본은 사용자의 유일한 사본일 수 있고, 편집이 잘못되면 되돌릴 수 없다.

```js
await saveHwp(doc, 'output/edited.hwp');   // ✅ 새 경로
await saveHwp(doc, inputPath);             // ❌ 원본 파괴
```

## CRITICAL: 컨트롤 인덱스를 캐싱하지 마라

**저장 → 재열기 후 `control_idx`가 바뀐다.** 표와 이미지 모두에서 확인됐다 (`ctrl=0` → `ctrl=1`).

```js
const { controlIdx } = JSON.parse(doc.createTable(0, 0, 0, 3, 2));   // controlIdx = 0
await saveHwp(doc, 'out.hwp');
const re = await openHwp('out.hwp');
re.getTableDimensions(0, 0, controlIdx);   // ❌ "지정된 컨트롤이 표가 아닙니다" — 이제 1 이다
```

파일을 다시 열었다면 **인덱스를 다시 탐색하라**. 문단 인덱스도 편집으로 밀릴 수 있으니 같은 원칙을 적용한다.

## 공통 워크플로

1. **열기 / 생성**: `openHwp(path)` 또는 `createHwp()`
2. **구조 파악**: `getSectionCount()`, `getParagraphCount(sec)`, `getTextRange(...)`로 어디를 고칠지 먼저 확인
3. **편집**: `insertText`, `applyCharFormat`, `insertTableRow` 등
4. **검증 + 저장 (필수)**: `saveHwp(doc, outPath)` — 검증 실패 시 파일을 쓰지 않고 throw한다
5. **독립 검증**: `node scripts/verify_hwp.mjs out.hwp`
6. **오류 수정**: `status`가 `errors_found`면 `error_summary`를 보고 고친 뒤 재검증

### 새 HWP 만들기

```js
import { createHwp, saveHwp } from './scripts/hwp/loader.mjs';

const doc = await createHwp();                    // createEmpty + createBlankDocument (템플릿)
doc.insertText(0, 0, 0, '안녕하세요. 보고서입니다.');
doc.applyCharFormat(0, 0, 0, 5, JSON.stringify({ bold: true, fontSize: 1400, textColor: '#FF0000' }));
doc.applyParaFormat(0, 0, JSON.stringify({ alignment: 'center' }));

console.log(await saveHwp(doc, 'output/new.hwp'));
// → { bytesLen: 3584, pageCountBefore: 1, pageCountAfter: 1, recovered: true, path: ..., bytesWritten: 3584 }
```

**서식 API 의 함정 두 가지** (실측):

- `applyCharFormat` 의 JSON 은 **병합이 아니라 치환**이다. 넘기지 않은 필드는 유지되지 않고
  초기화된다. 특히 **`fontFamily` 는 어떤 형태로 넘겨도 반영되지 않는다** — 지정해도 템플릿
  기본 폰트(함초롬돋움/바탕)가 남는다. 이 API 로 글꼴 이름은 못 바꾼다고 보면 된다.
- `applyParaFormat` 의 정렬 키는 `alignment` 다 (`align` 이 아니다). 파서가 모르는 키는 조용히
  무시하고 `{"ok":true}` 를 돌려주므로, 오타를 내도 에러 없이 그냥 안 먹는다.

### 문단 추가하기

**`insertText` 로 `\n` 을 넣어도 문단은 쪼개지지 않는다.** 텍스트에 개행 문자가 하나 들어갈 뿐
문단 수는 그대로다. 이 상태로 `createTable(0, 1, ...)` 을 부르면 "문단 인덱스 1 범위 초과" 가 난다.

문단을 늘리는 방법은 둘이다.

```js
doc.insertParagraph(0, i);        // i번 "앞에" 빈 문단을 끼운다 → 기존 i번이 i+1번으로 밀린다
doc.splitParagraph(0, p, off);    // p번 문단을 off 위치에서 둘로 쪼갠다
```

`insertParagraph` 는 **앞에 끼운다**. 문단 뒤에 붙이려면 인덱스를 하나 더해라.

```js
const doc = await createHwp();
doc.insertText(0, 0, 0, '2026년 상반기 실적 보고');
doc.insertParagraph(0, 1);        // 제목 "뒤" — 0 을 넘기면 제목 앞에 끼워진다
doc.createTable(0, 1, 0, 3, 2);   // 이제 1번 문단이 있다
```

문단 수를 늘렸으면 **뒤쪽 인덱스가 전부 밀린다.** 표·그림 인덱스를 미리 구해뒀다면 다시 구하라
(아래 "컨트롤 인덱스를 캐싱하지 마라" 참조).

```js
const n = doc.getParagraphCount(0);   // 늘어난 문단 수를 확인하고 진행하라
```

### 기존 HWP 편집

```js
import { openHwp, saveHwp } from './scripts/hwp/loader.mjs';

const doc = await openHwp('samples/복학원서.hwp');
console.log(doc.getSectionCount(), doc.getParagraphCount(0), doc.pageCount());

// 고치기 전에 현재 내용부터 확인한다
console.log(doc.getTextRange(0, 0, 0, 100));

doc.insertText(0, 0, 0, '[수정] ');
await saveHwp(doc, 'output/edited.hwp');   // 원본이 아닌 새 경로
```

### 텍스트 읽기 / 추출

```js
const doc = await openHwp('input.hwp');
for (let s = 0; s < doc.getSectionCount(); s++) {
  for (let p = 0; p < doc.getParagraphCount(s); p++) {
    const line = doc.getTextRange(s, p, 0, 10000);
    if (line) console.log(line);
  }
}
```

### 표 편집

표는 `(para_idx, control_idx)`로 지정한다. **인덱스를 가정하지 말고 탐색하라.**

```js
// 문서 안의 표를 모두 찾는다
const scanTables = (doc) => {
  const out = [];
  for (let p = 0; p < doc.getParagraphCount(0); p++)
    for (let c = 0; c < 4; c++) {
      try { const j = doc.getTableDimensions(0, p, c); if (j) out.push({ p, c, ...JSON.parse(j) }); } catch {}
    }
  return out;
};

const doc = await openHwp('samples/복학원서.hwp');
const t = scanTables(doc)[0];        // → { p:2, c:0, rowCount:5, colCount:4, cellCount:15 }

doc.insertTextInCell(0, t.p, t.c, 0, 0, 0, '셀 내용');    // cellIdx=0 의 문단0, offset0
doc.insertTableRow(0, t.p, t.c, 0, true);                 // 0행 아래에 행 추가
doc.insertTableColumn(0, t.p, t.c, 0, true);              // 0열 오른쪽에 열 추가
doc.mergeTableCells(0, t.p, t.c, 1, 0, 1, 1);             // (1,0)~(1,1) 가로 병합

await saveHwp(doc, 'output/table_edited.hwp');
```

새 표는 `createTable(sec, para, offset, rows, cols)`로 만든다 — 반환 JSON에 `paraIdx`/`controlIdx`가 담긴다.

```js
const doc = await createHwp();
const { paraIdx, controlIdx } = JSON.parse(doc.createTable(0, 0, 0, 3, 2));
doc.insertTextInCell(0, paraIdx, controlIdx, 0, 0, 0, 'R1C1');
await saveHwp(doc, 'output/table_new.hwp');
```

병합 전에는 `getCellInfo(sec, para, ctrl, cellIdx)`로 `rowSpan`/`colSpan`을 확인하라. 이미 병합된 셀을
다시 병합하면 "병합 범위를 벗어납니다" 오류가 난다.

### 양식 채우기 — 본문이 비어 있는 문서

블로그 양식·신청서 같은 문서는 **본문 문단이 비어 있고 내용이 표 안의 표에 들어 있다**.
`getTextRange(0, p, ...)`로 훑으면 전부 빈 문자열이 나와 편집 지점을 하나도 못 찾는다.
`findTextTargets()`가 중첩 표까지 재귀로 훑는다.

```js
import { openHwp, saveHwp, findTextTargets, insertAt } from './scripts/hwp/loader.mjs';

const doc = await openHwp('블로그 서평 양식.hwp');
const targets = findTextTargets(doc);
targets.forEach(t => console.log(JSON.stringify(t.text), t.path));
// " 제목"    [{controlIndex:3,cellIndex:0,...},{controlIndex:0,cellIndex:0,cellParaIndex:0}]
// " 지은이"  [{controlIndex:3,cellIndex:0,...},{controlIndex:0,cellIndex:2,cellParaIndex:0}]

insertAt(doc, targets[0], 0, '채울 내용');
await saveHwp(doc, 'output/filled.hwp');
```

양식은 보통 **라벨/값 셀 쌍** 구조다 — `cellIndex` 짝수(0,2,4…)가 라벨(`제목`/`지은이`),
홀수가 빈 값 칸이다. 라벨을 찾아 그 다음 셀에 값을 넣으면 된다.

글상자(TextBox) 안 텍스트도 같은 cell path 로 **읽고 쓴다**. 표가 아니므로 `getTableDimensions`로는
안 잡히고, `getTextBoxControlIndex()`가 주는 인덱스로도 접근되지 않는다. `findTextTargets()`가
컨트롤 인덱스를 훑어 처리한다 — 표 없는 브로슈어 문서에서 글상자 6개를 찾아 편집·저장·재열기까지 확인했다.

### 이미지 삽입

**이미지는 인라인이 아니라 용지에 고정된 부동 개체(floating)로 들어간다.** 한컴이 그림을 새로
넣을 때 "글자처럼 취급"이 기본 미체크인 동작을 맞춘 것이다 (`tac=false`, `vertRelTo/horzRelTo=Paper`,
`textWrap=Square`).

따라서 **위치를 반드시 지정해야 한다.** 마지막 두 인자 `paper_offset_x_hu`/`paper_offset_y_hu`를
빠뜨리면 `(0, 0)` — **용지 좌상단**에 붙는다.

```js
import { readFileSync } from 'node:fs';

const MM = 7200 / 25.4;          // 1mm = 283.46 HWPUNIT
const png = readFileSync('logo.png');
const natW = png.readUInt32BE(16), natH = png.readUInt32BE(20);   // PNG IHDR
const wHU = Math.round((natW / 96) * 7200);   // px → HWPUNIT (1인치 = 96px = 7200HU)
const hHU = Math.round((natH / 96) * 7200);

const doc = await openHwp('input.hwp');
doc.insertPicture(
  0, 0, 0, '[]', png, wHU, hHU, natW, natH, 'png', '설명',
  Math.round(30 * MM),    // ← paper_offset_x: 용지 왼쪽에서 30mm
  Math.round(150 * MM),   // ← paper_offset_y: 용지 위에서 150mm
);
await saveHwp(doc, 'output/with_image.hwp');
```

`cell_path_json`이 `'[]'`(또는 빈 문자열)이면 본문에, 그 외에는 표 셀 영역에 띄운다:
`'[{"controlIndex":0,"cellIndex":2,"cellParaIndex":0}]'`.

**크기도 확인하라.** 위 예제는 원본 픽셀 크기를 그대로 쓴다. 사진처럼 큰 이미지는 A4 폭(794px)을
넘겨 용지를 뒤덮으므로 배율을 줘야 한다 — 예: 1/3 크기는 `Math.round((natW / 96) * 7200 / 3)`.

> ⚠️ `pkg/rhwp.d.ts`의 `insertPicture` 주석은 `'[]'`가 "본문 inline 삽입"이라고 적고 있으나
> **사실이 아니다.** 실제 산출물은 `treatAsChar="0" vertRelTo="PAPER" horzRelTo="PAPER"` 부동
> 개체다 (HWPX로 내보내 `<hp:pos>`를 보면 확인된다). 주석을 믿지 말고 결과를 확인하라.

**배치 후 겹침을 반드시 확인하라.** `textWrap=Square`(어울림)이므로 이미지가 본문·표 위에 놓이면
한컴이 그 주위로 내용을 밀어낸다. rhwp 렌더러는 표 셀에 대해 이 감싸기를 구현하지 않아
**rhwp 화면에서는 멀쩡해 보여도 한컴에서는 표가 찌그러진다.**

```js
const tree = JSON.parse(doc.getPageRenderTree(0));
const boxes = [];
const walk = (n) => {
  if (!n || typeof n !== 'object') return;
  if (Array.isArray(n)) return n.forEach(walk);
  if (n.type === 'Image' || n.type === 'Table') boxes.push({ t: n.type, b: n.bbox });
  (n.children || []).forEach(walk);
};
walk(tree);
// 이미지와 표가 겹치면 한컴에서 표가 밀린다 — 위치를 다시 잡아라
```

이미지 바이트는 재인코딩 없이 그대로 보존된다. 확인:

```js
const data = doc.getControlImageData(0, para, '[]', ctrl);   // Uint8Array
const mime = doc.getControlImageMime(0, para, '[]', ctrl);   // "image/png"
```

### 차트

차트도 코드로 만든다. 다만 **미리보기 래스터화만 밖에서** 한다 — 한컴은 OLE 안의
미리보기 그림만 그리는데(XML 은 rhwp 전용), WASM 에는 래스터라이저가 없다.
`@resvg/resvg-js` 는 플랫폼별 prebuilt 를 제공하므로 Rust 툴체인이 필요 없다.

```bash
npm i @resvg/resvg-js
```

```js
import { createRequire } from 'node:module';
const { Resvg } = createRequire(import.meta.url)('@resvg/resvg-js');
import { openHwp, saveHwp } from './scripts/hwp/loader.mjs';

const MM = 7200 / 25.4;
const spec = {
  type: 'column',                       // column | bar | line | pie
  title: '2026 상반기 실적',
  categories: ['1분기', '2분기', '3분기', '4분기'],
  series: [
    { name: '매출액',   values: [184, 210, 175, 243] },
    { name: '영업이익', values: [21, 33, 18, 41] },
  ],
};

const wHU = Math.round(150 * MM), hHU = Math.round(90 * MM);   // 개체 150x90mm
const pxW = Math.round((wHU / 7200) * 96), pxH = Math.round((hHU / 7200) * 96);

const doc = await openHwp('input.hwp');
const svg = doc.renderChartSvg(JSON.stringify(spec), pxW, pxH);   // 1) WASM 이 SVG
const png = new Resvg(svg, { fitTo: { mode: 'width', value: pxW } }).render();
doc.insertChart(                                                   // 2) WASM 이 삽입
  0, 0, JSON.stringify(spec), png.pixels, png.width, png.height,
  wHU, hHU, Math.round(30 * MM), Math.round(60 * MM),              // 용지 기준 위치
);
await saveHwp(doc, 'output/with_chart.hwp');
```

**한컴에서 편집 가능한 진짜 차트**로 들어간다 — 더블클릭하면 차트 편집기가 뜨고 데이터를
고칠 수 있다. 그림이 아니다. rhwp 안에서도 XML 이 살아있어 `renderPageSvg` 로 그려진다.

이미지와 같은 부동 개체이므로 **위치를 반드시 지정**하고 겹침을 확인하라
(생략 시 용지 좌상단). 위 "이미지 삽입" 절의 겹침 검사 코드를 그대로 쓰면 된다.

> 차트는 OLE 개체다. 한컴이 이를 차트로 인식하는 근거는 OLE 내부 CFB 의 **Root CLSID**
> (`{4C3DA137-DC90-47B9-9BED-59DAE352A280}`)다. rhwp 가 넣어주므로 신경 쓸 필요 없으나,
> 차트가 그림처럼 박혀 편집이 안 된다면 이걸 먼저 의심하라.

> `Contents`(레거시 바이너리) 는 넣지 않는다. 없어도 한컴이 차트로 편집한다 — 실측 확인.

SVG/PNG 렌더, Markdown 내보내기, IR 비교, 레이아웃 디버깅은 `rhwp` CLI의 영역이다 → **rhwp-cli 스킬**을 사용하라 (CLI는 `cargo build --release`가 필요하다).

## 검증하기

```bash
node scripts/verify_hwp.mjs <파일.hwp>
```

대상 파일을 **읽기만 한다** (수정하지 않는다). 검사 항목:

1. **CFB 시그니처** — 진짜 HWP 5.0 복합 파일 컨테이너인가 (`D0CF11E0A1B11AE1`)
2. **재파싱** — 다시 열었을 때 IR이 깨지지 않는가
3. **라운드트립** — `exportHwpVerify()`로 자기 재로드 + 페이지 수 보존 확인

정상이면 exit 0:
```json
{
  "status": "success",
  "cfb_ok": true,
  "reparse_ok": true,
  "page_count": 1,
  "roundtrip": { "bytesLen": 3584, "pageCountBefore": 1, "pageCountAfter": 1, "recovered": true }
}
```

손상이면 exit 1 + `error_summary`:

| `type` | 의미 | 조치 |
|--------|------|------|
| `not_cfb` | HWP 컨테이너가 아님 | 저장 경로/바이트 확인 |
| `reparse_failed` | 재파싱 불가 — IR 손상 | 편집 단계 되짚기 |
| `not_recovered` | 자기 재로드 실패 | 해당 편집 연산 제거 후 이분 탐색 |
| `page_count_mismatch` | 페이지 수 변동 | 페이지네이션 영향 편집 검토 |

### `recovered: true` 는 "문서가 멀쩡하다"가 아니다

이 검사는 **rhwp 가 자기 산출물을 자기 파서로 읽는** 자기 채점이다. 통과하는 손상이 있다.

`exportHwpVerify()` 가 보장하는 것은 딱 셋이다 — 바이트가 CFB 이고, 자기 파서로 다시 읽히고,
직렬화 전후 페이지 수가 같다는 것. **내용이 옳은지는 보지 않는다.**

실제로 통과한 손상: 문단을 0번에 끼웠더니 1쪽이 통째로 비고 내용이 2쪽으로 밀렸는데
`recovered:true`, `pageCountBefore=2 pageCountAfter=2` 로 통과했다. 편집 **직후**부터 이미 2쪽이라
"직렬화 전후 비교" 는 잡을 수 없었다. 빈 쪽을 찾은 건 렌더해서 눈으로 본 뒤였다.

그러므로 **의도한 쪽수를 네가 직접 기억하고 대조하라.** 게이트는 네 의도를 모른다.

```js
const before = doc.pageCount();
// ... 편집 ...
if (doc.pageCount() !== before) throw new Error(`쪽수가 ${before} → ${doc.pageCount()}`);
```

그리고 **쪽마다 렌더해서 비어 있지 않은지 확인하라.** 게이트가 통과시킨 빈 쪽을 잡는 유일한 방법이다.

```js
for (let p = 0; p < doc.pageCount(); p++) {
  const tree = JSON.parse(doc.getPageRenderTree(p));
  const runs = [];
  const walk = (n) => {
    if (!n || typeof n !== 'object') return;
    if (Array.isArray(n)) return n.forEach(walk);
    if (n.type === 'TextRun' && n.text?.trim()) runs.push(n.text);
    (n.children || []).forEach(walk);
  };
  walk(tree);
  if (!runs.length) throw new Error(`${p}쪽이 비어 있다 — 편집이 내용을 밀어냈다`);
}
```

> 렌더 트리의 `bbox.y` 는 **쪽 기준 좌표**다. 서로 다른 쪽의 y 를 비교해 "같은 자리" 라고
> 판단하지 마라 — 모든 쪽의 본문 상단이 같은 y 다.


## CRITICAL: 빈 문서는 반드시 템플릿을 실어라

`createEmpty()`는 **맨바닥 껍데기**(`Document::default()`)를 만든다. 그대로 저장하면
**한컴·온라인 뷰어가 열지 못하는 파일**이 나온다. 내장 템플릿을 싣는 것은 `createBlankDocument()`다.

```js
const doc = HwpDocument.createEmpty();
doc.createBlankDocument();     // ← 필수. 없으면 version 0.0.0.0 짜리 깨진 파일
```

`scripts/hwp/loader.mjs`의 `createHwp()`가 두 호출을 묶어 처리한다. **직접 `createEmpty()`를 쓰지 마라.**

빠뜨렸을 때 실제로 벌어지는 일 (열리는 참조 문서와 비교):

| | `createEmpty()`만 | `createBlankDocument()` 후 | 열리는 참조 문서 |
|---|---|---|---|
| FileHeader version | **0.0.0.0** ❌ | 5.1.0.1 | 5.0.5.0 |
| `\x05HwpSummaryInformation` | **없음** ❌ | 있음 | 있음 |
| 압축 | **False** ❌ | True | True |
| `applyCharFormat` 범위 | **무시 — 문서 전체 물듦** ❌ | 정상 | — |

범위가 무시되는 이유: 껍데기 문서는 `char_shapes`가 비어 있어 `find_or_create_char_shape`이
새 ID 대신 `0`을 반환하고 **0번 글자모양을 덮어쓴다**. 모든 글자가 0번을 참조하므로 전부 물든다.

**`exportHwpVerify()`는 이 문제를 잡지 못한다** — rhwp가 자기 산출물을 자기가 읽는 검사라
version 0.0.0.0 도 `recovered:true`로 통과시킨다. 최종 판정은 **실제 뷰어/한컴**뿐이다.

## 편집은 아래쪽 레이아웃을 재계산한다

문단을 편집하면 rhwp 는 그 아래 전부의 vpos 를 자기 레이아웃 엔진으로 재계산한다
(`reflow_paragraph` → `recalculate_section_vpos(para_idx)` → `paginate`).
원본 파일에 든 한컴의 LINE_SEG 값이 그 지점부터 버려진다.

실문서 16건으로 시험했을 때 편집 가능한 12건 **전부** 편집 후에도 쪽수가 그대로였다
(94쪽 문서 포함). 다단 문서에서 단-밴드가 무너지던 결함은 수정됐다 (#2299).

> 다단 문서(2단 zone)는 편집 시 우측 단-밴드가 소멸해 쪽수가 배로 튀는 결함이 있었다.
> `recalculate_section_vpos`가 단-상대 vpos 리셋을 선형 누적으로 상쇄한 탓이며,
> `line_breaking.rs`에서 리셋 신호를 보존하도록 수정했다. shortcut.hwp 7쪽 유지 확인.

그래도 **편집 후 `pageCount()`를 원본과 비교하라.** 달라졌다면 레이아웃이 재계산된 것이고,
`exportHwpVerify()`는 이를 잡지 못한다 (편집 후 값끼리 비교하므로 통과한다).
관련: 저장소 이슈 #2279 (한글 fresh 레이아웃 예측 정밀도).

## 검증 체크리스트

- [ ] `saveHwp()`로 저장했는가 (`exportHwp()` 직접 호출 금지)
- [ ] `verify_hwp.mjs`가 `status: "success"`를 반환하는가
- [ ] 원본이 아닌 **새 경로**에 저장했는가
- [ ] 편집 결과를 재열기해 `getTextRange()`로 **내용이 실제로 들어갔는지** 확인했는가
- [ ] 편집 전후 `pageCount()`를 **직접 대조**했는가 (`recovered:true` 는 이걸 보지 않는다)
- [ ] 쪽마다 렌더해 **빈 쪽이 없는지** 확인했는가
- [ ] 서식·표·이미지를 건드렸다면 `renderPageSvg(0)`로 렌더해 확인했는가 (cargo 없이 node에서 가능)
- [ ] 재열기 후 컨트롤 인덱스를 **재탐색**했는가 (저장하면 밀린다)
- [ ] 편집 후 `pageCount()`가 원본과 같은가 (다르면 레이아웃이 재계산된 것)
- [ ] 이미지를 넣었다면 `paper_offset`을 지정하고 표·본문과 겹치지 않는지 확인했는가
- [ ] 새 문서를 만들었다면 `getDocumentInfo()`의 `version`이 `0.0.0.0`이 **아닌지** 확인했는가
- [ ] 한컴오피스/뷰어에서 경고 없이 열리는가 (**최종 판정은 사람이 한다** — 자체 검증으로는 알 수 없다)

## 핵심 API

`new HwpDocument(bytes)`로 열고, `createHwp()`(= `createEmpty()` + `createBlankDocument()`)로 만든다. 인덱스는 모두 **0부터**.

| 목적 | 시그니처 |
|------|----------|
| 빈 문서 생성 | `static createEmpty()` **+ `createBlankDocument()`** — 반드시 함께 (`createHwp()` 사용 권장) |
| 구조 조회 | `getSectionCount()`, `getParagraphCount(sec)`, `pageCount()`, `getDocumentInfo()` |
| 텍스트 읽기 | `getTextRange(sec, para, offset, count): string` |
| 텍스트 삽입 | `insertText(sec, para, offset, text): string` |
| 텍스트 삭제 | `deleteText(sec, para, offset, count): string` |
| 글자 서식 | `applyCharFormat(sec, para, start, end, props_json): string` |
| 문단 서식 | `applyParaFormat(sec, para, props_json): string` |
| 표 탐색 | `getTableDimensions(sec, para, ctrl): string` (JSON `{rowCount, colCount, cellCount}`) |
| 표 생성 | `createTable(sec, para, offset, rows, cols): string` (JSON `{paraIdx, controlIdx}`) |
| 셀 정보 | `getCellInfo(sec, para, ctrl, cellIdx): string` (JSON `{row, col, rowSpan, colSpan}`) |
| 표 행 | `insertTableRow(sec, parentPara, ctrl, rowIdx, below)` / `deleteTableRow(sec, parentPara, ctrl, rowIdx)` |
| 표 열 | `insertTableColumn(sec, parentPara, ctrl, colIdx, right)` / `deleteTableColumn(sec, parentPara, ctrl, colIdx)` |
| 셀 병합 | `mergeTableCells(sec, parentPara, ctrl, startRow, startCol, endRow, endCol)` |
| 셀 텍스트 | `insertTextInCell(sec, parentPara, ctrl, cellIdx, cellPara, offset, text)` / `getTextInCell(...)` |
| 중첩 셀 (경로) | `insertTextInCellByPath(sec, para, pathJson, offset, text)` / `getTextInCellByPath(...)` / `getTableDimensionsByPath(...)` / `getCellParagraphCountByPath(...)` |
| 이미지 삽입 | `insertPicture(sec, para, offset, cellPathJson, bytes, wHU, hHU, natW, natH, ext, desc)` |
| 이미지 조회 | `getControlImageData(sec, para, cellPathJson, ctrl)` / `getControlImageMime(...)` |
| 문단 추가 | `insertParagraph(sec, i)` (i번 **앞에**) / `splitParagraph(sec, para, off)` — `\n` 삽입은 안 쪼갠다 |
| 차트 | `renderChartSvg(specJson, w, h)` → 래스터화 → `insertChart(sec, para, specJson, rgba, pxW, pxH, wHU, hHU, xHU?, yHU?)` |
| 렌더 (시각 확인) | `renderPageSvg(page): string`, `renderPageHtml(page): string` |
| 검증 | `exportHwpVerify(): string` (JSON) |
| 내보내기 | `exportHwp(): Uint8Array`, `exportHwpx(): Uint8Array` |

편집 명령은 대부분 `{"ok":true, ...}` JSON **문자열**을 반환한다 — 필요하면 `JSON.parse`로 확인한다.

`applyCharFormat`의 `props_json` 키: `bold`(bool), `italic`(bool), `underline`(bool), `strikethrough`(bool), `fontSize`(정수, 1/100pt — 1400 = 14pt), `fontId`(정수), `textColor`/`shadeColor`/`underlineColor`(CSS 문자열, 예: `"#FF0000"`).

`applyParaFormat`의 `props_json` 키: `alignment`(문자열: `left`|`right`|`center`|`justify`|`distribute`), `lineSpacing`(정수), `lineSpacingType`(`Percent`|`Fixed`|`SpaceOnly`|`Minimum`), `indent`·`marginLeft`·`marginRight`·`spacingBefore`·`spacingAfter`(정수, HWPUNIT), `keepWithNext`·`widowOrphan`(bool).

> ⚠️ **키 이름이 틀리면 조용히 무시되고 `{"ok":true}`가 그대로 돌아온다.** (예: `align`은 무시된다 — 올바른 키는 `alignment`)
> 서식이 안 먹으면 먼저 키 철자를 의심하고 `src/document_core/helpers.rs`의 `parse_char_shape_mods` / `parse_para_shape_mods`에서 실제 키를 확인하라.

**여기 없는 API가 필요하면 `pkg/rhwp.d.ts`에서 확인하라** (332개 export의 자동 생성 명세, 한국어 doc 주석 포함). **API 이름을 추측하지 마라** — 존재하지 않는 이름은 조용히 실패하거나 예외를 던진다.

## 모범 사례

- **편집 전에 읽어라**: `getTextRange()`로 대상 문단의 현재 내용을 확인한 뒤 인덱스를 정한다. 문서 구조를 가정하지 마라.
- **작게 시작하라**: 문단 하나에 먼저 적용해 보고 전체로 확장한다.
- **인덱스는 0부터**: 섹션·문단·페이지 모두.
- **단위**: 1인치 = 7200 HWPUNIT = 96px. 글자 크기는 1/100pt.
- **출력 폴더**: `output/` 아래에 쓴다 (`.gitignore` 등록됨).
- **WASM 초기화는 프로세스당 1회**: `loader.mjs`가 캐시한다.
- **반환값을 믿지 말고 결과를 확인하라**: `{"ok":true}`는 명령이 접수됐다는 뜻이지, 문서가 의도대로 됐다는 뜻이 아니다. 저장 후 재열기해 확인한다.

## 흔한 실수

| 실수 | 바로잡기 |
|------|----------|
| 검증 없이 `exportHwp()`로 저장 | `saveHwp()` 사용 — "저장됨"과 "한컴에서 열림"은 다르다 |
| 원본 경로에 덮어쓰기 | 항상 새 출력 경로로 저장 |
| `{"ok":true}`만 보고 성공으로 간주 | 재열기 후 `getTextRange()`로 실제 내용 확인 |
| API 이름을 추측 | `pkg/rhwp.d.ts`에서 확인 — 없는 이름은 실패한다 |
| `insertText` 로 `\n` 을 넣어 문단 분리 시도 | 안 쪼개진다 — `insertParagraph` / `splitParagraph` |
| `insertParagraph(0, 0)` 으로 제목 뒤에 문단 추가 | 0 은 제목 **앞**이다 — 뒤는 `insertParagraph(0, 1)` |
| `recovered:true` 를 보고 성공으로 판단 | 자기 채점이다 — 쪽수 대조 + 쪽별 렌더로 따로 확인 |
| 서로 다른 쪽의 렌더 `bbox.y` 를 비교 | y 는 쪽 기준 좌표다 — 모든 쪽의 본문 상단이 같은 y |
| 패키지를 저장소에서 빌드하려 시도 | `npm i github:dongkseo/rhwp` 한 줄이다 — Rust/Docker 불필요 |
| CLI(`rhwp`)를 이 스킬에서 호출 | CLI는 `cargo` 빌드 필요 — 렌더/디버깅은 rhwp-cli 스킬 |
| HWPX를 거쳐 편집 | HWP 출처는 직접 편집해야 어댑터 no-op으로 원본 보존 |
| 저장 전 컨트롤 인덱스를 재열기 후 재사용 | 재열기 후 반드시 재탐색 — 인덱스가 밀린다 |
| 이미 병합된 셀을 다시 병합 | `getCellInfo`로 `colSpan`/`rowSpan` 먼저 확인 |
| `createEmpty()`만 쓰고 저장 | 뷰어가 못 여는 version 0.0.0.0 파일 — `createHwp()` 사용 |
| `recovered:true`를 한컴 호환으로 간주 | 자기 채점일 뿐 — 실제 뷰어로 열어봐야 안다 |
| 양식 문서에서 `getTextRange`만 훑고 "텍스트 없음" 판정 | 내용이 중첩 표 안에 있다 — `findTextTargets()` 사용 |
| `insertPicture`에 `paper_offset` 생략 | 용지 좌상단(0,0)에 붙는다 — 인라인이 아니다 |
| 이미지가 표·본문과 겹치는지 미확인 | 어울림 감싸기로 한컴에서만 표가 찌그러진다 |
| 글자 수를 손으로 세어 서식 범위 지정 | `text.indexOf('강조할 말')`로 계산하라 |
| `applyCharFormat` 로 글꼴 이름 지정 | 반영 안 된다 — 지정한 `fontFamily` 는 무시되고 템플릿 폰트가 남는다 |
| `applyCharFormat` 에 일부 필드만 넘기고 나머지 유지 기대 | 치환이라 초기화된다 — 필요한 필드를 모두 넘겨라 |
| `createTable` 후 문단 수를 표 하나만큼 셈 | 표 뒤에 빈 문단이 자동으로 하나 더 생긴다 |
| 차트 미리보기를 안 넘김 | 한컴은 미리보기만 그린다 — `renderChartSvg` → 래스터화 필수 |

## 참조

- WASM API 전체 명세: `pkg/rhwp.d.ts` (빌드 후 생성) 또는 `rhwp-studio/public/rhwp.d.ts` (커밋본)
- 공유 로더: `scripts/hwp/loader.mjs` — `loadWasm` / `openHwp` / `createHwp` / `saveHwp`
- 검증 게이트: `scripts/verify_hwp.mjs`
- 형제 스킬: `.claude/skills/rhwp-cli/`(CLI 분석·렌더·디버깅), `.claude/skills/rhwp-exam-ingest/`(시험지→HWPX)
- 핵심 소스: `src/wasm_api.rs`(WASM 바인딩), `src/document_core/commands/`(편집 구현)
