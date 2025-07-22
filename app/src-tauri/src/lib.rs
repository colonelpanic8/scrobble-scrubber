use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};

use lastfm_edit::LastFmEditClient;
use scrobble_scrubber::config::{LastFmConfig, ScrobbleScrubberConfig};
use scrobble_scrubber::persistence::{MemoryStorage, RewriteRulesState};
use scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginCredentials {
    username: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoginResult {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanResult {
    success: bool,
    message: String,
    tracks_processed: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArtistScanArgs {
    artist: String,
}

// Commands that can be sent to the LastFm client worker
#[derive(Debug)]
enum LastFmCommand {
    Login {
        username: String,
        password: String,
        response: oneshot::Sender<Result<(), String>>,
    },
    ProcessArtist {
        artist: String,
        response: oneshot::Sender<Result<u32, String>>,
    },
}

// Simplified app state that holds the scrubber directly
pub struct AppState {
    scrubber: Option<
        scrobble_scrubber::scrubber::ScrobbleScrubber<
            MemoryStorage,
            RewriteRulesScrubActionProvider,
        >,
    >,
    config: Option<ScrobbleScrubberConfig>,
    current_user: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scrubber: None,
            config: None,
            current_user: None,
        }
    }
}

// Worker function that runs in its own thread to handle LastFm operations
async fn lastfm_worker(mut receiver: mpsc::Receiver<LastFmCommand>) {
    log::info!("LastFm worker: Started and waiting for commands");
    let mut scrubber: Option<
        scrobble_scrubber::scrubber::ScrobbleScrubber<
            MemoryStorage,
            RewriteRulesScrubActionProvider,
        >,
    > = None;

    while let Some(command) = receiver.recv().await {
        log::info!(
            "LastFm worker: Received command: {:?}",
            std::mem::discriminant(&command)
        );

        match command {
            LastFmCommand::Login {
                username,
                password,
                response,
            } => {
                log::info!(
                    "LastFm worker: Processing login command for user: {}",
                    username
                );

                log::debug!("LastFm worker: Creating HTTP client");
                let http_client = http_client::native::NativeClient::new();
                log::debug!("LastFm worker: HTTP client created successfully");

                log::debug!("LastFm worker: Creating LastFmEditClient");
                let mut lastfm_client = LastFmEditClient::new(Box::new(http_client));
                log::debug!("LastFm worker: LastFmEditClient created successfully");

                log::info!("LastFm worker: Attempting to login to Last.fm...");
                match lastfm_client.login(&username, &password).await {
                    Ok(_) => {
                        log::info!("LastFm worker: Login successful for user: {}", username);

                        log::debug!("LastFm worker: Creating storage components");
                        let storage = Arc::new(Mutex::new(MemoryStorage::new()));
                        let rules_state = RewriteRulesState::default();
                        let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);
                        let config = ScrobbleScrubberConfig::default();
                        log::debug!("LastFm worker: Storage components created");

                        log::debug!("LastFm worker: Creating scrubber instance");
                        let new_scrubber = scrobble_scrubber::scrubber::ScrobbleScrubber::new(
                            storage,
                            lastfm_client,
                            action_provider,
                            config,
                        );
                        log::debug!("LastFm worker: Scrubber instance created successfully");

                        scrubber = Some(new_scrubber);
                        log::info!(
                            "LastFm worker: Login process completed, sending success response"
                        );

                        if let Err(e) = response.send(Ok(())) {
                            log::error!("LastFm worker: Failed to send success response: {:?}", e);
                        } else {
                            log::debug!("LastFm worker: Success response sent");
                        }
                    }
                    Err(e) => {
                        log::error!("LastFm worker: Login failed for user {}: {}", username, e);
                        if let Err(send_err) = response.send(Err(format!("Login failed: {}", e))) {
                            log::error!(
                                "LastFm worker: Failed to send error response: {:?}",
                                send_err
                            );
                        } else {
                            log::debug!("LastFm worker: Error response sent");
                        }
                    }
                }
            }
            LastFmCommand::ProcessArtist { artist, response } => {
                log::info!("LastFm worker: Processing artist scan for: {}", artist);

                if let Some(ref mut scrubber_instance) = scrubber {
                    log::debug!(
                        "LastFm worker: Scrubber instance available, starting artist processing"
                    );
                    match scrubber_instance.process_artist(&artist).await {
                        Ok(_) => {
                            log::info!("LastFm worker: Successfully processed artist: {}", artist);
                            if let Err(e) = response.send(Ok(0)) {
                                log::error!(
                                    "LastFm worker: Failed to send success response: {:?}",
                                    e
                                );
                            }
                        }
                        Err(e) => {
                            log::error!(
                                "LastFm worker: Failed to process artist {}: {}",
                                artist,
                                e
                            );
                            if let Err(send_err) =
                                response.send(Err(format!("Failed to process artist: {}", e)))
                            {
                                log::error!(
                                    "LastFm worker: Failed to send error response: {:?}",
                                    send_err
                                );
                            }
                        }
                    }
                } else {
                    log::warn!("LastFm worker: No scrubber instance available (not logged in)");
                    if let Err(e) = response.send(Err("Not logged in".to_string())) {
                        log::error!(
                            "LastFm worker: Failed to send not-logged-in response: {:?}",
                            e
                        );
                    }
                }
            }
        }
        log::debug!("LastFm worker: Command processing completed, waiting for next command");
    }
    log::warn!("LastFm worker: Channel closed, worker shutting down");
}

