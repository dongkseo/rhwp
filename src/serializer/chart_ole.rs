//! 차트 OLE blob 조립 — XML + 미리보기를 `BinData/BINxxxx.OLE` 바이트로 만든다.
//!
//! HWP 차트의 저장 구조 (실측: `samples/chart/**`):
//! ```text
//! BinData/BIN0001.OLE = deflate( [4B 길이] + CFB{
//!     \x02OlePres000       ← 화면에 그려지는 것 (CF_METAFILEPICT = WMF)
//!     OOXMLChartContents   ← rhwp 가 읽는 것 (생 UTF-8 DrawingML XML)
//!     Contents             ← 레거시 바이너리 (여기서는 생략)
//! })
//! ```
//!
//! 한컴·뷰어는 `\x02OlePres000` 만 그린다 (대조 실험: 미리보기를 지우면 아무것도
//! 안 보이고, XML 을 바꿔도 화면은 그대로). rhwp 는 반대로 XML 만 본다. 따라서
//! 둘 다 같은 데이터에서 만들어야 화면이 일치한다.
//!
//! `Contents`(레거시) 는 넣지 않는다. 뷰어 표시에는 불필요함을 확인했으나,
//! 한컴에서 차트로 편집 가능한지는 미검증이다 — 그림처럼 박힐 수 있다.

use super::chart_xml::{build_chart_xml, ChartSpec};
use super::mini_cfb::build_cfb;
use super::wmf_writer::{ole_presentation_stream, wmf_from_rgba};

/// 조립된 차트 OLE.
#[derive(Debug)]
pub struct ChartOle {
    /// `BinData/BINxxxx.OLE` 에 넣을 최종 바이트 (deflate 압축됨)
    pub bin_data: Vec<u8>,
    /// 압축 전 크기 (디버깅용)
    pub raw_len: usize,
}

/// raw deflate (wbits=-15) — HWP BinData 규약.
fn deflate(data: &[u8]) -> Result<Vec<u8>, String> {
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    let mut enc = DeflateEncoder::new(Vec::new(), Compression::best());
    enc.write_all(data).map_err(|e| e.to_string())?;
    enc.finish().map_err(|e| e.to_string())
}

/// 차트 사양 + 미리보기 픽셀 → BinData OLE 바이트.
///
/// `rgba`: 미리보기 이미지 (위→아래 행 순서, 4바이트/픽셀).
/// `extent_hu`: OLE 개체 영역 (HWPUNIT). 미리보기 스트림 헤더에 기록된다.
pub fn build_chart_ole(
    spec: &ChartSpec,
    rgba: &[u8],
    px_width: u32,
    px_height: u32,
    extent_x_hu: u32,
    extent_y_hu: u32,
) -> Result<ChartOle, String> {
    if rgba.len() < (px_width as usize * px_height as usize * 4) {
        return Err(format!(
            "미리보기 픽셀 부족: {}B < {}x{}x4",
            rgba.len(),
            px_width,
            px_height
        ));
    }

    let xml = build_chart_xml(spec);
    let wmf = wmf_from_rgba(rgba, px_width, px_height);
    let pres = ole_presentation_stream(&wmf, extent_x_hu, extent_y_hu);

    // 스트림 순서는 실물(묶은세로막대형.hwp)과 같게 둔다.
    let cfb = build_cfb(&[("\u{2}OlePres000", &pres), ("OOXMLChartContents", &xml)])?;

    // 실물 규약: CFB 앞에 4바이트 길이 프리픽스가 붙는다 (실측 prefix=002e0400).
    let mut raw = Vec::with_capacity(4 + cfb.len());
    raw.extend_from_slice(&(cfb.len() as u32).to_le_bytes());
    raw.extend_from_slice(&cfb);
    let raw_len = raw.len();

    Ok(ChartOle {
        bin_data: deflate(&raw)?,
        raw_len,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ooxml_chart::OoxmlChartType;
    use crate::serializer::chart_xml::ChartSeries;

    fn spec() -> ChartSpec {
        ChartSpec {
            chart_type: OoxmlChartType::Column,
            title: Some("분기 실적".into()),
            categories: vec!["1분기".into(), "2분기".into()],
            series: vec![ChartSeries {
                name: "매출액".into(),
                values: vec![184.0, 210.0],
            }],
        }
    }

    fn rgba(w: u32, h: u32) -> Vec<u8> {
        vec![200u8; (w * h * 4) as usize]
    }

    #[test]
    fn assembles_and_deflates() {
        let ole = build_chart_ole(&spec(), &rgba(32, 24), 32, 24, 20000, 12000).unwrap();
        assert!(!ole.bin_data.is_empty());
        assert!(
            ole.bin_data.len() < ole.raw_len,
            "압축이 되어야 한다: {} → {}",
            ole.raw_len,
            ole.bin_data.len()
        );
    }

    /// 실물과 같은 방식으로 되읽을 수 있는가: inflate → 4B 건너뛰기 → CFB 시그니처.
    #[test]
    fn inflates_back_to_cfb_with_length_prefix() {
        let ole = build_chart_ole(&spec(), &rgba(16, 16), 16, 16, 20000, 12000).unwrap();

        let mut dec = flate2::read::DeflateDecoder::new(&ole.bin_data[..]);
        let mut raw = Vec::new();
        std::io::Read::read_to_end(&mut dec, &mut raw).expect("raw deflate 로 풀려야 한다");
        assert_eq!(raw.len(), ole.raw_len);

        let declared = u32::from_le_bytes(raw[..4].try_into().unwrap()) as usize;
        assert_eq!(
            declared,
            raw.len() - 4,
            "길이 프리픽스가 CFB 크기와 맞아야 한다"
        );
        assert_eq!(
            &raw[4..12],
            &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
            "프리픽스 뒤는 CFB 시그니처"
        );
    }

    #[test]
    fn rejects_short_pixel_buffer() {
        let err = build_chart_ole(&spec(), &[0u8; 10], 32, 24, 20000, 12000).unwrap_err();
        assert!(err.contains("픽셀 부족"), "{}", err);
    }
}
