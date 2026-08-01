//! 클라우드 OAuth2 인증(X-37 — [ADR-0006](../../../docs/27-adr-0006-cloud-oauth.md)).
//!
//! **Authorization Code + PKCE**(공용 클라이언트 — `client_secret` 미사용·미동봉).
//! 흐름: verifier/challenge 생성(CNG) → 루프백 리스너(ws2_32) → 기본 브라우저로 인증
//! URL 열기 → `code` 1회 수신 → WinHTTP로 토큰 교환 → 호출자가 DPAPI로 보관.
//!
//! **UI 스레드 금지**: 네트워크·대기가 있으므로 전 과정을 워커에서 수행하고 결과만
//! PostMessage로 통지한다(07-21 SHFileOperation 교훈 계승).
//! 리디렉션은 **127.0.0.1 한정**이라 외부 노출이 없다.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6, TcpListener};
use std::time::{Duration, Instant};

/// 서비스별 엔드포인트·scope(ADR-0006 §2-4).
///
/// `client_id` 해석 순서 = **설정 `cloud_client_id_<종류>` > [`Service::default_client_id`]**
/// (하이브리드 — rclone 등 관례). 기본값 = SosomLab이 등록한 NexaDir 앱 ID로,
/// PKCE 공개 클라이언트라 exe에 동봉돼도 시크릿이 아니다. 다만 **쿼터가 전 사용자
/// 공유**이므로 대량 사용자는 자기 ID로 덮어쓰는 편이 안전하다.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Service {
    pub kind: &'static str,
    pub display: &'static str,
    pub auth_url: &'static str,
    pub token_url: &'static str,
    /// 파일 읽기 + refresh 발급에 필요한 최소 scope(1차 = 읽기 전용).
    pub scope: &'static str,
    /// 계정 표시명 조회 엔드포인트(연결 라벨용). 비면 조회 생략.
    pub me_url: &'static str,
    /// NexaDir 명의 기본 client_id. **빈 문자열 = 아직 미등록**(사용자 설정 필수).
    /// 등록 완료 시 이 상수만 채우면 전 사용자에게 즉시 적용된다(ADR-0006 §2-4).
    pub default_client_id: &'static str,
    /// **Google 전용** — Google은 데스크톱 앱에도 토큰 교환 시 `client_secret`을
    /// 요구한다(PKCE로 대체 불가. MS·Dropbox는 불요). Google 공식 문서가 네이티브
    /// 앱의 이 값을 "비밀이 아니다"라고 명시하므로 client_id와 같은 취급으로 동봉한다.
    /// 빈 문자열 = 전송하지 않음.
    pub default_client_secret: &'static str,
}

/// OneDrive — Entra 앱 등록 **무료**. `Files.Read`(내 파일 읽기)는 사용자 동의만으로
/// 충분해 조직 계정의 관리자 동의 요구를 피한다(`Files.Read.All`은 관리자 동의 필요).
pub const ONEDRIVE: Service = Service {
    kind: "onedrive",
    display: "OneDrive",
    auth_url: "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
    token_url: "https://login.microsoftonline.com/common/oauth2/v2.0/token",
    // X-37 4차(쓰기) — ReadWrite로 상향. 조직 계정도 사용자 동의만으로 충분한
    // "내 파일" 범위이며(`.All`은 관리자 동의 필요), 기존 연결은 **재로그인 1회** 필요.
    scope: "offline_access Files.ReadWrite User.Read",
    me_url: "https://graph.microsoft.com/v1.0/me",
    // SosomLab 등록(08-01) — Entra 앱 "NexaDir". PKCE 공개 클라이언트라 시크릿 아님.
    // 포털 리디렉션 URI 등록값 = `http://localhost`(포트는 MS가 무시 — PREFERRED_PORT 참조).
    default_client_id: "6685a1af-8493-40c5-984f-63c2825d2cdd",
    default_client_secret: "", // MS는 PKCE 공개 클라이언트 — 시크릿 불요
};
/// Google Drive — `drive.readonly`는 **restricted scope**라 CASA 연간 유료 감사
/// (연 $500~4,500)+매년 재검증이 필요하다. 미등록 시 사용자 자기 ID 입력이 유일 경로
/// (ADR-0006 §2-4 — 비용 판단 전까지 기본값 비움).
pub const GOOGLEDRIVE: Service = Service {
    kind: "googledrive",
    display: "Google Drive",
    auth_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    // `drive.file`은 **앱이 만들었거나 Picker로 고른 파일만** 접근 가능해 목록 열거가
    // 불가능하다(탐색기로 성립 안 함) → 읽기·쓰기 전체인 `drive` 사용.
    // 제한된 범위지만 **게시 상태 "테스트"면 CASA 없이** 테스트 사용자(100명)로 동작한다.
    // 대가: 테스트 모드의 refresh 토큰은 **7일 후 만료**(재로그인 필요) — 공개 배포 시
    // CASA 감사 필요(ADR-0006 §2-4).
    scope: "https://www.googleapis.com/auth/drive",
    // 계정 표시명은 **Drive API의 about**으로 얻는다 — `oauth2/v3/userinfo`는
    // `openid`/`userinfo.email` scope를 따로 요구해 콘솔 설정이 늘어난다.
    // `about?fields=user`는 이미 가진 `drive` scope만으로 emailAddress를 준다.
    me_url: "https://www.googleapis.com/drive/v3/about?fields=user",
    // SosomLab 등록(08-01) — GCP 프로젝트 "Nexa Dir" 데스크톱 클라이언트.
    default_client_id: "976847412158-jds0ouedo18itg1fotcuoe0lc00b6oe8.apps.googleusercontent.com",
    // SosomLab 등록(08-01) — GCP "Nexa Dir Desktop" 클라이언트 보안 비밀번호.
    // **의도적으로 공개 값**: Google 공식 문서가 네이티브 앱의 이 값을 "obviously not
    // treated as a secret"이라고 명시하며 소스 동봉을 전제한다(RFC 8252 공개 클라이언트).
    // 이것만으로는 어떤 사용자 데이터에도 접근할 수 없다 — 여전히 그 사용자의 브라우저
    // 로그인 + 해당 세션의 PKCE verifier가 필요하다.
    // 사용자 재정의: 설정 `cloud_client_secret_googledrive`.
    default_client_secret: "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",
};
/// Dropbox — 앱 등록 **무료**. 개발 상태는 최대 500명이나 **50명 연결 시 2주 내
/// 프로덕션 승인 신청**이 필요하다(심사 무료).
pub const DROPBOX: Service = Service {
    kind: "dropbox",
    display: "Dropbox",
    auth_url: "https://www.dropbox.com/oauth2/authorize",
    token_url: "https://api.dropboxapi.com/oauth2/token",
    scope: "files.metadata.read files.content.read account_info.read",
    me_url: "", // Dropbox는 POST 전용이라 1차 생략(라벨 = 서비스명 + 연결 시각)
    default_client_id: "", // TODO(SosomLab 등록): App key — 무료·즉시 가능
    default_client_secret: "", // Dropbox는 PKCE로 시크릿 대체 가능
};

