use crate::args::CliOutputFormat;
use api::{resolve_startup_auth_source, AnthropicClient, AuthSource};
use runtime::{
    clear_oauth_credentials, generate_pkce_pair, generate_state,
    parse_oauth_callback_request_target, save_oauth_credentials, ConfigLoader,
    OAuthAuthorizationRequest, OAuthConfig, OAuthTokenExchangeRequest,
};
use serde_json::json;
use std::env;
use std::io::{self, Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;

pub(crate) const DEFAULT_OAUTH_CALLBACK_PORT: u16 = 4545;

pub(crate) fn default_oauth_config() -> OAuthConfig {
    OAuthConfig {
        client_id: String::from("9d1c250a-e61b-44d9-88ed-5944d1962f5e"),
        authorize_url: String::from("https://platform.claude.com/oauth/authorize"),
        token_url: String::from("https://platform.claude.com/v1/oauth/token"),
        callback_port: None,
        manual_redirect_url: None,
        scopes: vec![
            String::from("user:profile"),
            String::from("user:inference"),
            String::from("user:sessions:claude_code"),
        ],
    }
}

pub(crate) fn run_login(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    let config = ConfigLoader::default_for(&cwd).load()?;
    let default_oauth = default_oauth_config();
    let oauth = config.oauth().unwrap_or(&default_oauth);
    let callback_port = oauth.callback_port.unwrap_or(DEFAULT_OAUTH_CALLBACK_PORT);
    let redirect_uri = runtime::loopback_redirect_uri(callback_port);
    let pkce = generate_pkce_pair()?;
    let state = generate_state()?;
    let authorize_url =
        OAuthAuthorizationRequest::from_config(oauth, redirect_uri.clone(), state.clone(), &pkce)
            .build_url();

    if output_format == CliOutputFormat::Text {
        println!("Starting Claude OAuth login...");
        println!("Listening for callback on {redirect_uri}");
    }
    if let Err(error) = open_browser(&authorize_url) {
        emit_login_browser_open_failure(
            output_format,
            &authorize_url,
            &error,
            &mut io::stdout(),
            &mut io::stderr(),
        )?;
    }

    let callback = wait_for_oauth_callback(callback_port)?;
    if let Some(error) = callback.error {
        let description = callback
            .error_description
            .unwrap_or_else(|| "authorization failed".to_string());
        return Err(io::Error::other(format!("{error}: {description}")).into());
    }
    let code = callback.code.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "callback did not include code")
    })?;
    let returned_state = callback.state.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "callback did not include state")
    })?;
    if returned_state != state {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "oauth state mismatch").into());
    }

    let client = AnthropicClient::from_auth(AuthSource::None).with_base_url(api::read_base_url());
    let exchange_request = OAuthTokenExchangeRequest::from_config(
        oauth,
        code,
        state,
        pkce.verifier,
        redirect_uri.clone(),
    );
    let runtime = tokio::runtime::Runtime::new()?;
    let token_set = runtime.block_on(client.exchange_oauth_code(oauth, &exchange_request))?;
    save_oauth_credentials(&runtime::OAuthTokenSet {
        access_token: token_set.access_token,
        refresh_token: token_set.refresh_token,
        expires_at: token_set.expires_at,
        scopes: token_set.scopes,
    })?;
    match output_format {
        CliOutputFormat::Text => println!("Claude OAuth login complete."),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "kind": "login",
                "callback_port": callback_port,
                "redirect_uri": redirect_uri,
                "message": "Claude OAuth login complete.",
            }))?
        ),
    }
    Ok(())
}

pub(crate) fn emit_login_browser_open_failure(
    output_format: CliOutputFormat,
    authorize_url: &str,
    error: &io::Error,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> io::Result<()> {
    writeln!(
        stderr,
        "warning: failed to open browser automatically: {error}"
    )?;
    match output_format {
        CliOutputFormat::Text => writeln!(stdout, "Open this URL manually:\n{authorize_url}"),
        CliOutputFormat::Json => writeln!(stderr, "Open this URL manually:\n{authorize_url}"),
    }
}

pub(crate) fn run_logout(output_format: CliOutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    clear_oauth_credentials()?;
    match output_format {
        CliOutputFormat::Text => println!("Claude OAuth credentials cleared."),
        CliOutputFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "kind": "logout",
                "message": "Claude OAuth credentials cleared.",
            }))?
        ),
    }
    Ok(())
}

pub(crate) fn open_browser(url: &str) -> io::Result<()> {
    let commands = if cfg!(target_os = "macos") {
        vec![("open", vec![url])]
    } else if cfg!(target_os = "windows") {
        vec![("cmd", vec!["/C", "start", "", url])]
    } else {
        vec![("xdg-open", vec![url])]
    };
    for (program, args) in commands {
        match Command::new(program).args(args).spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "no supported browser opener command found",
    ))
}

pub(crate) fn wait_for_oauth_callback(
    port: u16,
) -> Result<runtime::OAuthCallbackParams, Box<dyn std::error::Error>> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    let (mut stream, _) = listener.accept()?;
    let mut buffer = [0_u8; 4096];
    let bytes_read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..bytes_read]);
    let request_line = request.lines().next().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing callback request line")
    })?;
    let target = request_line.split_whitespace().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "missing callback request target",
        )
    })?;
    let callback = parse_oauth_callback_request_target(target)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let body = if callback.error.is_some() {
        "Claude OAuth login failed. You can close this window."
    } else {
        "Claude OAuth login succeeded. You can close this window."
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes())?;
    Ok(callback)
}

