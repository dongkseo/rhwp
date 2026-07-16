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
//! **여기서는 생 CFB 만 만든다.** deflate 압축과 4바이트 길이 프리픽스는
//! `cfb_writer` 가 붙인다 (`BinDataType::Storage` + CFB 매직으로 시작하는 데이터를
//! OLE Storage 로 인식해 프리픽스를 복원한다 — 파서가 `drain(..4)` 로 떼어낸 것을
//! 되돌리는 계약). 여기서 미리 압축하면 매직이 가려져 그 인식이 실패한다.
//!
//! `Contents`(레거시) 는 넣지 않는다. 뷰어 표시에는 불필요함을 확인했으나,
//! 한컴에서 차트로 편집 가능한지는 미검증이다 — 그림처럼 박힐 수 있다.

use super::chart_xml::{build_chart_xml, ChartSpec};
use super::mini_cfb::build_cfb;
use super::wmf_writer::{ole_presentation_stream, wmf_from_rgba};

/// 조립된 차트 OLE.
#[derive(Debug)]
pub struct ChartOle {
    /// `BinDataContent.data` 에 넣을 **생 CFB** (압축·프리픽스 없음 — cfb_writer 가 붙인다)
    pub cfb: Vec<u8>,
}

/// 차트 사양 + 미리보기 픽셀 → 생 CFB (`BinDataContent.data` 용).
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

    Ok(ChartOle { cfb })
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
    fn produces_bare_cfb_for_serializer_contract() {
        let ole = build_chart_ole(&spec(), &rgba(32, 24), 32, 24, 20000, 12000).unwrap();
        // cfb_writer 는 CFB 매직으로 시작하는 데이터만 OLE Storage 로 인식한다.
        assert_eq!(
            &ole.cfb[..8],
            &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
            "생 CFB 로 시작해야 직렬화기가 OLE 로 인식한다 (압축·프리픽스 금지)"
        );
    }

    #[test]
    fn cfb_carries_both_streams() {
        let ole = build_chart_ole(&spec(), &rgba(16, 16), 16, 16, 20000, 12000).unwrap();
        let s = String::from_utf8_lossy(&ole.cfb);
        assert!(s.contains("c:chartSpace"), "XML 이 들어있어야 한다");
        assert!(s.contains("1분기"), "카테고리");
        assert!(s.contains("매출액"), "계열명");
    }

    #[test]
    fn rejects_short_pixel_buffer() {
        let err = build_chart_ole(&spec(), &[0u8; 10], 32, 24, 20000, 12000).unwrap_err();
        assert!(err.contains("픽셀 부족"), "{}", err);
    }
}
