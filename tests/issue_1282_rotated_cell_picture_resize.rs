use rhwp::document_core::DocumentCore;
use rhwp::model::control::Control;
use rhwp::model::document::Document;
use rhwp::model::image::Picture;
use rhwp::model::table::{Cell, Table};
use rhwp::parser::parse_document;

fn read_fixture(path: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .unwrap_or_else(|e| panic!("read {path}: {e}"))
}

fn target_table(doc: &Document) -> &Table {
    match &doc.sections[0].paragraphs[0].controls[2] {
        Control::Table(table) => table,
        other => panic!("expected target table at para 0 control 2, got {other:?}"),
    }
}

fn target_cell(doc: &Document) -> &Cell {
    &target_table(doc).cells[2]
}

fn target_picture(doc: &Document) -> &Picture {
    let cell = target_cell(doc);
    match &cell.paragraphs[0].controls[0] {
        Control::Picture(pic) => pic,
        other => panic!("expected target cell picture, got {other:?}"),
    }
}

fn required_cell_height_for_picture(cell: &Cell, pic: &Picture) -> u32 {
    let angle = (pic.shape_attr.rotation_angle as f64).to_radians();
    let visual_height = if pic.shape_attr.rotation_angle.rem_euclid(360) == 0 {
        pic.common.height
    } else {
        let sin = angle.sin().abs();
        let cos = angle.cos().abs();
        ((pic.common.width as f64 * sin + pic.common.height as f64 * cos)
            .round()
            .max(1.0)) as u32
    };
    let vert_offset = (pic.common.vertical_offset as i32).max(0) as u32;
    vert_offset
        .saturating_add(visual_height)
        .saturating_add(cell.padding.top as u32)
        .saturating_add(cell.padding.bottom as u32)
}

fn picture_center(pic: &Picture) -> (i64, i64) {
    (
        pic.common.horizontal_offset as i32 as i64 + pic.common.width as i64 / 2,
        pic.common.vertical_offset as i32 as i64 + pic.common.height as i64 / 2,
    )
}

#[test]
fn issue_1282_resizing_rotated_cell_picture_grows_owner_cell_height() {
    let bytes = read_fixture("samples/ta-pic-001-r.hwp");
    let mut core = DocumentCore::from_bytes(&bytes).expect("load HWP fixture");
    let original_width = target_picture(core.document()).common.width;

    core.set_cell_picture_properties_by_path_native(
        0,
        0,
        r#"[{"controlIdx":2,"cellIdx":2,"cellParaIdx":0}]"#,
        0,
        r#"{"height":30000}"#,
    )
    .expect("resize rotated cell picture");

    let doc = core.document();
    let table = target_table(doc);
    let cell = target_cell(doc);
    let pic = target_picture(doc);
    let required_height = required_cell_height_for_picture(cell, pic);

    assert!(
        cell.height >= required_height,
        "owner cell must grow to contain resized rotated picture: cell.height={}, required={}, pic.vertOffset={}, pic.height={}, pad=({}, {})",
        cell.height,
        required_height,
        pic.common.vertical_offset,
        pic.common.height,
        cell.padding.top,
        cell.padding.bottom
    );
    assert!(
        table.common.height >= cell.height,
        "table common height must follow grown cell height: table.common.height={}, cell.height={}",
        table.common.height,
        cell.height
    );
    let grown_cell_height = cell.height;
    let grown_current_width = pic.shape_attr.current_width;
    let grown_current_height = pic.shape_attr.current_height;
    let grown_center = picture_center(pic);

    core.set_cell_picture_properties_by_path_native(
        0,
        0,
        r#"[{"controlIdx":2,"cellIdx":2,"cellParaIdx":0}]"#,
        0,
        r#"{"rotationAngle":0}"#,
    )
    .expect("unrotate cell picture");

    let doc = core.document();
    let table = target_table(doc);
    let cell = target_cell(doc);
    let pic = target_picture(doc);
    let required_height = required_cell_height_for_picture(cell, pic);
    let unrotated_center = picture_center(pic);

    assert_eq!(
        pic.common.width, grown_current_width,
        "rotation 0 edit must use the current image width"
    );
    assert_eq!(
        pic.common.height, grown_current_height,
        "rotation 0 edit must use the current image height"
    );
    assert!(
        (unrotated_center.0 - grown_center.0).abs() <= 1
            && (unrotated_center.1 - grown_center.1).abs() <= 1,
        "rotation edit must preserve visual center: grown={:?}, unrotated={:?}",
        grown_center,
        unrotated_center
    );
    assert!(
        cell.height < grown_cell_height,
        "owner cell height must shrink when rotation angle alone removes visual hull: grown={}, current={}",
        grown_cell_height,
        cell.height
    );
    assert_eq!(
        cell.height, required_height,
        "owner cell height must sync after rotation-only edit: cell.height={}, required={}",
        cell.height, required_height
    );
    assert!(
        table.common.height >= cell.height,
        "table common height must follow rotation-only cell height: table.common.height={}, cell.height={}",
        table.common.height,
        cell.height
    );

    core.set_cell_picture_properties_by_path_native(
        0,
        0,
        r#"[{"controlIdx":2,"cellIdx":2,"cellParaIdx":0}]"#,
        0,
        &format!(
            r#"{{"width":{},"height":12000,"rotationAngle":0}}"#,
            original_width
        ),
    )
    .expect("shrink and unrotate cell picture");

    let doc = core.document();
    let table = target_table(doc);
    let cell = target_cell(doc);
    let pic = target_picture(doc);
    let required_height = required_cell_height_for_picture(cell, pic);

    assert!(
        cell.height < grown_cell_height,
        "owner cell height must shrink after picture shrinks/unrotates: grown={}, current={}",
        grown_cell_height,
        cell.height
    );
    assert_eq!(
        cell.height, required_height,
        "owner cell height must sync to shrunken unrotated picture: cell.height={}, required={}",
        cell.height, required_height
    );
    assert!(
        table.common.height >= cell.height,
        "table common height must follow shrunken cell height: table.common.height={}, cell.height={}",
        table.common.height,
        cell.height
    );

    let exported = core.export_hwp_native().expect("export edited HWP");
    let reparsed = parse_document(&exported).expect("reparse edited HWP");
    let reparsed_table = target_table(&reparsed);
    let reparsed_cell = target_cell(&reparsed);
    let reparsed_pic = target_picture(&reparsed);
    let reparsed_required = required_cell_height_for_picture(reparsed_cell, reparsed_pic);

    assert!(
        reparsed_cell.height >= reparsed_required,
        "exported HWP must preserve grown owner cell height: cell.height={}, required={}",
        reparsed_cell.height,
        reparsed_required
    );
    assert!(
        reparsed_table.common.height >= reparsed_cell.height,
        "exported HWP table height must preserve grown cell height"
    );
}
