# 05: 데스크톱 커뮤니티 생성 화면

**What to build:** "커뮤니티 만들기" 카드를 누르면 이름 하나를 넣는 화면이 열리고, 입력하는 동안 `<이름>.app.pegboard.me` 미리보기와 가용성이 보인다. 만들면 upstream 흐름(접속 → 프로필 → 스타터 채널 → 환영 채널과 에이전트 킥오프)이 그대로 이어진다.

**Blocked by:** 03 (데스크톱 이메일 로그인 온보딩), 04 (커뮤니티 생성 API)

**Status:** done

- [x] 생성 경로를 계정 서비스로 바꿨다. 결정 변경: BuilderLab 클라이언트(`builderlab.rs`, `hostedCommunityApi.ts`, `HostedCommunityCreateFlow`)는 손대지 않고 그대로 두었다. hosted 모드에서는 어디에서도 렌더링되지 않아 동작에 영향이 없고, upstream 파일을 수정하지 않는 편이 머지 충돌이 적다. 대신 `hosted_community_check`/`hosted_community_create` 커맨드와 `HostedCommunityCreate` 화면을 `hosted-account` feature에 새로 두었다.
- [x] 생성 화면은 입력 하나. 타이핑 중 정규화(소문자, 공백→하이픈)와 `<이름>.<도메인>` 미리보기, 클라이언트 규칙 검증(길이·문자·하이픈·예약어), 400ms 디바운스된 서버 가용성 표시. 도메인은 계정 서비스 주소(`auth.<도메인>`)에서 파생.
- [x] 생성 성공 시 응답의 릴레이 URL로 upstream add-community 온보딩 트랜잭션을 시작한다(로컬 목록 추가 → 접속 → 프로필 → 스타터 채널 → 환영 채널은 upstream 코드).
- [x] 이미 하나를 만든 계정은 카드가 비활성이고 "Each account can create one community — yours is <host>" 안내가 보인다. 초대 조인은 제한 없음. (새 기기에서 로그인하면 03의 자동 접속이 먼저 동작하고, 그 온보딩을 취소했을 때 이 화면이 보인다.)
- [x] "커뮤니티 추가" 다이얼로그는 hosted 모드에서 같은 생성 화면을 쓴다. 설정의 hosted communities 카드(BuilderLab 계정 관리)는 hosted 모드에서 의미가 없어 06에서 다른 미노출 항목과 함께 숨긴다.
- [x] Playwright smoke(`hosted-login.spec.ts`): 두 카드 → 이름 입력 → 미리보기·가용성·예약어·사용 중 표시 → 생성 → upstream 온보딩 진입과 트랜잭션의 relayUrl, 소유 커뮤니티가 있는 계정의 자동 접속과 두 번째 생성 거부.
