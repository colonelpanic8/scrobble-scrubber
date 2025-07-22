use std::sync::Arc;
use tokio::sync::{Mutex, mpsc, oneshot};
use serde::{Deserialize, Serialize};

use scrobble_scrubber::config::{ScrobbleScrubberConfig, LastFmConfig};
use scrobble_scrubber::persistence::{MemoryStorage, RewriteRulesState};
use scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider;
use lastfm_edit::LastFmEditClient;

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

// App state to hold our communication channels
pub struct AppState {
    lastfm_sender: Option<mpsc::Sender<LastFmCommand>>,
    config: Option<ScrobbleScrubberConfig>,
    current_user: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            lastfm_sender: None,
            config: None,
            current_user: None,
        }
    }
}

// Worker function that runs in its own thread to handle LastFm operations
async fn lastfm_worker(mut receiver: mpsc::Receiver<LastFmCommand>) {
    log::info!("LastFm worker: Started and waiting for commands");
    let mut scrubber: Option<scrobble_scrubber::scrubber::ScrobbleScrubber<MemoryStorage, RewriteRulesScrubActionProvider>> = None;

    while let Some(command) = receiver.recv().await {
        log::info!("LastFm worker: Received command: {:?}", std::mem::discriminant(&command));
        
        match command {
            LastFmCommand::Login { username, password, response } => {
                log::info!("LastFm worker: Processing login command for user: {}", username);
                
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
                        log::info!("LastFm worker: Login process completed, sending success response");
                        
                        if let Err(e) = response.send(Ok(())) {
                            log::error!("LastFm worker: Failed to send success response: {:?}", e);
                        } else {
                            log::debug!("LastFm worker: Success response sent");
                        }
                    }
                    Err(e) => {
                        log::error!("LastFm worker: Login failed for user {}: {}", username, e);
                        if let Err(send_err) = response.send(Err(format!("Login failed: {}", e))) {
                            log::error!("LastFm worker: Failed to send error response: {:?}", send_err);
                        } else {
                            log::debug!("LastFm worker: Error response sent");
                        }
                    }
                }
            }
            LastFmCommand::ProcessArtist { artist, response } => {
                log::info!("LastFm worker: Processing artist scan for: {}", artist);
                
                if let Some(ref mut scrubber_instance) = scrubber {
                    log::debug!("LastFm worker: Scrubber instance available, starting artist processing");
                    match scrubber_instance.process_artist(&artist).await {
                        Ok(_) => {
                            log::info!("LastFm worker: Successfully processed artist: {}", artist);
                            if let Err(e) = response.send(Ok(0)) {
                                log::error!("LastFm worker: Failed to send success response: {:?}", e);
                            }
                        }
                        Err(e) => {
                            log::error!("LastFm worker: Failed to process artist {}: {}", artist, e);
                            if let Err(send_err) = response.send(Err(format!("Failed to process artist: {}", e))) {
                                log::error!("LastFm worker: Failed to send error response: {:?}", send_err);
                            }
                        }
                    }
                } else {
                    log::warn!("LastFm worker: No scrubber instance available (not logged in)");
                    if let Err(e) = response.send(Err("Not logged in".to_string())) {
                        log::error!("LastFm worker: Failed to send not-logged-in response: {:?}", e);
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
    log::info!("Login command: Received login request for username: {}", credentials.username);

    log::debug!("Login command: Acquiring app state lock");
    let mut app_state = state.lock().await;
    log::debug!("Login command: App state lock acquired");
    
    // Create channel if it doesn't exist
    if app_state.lastfm_sender.is_none() {
        log::info!("Login command: Creating new worker channel and spawning worker thread");
        let (sender, receiver) = mpsc::channel::<LastFmCommand>(32);
        app_state.lastfm_sender = Some(sender);
        
        // Spawn the worker task on a separate thread to avoid Send requirements
        std::thread::spawn(move || {
            log::info!("Worker thread: Starting new Tokio runtime");
            match tokio::runtime::Runtime::new() {
                Ok(rt) => {
                    log::info!("Worker thread: Tokio runtime created, starting worker");
                    rt.block_on(lastfm_worker(receiver));
                    log::info!("Worker thread: Worker finished");
                }
                Err(e) => {
                    log::error!("Worker thread: Failed to create Tokio runtime: {}", e);
                }
            }
        });
        log::debug!("Login command: Worker thread spawned");
    } else {
        log::debug!("Login command: Using existing worker channel");
    }

    if let Some(sender) = &app_state.lastfm_sender {
        log::debug!("Login command: Creating response channel");
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = LastFmCommand::Login {
            username: credentials.username.clone(),
            password: credentials.password.clone(),
            response: response_tx,
        };
        
        log::info!("Login command: Sending login command to worker");
        match sender.send(command).await {
            Ok(_) => {
                log::debug!("Login command: Command sent successfully, waiting for response");
            }
            Err(e) => {
                log::error!("Login command: Failed to send command to worker: {}", e);
                return Ok(LoginResult {
                    success: false,
                    message: "Failed to send login request to worker".to_string(),
                });
            }
        }
        
        // Drop the lock before waiting for response to avoid potential deadlocks
        drop(app_state);
        
        log::debug!("Login command: Waiting for response from worker");
        // Wait for response
        match response_rx.await {
            Ok(Ok(())) => {
                log::info!("Login command: Received success response from worker");
                
                // Reacquire the lock to update state
                log::debug!("Login command: Reacquiring app state lock to save user info");
                let mut app_state = state.lock().await;
                
                // Save config and username
                let lastfm_config = LastFmConfig {
                    username: credentials.username.clone(),
                    password: String::new(), // Don't store password
                    base_url: None,
                };
                let mut config = ScrobbleScrubberConfig::default();
                config.lastfm = lastfm_config;
                
                app_state.config = Some(config);
                app_state.current_user = Some(credentials.username.clone());
                
                log::info!("Login command: Login process completed successfully for user: {}", credentials.username);
                
                Ok(LoginResult {
                    success: true,
                    message: format!("Successfully logged in as {}", credentials.username),
                })
            }
            Ok(Err(e)) => {
                log::error!("Login command: Received error response from worker: {}", e);
                Ok(LoginResult {
                    success: false,
                    message: e,
                })
            }
            Err(e) => {
                log::error!("Login command: Failed to receive response from worker: {:?}", e);
                Ok(LoginResult {
                    success: false,
                    message: "Failed to receive login response from worker".to_string(),
                })
            }
        }
    } else {
        log::error!("Login command: No sender available after channel creation");
        Ok(LoginResult {
            success: false,
            message: "Failed to initialize communication channel".to_string(),
        })
    }
}

#[tauri::command]
async fn scan_artist(
    artist: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ScanResult, String> {
    log::info!("Starting artist scan for: {}", artist);

    let app_state = state.lock().await;
    
    if let Some(sender) = &app_state.lastfm_sender {
        let (response_tx, response_rx) = oneshot::channel();
        
        let command = LastFmCommand::ProcessArtist {
            artist: artist.clone(),
            response: response_tx,
        };
        
        if sender.send(command).await.is_err() {
            return Ok(ScanResult {
                success: false,
                message: "Failed to send scan request to worker".to_string(),
                tracks_processed: 0,
            });
        }
        
        // Wait for response
        match response_rx.await {
            Ok(Ok(tracks_processed)) => {
                log::info!("Successfully completed artist scan for: {}", artist);
                Ok(ScanResult {
                    success: true,
                    message: format!("Successfully scanned tracks for artist: {}", artist),
                    tracks_processed,
                })
            }
            Ok(Err(e)) => {
                log::error!("Artist scan failed for {}: {}", artist, e);
                Ok(ScanResult {
                    success: false,
                    message: e,
                    tracks_processed: 0,
                })
            }
            Err(_) => {
                Ok(ScanResult {
                    success: false,
                    message: "Failed to receive scan response".to_string(),
                    tracks_processed: 0,
                })
            }
        }
    } else {
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
async fn get_current_user(state: tauri::State<'_, Arc<Mutex<AppState>>>) -> Result<Option<String>, String> {
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
