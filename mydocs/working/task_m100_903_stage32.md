# Task m100 #903 Stage 32

## 1. 단계 목적

Stage 31 실제 구현 산출물은 rhwp-studio 내부 재로드 기준 9페이지였지만, 한컴 에디터에서
`파일 읽기 오류`가 발생했다.

Stage 30 성공 variant와 Stage 31 산출물의 가장 큰 구조 차이는 HWP 압축 여부다.

```text
Stage 30 성공 variant: 압축 예
Stage 31 실제 산출물: 압축 아니오
```

Stage 32는 파일 읽기 오류가 FileHeader/압축 축인지 확인한다.

## 2. 기준 파일

입력 HWPX:

```text
samples/hwpx/hwpx-h-01.hwpx
```

Stage 32 산출물:

```text
output/poc/hwpx2hwp/task903/stage32_file_header_probe/
```

## 3. Variant

| variant | 적용 내용 |
|---|---|
| 01_compressed_header_only | Stage 31 실제 구현 경로 + `FileHeader.compressed = true` |
| 02_reference_file_header | Stage 31 실제 구현 경로 + 한컴 정답 HWP `FileHeader` 복사 |

## 4. 작업지시자 판정 요청

내부 생성/재로드:

```text
cargo test --test hwpx_to_hwp_adapter task903_stage32_generate_file_header_probe_variants -- --nocapture
=> ok. 1 passed

01_compressed_header_only.hwp: bytes=374272, pages=9, compressed=true
02_reference_file_header.hwp: bytes=374272, pages=9, compressed=true
```

다음 파일을 한컴 에디터와 rhwp-studio에서 판정한다.

```text
output/poc/hwpx2hwp/task903/stage32_file_header_probe/01_compressed_header_only.hwp
output/poc/hwpx2hwp/task903/stage32_file_header_probe/02_reference_file_header.hwp
```

판정 항목:

```text
- 한컴 에디터 파일 읽기 오류가 사라지는지
- 한컴 에디터에서 9페이지 마지막 페이지가 출력되는지
- 표/셀 배치가 정상인지
- 꼬리말 페이지수가 검정색인지, 기존 결함인 빨간색인지
- rhwp-studio에서 9페이지로 재로드되는지
```

판정 기록:

| variant | 한컴 판정 유형 | 한컴 출력 페이지 | 마지막 페이지 출력 | 표/셀 배치 | 꼬리말 페이지수 색 | rhwp-studio 판정 | 비고 |
|---|---|---|---|---|---|---|---|
| 01_compressed_header_only | 파일 읽기 오류 |  |  |  |  |  | 파일 크기도 너무 작음 |
| 02_reference_file_header | 파일 읽기 오류 |  |  |  |  |  | 파일 크기도 너무 작음 |

## 5. 판정 해석

FileHeader/압축 축은 원인이 아니다.

```text
압축을 켜도 한컴 파일 읽기 오류가 동일하게 발생했다.
정답 HWP FileHeader를 복사해도 동일하게 파일 읽기 오류가 발생했다.
```

다음 단계는 Stage 30 성공 산출물과 Stage 32 실제 구현 산출물의 HWP 구조 차이를 비교한다.
특히 파일 크기가 정답 HWP보다 작다는 점을 같이 본다.
