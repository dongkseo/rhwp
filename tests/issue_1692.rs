//! Issue #1692: HWP3 글자색 인덱스가 CharShape.text_color로 보존되는지 검증한다.

use rhwp::model::control::Control;
use rhwp::model::footnote::Endnote;
use rhwp::model::paragraph::Paragraph;
use rhwp::model::style::{Alignment, HeadType};
use rhwp::parser::ole_container::is_hmapsi_ole_container;
use rhwp::parser::parse_document;
use rhwp::wasm_api::HwpDocument;
use serde_json::Value;

fn load(path: &str) -> rhwp::model::document::Document {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    parse_document(&bytes).unwrap_or_else(|e| panic!("parse {path}: {e:?}"))
}

fn load_wasm_doc(path: &str) -> HwpDocument {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    HwpDocument::from_bytes(&bytes).unwrap_or_else(|e| panic!("parse wasm {path}: {e:?}"))
}

fn collect_endnotes<'a>(paragraphs: &'a [Paragraph], out: &mut Vec<&'a Endnote>) {
    for paragraph in paragraphs {
        collect_endnotes_in_controls(&paragraph.controls, out);
    }
}

fn collect_endnotes_in_controls<'a>(controls: &'a [Control], out: &mut Vec<&'a Endnote>) {
    for control in controls {
        match control {
            Control::Endnote(endnote) => {
                out.push(endnote);
                collect_endnotes(&endnote.paragraphs, out);
            }
            Control::Footnote(footnote) => {
                collect_endnotes(&footnote.paragraphs, out);
            }
            Control::Table(table) => {
                for cell in &table.cells {
                    collect_endnotes(&cell.paragraphs, out);
                }
                if let Some(caption) = &table.caption {
                    collect_endnotes(&caption.paragraphs, out);
                }
            }
            Control::Picture(picture) => {
                if let Some(caption) = &picture.caption {
                    collect_endnotes(&caption.paragraphs, out);
                }
            }
            Control::Shape(shape) => {
                if let Some(drawing) = shape.drawing() {
                    if let Some(caption) = &drawing.caption {
                        collect_endnotes(&caption.paragraphs, out);
                    }
                    if let Some(text_box) = &drawing.text_box {
                        collect_endnotes(&text_box.paragraphs, out);
                    }
                }
            }
            Control::Header(header) => {
                collect_endnotes(&header.paragraphs, out);
            }
            Control::Footer(footer) => {
                collect_endnotes(&footer.paragraphs, out);
            }
            _ => {}
        }
    }
}

fn first_header_paragraph<'a>(
    doc: &'a rhwp::model::document::Document,
    needle: &str,
) -> &'a Paragraph {
    doc.sections[0]
        .paragraphs
        .iter()
        .flat_map(|paragraph| paragraph.controls.iter())
        .find_map(|control| match control {
            Control::Header(header) => header
                .paragraphs
                .iter()
                .find(|paragraph| paragraph.text.contains(needle)),
            _ => None,
        })
        .unwrap_or_else(|| panic!("header paragraph containing {needle}"))
}

fn page_render_tree(doc: &HwpDocument, page: u32) -> Value {
    let json = doc
        .get_page_render_tree(page)
        .unwrap_or_else(|err| panic!("page render tree {page}: {err:?}"));
    serde_json::from_str(&json).unwrap_or_else(|err| panic!("parse render tree {page}: {err}"))
}

fn text_width_in_tree(node: &Value, ancestor_type: &str, text: &str) -> Option<f64> {
    fn walk(node: &Value, ancestor_type: &str, text: &str, in_ancestor: bool) -> Option<f64> {
        let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
        let now_in_ancestor = in_ancestor || node_type == ancestor_type;
        if now_in_ancestor
            && node_type == "TextRun"
            && node.get("text").and_then(Value::as_str) == Some(text)
        {
            return node
                .get("bbox")
                .and_then(|bbox| bbox.get("w"))
                .and_then(Value::as_f64);
        }

        node.get("children")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(|child| walk(child, ancestor_type, text, now_in_ancestor))
    }

    walk(node, ancestor_type, text, false)
}

