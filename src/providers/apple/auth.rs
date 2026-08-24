use std::{path::Path, time::Duration};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use p256::{
    ecdsa::{Signature, SigningKey, signature::Signer},
    pkcs8::DecodePrivateKey,
};
use serde::{Deserialize, Serialize};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

use crate::{
    ChordriftError, Result,
    credentials::{CredentialStore, SecretId, SystemCredentialStore},
};

use super::client::AppleMusicClient;

const AUTH_HOST: &str = "127.0.0.1";
const AUTH_PORT: u16 = 8889;
const AUTH_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const DEVELOPER_TOKEN_LIFETIME_SECONDS: i64 = 15 * 60;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct DeveloperCredential {
    team_id: String,
    key_id: String,
    private_key_pem: String,
}

/// Result of securely importing an Apple Media Services private key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SetupReport {
    /// Apple Developer team identifier.
    pub team_id: String,
    /// Media Services key identifier.
    pub key_id: String,
}

/// Result of Apple Music subscriber authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthReport {
    /// Local Chordrift account label.
    pub account_label: String,
    /// Apple Music storefront returned for the subscriber.
    pub storefront: String,
}

/// Result of verifying Apple developer and subscriber authorization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthStatus {
    /// Local Chordrift account label.
    pub account_label: String,
    /// Configured Apple Developer team identifier.
    pub team_id: String,
    /// Configured Media Services key identifier.
    pub key_id: String,
    /// Whether a Music User Token is present and valid.
    pub user_authorized: bool,
    /// Subscriber storefront when user authorization is present.
    pub storefront: Option<String>,
}

/// Developer-authenticated Apple Music catalog session.
pub(crate) struct DeveloperSession {
    pub client: AppleMusicClient,
    pub developer_token: String,
    pub team_id: String,
    pub key_id: String,
}

/// Imports a downloaded Apple `.p8` key into macOS Passwords/Keychain.
pub fn configure(team_id: &str, key_id: &str, private_key_path: &Path) -> Result<SetupReport> {
    validate_identifier("Team ID", team_id)?;
    validate_identifier("Key ID", key_id)?;
    let private_key_pem = std::fs::read_to_string(private_key_path).map_err(|error| {
        ChordriftError::Configuration(format!(
            "could not read Apple private key {}: {error}",
            private_key_path.display()
        ))
    })?;
    parse_signing_key(&private_key_pem)?;
    let credential = DeveloperCredential {
        team_id: team_id.to_owned(),
        key_id: key_id.to_owned(),
        private_key_pem,
    };
    SystemCredentialStore.save(
        &developer_credential_id()?,
        &serde_json::to_vec(&credential)?,
    )?;
    Ok(SetupReport {
        team_id: team_id.to_owned(),
        key_id: key_id.to_owned(),
    })
}

/// Opens a local MusicKit-on-the-Web authorization page and stores its user token.
pub async fn authenticate(account_label: &str) -> Result<AuthReport> {
    let session = developer_session(None).await?;
    let listener = TcpListener::bind((AUTH_HOST, AUTH_PORT))
        .await
        .map_err(|error| {
            ChordriftError::Configuration(format!(
                "could not listen for Apple Music authorization on {AUTH_HOST}:{AUTH_PORT}: {error}"
            ))
        })?;
    let url = format!("http://{AUTH_HOST}:{AUTH_PORT}/");
    println!(
        "Open this Apple Music authorization URL if the browser does not open automatically:\n{url}"
    );
    let _ = webbrowser::open(&url);
    eprintln!("Waiting for Apple Music authorization...");
    let user_token = timeout(
        AUTH_TIMEOUT,
        receive_user_token(listener, &session.developer_token),
    )
    .await
    .map_err(|_| {
        ChordriftError::Configuration(
            "Apple Music authorization timed out after ten minutes".to_owned(),
        )
    })??;

    let client = AppleMusicClient::new(session.developer_token, Some(user_token.clone()))?;
    let storefront = client.storefront().await?;
    SystemCredentialStore.save(&user_credential_id(account_label)?, user_token.as_bytes())?;
    Ok(AuthReport {
        account_label: account_label.to_owned(),
        storefront: storefront.id,
    })
}

