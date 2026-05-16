use std::{
    io::{self, BufRead, BufReader, Read, Write},
    net::TcpListener,
    time::Duration,
};

use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge, RedirectUrl,
    Scope, TokenResponse, TokenUrl, basic::BasicClient,
};
use url::Url;

const REDIRECT_ADDR: &str = "127.0.0.1:8080";

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("invalid URL: {0}")]
    InvalidURL(#[from] url::ParseError),
    #[error("CSRF state mismatch")]
    CSRFStateMismatch,
    #[error("HTTP error: {0}")]
    HTTPError(#[from] oauth2::reqwest::Error),
    #[error("Request token error: {0}")]
    RequestTokenError(
        #[from]
        oauth2::RequestTokenError<
            oauth2::HttpClientError<oauth2::reqwest::Error>,
            oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
        >,
    ),
    #[error("No refresh token")]
    NoRefreshToken,
    #[error("IO error: {0}")]
    IO(#[from] io::Error),
    #[error("Listener terminated before accepting any connection")]
    TCPListenerError,
    #[error("Invalid request line: {0}")]
    InvalidRequestLine(String),
    #[error("Invalid JSON")]
    InvalidJSON(#[from] serde_json::Error),
    #[error("Base dirs error")]
    BadeDirsError(#[from] xdg::BaseDirectoriesError),
    #[error("Cannot find query param: {0}")]
    QueryParamNotFound(String),
}

#[derive(Clone, serde::Deserialize)]
pub struct App {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone)]
pub struct Service<'a> {
    pub name: &'a str,
    pub auth_url: &'a str,
    pub token_url: &'a str,
    pub scopes: &'a [&'a str],
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Token {
    access_token: String,
    refresh_token: Option<String>,
    created_at: std::time::SystemTime,
    expires_in: Option<Duration>,
}

impl Token {
    pub fn access_token(&self) -> &str {
        &self.access_token
    }

    pub fn is_expired(&self) -> bool {
        self.created_at
            .elapsed()
            .map(|e| e > self.expires_in.unwrap_or(std::time::Duration::MAX))
            .unwrap_or(false)
    }
}

pub fn login(app: &App, service: &Service) -> Result<Token, Error> {
    if let Ok(cached_token) = load_cached_token(app, service) {
        return Ok(cached_token);
    }

    let client = BasicClient::new(ClientId::new(app.client_id.clone()))
        .set_client_secret(ClientSecret::new(app.client_secret.clone()))
        .set_auth_uri(AuthUrl::new(service.auth_url.to_string())?)
        .set_token_uri(TokenUrl::new(service.token_url.to_string())?)
        .set_redirect_uri(RedirectUrl::new(format!("http://{}", REDIRECT_ADDR))?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let mut auth_request = client
        .authorize_url(CsrfToken::new_random)
        .set_pkce_challenge(pkce_challenge);
    for scope in service.scopes.iter() {
        auth_request = auth_request.add_scope(Scope::new(scope.to_string()));
    }

    let (auth_url, csrf_state) = auth_request.url();

    eprintln!("Browse to: {}", auth_url);

    let (code, state) = listen_for_redirect(REDIRECT_ADDR)?;
    if state != csrf_state {
        return Err(Error::CSRFStateMismatch);
    }

    let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
        .redirect(oauth2::reqwest::redirect::Policy::none())
        .build()?;

    let token_response = client
        .exchange_code(code)
        .set_pkce_verifier(pkce_verifier)
        .request(&http_client)?;

    let token = Token {
        access_token: token_response.access_token().secret().to_string(),
        refresh_token: token_response
            .refresh_token()
            .map(|t| t.secret().to_string()),
        expires_in: token_response.expires_in(),
        created_at: std::time::SystemTime::now(),
    };

    cache_token(&service.name, &token)?;

    Ok(token)
}

fn refresh_token(token: &Token, app: &App, service: &Service) -> Result<Token, Error> {
    if let Some(refresh_token) = &token.refresh_token {
        let client = BasicClient::new(ClientId::new(app.client_id.clone()))
            .set_client_secret(ClientSecret::new(app.client_secret.clone()))
            .set_auth_uri(AuthUrl::new(service.auth_url.to_string())?)
            .set_token_uri(TokenUrl::new(service.token_url.to_string())?)
            .set_redirect_uri(RedirectUrl::new(format!("http://{}", REDIRECT_ADDR))?);

        let http_client = oauth2::reqwest::blocking::ClientBuilder::new()
            .redirect(oauth2::reqwest::redirect::Policy::none())
            .build()?;

        let token = client
            .exchange_refresh_token(&oauth2::RefreshToken::new(refresh_token.to_string()))
            .request(&http_client)?;

        Ok(Token {
            access_token: token.access_token().secret().to_string(),
            refresh_token: token.refresh_token().map(|t| t.secret().to_string()),
            expires_in: token.expires_in(),
            created_at: std::time::SystemTime::now(),
        })
    } else {
        Err(Error::NoRefreshToken)
    }
}

fn listen_for_redirect(addr: &str) -> Result<(AuthorizationCode, CsrfToken), Error> {
    let mut stream = TcpListener::bind(addr)?
        .incoming()
        .flatten()
        .next()
        .ok_or(Error::TCPListenerError)?;

    let url = parse_url(&mut stream)?;
    let code = AuthorizationCode::new(find_query_param(&url, "code")?);
    let state = CsrfToken::new(find_query_param(&url, "state")?);

    respond(&mut stream, "Go back to your terminal :)")?;

    Ok((code, state))
}

fn parse_url(stream: &mut impl Read) -> Result<Url, Error> {
    let mut reader = BufReader::new(stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let redirect_url = request_line
        .split_whitespace()
        .nth(1)
        .ok_or(Error::InvalidRequestLine(request_line.clone()))?;
    let url = Url::parse(&("http://localhost".to_string() + redirect_url))?;
    Ok(url)
}

fn find_query_param(url: &Url, param: &str) -> Result<String, Error> {
    url.query_pairs()
        .find(|(key, _)| key == param)
        .map(|(_, value)| value.into_owned())
        .ok_or(Error::QueryParamNotFound(param.to_string()))
}

fn respond(stream: &mut impl Write, message: &str) -> Result<(), Error> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
        message.len(),
        message
    )?;
    Ok(())
}

fn load_cached_token(app: &App, service: &Service) -> Result<Token, Error> {
    let path = config_dir()?.get_cache_file(&service.name);
    let file = std::fs::File::open(path)?;
    let token: Token = serde_json::from_reader(file)?;
    if token.is_expired() {
        refresh_token(&token, app, service)
    } else {
        Ok(token)
    }
}

fn cache_token(name: &str, token: &Token) -> Result<(), Error> {
    let path = config_dir()?.place_cache_file(name)?;
    let file = std::fs::File::create(path)?;
    Ok(serde_json::to_writer_pretty(file, token)?)
}

fn config_dir() -> Result<xdg::BaseDirectories, Error> {
    Ok(xdg::BaseDirectories::with_prefix("radio")?)
}
