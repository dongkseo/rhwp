# Task M100 #2183 Stage 3 완료 보고 — draft PR GitHub Actions 실측

- 이슈: #2183
- 상위 추적: #2022
- PR: #2216
- PR URL: `https://github.com/edwardkim/rhwp/pull/2216`
- 브랜치: `postmelee:task2183-frontend-ci-gate`
- base: `edwardkim:devel`
- 검증 HEAD: `5183b8157d62b7c05041db5e4a054c1e8965df1d`
- 작성일: 2026-07-11
- 단계: draft PR 실측

## 1. 등록 결과

승인된 PR 본문으로 draft PR #2216을 생성하고 `@edwardkim`에게 직접 리뷰를 요청했다.

- 제목: `[CI] 프론트 패키지 변경 시 build/test gate 추가 (#2183)`
- issue 연결: `Refs #2183`
- 리뷰 요청 코멘트: `https://github.com/edwardkim/rhwp/pull/2216#issuecomment-4946198647`
- 상태: Draft / Open

issue 자동 close는 작업지시자 승인 전 수행하지 않기 위해 `Closes`가 아니라 `Refs`를 사용했다.

## 2. 첫 CI run

- CI run: `https://github.com/edwardkim/rhwp/actions/runs/29154371713`
- 시작: 2026-07-11 22:28 KST
- `Build & Test` 완료: 2026-07-11 22:39 KST
- 전체 결과: PASS

workflow 자체가 frontend 영향 경로이므로 preflight는 다음과 같이 판정했다.

```text
preflight=success
fast_pass=false
frontend_required=true
frontend_reason=frontend-path:.github/workflows/ci.yml
build_default=success
native_skia=success
frontend=success
```

기존 required surface인 `CI / Build & Test`는 별도 check 이름 변경 없이 2초에 PASS했다. release용
`WASM Build`는 PR에서 기존대로 skip됐다.

## 3. frontend worker 실측

`Frontend package gates`는 clean Ubuntu runner에서 2분 21초에 PASS했다.

| 단계 | 결과 | 소요 |
|------|------|------|
| checkout | PASS | 22초 |
| Rust 1.93.1 + wasm target | PASS | 10초 |
| wasm-pack installer | PASS | 1초 |
| frontend cargo cache restore | MISS | 1초 미만 |
| `wasm-pack build --target web --dev` | PASS | 69초 |
| Node.js setup | PASS, `22.23.1` | 3초 |
| 네 package dependency install | PASS | 12초 |
| binding/editor contract | PASS | 1초 미만 |
| `npm/editor test --if-present` | PASS | 1초 미만 |
| shared/extension SW tests | PASS | 1초 |
| Studio unit tests | PASS | 2초 |
| Studio build | PASS | 4초 |
| Chrome/Firefox build | PASS | 3초 |
| extension dist contract | PASS | 1초 |
| VS Code compile | PASS | 6초 |
| frontend cargo cache save | SKIP | PR restore-only 계약 |

Node.js 22와 fresh dev WASM 조합에서 local Stage 2와 같은 consumer gate가 모두 통과했다.

## 4. 전체 workflow 비교

| check/job | 결과 | 소요 |
|-----------|------|------|
| Frontend package gates | PASS | 2분 21초 |
| Native Skia tests | PASS | 4분 31초 |
| Canvas visual diff | PASS | 3분 49초 |
| CodeQL Rust analyze | PASS | 7분 32초 |
| Build default-feature tests | PASS | 10분 33초 |
| Build & Test aggregate | PASS | 2초 |

frontend worker는 default-feature worker보다 8분 이상 먼저 끝났다. 첫 cache miss run에서도 기존 critical
path 안에 들어가므로 이 PR의 실측에서는 전체 wall-clock을 늘리지 않았다.

## 5. cache 관찰

### frontend cargo cache

- key: `Linux-frontend-wasm-cargo-${Cargo.lock hash}`
- exact/prefix restore: MISS
- PR save step: 조건대로 SKIP

첫 run이므로 expected miss다. merge 후 trusted `devel` push에서 exact miss cache가 저장된 뒤 후속 frontend
PR의 restore hit를 확인할 수 있다.

### npm cache

