# Task #591 — 구현계획서

## iOS CG 렌더러: devel 신규 노드/필드 동기화

### 설계 개요

5단계 분리. 각 단계는 독립 검증 가능 (이전 단계가 깨지지 않음).

```
1. Codable 모델 갱신   ─→ 디코딩 안전 (그리기 변경 없음)
2. Placeholder/RawSvg ─→ 자리표시자 시각화 (차트 영역 인지 가능)
3. ImageEffect         ─→ 흑백/회색 효과 적용 + brightness/contrast
4. TextWrap 레이어     ─→ BehindText/InFrontOfText 두 번 순회
5. 통합 검증           ─→ 회귀 0 + sample.hwpx 기존 동작 유지
```

### 사전 검증 (Rust 측 정확한 정의)

```
RenderNodeType::Placeholder(PlaceholderNode)  // Task #195
RenderNodeType::RawSvg(RawSvgNode)            // Task #195

PlaceholderNode { fill_color: u32, stroke_color: u32, label: String }
RawSvgNode { svg: String }

ImageNode {
    bin_data_id, data, section/para/control_index, fill_mode, original_size, transform, crop,
    original_size_hu: Option<(u32, u32)>,        // 신규 (Task #430)
    effect: ImageEffect,                          // 신규 (Task #195)
    brightness: i8,                               // 신규 (Task #195)
    contrast: i8,                                 // 신규 (Task #195)
    text_wrap: Option<TextWrap>,                  // 신규 (Task #516)
}

TextWrap (enum): Square / Tight / Through / TopAndBottom / BehindText / InFrontOfText

TextStyle {
    ...,
    extra_dash_advance: f64,    // 신규 (Task #352)
}
```

### 구현 단계 (5단계)

---

#### 1단계: Codable 모델 갱신 — 디코딩 안전성 확보

**1-1. `RenderTree.swift` 신규 enum case 추가**

```swift
enum RenderNodeType: Decodable {
    // 기존 22 case ...
    case placeholder(PlaceholderNode)   // 신규
    case rawSvg(RawSvgNode)             // 신규
    case unknown
}
```

`init(from decoder:)`의 keyed container에 매핑 추가:
```swift
if let v = try? keyed.decode(PlaceholderNode.self, forKey: .init("Placeholder")) { self = .placeholder(v); return }
if let v = try? keyed.decode(RawSvgNode.self, forKey: .init("RawSvg")) { self = .rawSvg(v); return }
```

**1-2. 신규 노드 타입 구조체**

```swift
struct PlaceholderNode: Decodable {
    let fillColor: UInt32
    let strokeColor: UInt32
    let label: String

    enum CodingKeys: String, CodingKey {
        case label
        case fillColor = "fill_color"
        case strokeColor = "stroke_color"
    }
}

struct RawSvgNode: Decodable {
    let svg: String
}
```

**1-3. ImageNode 신규 필드 (Optional 처리로 회귀 차단)**

```swift
struct ImageNode: Decodable {
    // 기존 필드 ...
    let originalSizeHu: [UInt32]?       // (u32, u32) → 배열
    let effect: ImageEffect             // enum
    let brightness: Int8
    let contrast: Int8
    let textWrap: TextWrap?

    enum CodingKeys: String, CodingKey {
        // 기존 ...
        case originalSizeHu = "original_size_hu"
        case effect, brightness, contrast
        case textWrap = "text_wrap"
    }
}

enum ImageEffect: String, Decodable {
    case realPic = "RealPic"
    case grayScale = "GrayScale"
    case blackWhite = "BlackWhite"
    case pattern8x8 = "Pattern8x8"
}

enum TextWrap: String, Decodable {
    case square = "Square", tight = "Tight", through = "Through"
    case topAndBottom = "TopAndBottom"
    case behindText = "BehindText"
    case inFrontOfText = "InFrontOfText"
}
```

**1-4. TextStyle 신규 필드**

```swift
struct TextStyle: Decodable {
    // ... 기존
    let extraDashAdvance: Double

    enum CodingKeys: String, CodingKey {
        // ... 기존
        case extraDashAdvance = "extra_dash_advance"
    }
}
```

**검증**: Xcode 빌드 + iPad Simulator에서 sample.hwpx 정상 렌더링 (디코딩 회귀 차단)

---

#### 2단계: Placeholder + RawSvg 그리기

**2-1. `CGTreeRenderer.swift`의 노드 분기에 추가**

```swift
case .placeholder(let p):
    drawPlaceholder(p, bbox: node.bbox, in: ctx)
case .rawSvg(let raw):
    drawRawSvgFallback(raw, bbox: node.bbox, in: ctx)
```

**2-2. drawPlaceholder**

```swift
private func drawPlaceholder(_ p: PlaceholderNode, bbox: BBox, in ctx: CGContext) {
    let r = cgRect(bbox)
    ctx.setFillColor(colorRefToCGColor(p.fillColor))
    ctx.fill(r)
    ctx.setStrokeColor(colorRefToCGColor(p.strokeColor))
    ctx.setLineWidth(0.5)
    ctx.stroke(r)
    drawCenteredLabel(p.label, in: r, ctx: ctx)
}
```

**2-3. drawRawSvgFallback**

iOS는 SVG 네이티브 미지원 → 임시로 회색 박스 + "[SVG 차트]" 라벨 표시. 정식 구현은 M3로 이관 (별도 이슈 등록).