/// Validates developer access and any stored subscriber authorization.
pub async fn status(account_label: &str) -> Result<AuthStatus> {
    let user_token = load_user_token(account_label)?;
    let session = developer_session(user_token.clone()).await?;
    let storefront = if user_token.is_some() {
        Some(session.client.storefront().await?.id)
    } else {
        None
    };
    Ok(AuthStatus {
        account_label: account_label.to_owned(),
        team_id: session.team_id,
        key_id: session.key_id,
        user_authorized: storefront.is_some(),
        storefront,
    })
}

/// Removes a local Music User Token without revoking the developer key.
pub fn logout(account_label: &str) -> Result<bool> {
    SystemCredentialStore.delete(&user_credential_id(account_label)?)
}

pub(crate) async fn developer_session(
    music_user_token: Option<String>,
) -> Result<DeveloperSession> {
    let credential = load_developer_credential()?;
    let developer_token = developer_token(&credential)?;
    let client = AppleMusicClient::new(developer_token.clone(), music_user_token)?;
    client.test().await?;
    Ok(DeveloperSession {
        client,
        developer_token,
        team_id: credential.team_id,
        key_id: credential.key_id,
    })
}

fn load_developer_credential() -> Result<DeveloperCredential> {
    let bytes = SystemCredentialStore
        .load(&developer_credential_id()?)?
        .ok_or_else(|| {
            ChordriftError::Configuration(
                "no Apple developer credential is stored; run `chordrift apple configure`"
                    .to_owned(),
            )
        })?;
    serde_json::from_slice(&bytes).map_err(|_| {
        ChordriftError::Configuration(
            "stored Apple developer credential is invalid; configure it again".to_owned(),
        )
    })
}

fn load_user_token(account_label: &str) -> Result<Option<String>> {
    SystemCredentialStore
        .load(&user_credential_id(account_label)?)?
        .map(|bytes| {
            String::from_utf8(bytes).map_err(|_| {
                ChordriftError::Configuration(
                    "stored Apple Music user credential is invalid; authorize again".to_owned(),
                )
            })
        })
        .transpose()
}

fn developer_credential_id() -> Result<SecretId> {
    SecretId::new("apple_music", "chordrift", "developer")
}

fn user_credential_id(account_label: &str) -> Result<SecretId> {
    SecretId::new("apple_music", account_label, "music_user_token")
}

fn validate_identifier(label: &str, value: &str) -> Result<()> {
    if value.len() != 10
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(ChordriftError::Configuration(format!(
            "Apple {label} must contain exactly 10 ASCII letters or digits"
        )));
    }
    Ok(())
}

fn parse_signing_key(private_key_pem: &str) -> Result<SigningKey> {
    SigningKey::from_pkcs8_pem(private_key_pem).map_err(|_| {
        ChordriftError::Configuration(
            "Apple private key is not a valid P-256 PKCS#8 `.p8` key".to_owned(),
        )
    })
}

fn developer_token(credential: &DeveloperCredential) -> Result<String> {
    let now = Utc::now().timestamp();
    let header = serde_json::json!({
        "alg": "ES256",
        "kid": credential.key_id,
        "typ": "JWT"
    });
    let claims = serde_json::json!({
        "iss": credential.team_id,
        "iat": now,
        "exp": now + DEVELOPER_TOKEN_LIFETIME_SECONDS
    });
    let encoded_header = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?);
    let encoded_claims = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims)?);
    let signing_input = format!("{encoded_header}.{encoded_claims}");
    let signature: Signature =
        parse_signing_key(&credential.private_key_pem)?.sign(signing_input.as_bytes());
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    ))
}

async fn receive_user_token(listener: TcpListener, developer_token: &str) -> Result<String> {
    loop {
        let (mut stream, _) = listener.accept().await?;
        let request = read_request(&mut stream).await?;
        if request.method == "GET" && request.path == "/" {
            write_html(&mut stream, 200, &authorization_page(developer_token)).await?;
            continue;
        }
        if request.method == "POST" && request.path == "/token" {
            let token = String::from_utf8(request.body).map_err(|_| {
                ChordriftError::Configuration(
                    "Apple Music returned an invalid user token".to_owned(),
                )
            })?;
            if token.len() < 20 || token.contains(char::is_whitespace) {
                write_html(&mut stream, 400, "Invalid Apple Music user token").await?;
                continue;
            }
            write_html(
                &mut stream,
                200,
                "Apple Music authorization received. You may close this window.",
            )
            .await?;
            return Ok(token);
        }
        write_html(&mut stream, 404, "Not found").await?;
    }
}

struct HttpRequest {
    method: String,
    path: String,
    body: Vec<u8>,
}

