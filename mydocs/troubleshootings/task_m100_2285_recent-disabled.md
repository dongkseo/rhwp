# 트러블슈팅 — 최근 문서 메뉴가 계속 비활성화 (#2285)

- **이슈**: edwardkim/rhwp#2285 / PR #2286
- **작성일**: 2026-07-15

## 증상

파일 메뉴의 "최근 문서" 서브메뉴가 문서를 열어도 **계속 비활성화(disabled)** 상태로 남아 열리지 않음.

## 원인

최근 문서 기록 훅(`addRecentDoc`)이 `loadBytes()`에서 **`await initializeDocument()` 뒤**에 위치했다.
`initializeDocument()`는 내부 마지막에 `promptLocalFontsIfNeeded()`(로컬 글꼴 안내 **모달**)를 `await`한다.

- 문서가 미설치 글꼴을 사용해 모달이 뜨는 경우(또는 자동화/헤드리스처럼 모달이 즉시 닫히지 않는 경우), `initializeDocument()` 프라미스가 그 지점에서 대기 → **그 뒤의 기록 훅이 실행되지 않음**.
- 결과적으로 IndexedDB에 최근 문서가 저장되지 않아 목록이 비고, `renderRecentSubmenu()`가 서브메뉴를 계속 disabled로 유지.

## 해결

기록 훅을 **문서 로드 성공 직후·블로킹 UI(폰트/모달) 이전**으로 이동.
`wasm.loadDocument()` 성공 + `wasm.currentFileHandle = fileHandle` 설정 직후에 `addRecentDoc`를 호출한다(파일명/형식은 이 시점에 이미 유효).

```ts
wasm.currentFileHandle = fileHandle;
if (fileHandle) {
  void addRecentDoc({ fileName: wasm.fileName, sourceFormat: wasm.getSourceFormat(), handle: fileHandle })
    .catch((err) => console.warn('[recent] 최근 문서 기록 실패:', err));
}
await autosaveManager.beginDocument(...);
await initializeDocument(...);
```

## 재발 방지

- "부수 효과 기록"은 **가능한 한 이른 시점**(핵심 상태 확정 직후)에 배치하고, 사용자 상호작용을 동반하는 `await`(모달/프롬프트) **뒤에 두지 않는다**.
- puppeteer headless 진단으로 `open-document-bytes` → `loadBytes` → `addRecentDoc` → 렌더 체인을 재현하여 확인.

---

## 2차 증상 — "열기를 해도 등록되지 않음"

### 원인

1차 수정 후에도, 기록 훅이 **핸들이 있을 때만**(`if (fileHandle)`) 동작했다.
File System Access API 미지원 경로(브라우저 미지원·비보안 컨텍스트·임베디드 웹뷰의 `input[type=file]` 폴백, 드래그드롭)로 연 파일은 핸들이 `null`이라 **기록되지 않았다**.

### 해결 (바이트 기반 재설계)

핸들 유무와 무관하게 **모든 열기에서 등록**되도록 저장 모델을 바이트 기반으로 전환:

- `RecentDoc`에 `bytes`(문서 스냅샷) 저장, `handle`은 선택.
- `loadBytes` 훅: `if (fileHandle)` 제거 → 항상 기록(자동저장 복구본만 `options.skipRecent`로 제외).
- `file:open-recent` 재열기: **핸들 있으면 라이브 파일 우선**(권한 재확인), 실패/부재 시 **저장된 바이트로 열기**.
- 핸들이 clone 불가(DataCloneError)면 핸들 없이 바이트만 저장(`putRowResilient`).

### 검증

puppeteer headless로 `fileHandle: null`(핸들 없는 열기) → 등록·서브메뉴 활성 → 항목 클릭 → 저장 바이트로 재열기(pageCount 복원) 확인.
