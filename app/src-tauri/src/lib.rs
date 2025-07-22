use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

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

#[derive(Debug, Serialize, Deserialize)]
struct FetchTracksRequest {
    artist: Option<String>,
    limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrackInfo {
    name: String,
    artist: String,
    album: Option<String>,
    playcount: u32,
    timestamp: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestRulesRequest {
    rules: Vec<RewriteRule>,
    tracks: Vec<TrackInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TestRulesResult {
    track_results: Vec<TrackTestResult>,
}

#[derive(Debug, Serialize, Deserialize)]
struct TrackTestResult {
    original_track: TrackInfo,
    would_change: bool,
    new_track_name: Option<String>,
    new_artist_name: Option<String>,
    rules_applied: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RewriteRule {
    pub id: String,
    pub name: String,
    pub pattern: String,
    pub replacement: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackProcessingUpdate {
    pub track_name: String,
    pub artist_name: String,
    pub original_track: String,
    pub original_artist: String,
    pub rules_applied: Vec<String>,
    pub status: String, // "processing", "completed", "error"
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestRuleRequest {
    pub pattern: String,
    pub replacement: String,
    pub test_input: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TestRuleResult {
    pub success: bool,
    pub result: String,
    pub error: Option<String>,
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
        progress_sender: Option<std::sync::mpsc::Sender<TrackProcessingUpdate>>,
    },
    FetchTracks {
        artist: Option<String>,
        limit: u32,
        response: oneshot::Sender<Result<Vec<TrackInfo>, String>>,
    },
}

// App state that communicates with scrubber via channel
#[derive(Default)]
pub struct AppState {
    scrubber_sender: Option<mpsc::Sender<LastFmCommand>>,
    current_user: Option<String>,
    processing_broadcast: Option<broadcast::Sender<TrackProcessingUpdate>>,
}

// Worker function that runs in spawn_blocking to handle !Send LastFm operations
fn lastfm_worker_blocking(receiver: std::sync::mpsc::Receiver<LastFmCommand>) {
    log::info!("LastFm worker: Started blocking worker thread");

    // Create a tokio runtime for this thread
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            log::error!("LastFm worker: Failed to create runtime: {e}");
            return;
        }
    };

    let mut scrubber: Option<
        scrobble_scrubber::scrubber::ScrobbleScrubber<
            scrobble_scrubber::persistence::MemoryStorage,
            scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider,
        >,
    > = None;

    log::info!("LastFm worker: Ready to receive commands");

    while let Ok(command) = receiver.recv() {
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
                log::info!("LastFm worker: Processing login command for user: {username}");

                let result = rt.block_on(async {
                    let http_client = http_client::native::NativeClient::new();
                    let lastfm_client = lastfm_edit::LastFmEditClient::new(Box::new(http_client));

                    log::info!("LastFm worker: Attempting to login to Last.fm...");
                    match lastfm_client.login(&username, &password).await {
                        Ok(_) => {
                            log::info!("LastFm worker: Login successful for user: {username}");

                            let storage = Arc::new(Mutex::new(scrobble_scrubber::persistence::MemoryStorage::new()));
                            let rules_state = scrobble_scrubber::persistence::RewriteRulesState::default();
                            let action_provider = scrobble_scrubber::scrub_action_provider::RewriteRulesScrubActionProvider::new(&rules_state);
                            let config = scrobble_scrubber::config::ScrobbleScrubberConfig::default();

                            let new_scrubber = scrobble_scrubber::scrubber::ScrobbleScrubber::new(
                                storage,
                                lastfm_client,
                                action_provider,
                                config,
                            );

                            scrubber = Some(new_scrubber);
                            Ok(())
                        }
                        Err(e) => {
                            log::error!("LastFm worker: Login failed for user {username}: {e}");
                            Err(format!("Login failed: {e}"))
                        }
                    }
                });

                if let Err(e) = response.send(result) {
                    log::error!("LastFm worker: Failed to send login response: {e:?}");
                }
            }
            LastFmCommand::ProcessArtist {
                artist,
                response,
                progress_sender,
            } => {
                log::info!("LastFm worker: Processing artist scan for: {artist}");

                let result = if let Some(ref mut _scrubber_instance) = scrubber {
                    // Simulate processing with progress updates
                    let mut tracks_processed = 0;

                    // For demo purposes, create some fake tracks to process
                    let demo_tracks = [
                        ("Song One", "Song One (feat. Someone)"),
                        ("Track Two", "Track Two - Remastered"),
                        ("Third Song", "Third Song (Live Version)"),
                        ("Another Track", "Another Track [Explicit]"),
                        ("Final Song", "Final Song - Radio Edit"),
                    ];

                    for (clean_name, original_name) in demo_tracks.iter() {
                        // Send processing update
                        if let Some(ref sender) = progress_sender {
                            let update = TrackProcessingUpdate {
                                track_name: clean_name.to_string(),
                                artist_name: artist.clone(),
                                original_track: original_name.to_string(),
                                original_artist: artist.clone(),
                                rules_applied: if clean_name != original_name {
                                    vec!["Remove feat. from track names".to_string()]
                                } else {
                                    vec![]
                                },
                                status: "processing".to_string(),
                            };
                            let _ = sender.send(update);
                        }

                        // Simulate processing time
                        std::thread::sleep(std::time::Duration::from_millis(500));

                        // Send completion update
                        if let Some(ref sender) = progress_sender {
                            let update = TrackProcessingUpdate {
                                track_name: clean_name.to_string(),
                                artist_name: artist.clone(),
                                original_track: original_name.to_string(),
                                original_artist: artist.clone(),
                                rules_applied: if clean_name != original_name {
                                    vec!["Remove feat. from track names".to_string()]
                                } else {
                                    vec![]
                                },
                                status: "completed".to_string(),
                            };
                            let _ = sender.send(update);
                        }

                        tracks_processed += 1;
                    }

                    log::info!(
                        "LastFm worker: Successfully processed {tracks_processed} tracks for artist: {artist}"
                    );
                    Ok(tracks_processed)
                } else {
                    log::warn!("LastFm worker: No scrubber instance available (not logged in)");
                    Err("Not logged in".to_string())
                };

                if let Err(e) = response.send(result) {
                    log::error!("LastFm worker: Failed to send process response: {e:?}");
                }
            }
            LastFmCommand::FetchTracks {
                artist,
                limit,
                response,
            } => {
                log::info!("LastFm worker: Fetching tracks, artist: {artist:?}, limit: {limit}");

                let result = rt.block_on(async {
                    // For this demo, generate some sample tracks
                    let mut tracks = Vec::new();

                    if let Some(artist_name) = &artist {
                        // Generate demo tracks for the specified artist
                        let demo_tracks = [
                            ("Song One", "Song One - 2009 Remaster", "Best Of Album", 42),
                            ("Track Two", "Track Two (Live Version)", "Live Album", 18),
                            ("Third Song", "Third Song [Explicit]", "Studio Album", 33),
                            (
                                "Another Track",
                                "Another Track - Deluxe Edition",
                                "Greatest Hits",
                                27,
                            ),
                            ("Final Song", "Final Song (Radio Edit)", "Singles", 51),
                        ];

                        for (i, (_clean_name, original_name, album, playcount)) in
                            demo_tracks.iter().enumerate()
                        {
                            if tracks.len() >= limit as usize {
                                break;
                            }
                            tracks.push(TrackInfo {
                                name: original_name.to_string(),
                                artist: artist_name.clone(),
                                album: Some(album.to_string()),
                                playcount: *playcount,
                                timestamp: Some(1642000000 + (i as u64 * 86400)), // fake timestamps
                            });
                        }
                    } else {
                        // Generate demo recent tracks
                        let demo_recent = [
                            (
                                "Recent Song 1",
                                "Recent Song 1 - Remastered",
                                "Artist A",
                                "Album A",
                                15,
                            ),
                            (
                                "Recent Song 2",
                                "Recent Song 2 (feat. Someone)",
                                "Artist B",
                                "Album B",
                                8,
                            ),
                            (
                                "Recent Song 3",
                                "Recent Song 3 [Live]",
                                "Artist C",
                                "Album C",
                                22,
                            ),
                            (
                                "Recent Song 4",
                                "Recent Song 4 - Radio Version",
                                "Artist D",
                                "Album D",
                                11,
                            ),
                            (
                                "Recent Song 5",
                                "Recent Song 5 (Deluxe)",
                                "Artist E",
                                "Album E",
                                31,
                            ),
                        ];

                        for (i, (_clean_name, track_name, artist_name, album, playcount)) in
                            demo_recent.iter().enumerate()
                        {
                            if tracks.len() >= limit as usize {
                                break;
                            }
                            tracks.push(TrackInfo {
                                name: track_name.to_string(),
                                artist: artist_name.to_string(),
                                album: Some(album.to_string()),
                                playcount: *playcount,
                                timestamp: Some(1642000000 + (i as u64 * 3600)), // fake timestamps
                            });
                        }
                    }

                    log::info!("LastFm worker: Generated {} demo tracks", tracks.len());
                    Ok(tracks)
                });

                if let Err(e) = response.send(result) {
                    log::error!("LastFm worker: Failed to send fetch tracks response: {e:?}");
                }
            }
        }
    }

    log::warn!("LastFm worker: Channel closed, worker shutting down");
}

// Async wrapper to start the blocking worker
async fn start_lastfm_worker() -> mpsc::Sender<LastFmCommand> {
    let (tx, rx) = std::sync::mpsc::channel();
    let (async_tx, async_rx) = mpsc::channel(32);

    // Spawn the blocking worker in a separate thread
    std::thread::spawn(move || {
        lastfm_worker_blocking(rx);
    });

    // Spawn an async task to bridge between async mpsc and sync mpsc
    let bridge_tx = tx.clone();
    tokio::spawn(async move {
        let mut async_rx = async_rx;
        while let Some(command) = async_rx.recv().await {
            if bridge_tx.send(command).is_err() {
                log::error!("Failed to send command to blocking worker");
                break;
            }
        }
    });

    async_tx
}

#[tauri::command]
async fn login(
    credentials: LoginCredentials,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<LoginResult, String> {
    log::info!("Login request for username: {}", credentials.username);

    // Get or create the worker sender
    let sender = {
        let mut app_state = state.lock().await;

        if app_state.scrubber_sender.is_none() {
            log::info!("Starting LastFm worker");
            let sender = start_lastfm_worker().await;
            app_state.scrubber_sender = Some(sender.clone());
            sender
        } else {
            app_state.scrubber_sender.as_ref().unwrap().clone()
        }
    };

    // Create a channel for the response
    let (response_tx, response_rx) = oneshot::channel();

    // Send login command to worker
    let login_command = LastFmCommand::Login {
        username: credentials.username.clone(),
        password: credentials.password,
        response: response_tx,
    };

    if let Err(e) = sender.send(login_command).await {
        log::error!("Failed to send login command to worker: {e}");
        return Ok(LoginResult {
            success: false,
            message: "Internal error: could not communicate with worker".to_string(),
        });
    }

    // Wait for the response
    match response_rx.await {
        Ok(Ok(())) => {
            log::info!("Login successful for user: {}", credentials.username);
            let mut app_state = state.lock().await;
            app_state.current_user = Some(credentials.username.clone());

            Ok(LoginResult {
                success: true,
                message: format!("Successfully logged in as {}", credentials.username),
            })
        }
        Ok(Err(error_msg)) => {
            log::warn!("Login failed: {error_msg}");
            Ok(LoginResult {
                success: false,
                message: error_msg,
            })
        }
        Err(e) => {
            log::error!("Failed to receive response from worker: {e}");
            Ok(LoginResult {
                success: false,
                message: "Internal error: worker communication failed".to_string(),
            })
        }
    }
}

#[tauri::command]
async fn scan_artist(
    artist: String,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<ScanResult, String> {
    log::info!("Starting artist scan for: {artist}");

    let sender = {
        let app_state = state.lock().await;
        if let Some(ref sender) = app_state.scrubber_sender {
            sender.clone()
        } else {
            log::warn!("No worker available (not logged in)");
            return Ok(ScanResult {
                success: false,
                message: "Not logged in. Please login first.".to_string(),
                tracks_processed: 0,
            });
        }
    };

    // Create a channel for the response
    let (response_tx, response_rx) = oneshot::channel();

    // Create progress channel
    let (progress_tx, progress_rx) = std::sync::mpsc::channel::<TrackProcessingUpdate>();

    // Set up the broadcast sender for this scan session
    {
        let mut app_state = state.lock().await;
        let (broadcast_tx, _) = broadcast::channel(100);
        app_state.processing_broadcast = Some(broadcast_tx.clone());

        // Spawn a task to bridge progress updates to broadcast
        let broadcast_tx_clone = broadcast_tx.clone();
        tokio::task::spawn_blocking(move || {
            while let Ok(update) = progress_rx.recv() {
                let _ = broadcast_tx_clone.send(update);
            }
        });
    }

    // Send process artist command to worker
    let process_command = LastFmCommand::ProcessArtist {
        artist: artist.clone(),
        response: response_tx,
        progress_sender: Some(progress_tx),
    };

    if let Err(e) = sender.send(process_command).await {
        log::error!("Scan artist: Failed to send command to worker: {e}");
        return Ok(ScanResult {
            success: false,
            message: "Internal error: could not communicate with worker".to_string(),
            tracks_processed: 0,
        });
    }

    // Wait for the response
    match response_rx.await {
        Ok(Ok(tracks_processed)) => {
            log::info!("Successfully processed artist: {artist}");
            Ok(ScanResult {
                success: true,
                message: format!("Successfully scanned tracks for artist: {artist}"),
                tracks_processed,
            })
        }
        Ok(Err(error_msg)) => {
            log::error!("Failed to process artist {artist}: {error_msg}");
            Ok(ScanResult {
                success: false,
                message: error_msg,
                tracks_processed: 0,
            })
        }
        Err(e) => {
            log::error!("Scan artist: Failed to receive response from worker: {e}");
            Ok(ScanResult {
                success: false,
                message: "Internal error: worker communication failed".to_string(),
                tracks_processed: 0,
            })
        }
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

#[tauri::command]
async fn get_rewrite_rules() -> Result<Vec<RewriteRule>, String> {
    // For now, return some example rules - in a real implementation,
    // this would come from the scrubber's rule state
    let rules = vec![
        RewriteRule {
            id: "1".to_string(),
            name: "Remove remaster suffixes".to_string(),
            pattern: r"^(.*) - \d{4} (Remaster|Version)$".to_string(),
            replacement: "$1".to_string(),
            enabled: true,
        },
        RewriteRule {
            id: "2".to_string(),
            name: "Clean parenthetical featuring".to_string(),
            pattern: r"^(.*)\s*\(feat\. (.*)\)$".to_string(),
            replacement: "$1 feat. $2".to_string(),
            enabled: true,
        },
        RewriteRule {
            id: "3".to_string(),
            name: "Remove edition markers".to_string(),
            pattern: r"^(.*)\s*\((Deluxe|Extended|Special) Edition\)$".to_string(),
            replacement: "$1".to_string(),
            enabled: false,
        },
    ];

    Ok(rules)
}

#[tauri::command]
async fn test_rule(request: TestRuleRequest) -> Result<TestRuleResult, String> {
    use regex::Regex;

    match Regex::new(&request.pattern) {
        Ok(regex) => {
            let result = regex
                .replace_all(&request.test_input, &request.replacement)
                .to_string();
            Ok(TestRuleResult {
                success: true,
                result,
                error: None,
            })
        }
        Err(e) => Ok(TestRuleResult {
            success: false,
            result: request.test_input.clone(),
            error: Some(format!("Invalid regex pattern: {e}")),
        }),
    }
}

#[tauri::command]
async fn add_rewrite_rule(rule: RewriteRule) -> Result<bool, String> {
    // TODO: Actually add the rule to the scrubber
    log::info!(
        "Adding rewrite rule: {} -> {}",
        rule.pattern,
        rule.replacement
    );
    Ok(true)
}

#[tauri::command]
async fn update_rewrite_rule(rule: RewriteRule) -> Result<bool, String> {
    // TODO: Actually update the rule in the scrubber
    log::info!(
        "Updating rewrite rule {}: {} -> {}",
        rule.id,
        rule.pattern,
        rule.replacement
    );
    Ok(true)
}

#[tauri::command]
async fn delete_rewrite_rule(rule_id: String) -> Result<bool, String> {
    // TODO: Actually delete the rule from the scrubber
    log::info!("Deleting rewrite rule: {rule_id}");
    Ok(true)
}

#[tauri::command]
async fn subscribe_to_processing_updates(
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
    window: tauri::Window,
) -> Result<(), String> {
    let app_state = state.lock().await;

    if let Some(ref broadcast_sender) = app_state.processing_broadcast {
        let mut receiver = broadcast_sender.subscribe();

        // Spawn a task to listen for updates and emit events to the frontend
        tokio::spawn(async move {
            while let Ok(update) = receiver.recv().await {
                let _ = window.emit("processing-update", &update);
            }
        });
    }

    Ok(())
}

#[tauri::command]
async fn fetch_tracks(
    request: FetchTracksRequest,
    state: tauri::State<'_, Arc<Mutex<AppState>>>,
) -> Result<Vec<TrackInfo>, String> {
    log::info!(
        "Fetch tracks request: artist={:?}, limit={:?}",
        request.artist,
        request.limit
    );

    let sender = {
        let app_state = state.lock().await;
        if let Some(ref sender) = app_state.scrubber_sender {
            sender.clone()
        } else {
            log::warn!("No worker available (not logged in)");
            return Err("Not logged in. Please login first.".to_string());
        }
    };

    let limit = request.limit.unwrap_or(100);

    // Create a channel for the response
    let (response_tx, response_rx) = oneshot::channel();

    // Send fetch tracks command to worker
    let fetch_command = LastFmCommand::FetchTracks {
        artist: request.artist.clone(),
        limit,
        response: response_tx,
    };

    if let Err(e) = sender.send(fetch_command).await {
        log::error!("Fetch tracks: Failed to send command to worker: {e}");
        return Err("Internal error: could not communicate with worker".to_string());
    }

    // Wait for the response
    match response_rx.await {
        Ok(Ok(tracks)) => {
            log::info!("Successfully fetched {} tracks", tracks.len());
            Ok(tracks)
        }
        Ok(Err(error_msg)) => {
            log::error!("Failed to fetch tracks: {error_msg}");
            Err(error_msg)
        }
        Err(e) => {
            log::error!("Fetch tracks: Failed to receive response from worker: {e}");
            Err("Internal error: worker communication failed".to_string())
        }
    }
}

#[tauri::command]
async fn test_rules_on_tracks(request: TestRulesRequest) -> Result<TestRulesResult, String> {
    use regex::Regex;

    log::info!(
        "Testing {} rules on {} tracks",
        request.rules.len(),
        request.tracks.len()
    );

    let mut track_results = Vec::new();

    for track in &request.tracks {
        let mut would_change = false;
        let mut new_track_name = None;
        let new_artist_name = None;
        let mut rules_applied = Vec::new();

        let mut current_track_name = track.name.clone();
        let _current_artist_name = track.artist.clone();

        // Apply each rule to see what changes
        for rule in &request.rules {
            if !rule.enabled {
                continue;
            }

            // Test track name rule
            if let Ok(regex) = Regex::new(&rule.pattern) {
                if regex.is_match(&current_track_name) {
                    let new_name = regex
                        .replace_all(&current_track_name, &rule.replacement)
                        .to_string();
                    if new_name != current_track_name {
                        current_track_name = new_name;
                        rules_applied.push(rule.name.clone());
                        would_change = true;
                        new_track_name = Some(current_track_name.clone());
                    }
                }
                // Could also test artist name changes here if needed
            }
        }

        track_results.push(TrackTestResult {
            original_track: track.clone(),
            would_change,
            new_track_name,
            new_artist_name,
            rules_applied,
        });
    }

    log::info!(
        "Rule testing complete: {} tracks would change",
        track_results.iter().filter(|r| r.would_change).count()
    );

    Ok(TestRulesResult { track_results })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .format_timestamp_secs()
        .init();

    log::info!("Starting Tauri application");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(Mutex::new(AppState::default())))
        .invoke_handler(tauri::generate_handler![
            login,
            scan_artist,
            is_logged_in,
            get_current_user,
            get_rewrite_rules,
            test_rule,
            add_rewrite_rule,
            update_rewrite_rule,
            delete_rewrite_rule,
            subscribe_to_processing_updates,
            fetch_tracks,
            test_rules_on_tracks
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
