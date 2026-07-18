//! 차트 삽입 → 저장 → 재열기 왕복.
//!
//! 검증 축: rhwp 는 OOXMLChartContents XML 을 읽어 렌더하므로, 저장한 문서를
//! 다시 열었을 때 차트가 데이터와 함께 살아있어야 한다.
//! (한컴·뷰어가 보는 \x02OlePres000 미리보기는 별도 축 — 사람이 확인해야 한다.)

use rhwp::document_core::DocumentCore;
use rhwp::ooxml_chart::OoxmlChartType;
use rhwp::parser::parse_hwp;
use rhwp::serializer::chart_xml::{ChartSeries, ChartSpec};

const BLANK: &[u8] = include_bytes!("../saved/blank2010.hwp");

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

/// 미리보기 픽셀 (내용은 무관 — 구조만 본다)
fn rgba(w: u32, h: u32) -> Vec<u8> {
    vec![220u8; (w * h * 4) as usize]
}

fn insert_into_blank() -> Vec<u8> {
    let mut core = DocumentCore::from_bytes(BLANK).expect("blank2010 로드");
    let r = core
        .insert_chart_native(
            0,
            0,
            &spec(),
            &rgba(64, 40),
            64,
            40,
            20000,
            12000,
            Some(5000),
            Some(5000),
        )
        .expect("차트 삽입");
    assert!(r.contains("\"ok\":true"), "{}", r);
    core.export_hwp_native().expect("저장")
}

#[test]
fn inserted_chart_survives_save_and_reopen() {
    let bytes = insert_into_blank();

    // CFB 시그니처 — 진짜 HWP 컨테이너인가
    assert_eq!(
        &bytes[..8],
        &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]
    );

    let doc = parse_hwp(&bytes).expect("재열기");
    assert_eq!(
        doc.bin_data_content.len(),
        1,
        "BinData 가 하나 등록돼야 한다"
    );
    assert_eq!(
        doc.bin_data_content[0].extension, "OLE",
        "차트는 확장자 OLE (그림의 png 와 다르다)"
    );
    assert_eq!(doc.doc_info.bin_data_list.len(), 1);
    assert_eq!(
        doc.doc_info.bin_data_list[0].attr, 0x0002,
        "실물 차트와 같은 attr (Storage/Default/NotAccessed)"
    );
}

/// 저장한 문서의 BinData 를 뜯어 OOXMLChartContents 가 살아있는지.
#[test]
fn ole_blob_carries_chart_xml() {
    let bytes = insert_into_blank();
    let doc = parse_hwp(&bytes).expect("재열기");
    let blob = doc.bin_data_content[0].data.load();

    // 파서는 압축을 풀고 4B 프리픽스를 떼어낸 생 CFB 를 노출한다 (parser/mod.rs:669-675).
    // 실물 차트도 head=[d0,cf,11,e0,...] 로 같은 형태다.
    assert_eq!(
        &blob[..8],
        &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1],
        "파서가 노출하는 것은 생 CFB"
    );

    // XML 이 blob 안에 들어있는지 (CFB 파싱 없이 바이트 검색으로 충분)
    let s = String::from_utf8_lossy(&blob);
    assert!(s.contains("c:chartSpace"), "DrawingML 루트");
    assert!(s.contains("1분기"), "카테고리");
    assert!(s.contains("매출액"), "계열명");
    assert!(s.contains("<c:v>184</c:v>"), "값");
}

#[test]
fn rejects_mismatched_series_length() {
    let mut core = DocumentCore::from_bytes(BLANK).unwrap();
    let bad = ChartSpec {
        chart_type: OoxmlChartType::Column,
        title: None,
        categories: vec!["A".into(), "B".into()],
        series: vec![ChartSeries {
            name: "X".into(),
            values: vec![1.0],
        }], // 2 != 1
    };
    let err = core
        .insert_chart_native(0, 0, &bad, &rgba(8, 8), 8, 8, 20000, 12000, None, None)
        .unwrap_err();
    assert!(format!("{}", err).contains("카테고리 수"), "{}", err);
}

#[test]
fn rejects_empty_series() {
    let mut core = DocumentCore::from_bytes(BLANK).unwrap();
    let empty = ChartSpec {
        chart_type: OoxmlChartType::Column,
        title: None,
        categories: vec!["A".into()],
        series: vec![],
    };
    let err = core
        .insert_chart_native(0, 0, &empty, &rgba(8, 8), 8, 8, 20000, 12000, None, None)
        .unwrap_err();
    assert!(format!("{}", err).contains("계열이 비었다"), "{}", err);
}
