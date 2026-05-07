# Task #591 — 4단계 완료보고서

## TextWrap 다층 레이어 (3-pass 순회) ✅

### 작업 내용

`CGTreeRenderer`의 트리 순회를 3-pass로 분리하여 BehindText/InFrontOfText 이미지를 텍스트와 분리된 z-order로 그리기 가능하게 함.

### 추가/변경 사항

#### 1. RenderPass enum 도입

```swift
enum RenderPass {
    case behindText    // 텍스트 뒤
    case body          // 본문 + 일반 이미지 (Square/Tight/Through/TopAndBottom/None)
    case inFrontOfText // 텍스트 위
}
```

#### 2. render() 3번 호출

```swift
func render(tree: RenderNode, in context: CGContext, pageHeight: Double, document: RhwpDocument?) {
    // ...
    renderNode(tree, in: context, pass: .behindText)
    renderNode(tree, in: context, pass: .body)
    renderNode(tree, in: context, pass: .inFrontOfText)
}
```

#### 3. renderNode/renderChildren/renderGroup에 pass 인자 추가

각 노드 처리부에서 pass별 분기:

| 노드 | behindText | body | inFrontOfText |
|------|:---:|:---:|:---:|
| Page (배경 흰색) | – | ✓ | – |
| PageBackground | – | ✓ | – |
| Body / TableCell / Group | 자식 순회 (clip 유지) | 자식 순회 | 자식 순회 |
| Rectangle / Line / Ellipse / Path | – | ✓ | – |
| TextRun / FootnoteMarker | – | ✓ | – |
| Placeholder / RawSvg | – | ✓ | – |
| **Image** (textWrap별 분기) | BehindText 시 ✓ | Square/Tight/Through/TopAndBottom/None 시 ✓ | InFrontOfText 시 ✓ |

#### 4. Image 노드의 textWrap 분기

```swift
case .image(let img):
    let imageLayer: RenderPass = {
        switch img.textWrap {
        case .behindText: return .behindText
        case .inFrontOfText: return .inFrontOfText
        default: return .body
        }
    }()
    if pass == imageLayer { renderImage(img, ...) }
```

### 성능 고려

트리를 3번 순회 — 각 pass에서 대부분의 노드는 분기로 즉시 skip. 실제 그리기는 한 번만 발생. 측정 필요 (5단계).

### 검증 결과

- Xcode 빌드 (iPad Simulator): ✅ BUILD SUCCEEDED
- iPad Simulator: ✅ sample.hwpx 1/73쪽 정상 렌더링 (회귀 0)
- nipa 로고 이미지(textWrap=None or default)는 body pass에서 정상 표시
- 3-pass 순회 도입 후에도 기존 노드 처리 영향 없음

### 미구현 (5단계)

| 항목 | 단계 |
|------|------|
| iPhone 12 Pro 실기기 검증 | 5단계 |
| 회귀 sweep (스크롤 + 다중 페이지) | 5단계 |
| 작업지시자 시각 판정 | 5단계 |
