# 08: 프로덕션 전환 런북 실행

**What to build:** 팀 릴레이 `app.pegboard.me`가 멀티테넌트 릴레이로 전환되고, 팀은 계정 서비스로 만든 `app.pegboard.me` 커뮤니티에 이메일로 재가입한다. 이후 아무 `<이름>.app.pegboard.me`가 요청되어도 DNS와 TLS를 손대지 않고 서비스된다.

**Blocked by:** 05 (데스크톱 커뮤니티 생성 화면), 06 (키 관련 화면 미노출과 나가기), 07 (배포 번들: 와일드카드 TLS와 라우팅)

**Status:** ready-for-agent

- [ ] `/wizard`로 아래 단계를 사람이 순서대로 밟는 대화형 스크립트를 만든다. 되돌릴 수 없는 단계 앞에는 확인을 요구한다.
- [ ] Cloudflare: `*.app.pegboard.me` A 레코드(proxy OFF), Resend가 요구하는 SPF/DKIM/DMARC 레코드. Resend에서 도메인 검증 완료.
- [ ] 서버 `.env` 백업과 Postgres 덤프를 뜬 뒤 릴레이 DB를 초기화한다. `RELAY_OWNER_PUBKEY`와 `BUZZ_RELAY_PRIVATE_KEY`는 유지한다.
- [ ] 계정 서비스용 운영자 키와 마스터 키를 생성해 `.env`에 넣고, 릴레이 `.env`에 `RELAY_OPERATOR_PUBKEYS`, `RELAY_OPERATOR_API_ORIGIN`, `BUZZ_MAX_COMMUNITIES_PER_OWNER=1`을 넣는다.
- [ ] `feat/hosted-service`를 `deploy`로 merge하고 push해 이미지 빌드와 upgrade가 완료된다. `/_readiness`가 릴레이와 계정 서비스 모두 200이다.
- [ ] 운영자가 이메일로 로그인해 `app` 이름으로 커뮤니티를 만들어 `app.pegboard.me`가 그 커뮤니티가 된다(예약어 예외는 이 한 번만 운영자 경로로 처리).
- [ ] 팀원 전원이 새 DMG로 이메일 로그인 후 초대 링크로 재가입한다.
- [ ] 임의의 새 이름으로 커뮤니티를 하나 만들어 인증서 발급과 접속을 확인하고, 등록되지 않은 호스트는 인증서가 발급되지 않음을 확인한다.
- [ ] 새 `.env`와 계정 서비스 DB가 백업 대상에 추가된다.
