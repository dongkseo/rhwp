// [Task #2230] 그림 미지정 placeholder 선택 가능화 회귀 테스트.
//
// studio 개체 선택(findPictureAtClick)의 데이터 소스는 get_page_control_layout
// 이다. #2225 에서 MissingPicture placeholder 를 렌더 트리에 도입했지만 컨트롤
// 레이아웃에는 방출되지 않아(Placeholder 분기가 kind=="ole" 만 방출) 클릭
// 선택이 불가능했다. #2230 1단계: placeholder 에 문서 좌표(control_ref
// kind="picture")와 cell_context 를 배선하고, 컨트롤 레이아웃에
// type:"image" + missing:true 로 방출한다.
//
// 검증 문서: 36389312 결재문서 — 1페이지 상단 결재 표 "심볼" 셀에 bin_id=0
// (BinData 미첨부) Picture 컨트롤이 있어 MissingPicture placeholder 로
// 렌더된다 (좌표 실측 x≈646, y≈55, 75.6×75.6px).

use std::fs;
use std::path::Path;

fn load_doc() -> rhwp::wasm_api::HwpDocument {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
        "samples/hwpx/opengov/36389312_결재문서본문_특정소방대상물 화재발생 알림(화재번호 2026-177).hwpx",
    );
    let bytes = fs::read(&path).expect("샘플 읽기 실패");
    rhwp::wasm_api::HwpDocument::from_bytes(&bytes).expect("파싱 실패")
}

/// 컨트롤 레이아웃 JSON 에서 개별 컨트롤 오브젝트 문자열을 분리한다.
/// (수집기는 flat 배열 + 중첩 없는 스칼라/cellPath 필드만 방출하므로
/// "{\"type\":" 경계 분리로 충분하다. 표 컨트롤의 cells 중첩은 type 키가
/// 앞에 오는 경계 규칙 때문에 분리 대상에 걸리지 않는다.)
fn control_chunks(json: &str) -> Vec<&str> {
    let mut chunks = Vec::new();
    let mut rest = json;
    while let Some(pos) = rest.find("{\"type\":") {
        let tail = &rest[pos + 1..];
        let end = tail.find("{\"type\":").unwrap_or(tail.len());
        chunks.push(&rest[pos..pos + 1 + end]);
        rest = &rest[pos + 1..];
        rest = &rest[end.min(rest.len())..];
    }
    chunks
}

/// 미지정 그림 placeholder 가 클릭 선택 가능한 image 컨트롤로 방출된다.
#[test]
fn missing_picture_placeholder_emitted_as_selectable_image_control() {
    let doc = load_doc();
    let json = doc
        .get_page_control_layout(0)
        .expect("컨트롤 레이아웃 조회 실패");

    let missing: Vec<&str> = control_chunks(&json)
        .into_iter()
        .filter(|c| c.starts_with("{\"type\":\"image\"") && c.contains("\"missing\":true"))
        .collect();

    assert_eq!(
        missing.len(),
        1,
        "심볼 placeholder 가 missing image 컨트롤 1건으로 방출되어야 한다. json={json}"
    );

    let ctrl = missing[0];
    // 실측 좌표(x≈646.2, y≈54.9) — hit-test bbox 성립 확인
    assert!(
        ctrl.contains("\"x\":646.") && ctrl.contains("\"y\":54."),
        "심볼 placeholder bbox 좌표 불일치: {ctrl}"
    );
    // 문서 좌표 + 셀 경로 — enterPictureObjectSelectionDirect/커맨드 대상 특정에 필요
    for key in [
        "\"secIdx\":",
        "\"paraIdx\":",
        "\"controlIdx\":",
        "\"cellPath\":[",
    ] {
        assert!(ctrl.contains(key), "{key} 누락: {ctrl}");
    }
}

/// 그림이 실존하는 일반 image 컨트롤에는 missing 마커가 붙지 않는다.
#[test]
fn normal_image_control_has_no_missing_marker() {
    let doc = load_doc();
    let json = doc
        .get_page_control_layout(0)
        .expect("컨트롤 레이아웃 조회 실패");

    let normal: Vec<&str> = control_chunks(&json)
        .into_iter()
        .filter(|c| c.starts_with("{\"type\":\"image\"") && !c.contains("\"missing\":true"))
        .collect();

    // 좌측 기관 로고 1건 (실측 x≈84.1)
    assert_eq!(
        normal.len(),
        1,
        "일반 image 컨트롤 1건이어야 한다. json={json}"
    );
    assert!(
        normal[0].contains("\"x\":84."),
        "로고 컨트롤 좌표 불일치: {}",
        normal[0]
    );
}
