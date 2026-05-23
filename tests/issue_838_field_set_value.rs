//! Issue #838: set_field_text_at ClickHere 빈 필드에서 안내문 미삭제 → 파일 손상
//!
//! 재현: field-01.hwp (안내문 있는 ClickHere 필드) 로드 → set_field_value_by_name
//! → 안내문이 삭제되지 않고 입력값과 병기 → char_count 불일치 → 한컴 "파일 손상"

use std::fs;
use std::path::Path;

#[test]
fn set_field_value_removes_guide_text() {
    let repo_root = env!("CARGO_MANIFEST_DIR");
    let hwp_path = Path::new(repo_root).join("samples/field-01.hwp");
    let bytes =
        fs::read(&hwp_path).unwrap_or_else(|e| panic!("read {}: {}", hwp_path.display(), e));

    let mut core =
        rhwp::document_core::DocumentCore::from_bytes(&bytes).expect("parse field-01.hwp");

    let fields = core.collect_all_fields();
    assert!(!fields.is_empty(), "field-01.hwp should contain fields");

    let first_field = &fields[0];
    let field_name = first_field.field.field_name().unwrap_or("");
    assert!(!field_name.is_empty(), "first field should have a name");

    let result = core.set_field_value_by_name(field_name, "테스트회사");
    assert!(result.is_ok(), "set_field_value_by_name failed: {:?}", result.err());

    let fields_after = core.collect_all_fields();
    let updated = fields_after
        .iter()
        .find(|f| f.field.field_name() == Some(field_name))
        .expect("field should still exist after set");

    // 핵심 검증: 값이 정확히 설정한 것만 있어야 함 (안내문 병기 회귀 방지)
    assert_eq!(
        updated.value, "테스트회사",
        "field value should be exactly the set value without guide text; got: '{}'",
        updated.value
    );
}