/// 지원 서비스 전체(Connect Cloud 메뉴 순서).
pub const SERVICES: [Service; 3] = [ONEDRIVE, GOOGLEDRIVE, DROPBOX];

/// 종류 → 서비스 정의. **탐색 슬라이스(2차)**가 저장된 연결의 API를 호출할 때 쓴다.
#[allow(dead_code)] // 사용처 = ADR-0006 §3 2차(탐색) — 인증 슬라이스에서는 미호출
pub fn service_of(kind: &str) -> Option<Service> {
    SERVICES.into_iter().find(|s| s.kind == kind)
}

impl Service {
    /// 실효 client_id — **설정 우선, 없으면 NexaDir 기본값**(하이브리드, ADR-0006 §2-4).
    /// 둘 다 비면 빈 문자열 → 호출자가 등록 안내를 띄운다.
    pub fn resolve_client_id<'a>(&self, from_settings: &'a str) -> &'a str
    where
        Self: 'a,
    {
        let s = from_settings.trim();
        if s.is_empty() {
            self.default_client_id
        } else {
            s
        }
    }

    /// 실효 client_secret — 설정 우선, 없으면 기본값. 빈 문자열 = 전송하지 않음.
    pub fn resolve_client_secret<'a>(&self, from_settings: &'a str) -> &'a str
    where
        Self: 'a,
    {
        let s = from_settings.trim();
        if s.is_empty() {
            self.default_client_secret
        } else {
            s
        }
    }
}

/// 인증 성공 결과 — 호출자가 DPAPI로 보관(refresh) + 연결 등록.
#[derive(Clone, Debug, Default)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    /// 만료까지 남은 초(응답 `expires_in`) — 0 = 미제공.
    #[allow(dead_code)] // 사용처 = 2차(탐색) 만료 전 선제 refresh
    pub expires_in: u64,
    /// 계정 표시명(이메일·이름) — 조회 실패 시 빈 문자열.
    pub account: String,
}

/// 인증 실패 사유(사용자 안내용 — 상세는 로그 아님·문자열 1줄).
#[derive(Debug)]
pub enum AuthError {
    /// client_id 미설정(설정에서 입력 필요).
    NoClientId,
    /// 루프백 리스너 생성 실패.
    Listener,
    /// 사용자가 창을 닫거나 시간 초과(기본 5분).
    Timeout,
    /// 사용자가 동의 거부(`error=access_denied` 등).
    Denied(String),
    /// 토큰 교환 실패(네트워크·응답 오류).
    Exchange(String),
}

impl AuthError {
    /// i18n 키(호스트가 tr로 문구화).
    pub fn key(&self) -> &'static str {
        match self {
            AuthError::NoClientId => "cloud.err.noClientId",
            AuthError::Listener => "cloud.err.listener",
            AuthError::Timeout => "cloud.err.timeout",
            AuthError::Denied(_) => "cloud.err.denied",
            AuthError::Exchange(_) => "cloud.err.exchange",
        }
    }
}

/// PKCE `code_verifier`(43~128자 unreserved) — CNG 난수(bcrypt는 std가 이미 임포트).
fn gen_verifier() -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
    let mut buf = [0u8; 64];
    rand_bytes(&mut buf);
    buf.iter().map(|b| CHARS[*b as usize % CHARS.len()] as char).collect()
}

/// 암호학적 난수(BCryptGenRandom — bcryptprimitives는 이미 B3 화이트리스트).
fn rand_bytes(out: &mut [u8]) {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Security::Cryptography::{
            BCryptGenRandom, BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        };
        if BCryptGenRandom(None, out, BCRYPT_USE_SYSTEM_PREFERRED_RNG).is_ok() {
            return;
        }
    }
    // 폴백(비Windows 테스트·CNG 실패) — 시각·주소 기반 혼합(테스트 전용 품질)
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        ^ (out.as_ptr() as u64);
    for b in out.iter_mut() {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        *b = (seed >> 24) as u8;
    }
}

/// SHA-256(CNG) → base64url(패딩 없음) = PKCE `code_challenge`.
fn challenge_of(verifier: &str) -> String {
    base64url(&sha256(verifier.as_bytes()))
}

