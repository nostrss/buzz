# 03: 데스크톱 이메일 로그인 온보딩 (플래그 분기)

**What to build:** 계정 서비스 주소가 빌드에 들어 있으면 앱의 첫 화면이 "이메일로 시작"이다. 이메일과 코드를 넣으면 키 관련 화면 없이 릴레이에 접속되고, 커뮤니티가 없으면 "커뮤니티 만들기 / 초대 링크 붙여넣기" 화면이 나온다. 초대 링크로 앱을 열었다면 로그인이 끝나자마자 자동으로 조인된다. 주소가 없으면 upstream 온보딩이 그대로 나온다.

**Blocked by:** 02 (이메일 + 6자리 코드 로그인 API)

**Status:** done

- [x] 빌드 환경변수 하나(`VITE_BUZZ_ACCOUNT_URL`)로 분기한다. 분기 지점은 App.tsx의 기기 온보딩 진입과 커뮤니티 없음 화면 두 곳이며, 우리 화면은 `features/hosted-account/`의 새 파일에 둔다. 설정에서 machine config를 다시 열면 upstream 흐름이 나온다.
- [x] 이메일 입력 화면: 이메일 형식 검증, "코드 보내기", 분당 1회 안내 표시. (서버 오류 코드를 문구로 변환.)
- [x] 코드 입력 화면: 6자리 입력, 틀리면 남은 횟수 표시, 만료·폐기 시 "코드 다시 보내기".
- [x] Tauri 커맨드(`hosted_login_start/verify`, `hosted_session_me`, `hosted_logout`)가 계정 서비스를 호출한다. nsec은 Rust 안에서 `commit_imported_identity`로 키체인에 커밋되어 webview로 나가지 않으며, 기존 키를 덮어쓴다. 세션 토큰은 같은 키체인 블롭의 `hosted-account-session` 항목에 저장된다.
- [x] 앱 재시작 시 키체인의 세션으로 `/v1/me`를 불러 로그인 화면을 건너뛴다. 401이면 세션을 지우고 이메일 화면으로 돌아간다.
- [x] 로그인 후 커뮤니티 목록이 있고 이 기기에 커뮤니티가 없으면 upstream add-community 흐름으로 첫 커뮤니티에 접속하고, 없으면 "만들기 / 초대 링크" 두 카드 화면을 보여준다. "만들기" 카드는 비활성. (목록 접속 경로는 mock이 빈 목록만 돌려줘 e2e 미검증.)
- [x] 초대 링크 붙여넣기는 upstream 조인 흐름을 그대로 탄다. 초대 링크로 앱을 연 경우 기존 대기 게이트가 로그인 뒤 자동 조인을 이어간다.
- [x] 로그아웃은 서버 세션을 폐기(best effort)한 뒤 upstream `sign_out`으로 키체인 블롭(세션·신원 키)을 지우고 첫 실행으로 재시작한다.
- [x] 플래그가 없으면 upstream 온보딩이 그대로 동작한다(deep-link-invite, identity-key-help smoke 스펙 통과).
- [x] mock bridge에 계정 서비스 커맨드 스텁을 추가하고 `hosted-login.spec.ts`(smoke)로 이메일 → 코드 → 두 카드 화면, 코드 오류 표시, 세션 유지, 로그아웃, 초대 링크 자동 조인, 키 관련 문구 부재, 플래그 꺼짐 시 upstream 온보딩을 검증한다.
