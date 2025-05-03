use anyhow::{Result, anyhow};

#[derive(serde::Deserialize)]
struct Config {
    spotify: Option<radio::oauth::App>,
    tidal: Option<radio::oauth::App>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let config_path = xdg::BaseDirectories::with_prefix("radio")?.get_config_file("config.toml");
    let apps: Config = toml::from_str(&std::fs::read_to_string(config_path)?)?;

    let http_client = reqwest::Client::new();

    let tidal_service = radio::oauth::Service {
        auth_url: "https://login.tidal.com/authorize".to_string(),
        token_url: "https://auth.tidal.com/v1/oauth2/token".to_string(),
        scopes: vec!["search.read".to_string()],
    };
    let tidal_app = apps.tidal.ok_or(anyhow!("Tidal not configured"))?;
    let tidal_token = radio::oauth::login(tidal_app.clone(), tidal_service.clone()).await?;
    let tidal_response = http_client.get("https://openapi.tidal.com/v2/searchResults/radiohead%20in%20rainbows?countryCode=GB&include=albums")
        .bearer_auth(tidal_token)
        .send()
        .await?
        .text()
        .await?;
    println!("{tidal_response:#?}");

    let spotify_service = radio::oauth::Service {
        auth_url: "https://accounts.spotify.com/authorize".to_string(),
        token_url: "https://accounts.spotify.com/api/token".to_string(),
        scopes: vec![],
    };
    let spotify_app = apps.spotify.ok_or(anyhow!("Spotify not configured"))?;
    let spotify_token = radio::oauth::login(spotify_app.clone(), spotify_service.clone()).await?;
    let spotify_response = http_client
        .get("https://api.spotify.com/v1/search?q=radiohead%20in%20rainbows&type=album")
        .bearer_auth(spotify_token)
        .send()
        .await?
        .text()
        .await?;
    println!("{spotify_response:#?}");

    Ok(())
}