async fn read_request(stream: &mut TcpStream) -> Result<HttpRequest> {
    let mut bytes = Vec::with_capacity(4096);
    let mut chunk = [0_u8; 2048];
    let header_end = loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(ChordriftError::Configuration(
                "Apple Music authorization browser closed the request early".to_owned(),
            ));
        }
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > 32 * 1024 {
            return Err(ChordriftError::Configuration(
                "Apple Music authorization request was unexpectedly large".to_owned(),
            ));
        }
        if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
            break position + 4;
        }
    };
    let headers = String::from_utf8_lossy(&bytes[..header_end]);
    let mut lines = headers.lines();
    let mut request_line = lines.next().unwrap_or_default().split_ascii_whitespace();
    let method = request_line.next().unwrap_or_default().to_owned();
    let path = request_line.next().unwrap_or_default().to_owned();
    let content_length = lines
        .find_map(|line| {
            line.split_once(':').and_then(|(name, value)| {
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
        })
        .unwrap_or(0);
    if content_length > 16 * 1024 {
        return Err(ChordriftError::Configuration(
            "Apple Music user token was unexpectedly large".to_owned(),
        ));
    }
    while bytes.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    if bytes.len() < header_end + content_length {
        return Err(ChordriftError::Configuration(
            "Apple Music authorization request had an incomplete body".to_owned(),
        ));
    }
    Ok(HttpRequest {
        method,
        path,
        body: bytes[header_end..header_end + content_length].to_vec(),
    })
}

fn authorization_page(developer_token: &str) -> String {
    let token = serde_json::to_string(developer_token).expect("JWT is JSON-safe");
    format!(
        r#"<!doctype html>
<html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<title>Authorize Chordrift</title>
<script src="https://js-cdn.music.apple.com/musickit/v3/musickit.js" async></script>
<style>body{{font:16px system-ui;max-width:42rem;margin:5rem auto;padding:0 1.5rem}}button{{font:inherit;padding:.8rem 1.2rem}}#status{{margin-top:1rem}}</style>
<h1>Authorize Chordrift</h1>
<p>Allow read-only access to your Apple Music library. Chordrift v0.0.5 does not modify Apple Music.</p>
<button id="authorize" disabled>Continue with Apple Music</button><p id="status">Loading MusicKit…</p>
<script>
document.addEventListener('musickitloaded', async () => {{
  const button = document.getElementById('authorize');
  const status = document.getElementById('status');
  try {{
    await MusicKit.configure({{developerToken:{token},app:{{name:'Chordrift',build:'{version}'}}}});
    const music = MusicKit.getInstance();
    button.disabled = false;
    status.textContent = 'Ready.';
    button.addEventListener('click', async () => {{
      button.disabled = true; status.textContent = 'Waiting for Apple…';
      try {{
        const userToken = await music.authorize();
        if (typeof userToken !== 'string' || userToken.length < 20) throw new Error('Apple did not return a user token');
        const response = await fetch('/token', {{method:'POST',headers:{{'Content-Type':'text/plain'}},body:userToken}});
        if (!response.ok) throw new Error(await response.text());
        document.body.innerHTML = '<h1>Authorized</h1><p>You may close this window.</p>';
      }} catch (error) {{ status.textContent = `Authorization failed: ${{error.message || error}}`; button.disabled = false; }}
    }});
  }} catch (error) {{ status.textContent = `MusicKit failed to load: ${{error.message || error}}`; }}
}});
</script></html>"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

async fn write_html(stream: &mut TcpStream, status: u16, body: &str) -> Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        _ => "Not Found",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use p256::{ecdsa::SigningKey, elliptic_curve::rand_core::OsRng, pkcs8::EncodePrivateKey};

    use super::{DeveloperCredential, developer_token, validate_identifier};

    #[test]
    fn signs_an_es256_developer_token() {
        let key = SigningKey::random(&mut OsRng);
        let pem = key.to_pkcs8_pem(Default::default()).expect("encode key");
        let token = developer_token(&DeveloperCredential {
            team_id: "ABCDE12345".to_owned(),
            key_id: "FGHIJ67890".to_owned(),
            private_key_pem: pem.to_string(),
        })
        .expect("signed token");
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn validates_apple_identifiers() {
        assert!(validate_identifier("Team ID", "ABCDE12345").is_ok());
        assert!(validate_identifier("Team ID", "short").is_err());
    }
}