#[cfg(windows)]
fn sha256(data: &[u8]) -> Vec<u8> {
    use windows::core::w;
    use windows::Win32::Security::Cryptography::{
        BCryptCloseAlgorithmProvider, BCryptCreateHash, BCryptDestroyHash, BCryptFinishHash,
        BCryptHashData, BCryptOpenAlgorithmProvider, BCRYPT_ALG_HANDLE, BCRYPT_HASH_HANDLE,
        BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS,
    };
    unsafe {
        let mut alg = BCRYPT_ALG_HANDLE::default();
        if BCryptOpenAlgorithmProvider(
            &mut alg,
            w!("SHA256"),
            None,
            BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0),
        )
        .is_err()
        {
            return Vec::new();
        }
        let mut hash = BCRYPT_HASH_HANDLE::default();
        let mut out = vec![0u8; 32];
        if BCryptCreateHash(alg, &mut hash, None, None, 0).is_ok() {
            let _ = BCryptHashData(hash, data, 0);
            let _ = BCryptFinishHash(hash, &mut out, 0);
            let _ = BCryptDestroyHash(hash);
        }
        let _ = BCryptCloseAlgorithmProvider(alg, 0);
        out
    }
}

#[cfg(not(windows))]
fn sha256(_data: &[u8]) -> Vec<u8> {
    Vec::new() // 비Windows 빌드는 인증 미지원(UI 자체가 Windows 전용)
}

/// base64url 인코딩(패딩 제거) — crate 0(DR-8).
fn base64url(data: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for c in data.chunks(3) {
        let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        if c.len() > 1 {
            out.push(T[(n >> 6) as usize & 63] as char);
        }
        if c.len() > 2 {
            out.push(T[n as usize & 63] as char);
        }
    }
    out
}

