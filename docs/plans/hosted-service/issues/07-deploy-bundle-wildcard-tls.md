# 07: 배포 번들: 와일드카드 TLS와 라우팅

**What to build:** compose 번들이 `app.pegboard.me`와 `*.app.pegboard.me`를 한 Caddy로 받아, `auth.` 호스트는 계정 서비스로, 나머지는 릴레이로 보낸다. 인증서는 등록된 호스트에만 on-demand로 발급되고, 릴레이의 operator 경로는 밖에서 호출할 수 없다.

**Blocked by:** 04 (커뮤니티 생성 API)

**Status:** done

- [x] Caddyfile이 루트 도메인과 와일드카드를 on-demand TLS로 받고, `ask`가 계정 서비스의 `/v1/hosts/check`를 가리킨다. (Caddy는 `?domain=`으로 묻기 때문에 `hosts_check`가 `host`와 `domain` 두 이름을 모두 받도록 바꾸고 통합 테스트에 `?domain=` 케이스 추가. compose.caddy.yml의 caddy가 `account` healthy에도 의존.)
- [x] `auth.<도메인>`은 계정 서비스로, 그 외 호스트는 릴레이로 reverse_proxy된다.
- [x] 릴레이로 가는 요청 중 `/operator/*`는 Caddy에서 404로 차단된다. (`auth.` 호스트는 계정 서비스로 그대로 가며, 그쪽엔 operator 경로가 없다.)
- [x] compose `.env.example`에 `RELAY_OPERATOR_PUBKEYS`(CHANGE_ME), `RELAY_OPERATOR_API_ORIGIN=http://relay:3000`(계정 서비스 `ACCOUNT_RELAY_URL`과 정확히 일치해야 NIP-98 검증 통과), `BUZZ_MAX_COMMUNITIES_PER_OWNER=1`이 설명과 함께 들어가고, `BUZZ_DOMAIN`이 커뮤니티 루트이며 `ACCOUNT_COMMUNITY_DOMAIN`과 같아야 함을 적었다.
- [x] 로컬에서 확인: (1) `docker run --rm -v "$PWD/deploy/compose/Caddyfile:/etc/caddy/Caddyfile:ro" -e BUZZ_DOMAIN=app.test caddy:2-alpine caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile` → "Valid configuration". (2) 같은 Caddyfile을 `caddy adapt`로 JSON화한 뒤 jq로 listen을 `:8080`, TLS 제거, automatic_https 비활성으로 바꾸고, `relay`/`account` 이름의 `hashicorp/http-echo` 두 개와 함께 compose로 띄워 curl: `Host: auth.app.test /v1/me → 200 account`, `Host: team1.app.test / → 200 relay`, `Host: app.test /_readiness → 200 relay`, `Host: team1.app.test /operator/communities → 404`. 스크립트는 세션 스크래치(`caddy-route-check/run.sh`)에 두었고 저장소에는 넣지 않았다.
- [x] `deploy/compose/README.md`에 "Hosted mode" 절: 와일드카드 DNS(Cloudflare proxy off), on-demand TLS와 ask 게이트, `auth.` 호스트, Resend SPF/DKIM/DMARC, `/operator/*` 차단, 릴레이·계정 서비스 env 대응.
