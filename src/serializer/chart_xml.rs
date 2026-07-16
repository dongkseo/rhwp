//! 차트 데이터 → `OOXMLChartContents` (DrawingML `c:chartSpace`) XML 합성.
//!
//! HWP 차트 OLE 안의 `OOXMLChartContents` 스트림은 생 UTF-8 XML 이다
//! (ZIP 이 아니다 — `samples/chart/**` 실측). rhwp 는 이 XML 을 파싱해 렌더하므로,
//! 여기서 만든 XML 은 `ooxml_chart::parser::parse_chart_xml` 로 되읽어 검증할 수 있다.
//!
//! 배치 근거: `samples/chart/세로막대형/묶은세로막대형.hwp` 의 실물 XML 구조.
//! 한컴이 쓰는 요소 순서를 그대로 따른다.

use crate::ooxml_chart::OoxmlChartType;

/// 차트 한 계열.
#[derive(Debug, Clone)]
pub struct ChartSeries {
    /// 계열 이름 (범례에 표시)
    pub name: String,
    /// 값 목록. 카테고리 수와 같아야 한다.
    pub values: Vec<f64>,
}

/// 차트 생성 입력.
#[derive(Debug, Clone)]
pub struct ChartSpec {
    pub chart_type: OoxmlChartType,
    /// 제목. `None` 이면 한컴 기본 자동 제목("차트 제목")이 렌더된다.
    pub title: Option<String>,
    pub categories: Vec<String>,
    pub series: Vec<ChartSeries>,
}

/// XML 텍스트 이스케이프. 한컴 XML 은 속성에 작은따옴표를 쓰지 않으므로
/// `&`, `<`, `>` 만 처리해도 충분하나, 안전을 위해 따옴표도 함께 막는다.
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// `c:barChart` / `c:lineChart` / `c:pieChart` 중 무엇을 쓸지와 `c:barDir`.
fn plot_element(t: OoxmlChartType) -> (&'static str, Option<&'static str>) {
    match t {
        OoxmlChartType::Column => ("barChart", Some("col")),
        OoxmlChartType::Bar => ("barChart", Some("bar")),
        OoxmlChartType::Line => ("lineChart", None),
        OoxmlChartType::Pie => ("pieChart", None),
        // Scatter/Unknown 은 별도 스키마(xVal/yVal)라 여기서 다루지 않는다.
        _ => ("barChart", Some("col")),
    }
}

/// 문자열 캐시 (`c:cat` 의 카테고리, `c:tx` 의 계열명).
fn str_cache(f_ref: &str, values: &[String]) -> String {
    let mut s = format!(
        "<c:strRef><c:f>{}</c:f><c:strCache><c:ptCount val=\"{}\"/>",
        esc(f_ref),
        values.len()
    );
    for (i, v) in values.iter().enumerate() {
        s.push_str(&format!("<c:pt idx=\"{}\"><c:v>{}</c:v></c:pt>", i, esc(v)));
    }
    s.push_str("</c:strCache></c:strRef>");
    s
}

/// 숫자 캐시 (`c:val`).
fn num_cache(f_ref: &str, values: &[f64]) -> String {
    let mut s = format!(
        "<c:numRef><c:f>{}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val=\"{}\"/>",
        esc(f_ref),
        values.len()
    );
    for (i, v) in values.iter().enumerate() {
        s.push_str(&format!("<c:pt idx=\"{}\"><c:v>{}</c:v></c:pt>", i, v));
    }
    s.push_str("</c:numCache></c:numRef>");
    s
}

