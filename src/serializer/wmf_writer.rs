//! 최소 WMF(Windows Metafile) writer — 비트맵 1장을 담는 용도.
//!
//! HWP 차트는 OLE 개체이고, 한컴·뷰어가 화면에 그리는 것은 OLE 안의
//! `\x02OlePres000` 미리보기다 (`OOXMLChartContents` XML 은 화면에 영향이 없다 —
//! 실측: 미리보기를 지우면 아무것도 안 보이고, XML 을 바꿔도 화면은 그대로).
//! 그 미리보기 포맷이 `CF_METAFILEPICT`(WMF)이므로, 차트를 새로 만들려면
//! WMF 를 써야 한다.
//!
//! 한컴 원본은 수백 KB 짜리 벡터 드로잉 레코드 뭉치지만, 여기서는
//! `META_STRETCHDIB` 하나로 래스터 이미지를 통째로 얹는다. 뷰어 실측에서
//! 벡터 원본보다 오히려 안정적으로 그려졌다.
//!
//! 레코드 배치 근거: `src/wmf/parser/records/` (MS-WMF 스펙 주석 동반).

const META_EOF: u16 = 0x0000;
const META_SETWINDOWORG: u16 = 0x020B;
const META_SETWINDOWEXT: u16 = 0x020C;
const META_STRETCHDIB: u16 = 0x0F43;

const SRCCOPY: u32 = 0x00CC_0020;
const DIB_RGB_COLORS: u16 = 0;

/// WMF 레코드 하나: `RecordSize`(워드 수, 자신 포함) + `RecordFunction` + 파라미터.
fn record(func: u16, params: &[u8]) -> Vec<u8> {
    debug_assert_eq!(params.len() % 2, 0, "WMF 파라미터는 워드 정렬이어야 한다");
    let size_words = 3 + (params.len() / 2) as u32;
    let mut out = Vec::with_capacity(6 + params.len());
    out.extend_from_slice(&size_words.to_le_bytes());
    out.extend_from_slice(&func.to_le_bytes());
    out.extend_from_slice(params);
    out
}

/// RGBA(위→아래) 픽셀을 24bpp bottom-up DIB 로 변환한다.
///
/// DIB 는 행을 4바이트 경계로 정렬하고(stride), 아래에서 위로 저장한다.
/// 알파는 흰 배경에 합성한다 (미리보기는 불투명해야 한다).
fn rgba_to_dib(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let stride = (((width * 24) + 31) / 32) * 4;
    let mut bits = vec![0u8; (stride * height) as usize];

    for y in 0..height {
        let src_row = (y * width * 4) as usize;
        let dst_row = ((height - 1 - y) * stride) as usize; // bottom-up
        for x in 0..width {
            let s = src_row + (x * 4) as usize;
            let (r, g, b, a) = (rgba[s], rgba[s + 1], rgba[s + 2], rgba[s + 3]);
            // 흰 배경 합성
            let blend = |c: u8| -> u8 {
                let c = c as u32 * a as u32 + 255 * (255 - a as u32);
                (c / 255) as u8
            };
            let d = dst_row + (x * 3) as usize;
            bits[d] = blend(b);
            bits[d + 1] = blend(g);
            bits[d + 2] = blend(r);
        }
    }

    let mut dib = Vec::with_capacity(40 + bits.len());
    dib.extend_from_slice(&40u32.to_le_bytes()); // biSize
    dib.extend_from_slice(&(width as i32).to_le_bytes()); // biWidth
    dib.extend_from_slice(&(height as i32).to_le_bytes()); // biHeight (양수 = bottom-up)
    dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    dib.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
    dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    dib.extend_from_slice(&(bits.len() as u32).to_le_bytes()); // biSizeImage
    dib.extend_from_slice(&2835i32.to_le_bytes()); // biXPelsPerMeter (72dpi)
    dib.extend_from_slice(&2835i32.to_le_bytes()); // biYPelsPerMeter
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    dib.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant
    dib.extend_from_slice(&bits);
    dib
}

