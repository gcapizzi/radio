#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("empty response")]
    EmptyResponse,
    #[error("OAuth error")]
    OAuthError(#[from] crate::oauth::Error),
}

pub struct ResourceList {
    items: Vec<serde_json::Value>,
    http_client: reqwest::blocking::Client,
    token: crate::oauth::Token,
    url: Option<String>,
    total: u64,
}

const SERVICE: crate::oauth::Service = crate::oauth::Service {
    name: "spotify",
    auth_url: "https://accounts.spotify.com/authorize",
    token_url: "https://accounts.spotify.com/api/token",
    scopes: &[
        "user-library-read",
        "user-follow-read",
        "user-read-playback-position",
        "playlist-read-private",
    ],
};

pub fn login(app: &crate::oauth::App) -> Result<crate::oauth::Token, Error> {
    Ok(crate::oauth::login(app, &SERVICE)?)
}

pub fn get_albums(token: crate::oauth::Token) -> ResourceList {
    get_resource(token, "albums")
}

// https://api.spotify.com/v1/me/albums
// https://api.spotify.com/v1/me/audiobooks
// https://api.spotify.com/v1/me/episodes
// https://api.spotify.com/v1/me/following
// https://api.spotify.com/v1/me/playlists
// https://api.spotify.com/v1/me/shows
// https://api.spotify.com/v1/me/tracks
fn get_resource(token: crate::oauth::Token, resource_name: &str) -> ResourceList {
    ResourceList::new(
        token,
        format!("https://api.spotify.com/v1/me/{resource_name}"),
    )
}

impl ResourceList {
    pub fn new(token: crate::oauth::Token, url: String) -> ResourceList {
        ResourceList {
            items: Vec::new(),
            http_client: reqwest::blocking::Client::new(),
            token: token,
            url: Some(url),
            total: 0,
        }
    }

    fn get_items(&mut self) -> Result<(), Error> {
        if let Some(url) = self.url.clone() {
            let r: serde_json::Value = self
                .http_client
                .get(url)
                .bearer_auth(self.token.access_token())
                .send()?
                .json()?;
            self.items = r["items"].as_array().ok_or(Error::EmptyResponse)?.to_vec();
            self.url = r["next"].as_str().map(|s| s.to_string());
            self.total = r["total"].as_u64().ok_or(Error::EmptyResponse)?;
        }
        Ok(())
    }
}

impl Iterator for ResourceList {
    type Item = Result<serde_json::Value, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.items.is_empty() {
            if let Err(e) = self.get_items() {
                return Some(Err(e));
            }
        }
        self.items.pop().map(Result::Ok)
    }
}

impl ExactSizeIterator for ResourceList {
    fn len(&self) -> usize {
        self.total as usize
    }
}
