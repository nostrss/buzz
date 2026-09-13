# 01: 계정 서비스 뼈대와 배포 경로

**What to build:** 운영자가 `run.sh upgrade`를 실행하면 `buzz-account` 서비스가 릴레이 옆에 뜨고, 자기 데이터베이스에 마이그레이션을 적용한 뒤 `/_readiness`가 200을 돌려준다. 이미지는 릴레이와 같은 GitHub Actions 파이프라인으로 GHCR에 올라간다. 아직 다른 엔드포인트는 없다.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] 워크스페이스에 `buzz-account` crate가 추가되고 `just ci`가 통과한다. upstream 파일 수정은 워크스페이스 members 한 줄뿐이다.
- [ ] 설정은 환경변수로만 읽는다: DB URL, 릴레이 내부 URL, 운영자 비밀키, 마스터 키, Resend API 키, 발신 주소, 커뮤니티 도메인 접미사. 필수값이 빠지면 시작 시 명확한 오류로 종료한다.
- [ ] 시작 시 계정 서비스 전용 데이터베이스에 스펙의 4개 테이블(accounts, login_codes, sessions, communities)을 만드는 마이그레이션이 적용된다.
- [ ] `GET /_readiness`가 DB 연결을 확인하고 200을 돌려준다.
- [ ] Dockerfile과 docker.yml 빌드 잡이 추가되어 `ghcr.io/nostrss/buzz-account:deploy`가 게시된다. 릴레이 이미지 잡은 건드리지 않는다.
- [ ] compose 번들에 `account` 서비스가 추가되고, 같은 Postgres 인스턴스의 별도 데이터베이스를 쓰며, 릴레이 컨테이너에는 운영자 키와 마스터 키가 전달되지 않는다.
- [ ] compose `.env.example`에 새 변수가 설명과 함께 추가된다.
- [ ] 로컬에서 compose로 띄워 readiness가 200인 것을 확인한다.