#[tauri::command]
async fn login(
    credentials: LoginCredentials,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<LoginResult, String> {
    log::info!(
        "Login command: Received login request for username: {}",
        credentials.username
    );

    log::debug!("Login command: Creating HTTP client");
    let http_client = http_client::native::NativeClient::new();
    log::debug!("Login command: HTTP client created successfully");

    log::debug!("Login command: Creating LastFmEditClient");
    let mut lastfm_client = LastFmEditClient::new(Box::new(http_client));
    log::debug!("Login command: LastFmEditClient created successfully");

    log::info!("Login command: Attempting to login to Last.fm...");
    match lastfm_client
        .login(&credentials.username, &credentials.password)
        .await
    {
        Ok(_) => {
            log::info!(
                "Login command: Login successful for user: {}",
                credentials.username
            );

            log::debug!("Login command: Creating storage components");
            let storage = Arc::new(Mutex::new(MemoryStorage::new()));
            let rules_state = RewriteRulesState::default();
            let action_provider = RewriteRulesScrubActionProvider::new(&rules_state);
            let config = ScrobbleScrubberConfig::default();
            log::debug!("Login command: Storage components created");

            log::debug!("Login command: Creating scrubber instance");
            let scrubber = scrobble_scrubber::scrubber::ScrobbleScrubber::new(
                storage,
                lastfm_client,
                action_provider,
                config.clone(),
            );
            log::debug!("Login command: Scrubber instance created successfully");

            log::debug!("Login command: Acquiring app state lock");
            let mut app_state = state.lock().await;
            log::debug!("Login command: App state lock acquired");

            // Save scrubber, config and username
            let lastfm_config = LastFmConfig {
                username: credentials.username.clone(),
                password: String::new(), // Don't store password
                base_url: None,
            };
            let mut updated_config = config;
            updated_config.lastfm = lastfm_config;

            app_state.scrubber = Some(scrubber);
            app_state.config = Some(updated_config);
            app_state.current_user = Some(credentials.username.clone());

            log::info!(
                "Login command: Login process completed successfully for user: {}",
                credentials.username
            );

            Ok(LoginResult {
                success: true,
                message: format!("Successfully logged in as {}", credentials.username),
            })
        }
        Err(e) => {
            log::error!(
                "Login command: Login failed for user {}: {}",
                credentials.username,
                e
            );
            Ok(LoginResult {
                success: false,
                message: format!("Login failed: {}", e),
            })
        }
    }
}

#[tauri::command]
async fn scan_artist(
    artist: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ScanResult, String> {
    log::info!("Starting artist scan for: {}", artist);

    let mut app_state = state.lock().await;

    if let Some(ref mut scrubber) = app_state.scrubber {
        log::debug!("Scrubber instance available, starting artist processing");
        match scrubber.process_artist(&artist).await {
            Ok(_) => {
                log::info!("Successfully processed artist: {}", artist);
                Ok(ScanResult {
                    success: true,
                    message: format!("Successfully scanned tracks for artist: {}", artist),
                    tracks_processed: 0, // TODO: Return actual count if available
                })
            }
            Err(e) => {
                log::error!("Failed to process artist {}: {}", artist, e);
                Ok(ScanResult {
                    success: false,
                    message: format!("Failed to process artist: {}", e),
                    tracks_processed: 0,
                })
            }
        }
    } else {
        log::warn!("No scrubber instance available (not logged in)");
        Ok(ScanResult {
            success: false,
            message: "Not logged in. Please login first.".to_string(),
            tracks_processed: 0,
        })
    }
}

#[tauri::command]
async fn is_logged_in(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<bool, String> {
    let app_state = state.lock().await;
    Ok(app_state.current_user.is_some())
}

#[tauri::command]
async fn get_current_user(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Option<String>, String> {
    let app_state = state.lock().await;
    Ok(app_state.current_user.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Debug)
        .init();

    log::info!("Starting Tauri application with debug logging enabled");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(Mutex::new(AppState::default())))
        .invoke_handler(tauri::generate_handler![
            login,
            scan_artist,
            is_logged_in,
            get_current_user
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