pub(crate) fn resolve_cli_auth_source() -> Result<AuthSource, Box<dyn std::error::Error>> {
    let cwd = env::current_dir()?;
    Ok(resolve_cli_auth_source_for_cwd(&cwd, default_oauth_config)?)
}

pub(crate) fn resolve_cli_auth_source_for_cwd<F>(
    cwd: &Path,
    default_oauth: F,
) -> Result<AuthSource, api::ApiError>
where
    F: FnOnce() -> OAuthConfig,
{
    resolve_startup_auth_source(|| {
        Ok(Some(
            load_runtime_oauth_config_for(cwd)?.unwrap_or_else(default_oauth),
        ))
    })
}

pub(crate) fn load_runtime_oauth_config_for(
    cwd: &Path,
) -> Result<Option<OAuthConfig>, api::ApiError> {
    let config = ConfigLoader::default_for(cwd).load().map_err(|error| {
        api::ApiError::Auth(format!("failed to load runtime OAuth config: {error}"))
    })?;
    Ok(config.oauth().cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use runtime::{load_oauth_credentials, save_oauth_credentials, OAuthConfig};
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn temp_dir() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};

        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time should be after epoch")
            .as_nanos();
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("rusty-claude-cli-oauth-{nanos}-{unique}"))
    }

    fn sample_oauth_config(token_url: String) -> OAuthConfig {
        OAuthConfig {
            client_id: "runtime-client".to_string(),
            authorize_url: "https://console.test/oauth/authorize".to_string(),
            token_url,
            callback_port: Some(4545),
            manual_redirect_url: Some("https://console.test/oauth/callback".to_string()),
            scopes: vec!["org:create_api_key".to_string(), "user:profile".to_string()],
        }
    }

    fn spawn_token_server(response_body: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
        let address = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept connection");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).expect("read request");
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{}",
                response_body.len(),
                response_body
            );
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        format!("http://{address}/oauth/token")
    }

    #[test]
    fn login_browser_failure_keeps_json_stdout_clean() {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let error = std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no supported browser opener command found",
        );

        emit_login_browser_open_failure(
            CliOutputFormat::Json,
            "https://example.test/oauth/authorize",
            &error,
            &mut stdout,
            &mut stderr,
        )
        .expect("browser warning should render");

        assert!(stdout.is_empty());
        let stderr = String::from_utf8(stderr).expect("utf8");
        assert!(stderr.contains("failed to open browser automatically"));
        assert!(stderr.contains("Open this URL manually:"));
        assert!(stderr.contains("https://example.test/oauth/authorize"));
    }

    #[test]
    fn load_runtime_oauth_config_for_returns_none_without_project_config() {
        let _guard = env_lock();
        let root = temp_dir();
        std::fs::create_dir_all(&root).expect("workspace should exist");

        let oauth = load_runtime_oauth_config_for(&root)
            .expect("loading config should succeed when files are absent");

        std::fs::remove_dir_all(root).expect("temp workspace should clean up");

        assert_eq!(oauth, None);
    }

    #[test]
    fn resolve_cli_auth_source_uses_default_oauth_when_runtime_config_is_missing() {
        let _guard = env_lock();
        let workspace = temp_dir();
        let config_home = temp_dir();
        std::fs::create_dir_all(&workspace).expect("workspace should exist");
        std::fs::create_dir_all(&config_home).expect("config home should exist");

        let original_config_home = std::env::var("CLAW_CONFIG_HOME").ok();
        let original_api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let original_auth_token = std::env::var("ANTHROPIC_AUTH_TOKEN").ok();
        std::env::set_var("CLAW_CONFIG_HOME", &config_home);
        std::env::remove_var("ANTHROPIC_API_KEY");
        std::env::remove_var("ANTHROPIC_AUTH_TOKEN");

        save_oauth_credentials(&runtime::OAuthTokenSet {
            access_token: "expired-access-token".to_string(),
            refresh_token: Some("refresh-token".to_string()),
            expires_at: Some(0),
            scopes: vec!["org:create_api_key".to_string(), "user:profile".to_string()],
        })
        .expect("save expired oauth credentials");

        let token_url = spawn_token_server(
            r#"{"access_token":"refreshed-access-token","refresh_token":"refreshed-refresh-token","expires_at":4102444800,"scopes":["org:create_api_key","user:profile"]}"#,
        );

        let auth = resolve_cli_auth_source_for_cwd(&workspace, || sample_oauth_config(token_url))
            .expect("expired saved oauth should refresh via default config");

        let stored = load_oauth_credentials()
            .expect("load stored credentials")
            .expect("stored credentials should exist");

        match original_config_home {
            Some(value) => std::env::set_var("CLAW_CONFIG_HOME", value),
            None => std::env::remove_var("CLAW_CONFIG_HOME"),
        }
        match original_api_key {
            Some(value) => std::env::set_var("ANTHROPIC_API_KEY", value),
            None => std::env::remove_var("ANTHROPIC_API_KEY"),
        }
        match original_auth_token {
            Some(value) => std::env::set_var("ANTHROPIC_AUTH_TOKEN", value),
            None => std::env::remove_var("ANTHROPIC_AUTH_TOKEN"),
        }
        std::fs::remove_dir_all(workspace).expect("temp workspace should clean up");
        std::fs::remove_dir_all(config_home).expect("temp config home should clean up");

        assert_eq!(auth.bearer_token(), Some("refreshed-access-token"));
        assert_eq!(stored.access_token, "refreshed-access-token");
        assert_eq!(
            stored.refresh_token.as_deref(),
            Some("refreshed-refresh-token")
        );
    }
}