/// URL 퍼센트 인코딩(unreserved 외 전부) — crate 0.
pub fn percent(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 인증 URL 조립(브라우저로 열 주소 — **URL 복사** 제공에도 같은 값을 쓴다).
fn auth_url(svc: &Service, client_id: &str, redirect: &str, challenge: &str, state: &str) -> String {
    let mut u = format!(
        "{}?client_id={}&response_type=code&redirect_uri={}&scope={}&state={}\
         &code_challenge={}&code_challenge_method=S256",
        svc.auth_url,
        percent(client_id),
        percent(redirect),
        percent(svc.scope),
        percent(state),
        challenge
    );
    if svc.kind == "googledrive" {
        // refresh 토큰을 받으려면 명시 필요(Google 규약)
        u.push_str("&access_type=offline&prompt=consent");
    }
    if svc.kind == "dropbox" {
        u.push_str("&token_access_type=offline");
    }
    u
}

/// 루프백 **고정 선호 포트**(08-01 QA — Entra `redirect_uri` 불일치 오류 계기).
///
/// 제공자별 매칭 규칙이 갈린다:
/// - **Microsoft**: `localhost`는 **포트를 무시**하고 매칭(그 외에는 포트까지 일치해야 함).
///   `http` + `127.0.0.1` 등록은 포털 입력란으로 불가(매니페스트 직접 수정 필요).
/// - **Google**(데스크톱 클라이언트): 루프백은 임의 포트 허용.
/// - **Dropbox**: 등록한 URI와 **정확히 일치**해야 한다 → 임의 포트면 매번 실패.
///
/// 세 규칙을 동시에 만족하는 유일한 조합 = **`http://localhost:<고정 포트>/`**.
/// 포트가 점유돼 있으면 임의 포트로 폴백한다(Microsoft·Google은 계속 동작).
pub const PREFERRED_PORT: u16 = 42813;

/// 진행 중 인증 세션 — 브라우저에 띄울 URL과 대기용 리스너를 함께 보유.
/// **URL을 먼저 꺼내 사용자에게 보여줄 수 있다**(프라이빗 창 붙여넣기 — 사용자 요청 08-01).
pub struct AuthSession {
    pub url: String,
    /// 루프백 리스너 — IPv4(127.0.0.1) + 가능하면 IPv6(::1) **동시 대기**.
    /// `localhost`가 시스템에 따라 `::1`로 먼저 해석되므로 한쪽만 열면 브라우저가
    /// 닿지 못할 수 있다(MS가 `127.0.0.1`을 권하는 바로 그 이유).
    listeners: Vec<TcpListener>,
    verifier: String,
    state: String,
    redirect: String,
    svc: Service,
    client_id: String,
    client_secret: String,
}

impl AuthSession {
    /// 루프백 리스너를 열고 인증 URL을 조립한다(네트워크 요청 없음 — 즉시 반환).
    ///
    /// [`PREFERRED_PORT`] 우선(Dropbox 정확 일치 대응) → 점유 시 임의 포트 폴백.
    /// 리디렉션 호스트는 **`localhost`** — Microsoft가 `localhost`에 한해 포트를
    /// 무시하고, `http`+`127.0.0.1`은 포털에서 등록조차 할 수 없기 때문(08-01 QA).
    pub fn begin(
        svc: Service,
        client_id: &str,
        client_secret: &str,
    ) -> Result<AuthSession, AuthError> {
        if client_id.trim().is_empty() {
            return Err(AuthError::NoClientId);
        }
        // 고정 포트 우선 — 실패(점유) 시 OS 임의 할당. 루프백 한정이라 외부 노출 없음.
        let v4 = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, PREFERRED_PORT))
            .or_else(|_| TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
            .map_err(|_| AuthError::Listener)?;
        let port = v4.local_addr().map_err(|_| AuthError::Listener)?.port();
        let mut listeners = vec![v4];
        // `localhost`가 ::1로 먼저 해석되는 환경 대비 — 같은 포트로 IPv6도 시도(실패 무시)
        if let Ok(v6) = TcpListener::bind(SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0)) {
            listeners.push(v6);
        }
        let redirect = format!("http://localhost:{port}/");
        let verifier = gen_verifier();
        let mut sbuf = [0u8; 16];
        rand_bytes(&mut sbuf);
        let state = base64url(&sbuf);
        let url = auth_url(&svc, client_id, &redirect, &challenge_of(&verifier), &state);
        Ok(AuthSession {
            url,
            listeners,
            verifier,
            state,
            redirect,
            svc,
            client_id: client_id.to_string(),
            client_secret: client_secret.to_string(),
        })
    }

    /// 리디렉션 1회 수신 → 토큰 교환까지 **블로킹**(워커 전용).
    /// `timeout` 경과 = [`AuthError::Timeout`](사용자가 창을 닫은 경우 포함).
    pub fn wait_and_exchange(self, timeout: Duration) -> Result<Tokens, AuthError> {
        let code = self.wait_code(timeout)?;
        let mut body = format!(
            "client_id={}&code={}&redirect_uri={}&grant_type=authorization_code&code_verifier={}",
            percent(&self.client_id),
            percent(&code),
            percent(&self.redirect),
            percent(&self.verifier)
        );
        // Google은 데스크톱 앱에도 client_secret을 요구한다(PKCE로 대체 불가).
        // MS·Dropbox는 빈 값이라 전송되지 않는다.
        if !self.client_secret.is_empty() {
            body.push_str(&format!("&client_secret={}", percent(&self.client_secret)));
        }
        let resp = http_post_form(self.svc.token_url, &body)
            .map_err(AuthError::Exchange)?;
        let mut t = Tokens {
            access: json_str(&resp, "access_token").unwrap_or_default(),
            refresh: json_str(&resp, "refresh_token").unwrap_or_default(),
            expires_in: json_str(&resp, "expires_in")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            account: String::new(),
        };
        if t.access.is_empty() {
            let msg = json_str(&resp, "error_description")
                .or_else(|| json_str(&resp, "error"))
                .unwrap_or_else(|| "no access_token".into());
            return Err(AuthError::Exchange(msg));
        }
        if !self.svc.me_url.is_empty() {
            if let Ok(me) = http_get(self.svc.me_url, &t.access) {
                // 서비스마다 키가 다르다: Graph = userPrincipalName/mail,
                // Google Drive about = emailAddress, 기타 = email/displayName.
                t.account = json_str(&me, "userPrincipalName")
                    .or_else(|| json_str(&me, "mail"))
                    .or_else(|| json_str(&me, "emailAddress"))
                    .or_else(|| json_str(&me, "email"))
                    .or_else(|| json_str(&me, "displayName"))
                    .or_else(|| json_str(&me, "name"))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_default();
            }
        }
        Ok(t)
    }

    /// 루프백 리디렉션 1회 수신 → `code` 추출(+`state` 대조). 브라우저에는 안내 응답.
    fn wait_code(&self, timeout: Duration) -> Result<String, AuthError> {
        for l in &self.listeners {
            l.set_nonblocking(true).map_err(|_| AuthError::Listener)?;
        }
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            // IPv4·IPv6 루프백을 번갈아 폴링(먼저 도착한 쪽이 승부)
            let accepted = self
                .listeners
                .iter()
                .find_map(|l| match l.accept() {
                    Ok(p) => Some(Ok(p)),
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
                    Err(e) => Some(Err(e)),
                });
            match accepted {
                Some(Ok((mut stream, _))) => {
                    let mut buf = [0u8; 4096];
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]).to_string();
                    // "GET /?code=…&state=… HTTP/1.1"
                    let line = req.lines().next().unwrap_or("");
                    let q = line.split_whitespace().nth(1).unwrap_or("");
                    let got_state = query_param(q, "state").unwrap_or_default();
                    let code = query_param(q, "code");
                    let err = query_param(q, "error");
                    let ok = code.is_some() && got_state == self.state;
                    let page = if ok {
                        "<h3>Nexa Dir</h3><p>인증이 완료되었습니다. 이 창을 닫으세요.</p>\
                         <p>Authentication complete — you can close this window.</p>"
                    } else {
                        "<h3>Nexa Dir</h3><p>인증에 실패했습니다. 앱으로 돌아가세요.</p>\
                         <p>Authentication failed — return to the app.</p>"
                    };
                    let _ = stream.write_all(
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
                             Content-Length: {}\r\nConnection: close\r\n\r\n{page}",
                            page.len()
                        )
                        .as_bytes(),
                    );
                    let _ = stream.flush();
                    if let Some(e) = err {
                        return Err(AuthError::Denied(e));
                    }
                    if !ok {
                        return Err(AuthError::Denied("state mismatch".into()));
                    }
                    return Ok(code.unwrap_or_default());
                }
                Some(Err(_)) => return Err(AuthError::Listener),
                None => std::thread::sleep(Duration::from_millis(120)),
            }
        }
        Err(AuthError::Timeout)
    }
}

