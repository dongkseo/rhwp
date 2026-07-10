# 변경 기록 — 2026-07-11

## Issue #2186: iframe MessageChannel v1

- `@rhwp/editor`가 exact Studio origin으로 v1 `MessageChannel` version/capability를 협상하고 일반 10초·load/export 60초 timeout을 관리한다.
- load/export binary는 transferable로 전달하되 caller 입력 buffer는 복사해 detach하지 않는다.
- Studio protocol guard, RPC router, runtime을 분리해 version/session과 최초 parent source/origin/port binding을 검증한다.
- 협상·RPC 오류는 구조화된 v1 envelope로 전달하고 malformed response는 완료 응답으로 처리하지 않는다.
- 연결 또는 초기화 실패 시 생성 중인 iframe, port, listener, timer를 정리한다.
- 거부된 malformed/unsupported 연결과 foreign-origin 재연결의 transferred port도 즉시 닫고, runtime 종료 시 port handler를 해제한다.
- foreign source, non-connect, surplus transferred port도 즉시 닫아 수신 측 소유권을 남기지 않는다.
- SDK와 Studio는 HTTP(S) origin만 허용하고 `file:`·`data:`·브라우저 확장 등 opaque origin을 명시적으로 거부한다.
- 동기 `postMessage` 실패도 request timer와 pending map을 즉시 정리한다.
- 기존 method/return type과 legacy client message shape을 유지한다.
- browser E2E는 protocol을 재구현하지 않고 실제 `@rhwp/editor` entry로 load/export/destroy를 호출하며 caller buffer, forged sibling, legacy 경로를 함께 검증한다.
- 50 MiB `Uint8Array`도 v1 `loadFile`에서 number array 변환 없이 전달하며 caller의 원본 backing buffer를 분리하지 않음을 Node `MessageChannel` 계약 테스트로 검증한다.

별도 package와 caller buffer 직접 transfer는 각각 API 중복과 기존 소유권 변경 때문에 채택하지 않았다.
