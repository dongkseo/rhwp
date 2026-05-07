# Task #591 — 최종 완료보고서

## iOS CG 렌더러: devel 신규 노드/필드 동기화

### 목표

`ios/devel`이 4월 20일 분기된 후 devel에 누적된 1000+ 커밋 중 **렌더 트리 관련 신규 노드/필드**를 Swift 측 `RenderTree.swift` (Codable) 및 `CGTreeRenderer.swift` (그리기)에 반영하여, 차트/OLE/효과 이미지 누락 렌더링을 차단한다.

### 단계별 결과

| 단계 | 내용 | 결과 |
|------|------|------|
| 1 | Codable 모델 갱신 | ✅ Placeholder/RawSvg/ImageEffect/TextWrap/extra_dash_advance/originalSizeHu 모두 디코딩 가능 |
| 2 | Placeholder/RawSvg 그리기 | ✅ 자리표시자 박스 + 라벨, RawSvg 임시 폴백 |
| 3 | ImageEffect + brightness/contrast | ✅ Core Image 4종 필터 + 효과별 캐시 분리 |
| 4 | TextWrap 다층 레이어 | ✅ 3-pass 순회 (behindText / body / inFrontOfText) |
| 5 | 통합 검증 + 시각 판정 | ✅ iPhone 12 Pro 실기기 + iPad Simulator 통과 |

### 핵심 변경

#### Swift Codable 모델 (`RenderTree.swift`)

| 신규 항목 | 종류 |
|-----------|------|
| `case placeholder(PlaceholderNode)` | RenderNodeType variant |
| `case rawSvg(RawSvgNode)` | RenderNodeType variant |
| `PlaceholderNode { fillColor, strokeColor, label }` | 구조체 |
| `RawSvgNode { svg }` | 구조체 |
| `ImageEffect` (RealPic/GrayScale/BlackWhite/Pattern8x8) | enum |
| `TextWrap` (Square/Tight/Through/TopAndBottom/BehindText/InFrontOfText) | enum |
| `ImageNode.originalSizeHu` | Optional 필드 |
| `ImageNode.effect/brightness/contrast/textWrap` | 신규 필드 (fallback 처리) |
| `TextStyle.extraDashAdvance` | 신규 필드 (fallback 처리) |

**회귀 차단 패턴**: 신규 필드는 `try? c.decode(...) ?? 기본값` 또는 `decodeIfPresent`로 처리하여 기존 JSON 호환성 유지.

#### CG 렌더러 (`CGTreeRenderer.swift`)

| 추가 메서드/타입 | 역할 |
|---------------|------|
| `RenderPass` enum | 3-pass 순회 (behindText/body/inFrontOfText) |
| `renderPlaceholder` | 배경 rect + 테두리 + 중앙 라벨 |
| `renderRawSvgFallback` | 회색 점선 박스 + "[SVG 차트]" 임시 폴백 |
| `drawCenteredLabel` | Core Text 중앙 라벨 (Y축 반전 처리) |
| `applyImageEffect` | CIFilter 체인 (mono / contrast / brightness) |
| `ImageCacheKey` 구조체 | bin_data_id + effect + brightness + contrast별 캐시 |

#### 렌더링 파이프라인 변경

```
Before:  render() → renderNode(tree)                      [1회 순회]
After:   render() → renderNode(tree, pass: .behindText)    [3회 순회]
                  → renderNode(tree, pass: .body)
                  → renderNode(tree, pass: .inFrontOfText)
```

각 pass에서 노드별 분기로 즉시 skip → 실제 그리기는 적절한 pass에서만 1회.

### 회귀 차단 결과

| 검증 | 결과 |
|------|------|
| `cargo test --lib` | 1141 passed, 0 failed |
| Xcode 빌드 (Simulator + 실기기) | ✅ 둘 다 BUILD SUCCEEDED |
| iPad Pro 11 M4 Simulator | sample.hwpx 1/73쪽 정상 |
| iPhone 12 Pro 실기기 | 작업지시자 시각 판정 통과 |

### 미반영 (후속 이관)

| 항목 | 이관 |
|------|------|
| RawSvg 정식 구현 (SVG → CGImage 변환) | M3 별도 이슈 |
| Pattern8x8 정확한 도트 패턴 | M3 별도 이슈 |
| BlackWhite 정확한 threshold 2색화 | 향후 개선 |
| 차트/OLE/효과 이미지 권위 샘플 검증 | 샘플 확보 후 별도 이슈 |

### 후속 작업

- 차트 포함 권위 샘플 확보 시 추가 시각 검증
- M3에서 RawSvg/Pattern8x8 정식 구현
- 수식(Equation) 렌더링 품질 개선 (별도 이슈)
