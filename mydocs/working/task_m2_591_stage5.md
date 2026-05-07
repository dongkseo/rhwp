# Task #591 — 5단계 완료보고서

## 통합 검증 + iPhone 실기기 + 시각 판정 ✅

### 검증 결과

| 항목 | 결과 |
|------|------|
| `cargo test --lib` | ✅ 1141 passed, 0 failed (회귀 0, Rust 변경 없음) |
| Xcode 빌드 (iPad Simulator) | ✅ BUILD SUCCEEDED |
| Xcode 빌드 (iPhone 실기기) | ✅ BUILD SUCCEEDED |
| iPad Simulator 시각 (sample.hwpx) | ✅ 1/73쪽 정상 |
| iPhone 12 Pro 실기기 시각 (sample.hwpx) | ✅ 작업지시자 시각 판정 통과 |

### 검증 환경

- macOS, ios/devel 브랜치
- iPad Pro 11-inch M4 Simulator (iOS 26.4)
- iPhone 12 Pro 실기기 (iOS 26.x)
- 샘플: `sample.hwpx` (73페이지)

### 성능 영향 (3-pass 순회)

3-pass 순회로 트리를 3번 방문하지만 각 pass에서 대부분의 노드는 분기로 즉시 skip:
- BehindText/InFrontOfText 이미지가 없는 sample.hwpx에서는 **2개 pass(behindText/inFrontOfText)가 사실상 no-op**
- 실제 렌더링은 body pass에서만 1회 발생
- 사용자 체감 성능 저하 없음

### 회귀 차단 요약

전체 5단계가 회귀 없이 통합 완료:

| 단계 | 핵심 변경 | 회귀 |
|------|----------|:---:|
| 1 | Codable 모델 갱신 (Optional/fallback 처리) | 0 |
| 2 | Placeholder/RawSvg 그리기 (신규 case) | 0 (sample에는 노드 없음) |
| 3 | ImageEffect Core Image 필터 (RealPic 우회) | 0 |
| 4 | 3-pass 순회 (각 pass 분기로 skip) | 0 |
| 5 | 통합 검증 | 0 |

### 미구현 (M3로 이관)

| 항목 | 사유 |
|------|------|
| RawSvg 정식 SVG → CGImage 변환 | iOS는 SVG 네이티브 미지원, 별도 라이브러리/WebKit 필요 |
| Pattern8x8 정확한 도트 패턴 | HWP 8×8 dot pattern 표준 매핑 부재 |
| BlackWhite 정확한 threshold 분리 | 임시로 mono+contrast 4.0 적용 |
| 차트/OLE/효과 이미지 권위 샘플 검증 | 본 환경에 해당 샘플 부재 |

### 다음 작업

- 차트 포함 권위 샘플 확보 시 추가 검증 (별도 이슈)
- M3 사이클에서 RawSvg 정식 구현