```swift
private func drawRawSvgFallback(_ raw: RawSvgNode, bbox: BBox, in ctx: CGContext) {
    let r = cgRect(bbox)
    ctx.setFillColor(UIColor(white: 0.95, alpha: 1.0).cgColor)
    ctx.fill(r)
    ctx.setStrokeColor(UIColor(white: 0.7, alpha: 1.0).cgColor)
    ctx.setLineDash(phase: 0, lengths: [4, 2])
    ctx.setLineWidth(0.5)
    ctx.stroke(r)
    drawCenteredLabel("[SVG 차트]", in: r, ctx: ctx)
}
```

**2-4. drawCenteredLabel 헬퍼** (Core Text)

기존 텍스트 렌더링 패턴(Y축 반전 + 중앙 배치) 재사용.

**검증**: 차트/OLE 포함 권위 샘플 (없을 시 mock JSON)에서 자리표시자 박스 표시 확인

---

#### 3단계: ImageEffect + brightness/contrast

**3-1. CGImage 캐시 키 확장**

기존 `[UInt16: CGImage]` → `[ImageCacheKey: CGImage]`로 변경 (effect/brightness/contrast 별 별개 캐시).

```swift
struct ImageCacheKey: Hashable {
    let binDataId: UInt16
    let effect: ImageEffect
    let brightness: Int8
    let contrast: Int8
}
```

**3-2. ImageEffect Core Image 필터 적용**

| Rust effect | Core Image 필터 |
|-------------|----------------|
| RealPic | (변환 없음) |
| GrayScale | `CIPhotoEffectMono` |
| BlackWhite | `CIColorThreshold` (threshold=0.5) |
| Pattern8x8 | 임시: `CIPhotoEffectMono` 폴백 (정확 매핑은 M3) |

brightness/contrast: `CIColorControls` 필터 (`inputBrightness = brightness/100.0`, `inputContrast = 1.0 + contrast/100.0`)

```swift
private func applyImageEffect(_ cgImage: CGImage, effect: ImageEffect,
                              brightness: Int8, contrast: Int8) -> CGImage
```

**3-3. renderImage에 통합**

기존 캐시 조회 로직을 새 키로 변경, 미스 시 effect 적용.

**검증**: 흑백/회색 효과 이미지 포함 샘플 (있다면) 확인. 없으면 mock JSON으로 효과별 결과 비교.

---

#### 4단계: TextWrap 다층 레이어

**4-1. 트리 순회 분리 (3-pass)**

```swift
enum RenderPass { case behindText, body, inFrontOfText }

func render(tree: RenderNode, ...) {
    renderNode(tree, in: context, pass: .behindText)
    renderNode(tree, in: context, pass: .body)
    renderNode(tree, in: context, pass: .inFrontOfText)
}
```

**4-2. renderNode가 pass별 필터**

```swift
case .image(let img):
    let layer: RenderPass = {
        switch img.textWrap {
        case .behindText: return .behindText
        case .inFrontOfText: return .inFrontOfText
        default: return .body
        }
    }()
    if layer == pass { renderImage(img, bbox: node.bbox, in: ctx) }
```

비-image 노드는 `.body` pass에서만 그리기. Group 등 자식 순회 노드도 모든 pass에서 자식만 순회 (자식이 image면 자체 필터).

**검증**: BehindText/InFrontOfText 이미지 포함 mock JSON으로 순서 확인 (BehindText는 텍스트보다 뒤, InFrontOfText는 위)

---

#### 5단계: 통합 검증 + iPhone 실기기

**5-1. 검증 항목**

- iPad Simulator (iPad Pro 11 M4): sample.hwpx 회귀 0
- iPhone 12 Pro 실기기: sample.hwpx 회귀 0
- `cargo test --lib`: 1141 passed 유지
- Xcode 빌드: 경고 0

**5-2. 작업지시자 시각 판정**

차트/OLE 포함 권위 샘플 시각 확인. 없으면 sample.hwpx 회귀 0만으로 합격.

**5-3. 메모리 누수 점검**

ImageCache 크기 모니터링 (페이지 스크롤 시 unload 정상). Pass 분리로 인한 추가 순회 비용 측정 (66페이지 문서에서 체감 가능 여부).

---

### 파일 변경 목록

| 파일 | 변경 | 단계 |
|------|------|------|
| `rhwp-ios/Sources/RenderTree.swift` | Codable 모델 갱신 (Placeholder/RawSvg/ImageEffect/TextWrap/extra_dash_advance) | 1 |
| `rhwp-ios/Sources/CGTreeRenderer.swift` | Placeholder/RawSvg 그리기, drawCenteredLabel 헬퍼 | 2 |
| `rhwp-ios/Sources/CGTreeRenderer.swift` | ImageCacheKey + Core Image 필터 적용 | 3 |
| `rhwp-ios/Sources/CGTreeRenderer.swift` | RenderPass 3-pass 순회 분리 | 4 |
| `mydocs/working/task_m2_591_stage{1..5}.md` | 단계별 완료보고서 | 1~5 |
| `mydocs/report/task_m2_591_report.md` | 최종 완료보고서 | 5 |

### 후속 이관

| 항목 | 이관 |
|------|------|
| RawSvg 정식 구현 (SVG → CGImage 변환) | M3 별도 이슈 |
| Pattern8x8 정확한 렌더링 (HWP 8×8 dot pattern) | M3 |
| 수식(Equation) 렌더링 품질 개선 | 별도 이슈 |