fn text_concat_in_tree(node: &Value, ancestor_type: &str) -> String {
    fn walk(node: &Value, ancestor_type: &str, in_ancestor: bool, out: &mut String) {
        let node_type = node.get("type").and_then(Value::as_str).unwrap_or("");
        let now_in_ancestor = in_ancestor || node_type == ancestor_type;
        if now_in_ancestor && node_type == "TextRun" {
            if let Some(text) = node.get("text").and_then(Value::as_str) {
                out.push_str(text);
            }
        }

        if let Some(children) = node.get("children").and_then(Value::as_array) {
            for child in children {
                walk(child, ancestor_type, now_in_ancestor, out);
            }
        }
    }

    let mut out = String::new();
    walk(node, ancestor_type, false, &mut out);
    out
}

fn contains_node_type(node: &Value, node_type: &str) -> bool {
    if node.get("type").and_then(Value::as_str) == Some(node_type) {
        return true;
    }

    node.get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|child| contains_node_type(child, node_type))
}

#[test]
fn issue_1692_so_sueop_hwp3_preserves_blue_text_color_like_hwpx_reference() {
    let hwp3_doc = load("samples/SO-SUEOP.hwp");
    let hwpx_doc = load("samples/SO-SUEOP.hwpx");

    let blue = 0x00FF0000;
    let hwp3_blue_count = hwp3_doc
        .doc_info
        .char_shapes
        .iter()
        .filter(|cs| cs.text_color == blue)
        .count();
    let hwpx_blue_count = hwpx_doc
        .doc_info
        .char_shapes
        .iter()
        .filter(|cs| cs.text_color == blue)
        .count();

    assert!(
        hwp3_blue_count > 0,
        "SO-SUEOP.hwp must preserve HWP3 blue text_color into CharShape.text_color"
    );
    assert!(
        hwpx_blue_count > 0,
        "SO-SUEOP.hwpx reference must contain blue CharShape.text_color"
    );
    assert!(
        hwp3_doc
            .doc_info
            .char_shapes
            .iter()
            .any(|cs| cs.text_color == 0),
        "existing black text CharShape must remain available"
    );
}

#[test]
fn issue_1692_so_sueop_hwp3_line_box_reflects_para_margins() {
    let hwp3_doc = load("samples/SO-SUEOP.hwp");
    let section = &hwp3_doc.sections[0];

    let para_57 = &section.paragraphs[57];
    let ps_57 = &hwp3_doc.doc_info.para_shapes[para_57.para_shape_id as usize];
    assert_eq!(
        (ps_57.margin_left, ps_57.margin_right, ps_57.indent),
        (2000, 1000, 1000),
        "paragraph 57 ParaShape must use the common HWP5/HWPX IR scale"
    );

    let assert_line_box = |para_idx: usize, expected_start: i32, expected_width: i32| {
        let seg = section.paragraphs[para_idx]
            .line_segs
            .first()
            .unwrap_or_else(|| panic!("paragraph {para_idx} must have a first line segment"));
        assert_eq!(
            (seg.column_start, seg.segment_width),
            (expected_start, expected_width),
            "paragraph {para_idx} line box"
        );
    };

    assert_line_box(57, 1000, 41020);
    assert_line_box(77, 3000, 39520);
    assert_line_box(1000, 1000, 40520);
}

