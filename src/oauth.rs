use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
};

use anyhow::{Result, anyhow};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl, basic::BasicClient,
};
use url::Url;

const REDIRECT_ADDR: &str = "127.0.0.1:8080";

#[derive(Clone, serde::Deserialize)]
pub struct App {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone)]
pub struct Service {
    pub auth_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
}

pub async fn login(app: App, service: Service) -> Result<String> {
    let client = BasicClient::new(ClientId::new(app.client_id))
        .set_client_secret(ClientSecret::new(app.client_secret))
        .set_auth_uri(AuthUrl::new(service.auth_url.to_string())?)
        .set_token_uri(TokenUrl::new(service.token_url.to_string())?)
        .set_redirect_uri(RedirectUrl::new(format!("http://{}", REDIRECT_ADDR))?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
    for scope in service.scopes.iter() {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }

    let (auth_url, csrf_state) = auth_request.url();

    eprintln!("Browse to: {}", auth_url);

    let (code, state) = listen_for_redirect(REDIRECT_ADDR)?;
    if state != csrf_state {
        return Err(anyhow::anyhow!("State mismatch"));
    }

    let http_client = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .build()?;

    let token = client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .request_async(&http_client)
        .await?;

    Ok(token.access_token().secret().to_string())
}

fn listen_for_redirect(addr: &str) -> Result<(AuthorizationCode, CsrfToken)> {
    let mut stream = TcpListener::bind(addr)?
        .incoming()
        .flatten()
        .next()
        .ok_or(anyhow!(
            "Listener terminated without accepting a connection"
        ))?;

    let url = parse_url(&mut stream)?;
    let code = AuthorizationCode::new(find_query_param(&url, "code")?);
    let state = CsrfToken::new(find_query_param(&url, "state")?);

    respond(&mut stream, "Go back to your terminal :)")?;

    Ok((code, state))
}

fn parse_url(stream: &mut impl Read) -> Result<Url> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let redirect_url = request_line
        .split_whitespace()
        .nth(1)
        .ok_or(anyhow::anyhow!(
            "Failed to parse request line: {}",
            request_line
        ))?;
    let url = Url::parse(&("http://localhost".to_string() + redirect_url))?;
    Ok(url)
}

fn find_query_param(url: &Url, param: &str) -> Result<String> {
    url.query_pairs()
        .find(|(key, _)| key == param)
        .map(|(_, value)| value.into_owned())
        .ok_or(anyhow::anyhow!(
            "Failed to find '{}' in query parameters",
            param
        ))
}

fn respond(stream: &mut impl Write, message: &str) -> Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
        message.len(),
        message
    )?;
    Ok(())
}

#[cfg(test)]
pub mod mock {
    use anyhow::{Result, anyhow};

    pub struct Session {
        token: std::cell::RefCell<Result<String, String>>,
    }

    impl Session {
        pub fn new(token: Result<String, String>) -> Session {
            Session {
                token: std::cell::RefCell::new(token),
            }
        }
    }

    impl crate::oauth::Session for Session {
        async fn refresh(&self) -> Result<Self> {
            Ok(Session {
                token: std::cell::RefCell::new(self.token.clone() + "-refreshed"),
            })
        }
    }
}
