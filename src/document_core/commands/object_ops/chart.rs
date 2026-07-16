//! 차트 삽입 — OLE 개체로 문단에 꽂고 BinData 에 등록한다.
//!
//! HWP 차트는 그림과 달리 `BinDataType::Storage` + 확장자 `OLE` 로 등록된다
//! (실측: `samples/chart/**` 의 DocInfo BIN_DATA 레코드 `attr=0x0002`,
//! 확장자 `"OLE"` — 그림은 `attr=0x0101`/`Embedding`/`png`).
//!
//! 개체 자체는 `ShapeObject::Ole` 이다. `raw_tag_data` 를 비워두면 직렬화가
//! `extent_x/extent_y/bin_data_id` 필드에서 레코드를 새로 만든다
//! (`serializer/control.rs:1776` — 값이 있으면 라운드트립 보존용으로 그대로 통과).

use crate::document_core::DocumentCore;
use crate::error::HwpError;
use crate::model::bin_data::{
    BinData, BinDataCompression, BinDataContent, BinDataStatus, BinDataType,
};
use crate::model::control::Control;
use crate::model::shape::{
    CommonObjAttr, DrawingObjAttr, HorzRelTo, OleDrawingAspect, OleShape, ShapeObject, TextWrap,
    VertRelTo,
};
use crate::serializer::chart_ole::build_chart_ole;
use crate::serializer::chart_xml::ChartSpec;

impl DocumentCore {
    /// 차트를 OLE 개체로 삽입한다.
    ///
    /// `rgba` 는 미리보기 픽셀 (위→아래, 4바이트/픽셀). 한컴·뷰어는 이 그림만
    /// 그리므로 (XML 은 rhwp 전용), 호출자가 차트를 래스터화해 넘겨야 한다.
    /// 네이티브에서는 `render_chart_preview` 로 자동 생성할 수 있다.
    ///
    /// `width_hu`/`height_hu`: 문서상 개체 크기 (HWPUNIT).
    /// `paper_offset_*`: 용지 기준 위치 (HWPUNIT). 그림과 같은 규약 —
    /// 생략(None)하면 (0,0) 즉 용지 좌상단이다.
    #[allow(clippy::too_many_arguments)]
    pub fn insert_chart_native(
        &mut self,
        section_idx: usize,
        para_idx: usize,
        spec: &ChartSpec,
        rgba: &[u8],
        px_width: u32,
        px_height: u32,
        width_hu: u32,
        height_hu: u32,
        paper_offset_x_hu: Option<i32>,
        paper_offset_y_hu: Option<i32>,
    ) -> Result<String, HwpError> {
        if section_idx >= self.document.sections.len() {
            return Err(HwpError::RenderError(format!(
                "구역 {} 범위 초과",
                section_idx
            )));
        }
        if para_idx >= self.document.sections[section_idx].paragraphs.len() {
            return Err(HwpError::RenderError(format!(
                "문단 {} 범위 초과",
                para_idx
            )));
        }
        if spec.series.is_empty() {
            return Err(HwpError::RenderError("계열이 비었다".to_string()));
        }
        for s in &spec.series {
            if s.values.len() != spec.categories.len() {
                return Err(HwpError::RenderError(format!(
                    "계열 '{}' 의 값 개수({})가 카테고리 수({})와 다르다",
                    s.name,
                    s.values.len(),
                    spec.categories.len()
                )));
            }
        }

        // --- 1. OLE blob 조립 (생 CFB — 압축·프리픽스는 cfb_writer 가 붙인다) ---
        let ole_blob = build_chart_ole(spec, rgba, px_width, px_height, width_hu, height_hu)
            .map_err(HwpError::RenderError)?;
        let cfb_len = ole_blob.cfb.len();

        // --- 2. BinData 등록 (차트 = Storage/OLE, 그림과 다름) ---
        let storage_id = self.document.next_bin_data_storage_id();
        self.document.bin_data_content.push(BinDataContent {
            id: storage_id,
            data: ole_blob.cfb,
            extension: "OLE".to_string(),
        });
        // attr=0x0002: bits 0-3 = 2(Storage), bits 4-5 = 0(Default), bits 8-9 = 0(Init)
        self.document.doc_info.bin_data_list.push(BinData {
            raw_data: None,
            attr: 0x0002,
            data_type: BinDataType::Storage,
            compression: BinDataCompression::Default,
            status: BinDataStatus::NotAccessed,
            abs_path: None,
            rel_path: None,
            storage_id,
            extension: Some("OLE".to_string()),
        });
        // DocInfo 를 바꿨으므로 원본 스트림 통과를 끈다. 이걸 빠뜨리면
        // serialize_doc_info 가 원본 바이트를 그대로 뱉어 (doc_info.rs:24)
        // BIN_DATA 레코드가 유실되고, 파서가 BinData 스트림을 못 찾는다.
        self.document.doc_info.raw_stream_dirty = true;

        // --- 3. OleShape 생성 ---
        let (ox, oy) = (
            paper_offset_x_hu.unwrap_or(0).max(0) as u32,
            paper_offset_y_hu.unwrap_or(0).max(0) as u32,
        );
        // bits 15-17 = 4(Absolute width), bits 18-20 = 2(Absolute height)
        let common_attr: u32 = (4 << 15) | (2 << 18);
        let common = CommonObjAttr {
            ctrl_id: 0x6773_6F20, // "gso "
            attr: common_attr,
            treat_as_char: false,
            vert_rel_to: VertRelTo::Paper,
            horz_rel_to: HorzRelTo::Paper,
            text_wrap: TextWrap::Square,
            horizontal_offset: ox,
            vertical_offset: oy,
            width: width_hu,
            height: height_hu,
            ..Default::default()
        };

        let mut drawing = DrawingObjAttr::default();
        drawing.shape_attr.original_width = width_hu;
        drawing.shape_attr.original_height = height_hu;
        drawing.shape_attr.current_width = width_hu;
        drawing.shape_attr.current_height = height_hu;

        let shape = OleShape {
            common,
            drawing,
            extent_x: width_hu as i32,
            extent_y: height_hu as i32,
            flags: 0,
            drawing_aspect: OleDrawingAspect::Content,
            bin_data_id: storage_id as u32,
            preview: None,
            // 비워두면 직렬화가 extent/bin_data_id 필드로 레코드를 새로 만든다
            raw_tag_data: Vec::new(),
            caption: None,
        };

        // --- 4. 문단에 꽂기 ---
        let para = &mut self.document.sections[section_idx].paragraphs[para_idx];
        let control_idx = para.controls.len();
        para.controls
            .push(Control::Shape(Box::new(ShapeObject::Ole(Box::new(shape)))));

        self.document.sections[section_idx].raw_stream = None;
        self.rebuild_section(section_idx);
        self.paginate_if_needed();

        Ok(format!(
            "{{\"ok\":true,\"paraIdx\":{},\"controlIdx\":{},\"binDataId\":{},\"oleBytes\":{}}}",
            para_idx, control_idx, storage_id, cfb_len
        ))
    }
}
