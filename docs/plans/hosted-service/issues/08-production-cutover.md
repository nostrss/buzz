# 08: 프로덕션 전환 런북 실행

**What to build:** 팀 릴레이 `app.pegboard.me`가 멀티테넌트 릴레이로 전환되고, 팀은 계정 서비스로 만든 `app.pegboard.me` 커뮤니티에 이메일로 재가입한다. 이후 아무 `<이름>.app.pegboard.me`가 요청되어도 DNS와 TLS를 손대지 않고 서비스된다.

**Blocked by:** 05 (데스크톱 커뮤니티 생성 화면), 06 (키 관련 화면 미노출과 나가기), 07 (배포 번들: 와일드카드 TLS와 라우팅)

**Status:** done (2026-09-13 전환 완료)

- [x] 대화형 wizard 대신 서버에서 한 번에 도는 7단계 스크립트(`cutover.sh`: 체크아웃 갱신 → `.env` 추가 → 스택 중지 → DB 초기화 → 계정 서비스 선기동으로 운영자 공개키 추출 → 전체 기동 → 검증)로 실행했다. 실행은 사용자가 `ssh buzz-prod 'bash /root/cutover.sh'`로 했다(에이전트의 프로덕션 변경 명령은 권한 분류기가 차단).
- [x] Cloudflare: `*.app` A 레코드(proxy OFF) 추가, `auth.`/`team.` 호스트 해석 확인. Resend 도메인 검증 완료(DKIM `resend._domainkey` 존재).
- [x] `.env`, `.env.account` 백업과 `pg_dump`를 `/root/backup-20260913-1331/`에 저장한 뒤 릴레이 DB(`buzz`)를 DROP/CREATE. `RELAY_OWNER_PUBKEY`, `BUZZ_RELAY_PRIVATE_KEY` 유지. 계정 DB `buzz_account`는 계정 서비스가 첫 부팅에 생성.
- [x] `.env.account`는 사용자가 서버에서 `openssl rand`로 생성(운영자 키, 마스터 키, Resend 키, 발신 주소, `app.pegboard.me`). 릴레이 `.env`에 `RELAY_OPERATOR_API_ORIGIN=http://relay:3000`, `BUZZ_MAX_COMMUNITIES_PER_OWNER=1`, `RELAY_OPERATOR_PUBKEYS`(계정 서비스 로그의 `operator_pubkey`) 추가.
- [x] `deploy`를 `d2ef9df50`으로 fast-forward push. CI run 34760113907: 릴레이·계정 서비스 이미지 게시(둘 다 익명 pull 가능), 배포 잡 성공. 전환 후 `https://app.pegboard.me/_readiness`, `https://auth.app.pegboard.me/_readiness` 모두 200, TLS 검증 0.
- [x] 결정 변경: 루트 `app.pegboard.me`를 팀 커뮤니티로 특별 처리하지 않는다. 릴레이가 자동 생성한 루트 커뮤니티는 비워 두고, 팀은 다른 고객과 같은 앱 경로로 `<이름>.app.pegboard.me`를 만든다(사용자가 이름을 정해 직접 생성 예정).
- [ ] 팀원 전원이 새 DMG(`d2ef9df50` 기준, `desktop/.env.production` 포함)로 이메일 로그인 후 초대 링크로 재가입한다. 사용자가 서명된 DMG를 빌드 중이며 배포는 사용자 진행.
- [x] `auth.app.pegboard.me` 첫 요청에 Let's Encrypt 인증서가 on-demand로 발급됨(Caddy 로그 "certificate obtained successfully"). `nope.app.pegboard.me`는 TLS 핸드셰이크 실패(curl exit 35)로 인증서 미발급 확인. `/operator/communities`는 Caddy에서 404. 실제 Resend로 `POST /v1/login/start` → `{"status":"sent"}`. 새 커뮤니티 생성은 팀 커뮤니티 생성으로 확인 예정.
- [x] 백업 대상: `deploy/compose/.env`, `deploy/compose/.env.account`, Postgres(`buzz`, `buzz_account`), MinIO, git 볼륨. `run.sh backup-hint`에 `.env.account`가 포함된다.

**후속 (범위 밖, 기록용)**: 만료 코드·세션 청소 작업, 로그인 시작의 IP 기준 제한, 메일 발송 실패 알림. 로컬 검증용 `.env` 변경(RELAY_URL=ws://localtest.me 등)은 gitignored 로컬 파일이라 되돌릴 필요 없음.
