#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("empty response")]
    EmptyResponse,
    #[error("OAuth error")]
    OAuthError(#[from] crate::oauth::Error),
}

struct ResourceIterator {
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

pub fn get_albums(
    token: crate::oauth::Token,
) -> impl ExactSizeIterator<Item = Result<serde_json::Value, Error>> {
    get_resource(token, "albums")
}

// https://api.spotify.com/v1/me/albums
// https://api.spotify.com/v1/me/audiobooks
// https://api.spotify.com/v1/me/episodes
// https://api.spotify.com/v1/me/following
// https://api.spotify.com/v1/me/playlists
// https://api.spotify.com/v1/me/shows
// https://api.spotify.com/v1/me/tracks
fn get_resource(token: crate::oauth::Token, resource_name: &str) -> ResourceIterator {
    ResourceIterator::new(
        token,
        format!("https://api.spotify.com/v1/me/{resource_name}"),
    )
}

impl ResourceIterator {
    pub fn new(token: crate::oauth::Token, url: String) -> ResourceIterator {
        ResourceIterator {
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

impl Iterator for ResourceIterator {
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

impl ExactSizeIterator for ResourceIterator {
    fn len(&self) -> usize {
        self.total as usize
    }
}
