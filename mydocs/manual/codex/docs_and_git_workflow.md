# Documentation And Git Workflow

## Document Language

모든 프로젝트 문서는 한국어로 작성한다.

## Working Document Naming

단계별 작업 문서:

```text
mydocs/working/task_m100_{issue}_stage{N}.md
```

예:

```text
mydocs/working/task_m100_854_stage1.md
mydocs/working/task_m100_854_rebuild_stage4.md
```

최종 보고서:

```text
mydocs/report/task_m100_{issue}_report.md
```

오늘할일:

```text
mydocs/orders/YYYYMMDD.md
```

## Folder Roles

- `mydocs/orders/`: 오늘할일
- `mydocs/plans/`: 수행 계획서, 구현 계획서
- `mydocs/working/`: 단계별 완료 보고서
- `mydocs/report/`: 최종 보고서
- `mydocs/troubleshootings/`: 재발 방지용 문제 해결 기록
- `mydocs/tech/`: 기술 조사와 스펙 정리
- `mydocs/manual/`: 매뉴얼과 장기 지침
- `mydocs/manual/memory/`: Claude 메모리 덤프
- `mydocs/manual/codex/`: Codex 메모리 덤프

## Issue Workflow

이슈 기반 작업의 기본 순서:

1. GitHub Issue 확인 또는 생성
2. 열린 PR 확인
3. 이슈 assignee 지정
4. 작업 브랜치 생성 또는 전환
5. 오늘할일 문서 갱신
6. 계획서 작성
7. 작업지시자 승인
8. 구현과 테스트
9. 단계별 보고서 작성
10. 커밋
11. 작업지시자 승인 후 이슈 close

## GitHub CLI Usage

GitHub connector가 읽기는 가능하지만 mutation 권한 부족으로 403을 반환할 수 있다.
이슈 assignee 지정, 이슈/PR metadata 수정, 코멘트 작성 등 GitHub 변경 작업은
로컬 인증된 `gh` CLI를 사용한다.

예:

```bash
gh issue edit 1063 --add-assignee edwardkim -R edwardkim/rhwp
```

운영 규칙:

- connector mutation이 403으로 실패하면 `gh` CLI로 재시도한다.
- sandbox 네트워크 제한으로 `api.github.com` 연결 실패가 나면 동일 `gh` 명령을 escalation으로 재시도한다.
- `gh`로 수행한 GitHub 변경은 오늘할일, 계획서, 보고서 중 관련 문서에 기록한다.
- `gh` 사용도 하이퍼-워터폴 절차를 대체하지 않는다. 이슈 확인, 브랜치, 문서, 승인 게이트는 그대로 유지한다.

## PR Workflow

외부 기여자 PR은 내부 task와 다르게 처리한다.

문서 위치:

```text
mydocs/pr/
```

파일명:

```text
pr_{number}_review.md
pr_{number}_review_impl.md
pr_{number}_report.md
```

PR 댓글 톤은 과장하지 않는다. "정말 감사합니다", "정성스러운 PR" 같은 반복적이고 과한 표현보다 사실 중심으로 쓴다.

## Commit Rules

- 보고서와 오늘할일 갱신은 task 브랜치에서 소스 변경과 함께 커밋한다.
- merge 전에는 `git status`를 확인한다.
- 이슈 close 전에는 정정 commit이 `devel` 또는 대상 브랜치에 실제 포함되어 있는지 확인한다.
- 사용자가 만들었을 수 있는 변경은 임의로 되돌리지 않는다.

## Current Branch Memory

2026-05-22 현재 작업 브랜치는 `local/task_m100_1063` 이다.

Task #1063은 용지설정 대화창의 세로/가로 방향 가이드 아이콘 식별성 정정이다.
계획서와 구현 계획서는 작성되었고, GitHub assignee 지정은 `gh` CLI로 완료했다.
소스 구현은 작업지시자 승인 후 진행한다.
