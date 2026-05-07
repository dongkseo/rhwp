# Task #591 — 1단계 완료보고서

## Codable 모델 갱신 (RenderTree.swift) ✅

### 작업 내용

`rhwp-ios/Sources/RenderTree.swift`에 devel 신규 항목을 디코딩 가능하도록 갱신.

### 추가 항목

| 항목 | 처리 |
|------|------|
| `RenderNodeType::Placeholder` case | `case placeholder(PlaceholderNode)` 신규 |
| `RenderNodeType::RawSvg` case | `case rawSvg(RawSvgNode)` 신규 |
| `PlaceholderNode` 구조체 | fillColor, strokeColor, label |
| `RawSvgNode` 구조체 | svg (String) |
| `ImageEffect` enum | RealPic / GrayScale / BlackWhite / Pattern8x8 |
| `TextWrap` enum | Square / Tight / Through / TopAndBottom / BehindText / InFrontOfText |
| `ImageNode.originalSizeHu` | `[UInt32]?` |
| `ImageNode.effect` | `ImageEffect` (기본값 RealPic) |
| `ImageNode.brightness` | `Int8` (기본값 0) |
| `ImageNode.contrast` | `Int8` (기본값 0) |
| `ImageNode.textWrap` | `TextWrap?` |
| `TextStyle.extraDashAdvance` | `Double` (기본값 0) |

### 회귀 차단 전략

기존 sample.hwpx JSON에 신규 필드가 모두 포함되어 있는지는 알 수 없으므로, **신규 필드는 안전하게 fallback 처리**:

- ImageNode: 직접 `init(from:)` 작성 — `try?` + `?? 기본값` 패턴
- TextStyle: 직접 `init(from:)` 작성 — `extraDashAdvance`만 `try?` + `?? 0`
- Optional 필드(originalSizeHu, textWrap)는 `decodeIfPresent` 사용

### init(from:) 디코딩 예시

```swift
// ImageNode
effect = (try? c.decode(ImageEffect.self, forKey: .effect)) ?? .realPic
brightness = (try? c.decode(Int8.self, forKey: .brightness)) ?? 0
contrast = (try? c.decode(Int8.self, forKey: .contrast)) ?? 0
textWrap = try? c.decodeIfPresent(TextWrap.self, forKey: .textWrap)

// TextStyle
extraDashAdvance = (try? c.decode(Double.self, forKey: .extraDashAdvance)) ?? 0
```

### 검증 결과

- Xcode 빌드 (iPad Simulator): ✅ BUILD SUCCEEDED
- iPad Simulator (iPad Pro 11 M4): ✅ sample.hwpx 1/73쪽 정상 렌더링
- 디코딩 회귀: 0 (신규 필드 fallback으로 기존 JSON 호환)
- 그리기 변경: 0 (1단계는 모델만, CGTreeRenderer 미변경)

### 미구현 (후속 단계)

| 항목 | 단계 |
|------|------|
| Placeholder 그리기 | 2단계 |
| RawSvg 그리기 (임시 폴백) | 2단계 |
| ImageEffect Core Image 필터 | 3단계 |
| brightness/contrast 보정 | 3단계 |
| TextWrap 다층 레이어 (3-pass) | 4단계 |
