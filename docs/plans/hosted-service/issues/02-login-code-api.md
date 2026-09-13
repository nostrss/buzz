# 02: 이메일 + 6자리 코드 로그인 API

**What to build:** 사람이 이메일을 보내면 6자리 코드가 메일로 오고, 그 코드를 보내면 세션 토큰과 신원 키, 커뮤니티 목록을 받는다. 처음 로그인하면 신원 키가 발급되고, 다시 로그인하면 같은 키가 돌아온다.

**Blocked by:** 01 (계정 서비스 뼈대와 배포 경로)

**Status:** done

- [x] `POST /v1/login/start`는 email을 받아 CSPRNG 6자리 코드를 만들고, 해시만 저장(만료 10분, 시도 0/5)하며, Resend HTTP API로 발송한다. 이메일 존재 여부를 응답으로 드러내지 않는다. (발송 실패 시 코드 행을 지워 즉시 재시도 가능, 502 `mail_send_failed`.)
- [x] 같은 이메일로 분당 1회를 넘기면 429와 안내 메시지를 돌려주고 메일을 보내지 않는다. (`code_recently_sent` + `retry_after_seconds`. 코드가 소진되면 쿨다운도 끝난다.)
- [x] `POST /v1/login/verify`는 email과 code를 받아 검증한다. 틀리면 남은 시도 횟수를 돌려주고, 5회 실패 또는 만료 시 코드를 폐기한다. (401 `invalid_code`/`no_active_code`, 410 `code_expired`.)
- [x] 성공 시 계정이 없으면 생성하고 `nostr` crate로 신원 키를 발급해 AES-256-GCM(마스터 키, 행별 nonce, pubkey를 AAD로 바인딩)으로 암호화 저장한다. 응답에 세션 토큰(90일), nsec, 커뮤니티 목록(host, role)이 담긴다.
- [x] 같은 이메일로 다시 로그인하면 같은 pubkey와 nsec이 돌아온다.
- [x] 세션 토큰은 임의 32바이트이고 DB에는 해시만 저장된다. `GET /v1/me`는 email, pubkey, 커뮤니티 목록, 생성 가능 여부를 돌려준다.
- [x] `POST /v1/logout`은 세션을 폐기하고, 이후 그 토큰으로 `/v1/me`를 부르면 401이다.
- [x] 통합 테스트: 실제 Postgres에 라우터 전체를 띄우고 Resend를 가짜 HTTP 서버로 대체해 위 항목을 전부 검증한다. 저장된 신원 키가 평문이 아님을 확인하는 테스트를 포함한다. (`ACCOUNT_TEST_DATABASE_URL`을 주면 실행, 없으면 자체 스킵.)
- [x] 코드, 토큰, nsec, 마스터 키는 로그에 남지 않는다. (Config Debug는 비밀 필드를 생략하고 테스트로 고정. 메일 발송 실패 로그는 Resend 오류만 담는다.)