/// 엑셀 열 문자 (0=A, 1=B, …). 계열이 26개를 넘으면 AA, AB… 로 간다.
fn col_letter(mut i: usize) -> String {
    let mut out = Vec::new();
    loop {
        out.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_else(|_| "A".to_string())
}

/// `ChartSpec` → `OOXMLChartContents` XML 바이트.
pub fn build_chart_xml(spec: &ChartSpec) -> Vec<u8> {
    let (plot, bar_dir) = plot_element(spec.chart_type);
    let n_cat = spec.categories.len();

    let mut s = String::with_capacity(4096);
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes" ?>"#);
    s.push_str(
        r#"<c:chartSpace xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart">"#,
    );
    s.push_str(r#"<c:date1904 val="0"/><c:roundedCorners val="0"/><c:chart>"#);

    // 제목: 텍스트가 있으면 명시, 없으면 빈 c:title (한컴 자동 제목 규약)
    match &spec.title {
        Some(t) => s.push_str(&format!(
            r#"<c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>{}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title>"#,
            esc(t)
        )),
        None => s.push_str(r#"<c:title><c:layout/><c:overlay val="0"/></c:title>"#),
    }
    s.push_str(r#"<c:autoTitleDeleted val="0"/><c:plotArea><c:layout/>"#);

    s.push_str(&format!("<c:{}>", plot));
    if let Some(d) = bar_dir {
        s.push_str(&format!(r#"<c:barDir val="{}"/>"#, d));
    }
    if matches!(
        spec.chart_type,
        OoxmlChartType::Column | OoxmlChartType::Bar
    ) {
        s.push_str(r#"<c:grouping val="clustered"/>"#);
    }
    s.push_str(r#"<c:varyColors val="0"/>"#);

    for (i, ser) in spec.series.iter().enumerate() {
        // 실물 규약: 계열명은 Sheet1!$B$1, $C$1 …, 카테고리는 $A$2:$A$n+1, 값은 $B$2:$B$n+1
        let col = col_letter(i + 1);
        s.push_str(&format!(
            r#"<c:ser><c:idx val="{}"/><c:order val="{}"/>"#,
            i, i
        ));
        s.push_str(&format!(
            "<c:tx>{}</c:tx>",
            str_cache(
                &format!("Sheet1!${}$1", col),
                std::slice::from_ref(&ser.name)
            )
        ));
        if matches!(
            spec.chart_type,
            OoxmlChartType::Column | OoxmlChartType::Bar
        ) {
            s.push_str(r#"<c:invertIfNegative val="0"/>"#);
        }
        s.push_str(&format!(
            "<c:cat>{}</c:cat>",
            str_cache(&format!("Sheet1!$A$2:$A${}", n_cat + 1), &spec.categories)
        ));
        s.push_str(&format!(
            "<c:val>{}</c:val>",
            num_cache(
                &format!("Sheet1!${}$2:${}${}", col, col, n_cat + 1),
                &ser.values
            )
        ));
        s.push_str("</c:ser>");
    }

    if matches!(
        spec.chart_type,
        OoxmlChartType::Column | OoxmlChartType::Bar
    ) {
        s.push_str(r#"<c:gapWidth val="150"/><c:axId val="1"/><c:axId val="2"/>"#);
    }
    s.push_str(&format!("</c:{}>", plot));

    // 원형은 축이 없다
    if !matches!(spec.chart_type, OoxmlChartType::Pie) {
        s.push_str(
            r#"<c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="2"/></c:catAx>"#,
        );
        s.push_str(
            r#"<c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="1"/></c:valAx>"#,
        );
    }
    s.push_str(r#"</c:plotArea><c:legend><c:legendPos val="r"/><c:overlay val="0"/></c:legend>"#);
    s.push_str(r#"<c:plotVisOnly val="1"/></c:chart></c:chartSpace>"#);
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ooxml_chart::parser::parse_chart_xml;

    fn sample() -> ChartSpec {
        ChartSpec {
            chart_type: OoxmlChartType::Column,
            title: Some("분기 실적".to_string()),
            categories: vec![
                "1분기".into(),
                "2분기".into(),
                "3분기".into(),
                "4분기".into(),
            ],
            series: vec![
                ChartSeries {
                    name: "매출액".into(),
                    values: vec![184.0, 210.0, 175.0, 243.0],
                },
                ChartSeries {
                    name: "영업이익".into(),
                    values: vec![21.0, 33.0, 18.0, 41.0],
                },
            ],
        }
    }

    /// 합성한 XML 을 rhwp 자신의 파서로 되읽어 데이터가 살아있는지 (왕복 검증).
    #[test]
    fn roundtrips_through_own_parser() {
        let xml = build_chart_xml(&sample());
        let chart = parse_chart_xml(&xml).expect("파서가 합성 XML 을 받아들여야 한다");

        assert_eq!(chart.chart_type, OoxmlChartType::Column);
        assert_eq!(chart.title.as_deref(), Some("분기 실적"));
        assert_eq!(chart.categories, vec!["1분기", "2분기", "3분기", "4분기"]);
        assert_eq!(chart.series.len(), 2);
        assert_eq!(chart.series[0].name, "매출액");
        assert_eq!(chart.series[0].values, vec![184.0, 210.0, 175.0, 243.0]);
        assert_eq!(chart.series[1].name, "영업이익");
        assert_eq!(chart.series[1].values, vec![21.0, 33.0, 18.0, 41.0]);
    }

    #[test]
    fn renders_without_fallback() {
        use crate::ooxml_chart::renderer::render_chart_svg;
        let xml = build_chart_xml(&sample());
        let chart = parse_chart_xml(&xml).unwrap();
        let svg = render_chart_svg(&chart, 0.0, 0.0, 430.0, 250.0);
        assert!(
            !svg.contains("미지원"),
            "fallback 이 아니라 실제 차트가 그려져야 한다"
        );
        assert!(
            svg.contains("1분기"),
            "카테고리가 렌더돼야 한다:\n{}",
            &svg[..svg.len().min(400)]
        );
        assert!(svg.contains("매출액"), "계열명이 렌더돼야 한다");
    }

    #[test]
    fn bar_and_column_differ_in_bar_dir() {
        let mut spec = sample();
        spec.chart_type = OoxmlChartType::Bar;
        let xml = String::from_utf8(build_chart_xml(&spec)).unwrap();
        assert!(xml.contains(r#"<c:barDir val="bar"/>"#));

        spec.chart_type = OoxmlChartType::Column;
        let xml = String::from_utf8(build_chart_xml(&spec)).unwrap();
        assert!(xml.contains(r#"<c:barDir val="col"/>"#));
    }

    #[test]
    fn pie_omits_axes() {
        let mut spec = sample();
        spec.chart_type = OoxmlChartType::Pie;
        let xml = String::from_utf8(build_chart_xml(&spec)).unwrap();
        assert!(xml.contains("<c:pieChart>"));
        assert!(!xml.contains("<c:catAx>"), "원형은 축이 없다");
    }

    #[test]
    fn escapes_xml_metacharacters() {
        let spec = ChartSpec {
            chart_type: OoxmlChartType::Column,
            title: Some("A & B <test>".to_string()),
            categories: vec!["<1분기>".into()],
            series: vec![ChartSeries {
                name: "R&D".into(),
                values: vec![1.0],
            }],
        };
        let xml = String::from_utf8(build_chart_xml(&spec)).unwrap();

        // writer 는 올바른 XML 을 낸다
        assert!(xml.contains("A &amp; B &lt;test&gt;"));
        assert!(xml.contains("R&amp;D"));
        assert!(
            !xml.contains("<test>"),
            "이스케이프되지 않은 메타문자가 남으면 XML 이 깨진다"
        );
        // XML 로서 파싱은 된다 (깨진 문서가 아니다)
        assert!(parse_chart_xml(&build_chart_xml(&spec)).is_some());
    }

    /// 기존 파서 결함 기록: `ooxml_chart::parser` 가 `Event::Text` 에서 `decode()` 만
    /// 하고 `unescape()` 를 하지 않아 (`parser.rs:74`), XML 엔티티가 통째로 유실된다.
    /// `R&amp;D` → `RD`. writer 는 정상이므로 여기서는 현상만 고정해 둔다.
    /// 파서를 고치면 이 테스트가 실패하며, 그때 기대값을 `R&D` 로 바꾸면 된다.
    #[test]
    fn parser_drops_xml_entities_known_defect() {
        let spec = ChartSpec {
            chart_type: OoxmlChartType::Column,
            title: None,
            categories: vec!["1분기".into()],
            series: vec![ChartSeries {
                name: "R&D".into(),
                values: vec![1.0],
            }],
        };
        let chart = parse_chart_xml(&build_chart_xml(&spec)).unwrap();
        assert_eq!(
            chart.series[0].name, "RD",
            "파서가 &amp; 를 삼킨다 — 고쳐지면 R&D 가 되어야 한다"
        );
    }

    #[test]
    fn column_letters_go_past_z() {
        assert_eq!(col_letter(0), "A");
        assert_eq!(col_letter(25), "Z");
        assert_eq!(col_letter(26), "AA");
    }
}
