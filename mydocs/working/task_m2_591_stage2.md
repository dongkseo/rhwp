# Task #591 — 2단계 완료보고서

## Placeholder + RawSvg 그리기 ✅

### 작업 내용

`CGTreeRenderer.swift`에 `Placeholder`와 `RawSvg` 노드 그리기 추가.

### 추가된 메서드

| 메서드 | 역할 |
|--------|------|
| `renderPlaceholder` | PlaceholderNode → 배경 rect + 테두리 + 중앙 라벨 |
| `renderRawSvgFallback` | RawSvgNode → 회색 점선 박스 + "[SVG 차트]" 라벨 (임시 폴백) |
| `drawCenteredLabel` | Core Text 기반 사각형 중앙 라벨 (Y축 반전 처리 포함) |

### 노드 분기 추가

```swift
case .placeholder(let p):
    renderPlaceholder(p, bbox: node.bbox, in: ctx)
case .rawSvg(let raw):
    renderRawSvgFallback(raw, bbox: node.bbox, in: ctx)
```

### Placeholder 그리기

Rust 측이 fillColor / strokeColor / label을 제공하므로 단순 구현:

```swift
ctx.setFillColor(colorRefToCGColor(p.fillColor))
ctx.fill(r)
ctx.setStrokeColor(colorRefToCGColor(p.strokeColor))
ctx.setLineWidth(0.5)
ctx.stroke(r)
drawCenteredLabel(p.label, in: r, ctx: ctx, color: .darkGray)
```

### RawSvg 임시 폴백

iOS는 SVG 네이티브 렌더링 미지원. M3에서 SVG → CGImage 변환 정식 구현 예정. 현재는 사용자가 차트 영역을 인지할 수 있도록 시각화:

- 옅은 회색 배경 (#F5F5F5)
- 회색 점선 테두리 (dash 4-2)
- "[SVG 차트]" 중앙 라벨

### drawCenteredLabel 헬퍼

- 폰트 크기: `min(rect.width, rect.height) * 0.12` (최소 8pt)
- 폰트: AppleSDGothicNeo-Regular
- Y축 반전: 페이지 좌표계가 좌상단 원점이므로 텍스트만 국소 반전
- `CTLineGetImageBounds`로 정확한 중앙 정렬

### 검증 결과

- Xcode 빌드 (iPad Simulator): ✅ BUILD SUCCEEDED
- iPad Simulator: ✅ sample.hwpx 1/73쪽 정상 렌더링 (회귀 0)
- sample.hwpx에는 Placeholder/RawSvg 노드가 없으므로 신규 코드는 휴면 상태이지만 기존 노드 처리에 영향 없음

### 미구현 (후속 단계)

| 항목 | 단계 |
|------|------|
| ImageEffect (RealPic/GrayScale/BW/Pattern) | 3단계 |
| brightness/contrast Core Image 보정 | 3단계 |
| TextWrap 다층 레이어 (BehindText/InFrontOfText) | 4단계 |
| RawSvg 정식 구현 (SVG → CGImage) | M3 별도 이슈 |
