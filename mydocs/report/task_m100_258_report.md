# Task M100-258 최종 보고서 — 한글 누름틀 + 양식 모드 구현

- 이슈: https://github.com/edwardkim/rhwp/issues/258
- 브랜치: `local/task_m100_258`
- 작성일: 2026-06-15
- 기준 브랜치: `upstream/devel`

## 1. 완료 범위

- rhwp-studio에 `normal`/`form` 편집 모드를 추가했다.
- 양식 모드에서 `editable=true`인 ClickHere 누름틀 내부 텍스트 입력/삭제만 허용하고,
  일반 본문 입력, 삭제, 붙여넣기, 구조 삽입, 서식 변경, 누름틀 삭제를 차단했다.
- `getFieldInfoAt*`, `getFieldList()` JSON에 양식 모드 판단과 이동에 필요한
  `editableInForm`, `startCharIdx`, `endCharIdx`를 노출했다.
- 양식 모드에서 Tab/Shift+Tab으로 다음/이전 editable ClickHere로 이동하도록 했다.
- `insert:field` 스텁을 누름틀 삽입 대화상자로 교체했다.
- 본문/셀/중첩 cellPath 위치에 ClickHere field range와 command/CTRL_DATA를 생성하는
  Rust/WASM API를 추가했다.
- HWPX 직렬화에서 새로 생성한 ClickHere `Field.command`를 `hp:parameters`로 저장해
  HWPX 재파싱 시 안내문/메모가 보존되도록 했다.
- 누름틀 입력/고치기 대화상자는 바깥 클릭으로 닫히지 않게 했고, 누름틀 삽입 직후에는
  한컴처럼 안내문이 표시되도록 active field를 즉시 잡지 않게 했다.
- 누름틀 끝에서 오른쪽 이동 후 이어 입력하면 field range 밖 본문으로 들어가도록 했고,
  누름틀 경계 삭제는 한컴처럼 `[누름틀]을 지울까요?` 확인을 거치게 했다.
- 빈 누름틀 안내문 클릭 후 첫 입력 위치를 field start로 정규화해 `입력하세요` 클릭 뒤
  바로 `123` 같은 값을 입력할 수 있게 했다.
- 빈 누름틀 첫 입력 직후 active field와 마커를 새 field value 기준으로 다시 계산해,
  Enter 같은 추가 편집 없이도 입력값과 누름틀 마커가 즉시 표시되도록 보정했다.
- 빈 누름틀 첫 입력 뒤 `getCursorRect()`가 0폭 placeholder가 아니라 실제 입력값 기준
  caret x를 반환하도록 해 field end 마커가 입력값 끝을 따라가게 했다.

## 2. 검증

- `cargo fmt --check`
- `cargo test --test issue_258_clickhere_form_mode`
- `cargo test --test issue_838_field_set_value`
- `wasm-pack build --target web --out-dir pkg`
- `npm run build`
- `git diff --check`

Stage8 추가 검증:

- `cargo fmt`
- `cargo test --test issue_258_clickhere_form_mode`
- `npm run build`
- `git diff --check`

Stage9 추가 검증:

- `cargo fmt`
- `cargo test --test issue_258_clickhere_form_mode`
- `npm run build`

Stage10 추가 검증:

- `cargo test --test issue_258_clickhere_form_mode`
- 루트 `npm run build`는 스크립트가 없어 실행 불가 확인
- `cd rhwp-studio && npm run build`
- `git diff --check`

Stage11 추가 검증:

- `cargo test --test issue_258_clickhere_form_mode first_input_into_empty_clickhere_is_rendered -- --nocapture`
- `cargo test --test issue_258_clickhere_form_mode`
- `cargo test --lib rebuild_`
- `cargo fmt --check`
- `git diff --check`
- `wasm-pack build --target web --out-dir pkg`
- `http://localhost:7700/` Playwright 검증 통과
  (`value=123`, field range `0..3`, cursor x `113.4→135.4`, `「123」` 표시)

## 3. 남은 후속

- 사용자 정보, 문서 요약, 작성한 날짜, 파일 이름/경로 등 누름틀 외 필드 탭은 후속 이슈로 분리한다.
- 양식 개체 전체(Edit/CheckBox/RadioButton/ComboBox/PushButton)의 완전 상호작용은 기존
  FormObject 작업과 이어서 별도 처리한다.
- PR 생성과 전체 CI급 검증은 작업지시자 별도 승인 후 진행한다.
