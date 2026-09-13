# 04: 커뮤니티 생성 API

**What to build:** 로그인한 계정이 이름 하나를 보내면 `<이름>.app.pegboard.me` 커뮤니티가 릴레이에 만들어지고 그 계정이 owner가 된다. 계정당 하나만 만들 수 있고, 입력 중 가용성을 확인할 수 있다. Caddy가 인증서를 발급하기 전에 물어볼 호스트 확인 엔드포인트도 제공한다.

**Blocked by:** 02 (이메일 + 6자리 코드 로그인 API)

**Status:** done

- [x] 이름 정규화: trim → 소문자 → 공백을 하이픈으로 → 연속 하이픈 축약. 검증: `^[a-z0-9]([a-z0-9-]{1,28}[a-z0-9])?$`, 예약어(`app`, `auth`, `www`, `admin`, `api`, `mail`, `relay`) 거부. (사유 코드: too_short, too_long, invalid_characters, leading_or_trailing_hyphen, reserved.)
- [x] `POST /v1/communities/check`는 정규화 결과, 검증 오류 사유, 릴레이 availability API 결과를 돌려준다. (검증 실패면 릴레이를 부르지 않음. 점유 시 reason `taken`.)
- [x] `POST /v1/communities`는 계정당 1개(DB unique 제약)를 검사한 뒤 릴레이 `/operator/communities`를 NIP-98 서명으로 `create_only: true`, `initial_owner_pubkey` = 계정 pubkey, host = `<name>.<접미사>`로 호출한다. 응답에 host와 `wss://<host>`가 담긴다. (`buzz-auth`에는 클라이언트 서명기가 없어 `nostr` crate로 kind 27235를 직접 서명: `u`=릴레이 origin+경로(+쿼리), `method`, `payload`=본문 SHA-256, `nonce`. 응답 201.)
- [x] 릴레이 호출이 실패하면 오류를 그대로 전달하고 계정 서비스에 커뮤니티 행을 남기지 않는다. (502 `relay_error` + relay_status/message. 릴레이 409는 409 `host_taken`.)
- [x] 두 번째 생성 요청은 409와 "계정당 하나" 안내를 돌려준다. (`community_limit_reached`. 사전 검사 + UNIQUE 위반 백스톱.)
- [x] `GET /v1/hosts/check?host=`는 인증 없이 `auth.<접미사>`, 접미사의 루트 호스트, 계정 서비스가 만든 커뮤니티 host에만 200, 그 외 404.
- [x] 통합 테스트: 릴레이 operator API를 가짜 HTTP 서버로 대체해 호출 본문(경로, create_only, owner pubkey, host, NIP-98 헤더)을 검증하고, 정규화·검증·예약어·1개 제한·실패 시 미생성·hosts/check를 검증한다. (NIP-98 이벤트를 디코드해 서명·`u`·`method`·`payload` 태그까지 확인. 하네스는 `tests/common/mod.rs`로 공유.)
