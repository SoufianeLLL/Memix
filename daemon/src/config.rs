use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct AppConfig {
    pub port: Option<u16>,
    pub backend: Option<String>,
    pub redis_url: Option<String>,
    pub data_dir: Option<String>,
	pub workspace_root: Option<String>,
	pub project_id: Option<String>,
	pub team_id: Option<String>,
	pub team_secret: Option<String>,
	pub team_actor_id: Option<String>,
	pub license_public_key: Option<String>,
	pub license_server_url: Option<String>,
}

pub fn load_config() -> anyhow::Result<AppConfig> {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let config_path = home.join(".memix").join("config.toml");

    let settings = config::Config::builder()
        .set_default("port", 3456)?
        .set_default("backend", "redis")?
        .set_default("redis_url", "redis://127.0.0.1/")?
        .set_default("data_dir", ".memix")?
		.set_default("workspace_root", "")?
		.set_default("project_id", "")?
		.set_default("team_id", "")?
		.set_default("team_secret", "")?
		.set_default("team_actor_id", "")?
		.set_default("license_public_key", "")?
		.set_default("license_server_url", "")?
        .add_source(config::File::from(config_path).required(false))
        .add_source(config::Environment::with_prefix("MEMIX"))
        .build()?;

    let mut app_config: AppConfig = settings.try_deserialize()?;
	
	// Strip accidental wrapping quotes from .env variables
	if let Some(rurl) = &app_config.redis_url {
		let trimmed = rurl.trim_matches(|c| c == '\'' || c == '"').to_string();
		app_config.redis_url = Some(trimmed);
	}

	// Normalize workspace_root: treat empty as None
	if let Some(root) = &app_config.workspace_root {
		let trimmed = root.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.workspace_root = None;
		} else {
			app_config.workspace_root = Some(trimmed);
		}
	}

	// Normalize project_id: treat empty as None
	if let Some(pid) = &app_config.project_id {
		let trimmed = pid.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.project_id = None;
		} else {
			app_config.project_id = Some(trimmed);
		}
	}

	// Normalize team_id: treat empty as None
	if let Some(team_id) = &app_config.team_id {
		let trimmed = team_id.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.team_id = None;
		} else {
			app_config.team_id = Some(trimmed);
		}
	}

	// Normalize team_secret: treat empty as None
	if let Some(team_secret) = &app_config.team_secret {
		let trimmed = team_secret.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.team_secret = None;
		} else {
			app_config.team_secret = Some(trimmed);
		}
	}

	// Normalize team_actor_id: treat empty as None
	if let Some(team_actor_id) = &app_config.team_actor_id {
		let trimmed = team_actor_id.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.team_actor_id = None;
		} else {
			app_config.team_actor_id = Some(trimmed);
		}
	}

	if let Some(license_public_key) = &app_config.license_public_key {
		let trimmed = license_public_key.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.license_public_key = None;
		} else {
			app_config.license_public_key = Some(trimmed);
		}
	}

	if let Some(license_server_url) = &app_config.license_server_url {
		let trimmed = license_server_url.trim_matches(|c| c == '\'' || c == '"').trim().to_string();
		if trimmed.is_empty() {
			app_config.license_server_url = None;
		} else {
			app_config.license_server_url = Some(trimmed);
		}
	}

	// Force daemon-managed data dir into workspace-local .memix when workspace_root is present.
	if let Some(root) = &app_config.workspace_root {
		let workspace_memix = std::path::PathBuf::from(root).join(".memix");
		app_config.data_dir = Some(workspace_memix.to_string_lossy().to_string());
	}

    Ok(app_config)
}
