# 07: 배포 번들: 와일드카드 TLS와 라우팅

**What to build:** compose 번들이 `app.pegboard.me`와 `*.app.pegboard.me`를 한 Caddy로 받아, `auth.` 호스트는 계정 서비스로, 나머지는 릴레이로 보낸다. 인증서는 등록된 호스트에만 on-demand로 발급되고, 릴레이의 operator 경로는 밖에서 호출할 수 없다.

**Blocked by:** 04 (커뮤니티 생성 API)

**Status:** ready-for-agent

- [ ] Caddyfile이 루트 도메인과 와일드카드를 on-demand TLS로 받고, `ask`가 계정 서비스의 `/v1/hosts/check`를 가리킨다.
- [ ] `auth.<도메인>`은 계정 서비스로, 그 외 호스트는 릴레이로 reverse_proxy된다.
- [ ] 릴레이로 가는 요청 중 `/operator/*`는 Caddy에서 403 또는 404로 차단된다.
- [ ] compose `.env.example`에 릴레이 쪽 새 값(`BUZZ_MAX_COMMUNITIES_PER_OWNER=1`, `RELAY_OPERATOR_PUBKEYS`, `RELAY_OPERATOR_API_ORIGIN` 내부 주소)과 계정 서비스 값이 설명과 함께 들어간다.
- [ ] 로컬 compose(TLS 없이)에서 호스트 헤더를 바꿔가며 라우팅과 operator 차단이 맞는지 확인한다.
- [ ] `deploy/compose/README.md`에 와일드카드 DNS, Resend DNS 레코드, on-demand TLS 동작이 설명된다.