/// refresh 토큰으로 access 재발급(만료 시 무개입 갱신 — ADR-0006 §2-3).
#[allow(dead_code)] // 사용처 = ADR-0006 §3 2차(탐색) — 목록 조회 직전 갱신
pub fn refresh(
    svc: &Service,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<Tokens, AuthError> {
    let mut body = format!(
        "client_id={}&refresh_token={}&grant_type=refresh_token",
        percent(client_id),
        percent(refresh_token)
    );
    if !client_secret.is_empty() {
        body.push_str(&format!("&client_secret={}", percent(client_secret)));
    }
    let resp = http_post_form(svc.token_url, &body).map_err(AuthError::Exchange)?;
    let access = json_str(&resp, "access_token").unwrap_or_default();
    if access.is_empty() {
        return Err(AuthError::Exchange(
            json_str(&resp, "error").unwrap_or_else(|| "refresh failed".into()),
        ));
    }
    Ok(Tokens {
        access,
        // 회전(rotation) 미제공 서비스는 기존 refresh 유지
        refresh: json_str(&resp, "refresh_token").unwrap_or_else(|| refresh_token.to_string()),
        expires_in: json_str(&resp, "expires_in")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        account: String::new(),
    })
}

/// 쿼리 문자열에서 파라미터 1개(퍼센트 디코드).
fn query_param(q: &str, key: &str) -> Option<String> {
    let qs = q.split_once('?')?.1;
    for pair in qs.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == key {
                return Some(percent_decode(v));
            }
        }
    }
    None
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// JSON 최상위(또는 임의 위치)의 `"key": "값"` / `"key": 숫자` 추출 — 최소 수제 파서(DR-8).
/// 중첩·배열은 다루지 않는다(토큰 응답·userinfo에 충분).
pub fn json_str(json: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let rest = &json[json.find(&pat)? + pat.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    if let Some(body) = rest.strip_prefix('"') {
        let mut out = String::new();
        let mut esc = false;
        for ch in body.chars() {
            if esc {
                out.push(match ch {
                    'n' => '\n',
                    't' => '\t',
                    other => other,
                });
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                return Some(out);
            } else {
                out.push(ch);
            }
        }
        None
    } else {
        let end = rest
            .find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')
            .unwrap_or(rest.len());
        (end > 0).then(|| rest[..end].to_string())
    }
}

/// JSON 배열 `"key": [ {...}, {...} ]`의 **원소 객체들을 문자열로 분리**(X-37 2차).
/// 중첩 중괄호·문자열 안 괄호를 존중하는 최소 스캐너 — 각 원소는 [`json_str`]로 읽는다.
/// 배열이 없거나 형식 밖이면 빈 Vec(관용 — crate 0 유지, DR-8).
pub fn json_objects(json: &str, key: &str) -> Vec<String> {
    let mut out = Vec::new();
    let pat = format!("\"{key}\"");
    let Some(p) = json.find(&pat) else { return out };
    let rest = &json[p + pat.len()..];
    let Some(open) = rest.find('[') else { return out };
    let body = &rest[open + 1..];
    let (mut depth, mut start, mut in_str, mut esc) = (0i32, 0usize, false, false);
    for (i, ch) in body.char_indices() {
        if in_str {
            if esc {
                esc = false;
            } else if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                in_str = false;
            }
            continue;
        }
        match ch {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    start = i;
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    out.push(body[start..=i].to_string());
                }
            }
            ']' if depth == 0 => break, // 배열 종료
            _ => {}
        }
    }
    out
}

/// 객체에 해당 키가 **존재**하는가(값 무관 — Graph의 `folder`/`file` 판별용).
pub fn json_has(json: &str, key: &str) -> bool {
    json.contains(&format!("\"{key}\""))
}

// ── HTTP(WinHTTP — ADR-0006 §2-2. TLS는 schannel 위임) ────────────────────────

