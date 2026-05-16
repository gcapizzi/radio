use std::collections::HashMap;

use anyhow::{Context, Result, anyhow};
use indicatif::ProgressIterator;

fn main() -> Result<()> {
    let config = load_config()?;

    // radio::oauth::Service {
    //     name: "tidal".to_string(),
    //     auth_url: "https://login.tidal.com/authorize".to_string(),
    //     token_url: "https://auth.tidal.com/v1/oauth2/token".to_string(),
    //     scopes: vec!["search.read".to_string()],
    // }

    let app = config
        .get("spotify")
        .ok_or(anyhow!("Spotify not configured"))?;
    let token = radio::spotify::login(&app)?;
    let albums = radio::spotify::get_playlists(token);
    for item in albums.progress() {
        print_item(&item?)?;
    }

    Ok(())
}

fn print_item(value: &serde_json::Value) -> Result<()> {
    println!("{}", serde_json::to_string(value)?);
    Ok(())
}

fn load_config() -> Result<HashMap<String, radio::oauth::App>> {
    let path = config_dir()?.get_config_file("config.toml");
    let config_str = &std::fs::read_to_string(path).context("failed to load config")?;
    toml::from_str(config_str).context("failed to parse config")
}

fn config_dir() -> Result<xdg::BaseDirectories> {
    xdg::BaseDirectories::with_prefix("radio").context("failed to locate config")
}
