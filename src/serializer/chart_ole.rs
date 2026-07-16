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
use super::mini_cfb::build_cfb_with_clsid;
use super::wmf_writer::{ole_presentation_stream, wmf_from_rgba};

/// 한컴 차트 OLE 개체의 클래스 ID — `{4C3DA137-DC90-47B9-9BED-59DAE352A280}`.
///
/// 실물 `samples/chart/**` 의 내부 CFB Root Entry 에서 실측했다. 이 값이 없으면
/// 한컴은 개체를 차트로 인식하지 못하고 미리보기만 그린다 (편집 불가).
/// GUID 바이트 배치: Data1(u32 LE) Data2(u16 LE) Data3(u16 LE) Data4(8B BE).
const HANCOM_CHART_CLSID: [u8; 16] = [
    0x37, 0xA1, 0x3D, 0x4C, // Data1 = 0x4C3DA137 (LE)
    0x90, 0xDC, // Data2 = 0xDC90 (LE)
    0xB9, 0x47, // Data3 = 0x47B9 (LE)
    0x9B, 0xED, 0x59, 0xDA, 0xE3, 0x52, 0xA2, 0x80, // Data4 (BE)
];

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
    // Root CLSID 를 반드시 넣는다 — 없으면 한컴이 개체 종류를 몰라 미리보기
    // 그림만 표시하고 차트로 편집되지 않는다 (실측 확인).
    let cfb = build_cfb_with_clsid(
        &[("\u{2}OlePres000", &pres), ("OOXMLChartContents", &xml)],
        HANCOM_CHART_CLSID,
    )?;

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
    fn root_clsid_matches_hancom_chart() {
        let ole = build_chart_ole(&spec(), &rgba(16, 16), 16, 16, 20000, 12000).unwrap();
        // CFB 헤더(512B) 뒤 디렉터리 섹터의 첫 엔트리(Root) 오프셋 80..96 이 CLSID.
        let sect_shift = u16::from_le_bytes(ole.cfb[30..32].try_into().unwrap());
        let sect_size = 1usize << sect_shift;
        let dir_start = u32::from_le_bytes(ole.cfb[48..52].try_into().unwrap()) as usize;
        let dir_off = 512 + dir_start * sect_size;
        assert_eq!(
            &ole.cfb[dir_off + 80..dir_off + 96],
            &HANCOM_CHART_CLSID,
            "Root CLSID 가 없으면 한컴이 차트로 인식하지 못한다"
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