#[test]
fn issue_1692_so_sueop_hwp3_endnotes_follow_hwpx_numbering_and_width() {
    let hwp3_doc = load("samples/SO-SUEOP.hwp");
    let hwpx_doc = load("samples/SO-SUEOP.hwpx");

    let mut hwp3_endnotes = Vec::new();
    let mut hwpx_endnotes = Vec::new();
    collect_endnotes(&hwp3_doc.sections[0].paragraphs, &mut hwp3_endnotes);
    collect_endnotes(&hwpx_doc.sections[0].paragraphs, &mut hwpx_endnotes);

    assert_eq!(hwp3_endnotes.len(), hwpx_endnotes.len());
    assert_eq!(hwp3_endnotes.len(), 223);
    assert_eq!(hwp3_endnotes.first().unwrap().number, 1);
    assert_eq!(hwp3_endnotes.last().unwrap().number, 223);
    assert!(
        hwp3_endnotes
            .iter()
            .all(|endnote| endnote.after_decoration_letter == ')' as u16),
        "HWP3 endnote markers must use the same ')' suffix as the HWPX reference"
    );

    let hwp3_initial_column = hwp3_doc.sections[0].paragraphs[0]
        .controls
        .iter()
        .find_map(|control| match control {
            Control::ColumnDef(column_def) => Some(column_def),
            _ => None,
        })
        .expect("HWP3 section must restore the initial one-column body definition");
    assert_eq!(hwp3_initial_column.column_count, 1);

    let hwp3_first_seg = hwp3_endnotes[0].paragraphs[0]
        .line_segs
        .first()
        .expect("HWP3 first endnote paragraph line segment");
    let hwpx_first_seg = hwpx_endnotes[0].paragraphs[0]
        .line_segs
        .first()
        .expect("HWPX first endnote paragraph line segment");
    assert_eq!(
        hwp3_first_seg.segment_width, hwpx_first_seg.segment_width,
        "HWP3 endnote paragraph width must match the HWPX two-column note width"
    );

    let hwp3_shape = &hwp3_doc.sections[0].section_def.endnote_shape;
    let hwpx_shape = &hwpx_doc.sections[0].section_def.endnote_shape;
    assert_eq!(hwp3_shape.suffix_char, hwpx_shape.suffix_char);
    assert_eq!(
        hwp3_shape.separator_margin_top,
        hwpx_shape.separator_margin_top
    );
    assert_eq!(hwp3_shape.note_spacing, hwpx_shape.note_spacing);
    assert_eq!(
        hwp3_shape.separator_line_width,
        hwpx_shape.separator_line_width
    );

    let hwp3_answer = hwp3_doc.sections[0]
        .paragraphs
        .iter()
        .rev()
        .find(|paragraph| paragraph.text.contains("해답"))
        .expect("HWP3 answer heading paragraph");
    let hwpx_answer = hwpx_doc.sections[0]
        .paragraphs
        .iter()
        .rev()
        .find(|paragraph| paragraph.text.contains("해답"))
        .expect("HWPX answer heading paragraph");
    let hwp3_column = hwp3_answer
        .controls
        .iter()
        .find_map(|control| match control {
            Control::ColumnDef(column_def) => Some(column_def),
            _ => None,
        })
        .expect("HWP3 answer heading must restore the two-column note zone");
    let hwpx_column = hwpx_answer
        .controls
        .iter()
        .find_map(|control| match control {
            Control::ColumnDef(column_def) => Some(column_def),
            _ => None,
        })
        .expect("HWPX answer heading column definition");
    assert_eq!(hwp3_column.column_count, hwpx_column.column_count);
    assert_eq!(hwp3_column.spacing, hwpx_column.spacing);
    assert_eq!(
        hwp3_answer.line_segs[0].segment_width,
        hwpx_answer.line_segs[0].segment_width
    );

    assert_eq!(hwp3_answer.text, hwpx_answer.text);
    assert!(!hwp3_answer.text.starts_with('-'));
    assert!(!hwp3_answer.text.contains('\u{FFFC}'));

    let hwp3_answer_shape = &hwp3_doc.doc_info.para_shapes[hwp3_answer.para_shape_id as usize];
    let hwpx_answer_shape = &hwpx_doc.doc_info.para_shapes[hwpx_answer.para_shape_id as usize];
    assert_eq!(hwp3_answer_shape.head_type, HeadType::Number);
    assert_eq!(
        hwp3_answer_shape.numbering_id,
        hwpx_answer_shape.numbering_id
    );
    assert_eq!(hwp3_answer_shape.para_level, hwpx_answer_shape.para_level);
    assert_eq!(hwp3_doc.doc_info.numberings[0].level_formats[0], "^1.");
    assert_eq!(hwp3_doc.doc_info.numberings[0].level_formats[1], "^2)");
    assert_eq!(hwp3_doc.doc_info.numberings[0].level_formats[2], "(^3)");
}