/// `application/x-www-form-urlencoded` POST → 응답 본문(UTF-8).
#[cfg(windows)]
pub fn http_post_form(url: &str, body: &str) -> Result<String, String> {
    winhttp_request(
        url,
        "POST",
        Some(body.as_bytes()),
        Some("application/x-www-form-urlencoded"),
        "",
        None,
        TEXT_LIMIT,
        None,
        None,
    )
    .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// Bearer GET → 응답 본문(UTF-8).
#[cfg(windows)]
pub fn http_get(url: &str, bearer: &str) -> Result<String, String> {
    winhttp_request(url, "GET", None, None, "", Some(bearer), TEXT_LIMIT, None, None)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// GET → **원본 바이트**(X-37 3차 다운로드 — 이진 파일 무손상).
/// `bearer`가 `None`이면 인증 헤더를 붙이지 않는다 — Graph의 `downloadUrl`은
/// **사전 인증된 URL**이라 Authorization을 함께 보내면 거부될 수 있다.
#[cfg(windows)]
#[allow(dead_code)] // 진행 통지 없는 단순 GET — Dropbox content API 등 후속 사용처 대비
pub fn http_get_bytes(url: &str, bearer: Option<&str>, limit: usize) -> Result<Vec<u8>, String> {
    winhttp_request(url, "GET", None, None, "", bearer, limit, None, None)
}

/// 진행 통지가 붙은 GET(X-37 진행률) — 콜백이 `false`를 돌리면 [`CANCELLED`] 오류.
#[cfg(windows)]
pub fn http_get_bytes_progress(
    url: &str,
    bearer: Option<&str>,
    limit: usize,
    on_progress: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Vec<u8>, String> {
    winhttp_request(url, "GET", None, None, "", bearer, limit, Some(on_progress), None)
}

#[cfg(not(windows))]
pub fn http_get_bytes_progress(
    _u: &str,
    _b: Option<&str>,
    _l: usize,
    _p: &mut dyn FnMut(u64, u64) -> bool,
) -> Result<Vec<u8>, String> {
    Err("windows only".into())
}

/// 임의 메서드 + 바이너리/JSON 본문(X-37 4차 쓰기 — PUT/POST/PATCH/DELETE).
/// `extra`는 CRLF로 끝나는 추가 헤더 문자열(업로드 `Content-Range` 등).
#[cfg(windows)]
pub fn http_send(
    url: &str,
    method: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    extra: &str,
    bearer: Option<&str>,
) -> Result<String, String> {
    winhttp_request(url, method, body, content_type, extra, bearer, TEXT_LIMIT, None, None)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// 재개 업로드 **세션 생성** — 응답 `Location` 헤더(세션 URI)를 돌려준다.
#[cfg(windows)]
pub fn http_start_session(
    url: &str,
    body: &[u8],
    content_type: &str,
    bearer: &str,
) -> Result<String, String> {
    let mut loc = String::new();
    winhttp_request(
        url,
        "POST",
        Some(body),
        Some(content_type),
        "",
        Some(bearer),
        TEXT_LIMIT,
        None,
        Some(&mut loc),
    )?;
    if loc.is_empty() {
        return Err("no upload session Location".into());
    }
    Ok(loc)
}

#[cfg(not(windows))]
pub fn http_start_session(_u: &str, _b: &[u8], _c: &str, _r: &str) -> Result<String, String> {
    Err("windows only".into())
}

/// 임의 메서드 → **원본 바이트**(Dropbox content API처럼 본문이 이진인 응답).
#[cfg(windows)]
pub fn http_send_bytes(
    url: &str,
    method: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    extra: &str,
    bearer: Option<&str>,
    limit: usize,
) -> Result<Vec<u8>, String> {
    winhttp_request(url, method, body, content_type, extra, bearer, limit, None, None)
}

#[cfg(not(windows))]
pub fn http_send(
    _u: &str,
    _m: &str,
    _b: Option<&[u8]>,
    _c: Option<&str>,
    _e: &str,
    _r: Option<&str>,
) -> Result<String, String> {
    Err("windows only".into())
}

#[cfg(not(windows))]
#[allow(clippy::too_many_arguments)]
pub fn http_send_bytes(
    _u: &str,
    _m: &str,
    _b: Option<&[u8]>,
    _c: Option<&str>,
    _e: &str,
    _r: Option<&str>,
    _l: usize,
) -> Result<Vec<u8>, String> {
    Err("windows only".into())
}

#[cfg(not(windows))]
pub fn http_get_bytes(_u: &str, _b: Option<&str>, _l: usize) -> Result<Vec<u8>, String> {
    Err("windows only".into())
}

/// 사용자 취소를 나타내는 오류 문자열(호출자가 실패와 구분해 조용히 마감).
pub const CANCELLED: &str = "__nexa_cancelled__";

/// 텍스트 응답(토큰·JSON 메타) 상한 — 폭주 방어.
#[cfg(windows)]
const TEXT_LIMIT: usize = 8 * 1024 * 1024;

#[cfg(not(windows))]
pub fn http_post_form(_url: &str, _body: &str) -> Result<String, String> {
    Err("windows only".into())
}
#[cfg(not(windows))]
pub fn http_get(_url: &str, _bearer: &str) -> Result<String, String> {
    Err("windows only".into())
}

#[cfg(windows)]
#[allow(clippy::too_many_arguments)]
fn winhttp_request(
    url: &str,
    method: &str,
    body: Option<&[u8]>,
    content_type: Option<&str>,
    extra_headers: &str,
    bearer: Option<&str>,
    limit: usize,
    // 청크마다 (누적 바이트, 전체 바이트[0=미상])을 통지. `false` 반환 = 취소.
    on_progress: Option<&mut dyn FnMut(u64, u64) -> bool>,
    // 응답 `Location` 헤더를 받아갈 자리(재개 업로드 세션 URI).
    out_location: Option<&mut String>,
) -> Result<Vec<u8>, String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Networking::WinHttp::{
        WinHttpConnect, WinHttpCrackUrl, WinHttpOpen, WinHttpOpenRequest,
        WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData, WinHttpReceiveResponse,
        WinHttpSendRequest, URL_COMPONENTS, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
        WINHTTP_FLAG_SECURE, WINHTTP_QUERY_CONTENT_LENGTH, WINHTTP_QUERY_FLAG_NUMBER,
        WINHTTP_QUERY_LOCATION, WINHTTP_QUERY_STATUS_CODE,
    };
    unsafe {
        let wurl = HSTRING::from(url);
        // URL 분해(호스트/경로/포트) — 수제 파싱 대신 WinHTTP 제공 함수 사용
        let mut host = [0u16; 256];
        let mut path = [0u16; 2048];
        let mut uc = URL_COMPONENTS {
            dwStructSize: std::mem::size_of::<URL_COMPONENTS>() as u32,
            lpszHostName: windows::core::PWSTR(host.as_mut_ptr()),
            dwHostNameLength: host.len() as u32,
            lpszUrlPath: windows::core::PWSTR(path.as_mut_ptr()),
            dwUrlPathLength: path.len() as u32,
            ..Default::default()
        };
        let wide: Vec<u16> = url.encode_utf16().collect();
        WinHttpCrackUrl(&wide, 0, &mut uc).map_err(|e| format!("url: {e}"))?;
        let _ = &wurl;
        let host_s = String::from_utf16_lossy(&host[..uc.dwHostNameLength as usize]);
        let path_s = String::from_utf16_lossy(&path[..uc.dwUrlPathLength as usize]);

        let agent = HSTRING::from("NexaDir");
        let sess = WinHttpOpen(
            PCWSTR(agent.as_ptr()),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            PCWSTR::null(),
            PCWSTR::null(),
            0,
        );
        if sess.is_null() {
            return Err("WinHttpOpen".into());
        }
        let guard = HandleGuard(sess);
        let hostw = HSTRING::from(host_s.as_str());
        let conn = WinHttpConnect(sess, PCWSTR(hostw.as_ptr()), uc.nPort, 0);
        if conn.is_null() {
            return Err("WinHttpConnect".into());
        }
        let cguard = HandleGuard(conn);
        let mw = HSTRING::from(method);
        let pw = HSTRING::from(path_s.as_str());
        let req = WinHttpOpenRequest(
            conn,
            PCWSTR(mw.as_ptr()),
            PCWSTR(pw.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            std::ptr::null(),
            WINHTTP_FLAG_SECURE,
        );
        if req.is_null() {
            return Err("WinHttpOpenRequest".into());
        }
        let rguard = HandleGuard(req);
        let mut headers = String::new();
        if let Some(ct) = content_type {
            headers.push_str(&format!("Content-Type: {ct}\r\n"));
        }
        if let Some(b) = bearer {
            headers.push_str(&format!("Authorization: Bearer {b}\r\n"));
        }
        headers.push_str(extra_headers);
        let hw: Vec<u16> = headers.encode_utf16().collect();
        let body_bytes: &[u8] = body.unwrap_or(&[]);
        WinHttpSendRequest(
            req,
            if hw.is_empty() { None } else { Some(&hw) },
            Some(body_bytes.as_ptr() as *const core::ffi::c_void),
            body_bytes.len() as u32,
            body_bytes.len() as u32,
            0,
        )
        .map_err(|e| format!("send: {e}"))?;
        WinHttpReceiveResponse(req, std::ptr::null_mut()).map_err(|e| format!("recv: {e}"))?;
        // 상태 코드(비2xx는 본문에 실린 오류 설명과 함께 실패로 돌린다)
        let mut status: u32 = 0;
        let mut sz = std::mem::size_of::<u32>() as u32;
        let _ = WinHttpQueryHeaders(
            req,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut status as *mut u32 as *mut core::ffi::c_void),
            &mut sz,
            std::ptr::null_mut(),
        );
        // 전체 크기(Content-Length) — 없으면 0(진행률은 불확정 표시)
        let mut total: u64 = 0;
        {
            let mut clen: u32 = 0;
            let mut csz = std::mem::size_of::<u32>() as u32;
            if WinHttpQueryHeaders(
                req,
                WINHTTP_QUERY_CONTENT_LENGTH | WINHTTP_QUERY_FLAG_NUMBER,
                PCWSTR::null(),
                Some(&mut clen as *mut u32 as *mut core::ffi::c_void),
                &mut csz,
                std::ptr::null_mut(),
            )
            .is_ok()
            {
                total = clen as u64;
            }
        }
        // Location 헤더(재개 업로드 세션 URI) — 요청한 경우에만 조회
        if let Some(slot) = out_location {
            let mut buf = [0u16; 2048];
            let mut sz = (buf.len() * 2) as u32;
            if WinHttpQueryHeaders(
                req,
                WINHTTP_QUERY_LOCATION,
                PCWSTR::null(),
                Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
                &mut sz,
                std::ptr::null_mut(),
            )
            .is_ok()
            {
                let n = (sz as usize / 2).min(buf.len());
                *slot = String::from_utf16_lossy(&buf[..n]);
            }
        }
        let mut progress = on_progress;
        let mut out = Vec::new();
        loop {
            let mut avail = 0u32;
            if WinHttpQueryDataAvailable(req, &mut avail).is_err() || avail == 0 {
                break;
            }
            let mut chunk = vec![0u8; avail as usize];
            let mut read = 0u32;
            if WinHttpReadData(
                req,
                chunk.as_mut_ptr() as *mut core::ffi::c_void,
                avail,
                &mut read,
            )
            .is_err()
            {
                break;
            }
            chunk.truncate(read as usize);
            out.extend_from_slice(&chunk);
            if out.len() > limit {
                return Err("response too large".into()); // 상한 초과 = 명시 실패
            }
            if let Some(cb) = progress.as_deref_mut() {
                if !cb(out.len() as u64, total) {
                    return Err(CANCELLED.into()); // 사용자 취소 — 호출자가 식별
                }
            }
        }
        drop((rguard, cguard, guard));
        if !(200..300).contains(&status) {
            let body = String::from_utf8_lossy(&out);
            let msg = json_str(&body, "message")
                .or_else(|| json_str(&body, "error_description"))
                .unwrap_or_else(|| body.chars().take(200).collect());
            return Err(format!("HTTP {status}: {msg}"));
        }
        Ok(out)
    }
}

/// WinHTTP 핸들 RAII(조기 반환 경로의 누수 차단).
#[cfg(windows)]
struct HandleGuard(*mut core::ffi::c_void);
#[cfg(windows)]
impl Drop for HandleGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = windows::Win32::Networking::WinHttp::WinHttpCloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_no_padding() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
        assert!(!base64url(&[251, 255, 190]).contains('+'));
    }

    #[test]
    fn percent_encode_and_decode_roundtrip() {
        let s = "a b/c?d=e&f~g_h";
        assert_eq!(percent_decode(&percent(s)), s);
        assert_eq!(percent("~_-."), "~_-.");
        assert_eq!(percent(" "), "%20");
    }

    #[test]
    fn verifier_is_pkce_compliant() {
        let v = gen_verifier();
        assert!((43..=128).contains(&v.len()), "len={}", v.len());
        assert!(v
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
        assert_ne!(gen_verifier(), gen_verifier(), "매 호출 달라야 함");
    }

    #[test]
    fn json_extracts_strings_and_numbers() {
        let j = r#"{"access_token":"abc.def","expires_in":3599,"scope":"a b","refresh_token":"r\"x"}"#;
        assert_eq!(json_str(j, "access_token").as_deref(), Some("abc.def"));
        assert_eq!(json_str(j, "expires_in").as_deref(), Some("3599"));
        assert_eq!(json_str(j, "refresh_token").as_deref(), Some("r\"x"));
        assert_eq!(json_str(j, "missing"), None);
    }

    #[test]
    fn query_param_parses_redirect() {
        let q = "/?code=A%2FB&state=xyz";
        assert_eq!(query_param(q, "code").as_deref(), Some("A/B"));
        assert_eq!(query_param(q, "state").as_deref(), Some("xyz"));
        assert_eq!(query_param(q, "error"), None);
    }

    /// 08-01 QA 회귀: 리디렉션은 **`localhost`**(Microsoft가 이 호스트에 한해 포트를
    /// 무시하고, `http`+`127.0.0.1`은 포털 등록 자체가 불가) + 고정 선호 포트
    /// (Dropbox 정확 일치 대응). 호스트가 `127.0.0.1`로 회귀하면 Entra가
    /// `invalid_request: redirect_uri is not valid`로 거부한다.
    #[test]
    fn redirect_uri_uses_localhost_and_preferred_port() {
        let s = AuthSession::begin(ONEDRIVE, "cid", "").expect("루프백 리스너");
        assert!(
            s.redirect.starts_with("http://localhost:"),
            "redirect={}",
            s.redirect
        );
        assert!(s.redirect.ends_with('/'), "경로 없는 URI는 슬래시 종단");
        assert!(
            !s.redirect.contains("127.0.0.1"),
            "127.0.0.1은 포털 등록 불가 — 회귀 금지"
        );
        // 인증 URL에도 인코딩되어 실려야 한다
        assert!(s.url.contains(&percent(&s.redirect)));
        // 선호 포트가 비어 있으면 그 포트를 쓴다(점유 시 폴백은 허용)
        let port: u16 = s.redirect["http://localhost:".len()..]
            .trim_end_matches('/')
            .parse()
            .expect("포트 파싱");
        assert!(port > 0);
    }

    #[test]
    fn auth_url_has_pkce_and_service_extras() {
        let u = auth_url(&GOOGLEDRIVE, "cid", "http://127.0.0.1:1/", "chal", "st");
        assert!(u.starts_with(GOOGLEDRIVE.auth_url));
        assert!(u.contains("code_challenge=chal"));
        assert!(u.contains("code_challenge_method=S256"));
        assert!(u.contains("client_id=cid"));
        assert!(u.contains("access_type=offline"), "Google refresh 필수 파라미터");
        assert!(!u.contains("client_secret"), "PKCE = 시크릿 미사용");
        let d = auth_url(&DROPBOX, "cid", "http://127.0.0.1:1/", "c", "s");
        assert!(d.contains("token_access_type=offline"));
    }

    /// X-37 2차: Graph 응답의 `value` 배열 원소 분리(중첩·문자열 안 괄호 존중).
    #[test]
    fn json_objects_splits_nested_array() {
        let j = r#"{"value":[
            {"id":"1","name":"Docs","folder":{"childCount":3}},
            {"id":"2","name":"a}b.txt","file":{"mimeType":"text/plain"},"size":10}
        ],"@odata.nextLink":"x"}"#;
        let objs = json_objects(j, "value");
        assert_eq!(objs.len(), 2, "원소 2개 — 중첩 객체가 경계를 깨지 않음");
        assert_eq!(json_str(&objs[0], "name").as_deref(), Some("Docs"));
        assert!(json_has(&objs[0], "folder") && !json_has(&objs[0], "file"));
        assert_eq!(
            json_str(&objs[1], "name").as_deref(),
            Some("a}b.txt"),
            "문자열 안 중괄호 무시"
        );
        assert!(json_has(&objs[1], "file"));
        assert_eq!(json_str(&objs[1], "size").as_deref(), Some("10"));
        assert!(json_objects("{}", "value").is_empty(), "배열 부재 = 빈 Vec");
    }

    #[test]
    fn service_lookup() {
        assert_eq!(service_of("onedrive").map(|s| s.display), Some("OneDrive"));
        assert_eq!(service_of("dropbox").map(|s| s.kind), Some("dropbox"));
        assert!(service_of("nope").is_none());
    }

    /// 서비스 정의 무결성 — 엔드포인트가 HTTPS이고 scope에 읽기 권한이 있어야 한다.
    #[test]
    fn services_are_https_and_scoped() {
        for s in SERVICES {
            assert!(s.auth_url.starts_with("https://"), "{}", s.kind);
            assert!(s.token_url.starts_with("https://"), "{}", s.kind);
            assert!(!s.scope.is_empty(), "{}", s.kind);
        }
    }

    /// client_id 해석 = **설정 우선 → NexaDir 기본값**(ADR-0006 §2-4 하이브리드).
    #[test]
    fn client_id_resolution_prefers_settings_then_default() {
        let svc = Service {
            default_client_id: "nexadir-default",
            ..ONEDRIVE
        };
        assert_eq!(svc.resolve_client_id("user-own"), "user-own", "설정 우선");
        assert_eq!(svc.resolve_client_id("  "), "nexadir-default", "공백 = 미설정");
        assert_eq!(svc.resolve_client_id(""), "nexadir-default", "빈 값 = 기본");
        // 기본값도 비면 빈 문자열 → 호출자가 등록 안내
        let none = Service {
            default_client_id: "",
            ..ONEDRIVE
        };
        assert_eq!(none.resolve_client_id(""), "", "둘 다 없음 = 안내 경로");
    }
}
