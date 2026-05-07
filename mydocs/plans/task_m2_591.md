# Task #591 — 수행계획서

## iOS CG 렌더러: devel 신규 노드/필드 동기화

### 배경

`ios/devel`은 2026-04-20 Task #93 완료 시점에 분기되었다. 이후 devel에는 다수의 렌더링 관련 작업이 누적되어 Rust 측 렌더 트리에 새 노드 타입과 필드가 추가되었다. 5월 7일 시점에 누적 분량을 ios/devel로 머지 완료했지만, **Swift 측 `RenderTree.swift` (Codable 모델)과 `CGTreeRenderer.swift` (실제 그리기)는 아직 4월 20일 시점**이다.

결과적으로 차트/OLE 객체, 흑백/회색 이미지 효과, BehindText/InFrontOfText 레이어 처리가 포함된 HWP 문서를 열면 해당 영역이 무시되어 빈 영역으로 표시된다. 샘플 파일(sample.hwpx)에는 영향이 없어 시각 검증으로 즉시 드러나지 않는다.

### 누락 항목 (Rust → Swift 미반영)

#### `RenderTree.swift` (Codable 디코딩)

| Rust 측 신규 | 도입 | 영향 |
|-------------|:---:|------|
| `RenderNodeType::Placeholder(PlaceholderNode)` | Task #195 | OLE/Chart 자리표시자 → unknown 폴백 |
| `RenderNodeType::RawSvg(RawSvgNode)` | Task #195 | OOXML 차트, EMF 변환 SVG → 무시 |
| `TextStyle.extra_dash_advance` | Task #352 | dash leader 글자 간격 누락 (점선 …) |
| `ImageNode.original_size_hu` | Task #430 | crop 좌표 변환 정확도 |
| `ImageNode.effect` (RealPic/GrayScale/BW/Pattern8x8) | Task #195 | 흑백/회색 효과 무시 |
| `ImageNode.brightness/contrast` | Task #195 | 이미지 밝기/대비 보정 무시 |
| `ImageNode.text_wrap` | Task #516 | 다층 레이어 (BehindText/InFrontOfText) |

#### `CGTreeRenderer.swift` (실제 그리기)

위 항목들이 Codable에 반영되더라도 **그리기 로직**도 추가되어야 함:
- Placeholder: 배경 rect + 중앙 텍스트 라벨
- RawSvg: SVG 조각 → CGImage 변환 (또는 별도 SVG 렌더러)
- ImageEffect: Core Image 필터 또는 CGImage 픽셀 조작
- brightness/contrast: Core Image 보정
- TextWrap 레이어: BehindText는 텍스트보다 먼저, InFrontOfText는 나중에 그리기

### 목표

1. **Swift Codable 모델 정합** — devel의 모든 신규 노드/필드를 디코딩 가능하게 함
2. **그리기 로직 단계적 보강** — Placeholder/RawSvg → ImageEffect → TextWrap 순
3. **회귀 차단** — 기존 sample.hwpx 렌더링 결과 유지

### 범위

**포함:**
- `RenderTree.swift` Codable 모델 갱신 (모든 신규 항목)
- `CGTreeRenderer.swift` 그리기 로직 추가 (단계별)
- 차트/OLE/효과 이미지 포함 샘플로 검증

**제외 (후속 이관):**
- 수식 (Equation) 렌더링 품질 개선 (별도 이슈)
- Skia 백엔드 통합 (PR #599 영역)
- 폰트 폴백 추가 매핑

### 기술 검토

#### Placeholder/RawSvg 처리 전략

**Placeholder**: Rust 측이 텍스트 라벨 + 배경 rect를 만들어주므로 단순 구현 가능. CGTreeRenderer에서 rect → text 순으로 그리기.

**RawSvg**: 핵심 난제. iOS는 SVG 네이티브 렌더링 API가 없다 (CGSVGDocument는 비공개 API). 옵션:
- A) WebKit으로 SVG → CGImage 변환 (느림, 비동기)
- B) `usvg` (Rust)로 SVG → 도형 트리 변환 후 RenderTree에 통합 (Rust 측 변경 필요)
- C) iOS 13+의 `SVGKit` 등 외부 라이브러리
- **D) 임시 placeholder 박스만 표시** + 향후 정식 구현 (M3 후보)

Task #591 범위에서는 **D안** 채택 권장 — Placeholder처럼 "차트 표시 영역" 박스로 그려 시각 누락 인지 가능하게 함.

#### ImageEffect 처리

Core Image 필터 (`CIFilter`):
- GrayScale: `CIColorMonochrome` 또는 `CIPhotoEffectMono`
- BlackWhite: `CIColorThreshold`
- Pattern8x8: HWP 8×8 dot pattern은 iOS 표준 매핑 없음 → CGImage 픽셀 조작 또는 단색 폴백

#### TextWrap 다층 레이어

CGTreeRenderer가 PageRenderTree를 한 번 순회하지만, BehindText 이미지는 **텍스트 그리기 전에**, InFrontOfText는 **나중에** 그려야 한다. 두 가지 전략:
- A) 두 번 순회: BehindText 먼저 + 본문 + InFrontOfText
- B) Rust 측이 이미 layer 분리한 트리를 보내면 자연스러운 순서 — devel의 layer_renderer 활용 검토

A안이 단순하므로 우선 채택.

### 위험 요소

| 위험 | 대응 |
|------|------|
| 신규 필드 디코딩 실패로 기존 동작 회귀 | 새 필드는 Optional로 처리, 기본값 동작 보장 |
| RawSvg 미구현으로 차트가 빈 영역 | 임시 placeholder 박스 표시 (시각 인지) |
| Core Image 성능 (이미지마다 필터 적용) | 캐시: `(binDataId, effect) → CGImage` |
| Rust 측 추가 필드 누락 가능성 | devel HEAD에서 다시 한번 grep 확인 |

### 산출물

| 파일 | 내용 |
|------|------|
| `rhwp-ios/Sources/RenderTree.swift` | Codable 모델 갱신 |
| `rhwp-ios/Sources/CGTreeRenderer.swift` | Placeholder, RawSvg, ImageEffect, TextWrap 처리 추가 |
| `mydocs/working/task_m2_591_stage*.md` | 단계별 완료보고서 |
| `mydocs/report/task_m2_591_report.md` | 최종 완료보고서 |

### 검증 방법

- `cargo test --lib` 회귀 0
- iPad Simulator/iPhone 실기기에서 sample.hwpx 기존 렌더링 결과 유지
- 차트/OLE/이미지 효과 포함 권위 샘플 (있다면) 시각 확인
- 작업지시자 시각 판정
