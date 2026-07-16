//! 차트 미리보기 자동 렌더 (네이티브 전용).
//!
//! 한컴·뷰어는 OLE 안의 `\x02OlePres000` 미리보기만 그리므로, 차트를 새로
//! 만들려면 그림을 함께 넣어야 한다. 여기서는 rhwp 자신의 차트 렌더러로 SVG 를
//! 만들고 resvg 로 래스터화한다 — rhwp 화면과 한컴 화면이 같은 소스에서 나온다.
//!
//! `resvg` 는 `native-skia` feature + 네이티브 전용이라 WASM 에서는 쓸 수 없다
//! (`export-png` 도 같은 제약을 오류 메시지로 안내한다). WASM 호출자는 직접
//! 래스터화해 `insert_chart_native` 에 `rgba` 를 넘기면 된다.

use resvg::{tiny_skia, usvg};

use super::chart_xml::{build_chart_xml, ChartSpec};
use crate::ooxml_chart::parser::parse_chart_xml;
use crate::ooxml_chart::renderer::render_chart_svg;

/// 래스터화된 미리보기.
pub struct ChartPreview {
    /// RGBA 픽셀 (위→아래, 4바이트/픽셀)
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// 차트 사양을 미리보기 이미지로 렌더한다.
///
/// `px_width`/`px_height`: 래스터 크기. OLE 개체 크기와 비례하게 주면 된다.
pub fn render_chart_preview(
    spec: &ChartSpec,
    px_width: u32,
    px_height: u32,
) -> Result<ChartPreview, String> {
    if px_width == 0 || px_height == 0 {
        return Err("미리보기 크기가 0 이다".to_string());
    }

    // 1. 사양 → XML → rhwp 차트 IR (직렬화될 XML 과 같은 것을 쓴다)
    let xml = build_chart_xml(spec);
    let chart = parse_chart_xml(&xml).ok_or_else(|| "합성한 차트 XML 파싱 실패".to_string())?;

    // 2. rhwp 렌더러로 SVG 조각 → 독립 SVG 문서로 감싼다
    let body = render_chart_svg(&chart, 0.0, 0.0, px_width as f64, px_height as f64);
    let mut svg = String::with_capacity(body.len() + 256);
    svg.push_str("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
    svg.push_str(&px_width.to_string());
    svg.push_str("\" height=\"");
    svg.push_str(&px_height.to_string());
    svg.push_str("\" viewBox=\"0 0 ");
    svg.push_str(&px_width.to_string());
    svg.push(' ');
    svg.push_str(&px_height.to_string());
    svg.push_str("\"><rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>");
    svg.push_str(&body);
    svg.push_str("</svg>");

    // 3. 래스터화
    let mut options = usvg::Options::default();
    options.fontdb_mut().load_system_fonts();
    let tree = usvg::Tree::from_str(&svg, &options).map_err(|e| format!("SVG 파싱 실패: {}", e))?;
    let mut pixmap = tiny_skia::Pixmap::new(px_width, px_height)
        .ok_or_else(|| "픽스맵 생성 실패".to_string())?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pixmap.as_mut());

    Ok(ChartPreview {
        rgba: pixmap.take(),
        width: px_width,
        height: px_height,
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
            categories: vec![
                "1분기".into(),
                "2분기".into(),
                "3분기".into(),
                "4분기".into(),
            ],
            series: vec![ChartSeries {
                name: "매출액".into(),
                values: vec![184.0, 210.0, 175.0, 243.0],
            }],
        }
    }

    #[test]
    fn renders_non_blank_pixels() {
        let p = render_chart_preview(&spec(), 432, 252).unwrap();
        assert_eq!(p.rgba.len(), 432 * 252 * 4);

        // 흰 배경 위에 뭔가 그려졌는가 — 전부 흰색이면 렌더 실패다
        let non_white = p
            .rgba
            .chunks(4)
            .filter(|px| px[0] < 250 || px[1] < 250 || px[2] < 250)
            .count();
        assert!(
            non_white > 500,
            "차트가 그려져야 한다 (비흰색 픽셀 {}개)",
            non_white
        );
    }

    #[test]
    fn rejects_zero_size() {
        assert!(render_chart_preview(&spec(), 0, 100).is_err());
    }
}
