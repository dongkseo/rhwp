//! 한컴 판정용 차트 문서 생성.
//!
//! 실행: cargo run --release --features native-skia --example make_chart_docs

use rhwp::document_core::DocumentCore;
use rhwp::ooxml_chart::OoxmlChartType;
use rhwp::serializer::chart_preview::render_chart_preview;
use rhwp::serializer::chart_xml::{ChartSeries, ChartSpec};

const BLANK: &[u8] = include_bytes!("../saved/blank2010.hwp");
const MM: f64 = 7200.0 / 25.4;

fn make(name: &str, spec: ChartSpec) -> Result<(), String> {
    // OLE 개체 크기: 150mm x 90mm
    let w_hu = (150.0 * MM) as u32;
    let h_hu = (90.0 * MM) as u32;
    // 래스터: 개체 비율 그대로, 96dpi 기준
    let px_w = (w_hu as f64 / 7200.0 * 96.0) as u32;
    let px_h = (h_hu as f64 / 7200.0 * 96.0) as u32;

    let preview = render_chart_preview(&spec, px_w, px_h)?;

    let mut core = DocumentCore::from_bytes(BLANK).map_err(|e| e.to_string())?;
    // 제목 한 줄
    core.insert_text_native(0, 0, 0, &format!("{} 차트 시험", name))
        .map_err(|e| e.to_string())?;

    let ret = core
        .insert_chart_native(
            0,
            0,
            &spec,
            &preview.rgba,
            preview.width,
            preview.height,
            w_hu,
            h_hu,
            Some((30.0 * MM) as i32),
            Some((60.0 * MM) as i32),
        )
        .map_err(|e| e.to_string())?;

    let bytes = core.export_hwp_native().map_err(|e| e.to_string())?;
    let path = format!("output/chart_{}.hwp", name);
    std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    println!(
        "  {:<10} {:>7}B  래스터 {}x{}  {}",
        path,
        bytes.len(),
        preview.width,
        preview.height,
        ret
    );
    Ok(())
}

fn main() {
    std::fs::create_dir_all("output").ok();
    let cats = vec![
        "1분기".to_string(),
        "2분기".into(),
        "3분기".into(),
        "4분기".into(),
    ];

    let cases: Vec<(&str, OoxmlChartType, Vec<ChartSeries>)> = vec![
        (
            "column",
            OoxmlChartType::Column,
            vec![
                ChartSeries {
                    name: "매출액".into(),
                    values: vec![184.0, 210.0, 175.0, 243.0],
                },
                ChartSeries {
                    name: "영업이익".into(),
                    values: vec![21.0, 33.0, 18.0, 41.0],
                },
            ],
        ),
        (
            "bar",
            OoxmlChartType::Bar,
            vec![ChartSeries {
                name: "매출액".into(),
                values: vec![184.0, 210.0, 175.0, 243.0],
            }],
        ),
        (
            "line",
            OoxmlChartType::Line,
            vec![
                ChartSeries {
                    name: "매출액".into(),
                    values: vec![184.0, 210.0, 175.0, 243.0],
                },
                ChartSeries {
                    name: "영업이익".into(),
                    values: vec![21.0, 33.0, 18.0, 41.0],
                },
            ],
        ),
        (
            "pie",
            OoxmlChartType::Pie,
            vec![ChartSeries {
                name: "매출 비중".into(),
                values: vec![184.0, 210.0, 175.0, 243.0],
            }],
        ),
    ];

    println!("한컴 판정용 차트 문서 생성:");
    for (name, ct, series) in cases {
        let spec = ChartSpec {
            chart_type: ct,
            title: Some(format!("2026 {} 실적", name)),
            categories: cats.clone(),
            series,
        };
        if let Err(e) = make(name, spec) {
            eprintln!("  {} 실패: {}", name, e);
        }
    }
}
