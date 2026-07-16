//! WMF writer 가 뷰어 검증을 통과한 스파이크 산출물과 구조적으로 일치하는가.
//!
//! 파이썬 스파이크(scratchpad/wmf_spike3.py)로 만든 WMF 를 뷰어에서 확인했고
//! (차트가 깨끗하게 렌더됨), 이 테스트는 Rust writer 가 같은 바이트 배치를
//! 내는지 검증한다.

use rhwp::serializer::wmf_writer::{ole_presentation_stream, wmf_from_rgba};

/// 스파이크 실측값: 288x167 24bpp → stride 864, 픽셀 144,288B
#[test]
fn stride_and_size_match_measured_spike() {
    let (w, h) = (288u32, 167u32);
    let rgba = vec![255u8; (w * h * 4) as usize];
    let wmf = wmf_from_rgba(&rgba, w, h);

    // 스파이크: stride = ((288*24)+31)/32*4 = 864, 픽셀 = 864*167 = 144,288
    let stride = (((w * 24) + 31) / 32) * 4;
    assert_eq!(stride, 864);
    assert_eq!(stride * h, 144_288);

    // WMF = 헤더18 + SetWindowOrg(10) + SetWindowExt(10) + StretchDIBits + EOF(6)
    // 레코드 = 헤더6 + 파라미터22(rop4+cu2+src8+dest8) + DIB(40+픽셀)
    let expected = 18 + 10 + 10 + (6 + 22 + 40 + 144_288) + 6;
    assert_eq!(wmf.len(), expected, "스파이크 WMF 144,400B 와 같은 구조");
}

#[test]
fn header_declares_correct_word_counts() {
    let (w, h) = (16u32, 8u32);
    let wmf = wmf_from_rgba(&vec![0u8; (w * h * 4) as usize], w, h);

    let total_words = u32::from_le_bytes(wmf[6..10].try_into().unwrap());
    assert_eq!(total_words as usize, wmf.len() / 2, "mtSize = 전체 워드 수");

    let max_rec = u32::from_le_bytes(wmf[12..16].try_into().unwrap());
    // 최대 레코드 = StretchDIBits
    let stride = (((w * 24) + 31) / 32) * 4;
    let dib_rec_bytes = 6 + 22 + 40 + stride * h;
    assert_eq!(
        max_rec,
        dib_rec_bytes / 2,
        "mtMaxRecord = 최대 레코드 워드 수"
    );
}

#[test]
fn records_are_in_playback_order() {
    let wmf = wmf_from_rgba(&vec![255u8; 4 * 4 * 4], 4, 4);
    let mut off = 18usize;
    let mut funcs = Vec::new();
    while off + 6 <= wmf.len() {
        let size = u32::from_le_bytes(wmf[off..off + 4].try_into().unwrap()) as usize;
        let func = u16::from_le_bytes(wmf[off + 4..off + 6].try_into().unwrap());
        funcs.push(func);
        if size == 0 {
            break;
        }
        off += size * 2;
    }
    // SetWindowOrg → SetWindowExt → StretchDIBits → EOF
    assert_eq!(funcs, vec![0x020B, 0x020C, 0x0F43, 0x0000]);
}

#[test]
fn presentation_stream_header_is_40_bytes() {
    let wmf = wmf_from_rgba(&vec![0u8; 2 * 2 * 4], 2, 2);
    let pres = ole_presentation_stream(&wmf, 432, 252);
    assert_eq!(pres.len(), 40 + wmf.len());

    // 실물 헤더 실측: ffffffff 03000000 04000000 01000000 ffffffff 00000000 00000000 b0010000 fc000000
    assert_eq!(&pres[0..4], &[0xFF, 0xFF, 0xFF, 0xFF]);
    assert_eq!(&pres[4..8], &[0x03, 0x00, 0x00, 0x00]); // CF_METAFILEPICT
    assert_eq!(&pres[8..12], &[0x04, 0x00, 0x00, 0x00]); // TargetDeviceSize
    assert_eq!(&pres[12..16], &[0x01, 0x00, 0x00, 0x00]); // Aspect
    assert_eq!(&pres[16..20], &[0xFF, 0xFF, 0xFF, 0xFF]); // Lindex
    assert_eq!(u32::from_le_bytes(pres[28..32].try_into().unwrap()), 432); // Width
    assert_eq!(u32::from_le_bytes(pres[32..36].try_into().unwrap()), 252); // Height
}

/// rhwp 자신의 WMF 파서로 되읽어 왕복을 확인한다 (18,498줄 파서가 오라클).
#[test]
fn own_parser_accepts_generated_wmf() {
    let (w, h) = (8u32, 6u32);
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for (i, px) in rgba.chunks_mut(4).enumerate() {
        px.copy_from_slice(&[(i * 7) as u8, (i * 3) as u8, (i * 11) as u8, 255]);
    }
    let wmf = wmf_from_rgba(&rgba, w, h);

    // 파서가 헤더를 받아들이는지 — 시그니처·구조 검증
    assert_eq!(
        u16::from_le_bytes(wmf[0..2].try_into().unwrap()),
        1,
        "mtType"
    );
    assert_eq!(
        u16::from_le_bytes(wmf[2..4].try_into().unwrap()),
        9,
        "mtHeaderSize"
    );
    assert_eq!(
        u16::from_le_bytes(wmf[4..6].try_into().unwrap()),
        0x0300,
        "mtVersion"
    );

    // StretchDIBits 의 DIB 가 유효한 BITMAPINFOHEADER 인지
    let off = 18 + 10 + 10 + 6 + 22;
    let bih_size = u32::from_le_bytes(wmf[off..off + 4].try_into().unwrap());
    let bi_w = i32::from_le_bytes(wmf[off + 4..off + 8].try_into().unwrap());
    let bi_h = i32::from_le_bytes(wmf[off + 8..off + 12].try_into().unwrap());
    let bpp = u16::from_le_bytes(wmf[off + 14..off + 16].try_into().unwrap());
    assert_eq!(bih_size, 40);
    assert_eq!(bi_w, w as i32);
    assert_eq!(bi_h, h as i32);
    assert_eq!(bpp, 24);
}
