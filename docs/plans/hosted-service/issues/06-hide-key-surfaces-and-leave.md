# 06: 키 관련 화면 미노출과 커뮤니티 나가기

**What to build:** 플래그가 켜져 있으면 사용자는 앱 어디에서도 nsec, npub, 키 내보내기·가져오기, 폰 페어링, "기존 키 사용"을 보지 않는다. 멤버는 커뮤니티에서 나갈 수 있다.

**Blocked by:** 03 (데스크톱 이메일 로그인 온보딩)

**Status:** done

- [x] 플래그가 켜져 있으면 설정의 프로필 카드에서 개인 키 백업 행이 빠지고, 설정 내비게이션에서 Mobile(페어링)과 Hosted communities(BuilderLab) 섹션이 사라진다. 로그아웃 다이얼로그는 nsec을 조회·표시하지 않고 문구 입력만 요구하며, 확인 시 `hosted_logout`(서버 세션 폐기 + upstream sign_out)을 호출한다.
- [x] 릴레이 프로필 온보딩 단계의 "I already have a key" 버튼이 렌더링되지 않는다(`importExistingKey`를 넘기지 않음). "키 다시 가져오기"(identity lost) 경로는 hosted 모드에서 기기 온보딩 단계의 이메일 로그인이 먼저 처리한다.
- [x] 플래그가 꺼져 있으면 upstream 그대로다. 변경은 조건 분기뿐이고 기존 smoke 스펙으로 확인.
- [x] 커뮤니티 나가기는 upstream 커뮤니티 스위처 메뉴의 "Leave community"(kind 28936)가 hosted 모드에서도 그대로 보인다. 나간 뒤 커뮤니티가 없으면 App.tsx 분기에 따라 두 카드 화면(HostedWelcome)이 나온다. upstream `community-rail.spec.ts`가 나가기 동작을 덮는다.
- [x] Playwright smoke(`hosted-login.spec.ts`): hosted 모드에서 설정 내비게이션의 두 섹션 부재, 개인 키 행 부재, 로그아웃 다이얼로그에 nsec 부재와 문구만으로 활성화, 로그아웃 후 세션 삭제. 플래그 꺼짐 시 기존 카드 존재는 upstream smoke 스펙이 덮는다.