/// RGBA 이미지를 담은 WMF 를 만든다.
pub fn wmf_from_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    let dib = rgba_to_dib(rgba, width, height);
    let (w, h) = (width as i16, height as i16);

    let mut params = Vec::with_capacity(20 + dib.len());
    params.extend_from_slice(&SRCCOPY.to_le_bytes());
    params.extend_from_slice(&DIB_RGB_COLORS.to_le_bytes());
    // src_height, src_width, y_src, x_src
    for v in [h, w, 0, 0] {
        params.extend_from_slice(&v.to_le_bytes());
    }
    // dest_height, dest_width, y_dst, x_dst
    for v in [h, w, 0, 0] {
        params.extend_from_slice(&v.to_le_bytes());
    }
    params.extend_from_slice(&dib);
    if params.len() % 2 == 1 {
        params.push(0);
    }

    let records = [
        record(META_SETWINDOWORG, &[0, 0, 0, 0]), // y=0, x=0
        record(META_SETWINDOWEXT, {
            let mut p = Vec::with_capacity(4);
            p.extend_from_slice(&h.to_le_bytes());
            p.extend_from_slice(&w.to_le_bytes());
            &p.clone()
        }),
        record(META_STRETCHDIB, &params),
        record(META_EOF, &[]),
    ];

    let body_len: usize = records.iter().map(|r| r.len()).sum();
    let max_record_words = records.iter().map(|r| r.len() / 2).max().unwrap_or(0) as u32;
    let total_words = ((18 + body_len) / 2) as u32;

    let mut wmf = Vec::with_capacity(18 + body_len);
    wmf.extend_from_slice(&1u16.to_le_bytes()); // mtType = memory
    wmf.extend_from_slice(&9u16.to_le_bytes()); // mtHeaderSize (words)
    wmf.extend_from_slice(&0x0300u16.to_le_bytes()); // mtVersion = Windows 3.0
    wmf.extend_from_slice(&total_words.to_le_bytes());
    wmf.extend_from_slice(&1u16.to_le_bytes()); // mtNoObjects
    wmf.extend_from_slice(&max_record_words.to_le_bytes());
    wmf.extend_from_slice(&0u16.to_le_bytes()); // mtNoParameters
    for r in &records {
        wmf.extend_from_slice(r);
    }
    wmf
}

/// WMF 를 OLE 미리보기 스트림(`\x02OlePres000`) 으로 감싼다.
///
/// 배치 근거: MS-OLEDS 2.2.3.3 OLEPresentationStream + 실물 실측
/// (`samples/chart/**` 의 `\x02OlePres000` 헤더 40바이트).
pub fn ole_presentation_stream(wmf: &[u8], width_hu: u32, height_hu: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(40 + wmf.len());
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // MarkerOrLength
    out.extend_from_slice(&3u32.to_le_bytes()); // CF_METAFILEPICT
    out.extend_from_slice(&4u32.to_le_bytes()); // TargetDeviceSize
    out.extend_from_slice(&1u32.to_le_bytes()); // Aspect = DVASPECT_CONTENT
    out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes()); // Lindex
    out.extend_from_slice(&0u32.to_le_bytes()); // Advf
    out.extend_from_slice(&0u32.to_le_bytes()); // Reserved
    out.extend_from_slice(&width_hu.to_le_bytes());
    out.extend_from_slice(&height_hu.to_le_bytes());
    out.extend_from_slice(&(wmf.len() as u32).to_le_bytes());
    out.extend_from_slice(wmf);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_rgba(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter()
            .copied()
            .cycle()
            .take((w * h * 4) as usize)
            .collect()
    }

    #[test]
    fn wmf_header_matches_hancom_signature() {
        // 한컴 실물 미리보기의 앞 6바이트: 01 00 09 00 00 03
        let wmf = wmf_from_rgba(&solid_rgba(4, 4, [255, 0, 0, 255]), 4, 4);
        assert_eq!(&wmf[..6], &[0x01, 0x00, 0x09, 0x00, 0x00, 0x03]);
    }

    #[test]
    fn dib_is_bottom_up_with_padded_stride() {
        // 폭 3px → 3*3=9B → stride 12B (4바이트 정렬)
        let dib = rgba_to_dib(&solid_rgba(3, 2, [10, 20, 30, 255]), 3, 2);
        let width = i32::from_le_bytes(dib[4..8].try_into().unwrap());
        let height = i32::from_le_bytes(dib[8..12].try_into().unwrap());
        let size_image = u32::from_le_bytes(dib[20..24].try_into().unwrap());
        assert_eq!(width, 3);
        assert_eq!(height, 2, "bottom-up 이면 높이가 양수여야 한다");
        assert_eq!(size_image, 12 * 2, "stride 12B × 2행");
        // BGR 순서 확인 (RGBA 10,20,30 → BGR 30,20,10)
        assert_eq!(&dib[40..43], &[30, 20, 10]);
    }

    #[test]
    fn alpha_composites_onto_white() {
        // 완전 투명 → 흰색
        let dib = rgba_to_dib(&solid_rgba(1, 1, [0, 0, 0, 0]), 1, 1);
        assert_eq!(&dib[40..43], &[255, 255, 255]);
    }

    #[test]
    fn presentation_stream_declares_metafilepict() {
        let wmf = wmf_from_rgba(&solid_rgba(2, 2, [0, 0, 255, 255]), 2, 2);
        let pres = ole_presentation_stream(&wmf, 432, 252);
        assert_eq!(&pres[0..4], &0xFFFF_FFFFu32.to_le_bytes());
        assert_eq!(&pres[4..8], &3u32.to_le_bytes(), "CF_METAFILEPICT");
        assert_eq!(&pres[36..40], &(wmf.len() as u32).to_le_bytes());
        assert_eq!(&pres[40..], &wmf[..]);
    }
}