#[test]
fn issue_1692_so_sueop_header_footer_page5_matches_reference_contract() {
    let hwp3_model = load("samples/SO-SUEOP.hwp");
    let hwpx_model = load("samples/SO-SUEOP.hwpx");

    let hwp3_header = first_header_paragraph(&hwp3_model, "수업용소설해설");
    let hwpx_header = first_header_paragraph(&hwpx_model, "수업용소설해설");
    assert_eq!(
        hwp3_model.doc_info.para_shapes[hwp3_header.para_shape_id as usize].alignment,
        Alignment::Justify,
        "HWP3 원본 머리말은 단일 줄 Justify이며 렌더 단계에서 머리말 폭으로 분배해야 한다"
    );
    assert_eq!(
        hwpx_model.doc_info.para_shapes[hwpx_header.para_shape_id as usize].alignment,
        Alignment::Justify,
        "HWPX DISTRIBUTE_SPACE 머리말 문단은 공백 기반 Justify로 파싱되어야 한다"
    );

    let hwp3_doc = load_wasm_doc("samples/SO-SUEOP.hwp");
    let hwpx_doc = load_wasm_doc("samples/SO-SUEOP.hwpx");
    assert_eq!(hwp3_doc.page_count(), 46);
    assert_eq!(hwpx_doc.page_count(), 46);

    let hwp3_tree = page_render_tree(&hwp3_doc, 4);
    let hwpx_tree = page_render_tree(&hwpx_doc, 4);

    for tree in [&hwp3_tree, &hwpx_tree] {
        let footer_text = text_concat_in_tree(tree, "Footer");
        assert!(
            footer_text.contains("협성고등학교"),
            "page 5 footer school label must render"
        );
        assert!(
            footer_text.contains('5'),
            "page 5 footer AutoNumber(Page) must render the current page number"
        );
    }

    let hwp3_header_width = text_width_in_tree(&hwp3_tree, "Header", "수업용소설해설 박전현선생")
        .expect("HWP3 page 5 distributed header text width");
    assert!(
        hwp3_header_width > 500.0,
        "HWP3 justified header should span the header width, got {hwp3_header_width}"
    );

    let hwpx_header_width = text_width_in_tree(&hwpx_tree, "Header", "수업용소설해설 박전현선생")
        .expect("HWPX page 5 distributed header text width");
    assert!(
        hwpx_header_width > 500.0,
        "HWPX distributed header should span the header width, got {hwpx_header_width}"
    );
}

#[test]
fn issue_1692_so_sueop_hwpx_title_ole_renders_from_embedded_preview() {
    let hwpx_model = load("samples/SO-SUEOP.hwpx");
    let ole_content = hwpx_model
        .bin_data_content
        .first()
        .expect("SO-SUEOP HWPX must load ole1.ole as BinData #1");
    assert!(
        is_hmapsi_ole_container(&ole_content.data),
        "SO-SUEOP title OLE must be identified as HMapsi fallback content"
    );

    let hwpx_doc = load_wasm_doc("samples/SO-SUEOP.hwpx");
    let tree = page_render_tree(&hwpx_doc, 0);
    assert!(
        contains_node_type(&tree, "RawSvg") || contains_node_type(&tree, "Image"),
        "SO-SUEOP page 1 title OLE must render as image-like content"
    );
    assert!(
        !contains_node_type(&tree, "Placeholder"),
        "SO-SUEOP page 1 title OLE must not fall back to Placeholder"
    );
}

#[test]
fn issue_1692_so_sueop_hwp3_endnote_internal_vpos_zero_is_normalized() {
    let hwp3_doc = load("samples/SO-SUEOP.hwp");

    let mut hwp3_endnotes = Vec::new();
    collect_endnotes(&hwp3_doc.sections[0].paragraphs, &mut hwp3_endnotes);

    let endnote_22 = hwp3_endnotes
        .iter()
        .find(|endnote| endnote.number == 22)
        .expect("HWP3 endnote 22");
    let line_vpos: Vec<i32> = endnote_22.paragraphs[0]
        .line_segs
        .iter()
        .map(|seg| seg.vertical_pos)
        .collect();

    assert_eq!(
        line_vpos,
        vec![0, 960, 1920, 2880],
        "HWP3 note-internal line vpos=0 must be normalized as a continuation line"
    );
}

#[test]
fn issue_1692_so_sueop_hwpx_endnote_internal_vpos_zero_is_normalized() {
    let hwpx_doc = load("samples/SO-SUEOP.hwpx");

    let mut hwpx_endnotes = Vec::new();
    collect_endnotes(&hwpx_doc.sections[0].paragraphs, &mut hwpx_endnotes);

    let endnote_161 = hwpx_endnotes
        .iter()
        .find(|endnote| endnote.number == 161)
        .expect("HWPX endnote 161");
    let para = &endnote_161.paragraphs[0];
    assert_eq!(para.line_segs.len(), 2);

    let first = &para.line_segs[0];
    let second = &para.line_segs[1];
    assert_eq!(
        second.vertical_pos,
        first
            .vertical_pos
            .saturating_add(first.line_height)
            .saturating_add(first.line_spacing),
        "HWPX note-internal line vpos=0 must be normalized as a continuation line"
    );
}