`actions/setup-node`는 네 lockfile hash의 npm cache를 찾지 못했다. post step에서는 같은 key를 다른 job이
생성 중이라는 이유로 reserve에 실패했지만 job conclusion에는 영향을 주지 않았다. `node_modules`를 cache하지
않고 `npm ci`를 실행한다는 계약은 유지됐다.

## 6. 도구 버전 관찰

기존 release/Render Diff와 같은 installer URL은 runner에 `wasm-pack 0.13.1`을 설치했고 0.15.0 업데이트
경고를 출력했다. local Docker는 Dockerfile에 고정된 `wasm-pack 0.15.0`을 사용했다.

두 버전 모두 같은 HEAD의 fresh binding과 consumer gate를 통과했다. #2183에서 release/Render Diff installer
정책까지 함께 바꾸면 범위가 확장되므로 즉시 수정하지 않고 maintainer 리뷰 관찰로 남긴다. pin 통일이 필요하면
별도 workflow 도구chain 정리 단위로 다룬다.

## 7. 확인된 계약

1. workflow 변경은 frontend 영향으로 판정된다.
2. clean runner에서 fresh dev `pkg/` 생성 후 모든 package gate가 통과한다.
3. PR frontend cargo cache는 restore-only로 동작한다.
4. frontend success가 기존 `Build & Test` aggregate에 실제 반영된다.
5. release `WASM Build` trigger와 artifact 정책은 바뀌지 않는다.
6. frontend worker는 첫 miss에서도 기존 Rust critical path보다 짧다.

## 8. 남은 확인과 다음 단계

- trusted `devel` push의 frontend cargo cache save는 merge 뒤에만 실측 가능하다.
- Rust-only PR의 frontend skip은 fixture로 검증했으며 실제 별도 PR을 만들지 않는다.
- Stage 3 보고서의 trailing docs commit에서 review-only fast-pass 실패를 실측했고 아래 보정 작업으로 전환했다.
- PR 본문 체크리스트 갱신과 CI 결과 코멘트는 초안을 작업지시자에게 먼저 제시한 뒤 게시한다.
- maintainer 리뷰 승인 전 draft 해제, merge, #2183 close를 수행하지 않는다.

## 9. trailing docs fast-pass 실패와 보정

Stage 3 보고서 commit `b73a1255`를 push한 두 번째 CI run
(`https://github.com/edwardkim/rhwp/actions/runs/29154801461`)에서 review-only fast-pass가 성립하지 않고 전체
jobs가 재실행됐다. 전체 결과는 다시 PASS했고 frontend worker는 2분 10초였지만 preflight reason은 다음이었다.

```text
fast_pass=false
reason=missing-build-and-test:82a297ab53e0e67c15f2622d4a8819699fef8cb7
```

API를 직접 비교한 결과는 다음과 같다.

- candidate SHA의 Check Runs API: 0건
- 첫 CI run metadata `head_sha`: candidate SHA와 일치
- 첫 CI run jobs API: `Build & Test=completed/success` 존재
- Actions workflow-runs의 `head_sha=` server filter: fork run 누락
- source branch filter: 두 CI run 반환, 응답 `head_sha`로 exact match 가능

이는 새 frontend 판정 문제가 아니라 기존 review-only detector가 fork PR의 API 가시성 차이를 처리하지 못한
경우다. 구현 계획의 fast-pass 보존 완료 조건을 만족하기 위해 다음 최소 보정을 적용한다.

1. preflight에 read-only `actions: read` 추가
2. 기존 Check Runs 조회를 우선 유지
3. check-run 부재 시 현재 PR branch의 `ci.yml` runs 조회
4. 응답 `head_sha`를 candidate와 exact match
5. 해당 run의 `Build & Test` job이 `completed/success`인 경우에만 fast-pass
6. 누락·진행·실패·API 오류는 full CI

실제 inline script를 추출한 fixture 8건에서 check-run 성공, Actions job fallback 성공, 진행 중, job 실패,
job 누락, wrong SHA, API 오류, trailing docs 부재를 검증했다. 최종 GitHub 실측은 최신
`upstream/devel@413d8a67` rebase와 보정 commit push 후 수행한다.
