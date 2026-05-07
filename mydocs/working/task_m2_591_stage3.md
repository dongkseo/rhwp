# Task #591 — 3단계 완료보고서

## ImageEffect + brightness/contrast 보정 ✅

### 작업 내용

`CGTreeRenderer.swift`에 이미지 효과(GrayScale/BlackWhite/Pattern8x8) 및 밝기/대비 보정 적용.

### 변경 사항

#### 1. import 추가

```swift
import CoreImage
```

#### 2. ImageCacheKey 구조체 (캐시 키 확장)

```swift
struct ImageCacheKey: Hashable {
    let binDataId: UInt16
    let effect: ImageEffect
    let brightness: Int8
    let contrast: Int8
}
```

기존 `[UInt16: CGImage]` → `[ImageCacheKey: CGImage]`. 동일 이미지의 효과별 별개 캐시.

#### 3. CIContext 인스턴스

```swift
private let ciContext = CIContext(options: [.useSoftwareRenderer: false])
```

GPU 가속 활성화. 효과 적용 결과를 CGImage로 변환할 때 재사용.

#### 4. applyImageEffect 메서드

| Rust effect | iOS 처리 |
|-------------|----------|
| RealPic | 변환 없음 (원본) |
| GrayScale | `CIPhotoEffectMono` |
| BlackWhite | `CIPhotoEffectMono` + `CIColorControls(contrast: 4.0)` (임계값 분리 효과) |
| Pattern8x8 | 임시: `CIPhotoEffectMono` 폴백 (정확 매핑은 M3 별도 이슈) |

밝기/대비:
- `CIColorControls`
- brightness: HWP `-100..+100` → CI `inputBrightness -1.0..+1.0`
- contrast: HWP `-100..+100` → CI `inputContrast 0.0..2.0` (1.0이 원본)

#### 5. renderImage 통합

```swift
let key = ImageCacheKey(binDataId: img.binDataId, effect: img.effect,
                       brightness: img.brightness, contrast: img.contrast)

if let cached = imageCache[key] { cgImage = cached }
else {
    // ... 데이터 로드 ...
    let processed = (img.effect != .realPic
                    || img.brightness != 0
                    || img.contrast != 0)
        ? applyImageEffect(raw, effect: img.effect, brightness: img.brightness, contrast: img.contrast)
        : raw
    imageCache[key] = processed
    cgImage = processed
}
```

**최적화**: 효과/보정이 모두 기본값(RealPic, 0, 0)이면 CIFilter 우회하여 원본 캐시.

### 검증 결과

- Xcode 빌드 (iPad Simulator): ✅ BUILD SUCCEEDED
- iPad Simulator: ✅ sample.hwpx 1/73쪽 정상 렌더링 (회귀 0)
- nipa 로고 (RealPic 효과)는 변환 없이 원본 표시 — 효과 분기 정상

### 미구현 (후속)

| 항목 | 단계/이관 |
|------|------|
| TextWrap 다층 레이어 (BehindText/InFrontOfText) | 4단계 |
| Pattern8x8 정확한 도트 패턴 | M3 별도 이슈 |
| BlackWhite의 정확한 threshold (0.5 기준 2색화) | 향후 개선 |
