use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
// use uuid::Uuid;

use crate::persistence::StateStorage;
use crate::rewrite::{RewriteRule, SdRule};
use crate::scrub_action_provider::ScrubActionProvider;
use crate::scrubber::ScrobbleScrubber;

#[derive(Serialize, Deserialize)]
struct ApproveEditRequest {
    action: String, // "approve" or "reject"
}

#[derive(Serialize, Deserialize)]
struct ApproveRuleRequest {
    action: String, // "approve" or "reject"
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct ProcessArtistRequest {
    artist_name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct FetchTracksRequest {
    artist: Option<String>,
    limit: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TrackInfo {
    name: String,
    artist: String,
    album: Option<String>,
    playcount: u32,
    timestamp: Option<u64>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TestRulesRequest {
    rules: Vec<RewriteRule>,
    tracks: Vec<TrackInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TestRulesResult {
    track_results: Vec<TrackTestResult>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
struct TrackTestResult {
    original_track: TrackInfo,
    would_change: bool,
    new_track_name: Option<String>,
    new_artist_name: Option<String>,
    rules_applied: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

// Channel message types for background processing
#[derive(Debug)]
pub enum ProcessingRequest {
    ProcessArtist {
        artist_name: String,
        response_tx: oneshot::Sender<Result<(), String>>,
    },
}

pub type ProcessingRequestSender = mpsc::UnboundedSender<ProcessingRequest>;

fn format_sd_rule(rule: &SdRule, field_name: &str) -> String {
    let rule_type = "<span style='color: #dc3545; font-weight: bold;'>regex</span>";

    let flags_str = rule
        .flags
        .as_ref()
        .map(|f| format!(" <span style='color: #6c757d;'>(flags: {f})</span>"))
        .unwrap_or_default();

    // Note: max_replacements is no longer supported in the simplified API
    let max_replacements_str = String::new();

    format!(
        "&nbsp;&nbsp;<strong style='color: #495057;'>{}:</strong> {} <code style='background: #e9ecef; padding: 2px 4px;'>\"{}\"</code> → <code style='background: #e9ecef; padding: 2px 4px;'>\"{}\"</code>{}{}",
        field_name,
        rule_type,
        html_escape(&rule.find),
        html_escape(&rule.replace),
        flags_str,
        max_replacements_str
    )
}

fn format_rule_details(rule: &RewriteRule) -> String {
    let mut details = Vec::new();

    if let Some(track_rule) = &rule.track_name {
        details.push(format_sd_rule(track_rule, "Track Name"));
    }

    if let Some(artist_rule) = &rule.artist_name {
        details.push(format_sd_rule(artist_rule, "Artist Name"));
    }

    if let Some(album_rule) = &rule.album_name {
        details.push(format_sd_rule(album_rule, "Album Name"));
    }

    if let Some(album_artist_rule) = &rule.album_artist_name {
        details.push(format_sd_rule(album_artist_rule, "Album Artist"));
    }

    if rule.requires_confirmation {
        details.push("&nbsp;&nbsp;<span style='color: #ffc107; font-weight: bold;'>⚠️ Requires confirmation</span>".to_string());
    }

    if details.is_empty() {
        "No transformations defined".to_string()
    } else {
        details.join("<br>")
    }
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

pub struct WebInterfaceState<S: StateStorage, P: ScrubActionProvider> {
    pub storage: Arc<Mutex<S>>,
    pub scrubber: Arc<Mutex<ScrobbleScrubber<S, P>>>,
    pub processing_tx: ProcessingRequestSender,
}

impl<S: StateStorage, P: ScrubActionProvider> Clone for WebInterfaceState<S, P> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            scrubber: self.scrubber.clone(),
            processing_tx: self.processing_tx.clone(),
        }
    }
}

pub fn create_router<
    S: StateStorage + Send + Sync + 'static,
    P: ScrubActionProvider + Send + Sync + 'static,
>() -> Router<WebInterfaceState<S, P>> {
    Router::new()
        .route("/", get(dashboard))
        .route("/api/edits/:id/:action", post(handle_edit_action))
        .route("/api/rules/:id/:action", post(handle_rule_action))
        .route("/api/scrubber/status", get(scrubber_status))
        .route("/api/scrubber/process-artist", post(process_artist))
        .route(
            "/api/workshop/fetch-tracks",
            post(fetch_tracks_for_workshop::<S, P>),
        )
        .route(
            "/api/workshop/test-rules",
            post(test_rules_on_tracks::<S, P>),
        )
}

async fn dashboard<S: StateStorage, P: ScrubActionProvider>(
    State(state): State<WebInterfaceState<S, P>>,
) -> Result<Html<String>, StatusCode> {
    let storage = state.storage.lock().await;

    let pending_edits = storage
        .load_pending_edits_state()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .pending_edits;

    let pending_rules = storage
        .load_pending_rewrite_rules_state()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .pending_rules;

    let existing_rules = storage
        .load_rewrite_rules_state()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .rewrite_rules;

    let html = format!(
        r#"
<!DOCTYPE html>
<html>
<head>
    <title>Scrobble Scrubber Dashboard</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 0; }}
        .header {{ background: #343a40; color: white; padding: 15px 20px; }}
        .header h1 {{ margin: 0; }}
        .tabs {{ background: #f8f9fa; border-bottom: 1px solid #dee2e6; }}
        .tab-buttons {{ display: flex; margin: 0; padding: 0; list-style: none; }}
        .tab-button {{ padding: 12px 24px; cursor: pointer; border-bottom: 3px solid transparent; transition: all 0.3s; }}
        .tab-button.active {{ border-bottom-color: #007bff; background: white; }}
        .tab-button:hover {{ background: #e9ecef; }}
        .tab-content {{ display: none; padding: 20px; }}
        .tab-content.active {{ display: block; }}
        .section {{ margin-bottom: 30px; padding: 20px; border: 1px solid #ddd; }}
        .btn {{ padding: 8px 16px; margin: 4px; background: #007bff; color: white; border: none; cursor: pointer; border-radius: 4px; }}
        .btn.danger {{ background: #dc3545; }}
        .btn:hover {{ opacity: 0.8; }}
        .item {{ margin: 10px 0; padding: 15px; background: #f9f9f9; border-radius: 6px; }}
        .rule-details {{ margin: 10px 0; padding: 12px; background: #f0f0f0; border-radius: 4px; font-family: 'Courier New', monospace; font-size: 14px; line-height: 1.4; }}
        .rule-details strong {{ color: #2c3e50; }}
        .transformation-preview {{ margin: 10px 0; padding: 12px; background: #e8f5e8; border: 2px solid #4CAF50; border-radius: 4px; font-size: 14px; line-height: 1.4; }}
        .transformation-preview strong {{ color: #2e7d32; }}
        .track-result {{ margin: 8px 0; padding: 12px; background: #f9f9f9; border: 2px solid #e0e0e0; border-radius: 4px; }}
        .track-result.changed {{ background: #eafaf1; border-color: #28a745; }}
        .track-original {{ font-weight: bold; color: #333; }}
        .track-changed {{ margin-top: 5px; color: #28a745; }}
        .track-arrow {{ font-weight: bold; font-size: 16px; color: #28a745; margin: 0 8px; }}
        .track-rules {{ font-size: 12px; color: #666; margin-top: 5px; }}
        .btn:disabled {{ background: #6c757d; cursor: not-allowed; }}
        .workshop-controls {{ display: flex; gap: 15px; align-items: flex-end; margin-bottom: 20px; flex-wrap: wrap; }}
        .form-group {{ display: flex; flex-direction: column; }}
        .form-group label {{ font-weight: bold; margin-bottom: 5px; }}
        .form-group input {{ padding: 8px; border: 1px solid #ccc; border-radius: 4px; }}
        .tracks-info {{ padding: 10px; background: #d4edda; border: 1px solid #c3e6cb; border-radius: 4px; margin-bottom: 15px; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>🎵 Scrobble Scrubber Dashboard</h1>
    </div>
    
    <div class="tabs">
        <ul class="tab-buttons">
            <li class="tab-button active" onclick="showTab('dashboard')">Dashboard</li>
            <li class="tab-button" onclick="showTab('workshop')">Rule Workshop</li>
        </ul>
    </div>

    <div id="dashboard" class="tab-content active">
        <div class="section">
            <h2>Pending Edits ({})</h2>
            {}
        </div>

        <div class="section">
            <h2>Pending Rules ({})</h2>
            {}
        </div>

        <div class="section">
            <h2>Active Rewrite Rules ({})</h2>
            {}
        </div>

        <div class="section">
            <h2>Process Artist</h2>
            <form id="artistForm">
                <input type="text" id="artistName" placeholder="Enter artist name..." style="width: 300px; padding: 8px;">
                <button type="submit" class="btn" style="margin-left: 10px;">Process Artist</button>
            </form>
            <div id="artistStatus" style="margin-top: 10px; font-weight: bold;"></div>
        </div>
    </div>

    <div id="workshop" class="tab-content">
        <div class="section">
            <h2>🔧 Rule Workshop</h2>
            <p>Test how your rewrite rules would apply to recent tracks or tracks from a specific artist.</p>
            
            <div class="workshop-controls">
                <div class="form-group">
                    <label for="workshopArtist">Artist (optional):</label>
                    <input type="text" id="workshopArtist" placeholder="Leave empty for recent tracks" style="width: 250px;">
                </div>
                <div class="form-group">
                    <label for="workshopLimit">Limit:</label>
                    <input type="number" id="workshopLimit" value="100" min="1" max="500" style="width: 80px;">
                </div>
                <button id="loadTracksBtn" class="btn">Load Tracks</button>
            </div>
            
            <div id="tracksInfo" class="tracks-info" style="display: none;">
                <strong id="tracksCount"></strong>
            </div>
            
            <div id="workshopResults" style="margin-top: 20px;"></div>
        </div>
    </div>

    <script>
        function showTab(tabName) {{
            // Hide all tab contents
            document.querySelectorAll('.tab-content').forEach(tab => {{
                tab.classList.remove('active');
            }});
            
            // Remove active class from all tab buttons
            document.querySelectorAll('.tab-button').forEach(button => {{
                button.classList.remove('active');
            }});
            
            // Show selected tab content
            document.getElementById(tabName).classList.add('active');
            
            // Add active class to clicked button
            event.target.classList.add('active');
        }}

        async function handleEdit(id, action) {{
            const response = await fetch(`/api/edits/${{id}}/${{action}}`, {{ method: 'POST' }});
            const result = await response.json();
            alert(result.message);
            if (result.success) location.reload();
        }}

        async function handleRule(id, action) {{
            const response = await fetch(`/api/rules/${{id}}/${{action}}`, {{ method: 'POST' }});
            const result = await response.json();
            alert(result.message);
            if (result.success) location.reload();
        }}

        document.getElementById('artistForm').addEventListener('submit', async function(e) {{
            e.preventDefault();
            const artistName = document.getElementById('artistName').value.trim();
            const statusDiv = document.getElementById('artistStatus');

            if (!artistName) {{
                statusDiv.textContent = 'Please enter an artist name';
                statusDiv.style.color = 'red';
                return;
            }}

            statusDiv.textContent = 'Processing...';
            statusDiv.style.color = 'blue';

            try {{
                const response = await fetch('/api/scrubber/process-artist', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{ artist_name: artistName }})
                }});

                const result = await response.json();
                statusDiv.textContent = result.message;
                statusDiv.style.color = result.success ? 'green' : 'red';

                if (result.success) {{
                    document.getElementById('artistName').value = '';
                    setTimeout(() => location.reload(), 2000);
                }}
            }} catch (error) {{
                statusDiv.textContent = 'Error: ' + error.message;
                statusDiv.style.color = 'red';
            }}
        }});

        // Workshop functionality
        let workshopTracks = [];
        
        document.getElementById('loadTracksBtn').addEventListener('click', async function() {{
            const artistName = document.getElementById('workshopArtist').value.trim();
            const limit = parseInt(document.getElementById('workshopLimit').value) || 100;
            const btn = this;
            const tracksInfo = document.getElementById('tracksInfo');
            const tracksCount = document.getElementById('tracksCount');
            const resultsDiv = document.getElementById('workshopResults');
            
            btn.textContent = 'Loading...';
            btn.disabled = true;
            
            try {{
                const response = await fetch('/api/workshop/fetch-tracks', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{
                        artist: artistName || null,
                        limit: limit
                    }})
                }});
                
                if (response.ok) {{
                    workshopTracks = await response.json();
                    tracksCount.textContent = `Loaded ${{workshopTracks.length}} tracks`;
                    tracksInfo.style.display = 'block';
                    
                    // Automatically test rules when tracks are loaded
                    await testRulesOnTracks();
                }} else {{
                    alert('Failed to fetch tracks');
                }}
            }} catch (error) {{
                alert('Error: ' + error.message);
            }} finally {{
                btn.textContent = 'Load Tracks';
                btn.disabled = false;
            }}
        }});
        
        async function testRulesOnTracks() {{
            if (workshopTracks.length === 0) return;
            
            const resultsDiv = document.getElementById('workshopResults');
            resultsDiv.innerHTML = '<p>Testing rules...</p>';
            
            try {{
                // Get current active rules
                const activeRulesJson = document.querySelector('script').textContent.match(/serde_json::to_string\(&existing_rules\).*?"(.*?)"/)?.[1] || '[]';
                const activeRules = JSON.parse(activeRulesJson.replace(/&quot;/g, '"'));
                
                const response = await fetch('/api/workshop/test-rules', {{
                    method: 'POST',
                    headers: {{ 'Content-Type': 'application/json' }},
                    body: JSON.stringify({{
                        rules: activeRules,
                        tracks: workshopTracks
                    }})
                }});
                
                if (response.ok) {{
                    const results = await response.json();
                    displayWorkshopResults(results);
                }} else {{
                    resultsDiv.innerHTML = '<p>Failed to test rules</p>';
                }}
            }} catch (error) {{
                resultsDiv.innerHTML = '<p>Error: ' + error.message + '</p>';
            }}
        }}
        
        function displayWorkshopResults(results) {{
            const resultsDiv = document.getElementById('workshopResults');
            const affectedCount = results.track_results.filter(r => r.would_change).length;
            
            let html = `<h3>Rule Test Results: ${{affectedCount}} of ${{results.track_results.length}} tracks would change</h3>`;
            
            results.track_results.forEach(result => {{
                const trackClass = result.would_change ? 'track-result changed' : 'track-result';
                const albumInfo = result.original_track.album ? ` from ${{result.original_track.album}}` : '';
                
                html += `<div class="${{trackClass}}">`;
                html += `<div class="track-original">${{result.original_track.artist}} - ${{result.original_track.name}}${{albumInfo}}</div>`;
                
                if (result.would_change) {{
                    const newTrackName = result.new_track_name || result.original_track.name;
                    const newArtistName = result.new_artist_name || result.original_track.artist;
                    html += `<div class="track-changed">`;
                    html += `<span class="track-arrow">→</span>`;
                    html += `<strong>${{newArtistName}} - ${{newTrackName}}</strong>`;
                    html += `</div>`;
                    if (result.rules_applied.length > 0) {{
                        html += `<div class="track-rules">Rules: ${{result.rules_applied.join(', ')}}</div>`;
                    }}
                }} else {{
                    html += `<div style="font-style: italic; color: #666; margin-top: 5px;">(no change)</div>`;
                }}
                
                html += `</div>`;
            }});
            
            resultsDiv.innerHTML = html;
        }}
    </script>
</body>
</html>
    "#,
        pending_edits.len(),
        pending_edits
            .iter()
            .take(5)
            .fold(String::new(), |mut acc, edit| {
                use std::fmt::Write;
                let _ = write!(
                    acc,
                    r#"
                <div class="item">
                    <strong>{} - {}</strong><br>
                    <small>ID: {}</small><br>
                    <button class="btn" onclick="handleEdit('{}', 'approve')">Approve</button>
                    <button class="btn danger" onclick="handleEdit('{}', 'reject')">Reject</button>
                </div>
            "#,
                    edit.original_artist_name, edit.original_track_name, edit.id, edit.id, edit.id
                );
                acc
            }),
        pending_rules.len(),
        pending_rules
            .iter()
            .take(5)
            .fold(String::new(), |mut acc, rule| {
                use std::fmt::Write;
                let rule_details = format_rule_details(&rule.rule);
                let transformation_preview = match rule.apply_rule_to_example() {
                    Ok(transformed) => {
                        let mut changes = Vec::new();
                        if let Some(new_track) = &transformed.transformed_track_name {
                            changes.push(format!(
                                "Track: {} → <strong>{}</strong>",
                                html_escape(&transformed.original_track_name),
                                html_escape(new_track)
                            ));
                        }
                        if let Some(new_artist) = &transformed.transformed_artist_name {
                            changes.push(format!(
                                "Artist: {} → <strong>{}</strong>",
                                html_escape(&transformed.original_artist_name),
                                html_escape(new_artist)
                            ));
                        }
                        if let Some(new_album) = &transformed.transformed_album_name {
                            if let Some(orig_album) = &transformed.original_album_name {
                                changes.push(format!(
                                    "Album: {} → <strong>{}</strong>",
                                    html_escape(orig_album),
                                    html_escape(new_album)
                                ));
                            }
                        }
                        if let Some(new_album_artist) = &transformed.transformed_album_artist_name {
                            if let Some(orig_album_artist) = &transformed.original_album_artist_name
                            {
                                changes.push(format!(
                                    "Album Artist: {} → <strong>{}</strong>",
                                    html_escape(orig_album_artist),
                                    html_escape(new_album_artist)
                                ));
                            }
                        }
                        if changes.is_empty() {
                            "<em>No changes would be made to this example</em>".to_string()
                        } else {
                            changes.join("<br>")
                        }
                    }
                    Err(e) => format!(
                        "<em>Error applying rule: {}</em>",
                        html_escape(&e.to_string())
                    ),
                };

                let _ = write!(
                    acc,
                    r#"
                <div class="item">
                    <strong>{}</strong><br>
                    <small>Example track: {} - {}</small><br>
                    <div class="rule-details">
                        <strong>Rule Details:</strong><br>
                        {}
                    </div>
                    <div class="transformation-preview">
                        <strong>Example Transformation:</strong><br>
                        {}
                    </div>
                    <button class="btn" onclick="handleRule('{}', 'approve')">Approve</button>
                    <button class="btn danger" onclick="handleRule('{}', 'reject')">Reject</button>
                </div>
            "#,
                    html_escape(&rule.reason),
                    html_escape(&rule.example_artist_name),
                    html_escape(&rule.example_track_name),
                    rule_details,
                    transformation_preview,
                    rule.id,
                    rule.id
                );
                acc
            }),
        existing_rules.len(),
        existing_rules
            .iter()
            .take(10)
            .fold(String::new(), |mut acc, rule| {
                use std::fmt::Write;
                let rule_details = format_rule_details(rule);
                let _ = write!(
                    acc,
                    r#"
                <div class="item">
                    <div class="rule-details">
                        {rule_details}
                    </div>
                </div>
            "#
                );
                acc
            })
    );

    // Add the active rules as a JSON script for the workshop
    let rules_script = format!(
        "<script>window.activeRules = {};</script>",
        serde_json::to_string(&existing_rules).unwrap_or_else(|_| "[]".to_string())
    );

    Ok(Html(format!("{html}\n{rules_script}")))
}

async fn handle_edit_action<S: StateStorage, P: ScrubActionProvider>(
    State(state): State<WebInterfaceState<S, P>>,
    Path((id, action)): Path<(String, String)>,
) -> Result<Json<ApiResponse>, StatusCode> {
    let edit_id = id; // Now using String IDs directly

    if action != "approve" && action != "reject" {
        return Ok(Json(ApiResponse {
            success: false,
            message: "Invalid action. Use 'approve' or 'reject'".to_string(),
        }));
    }

    let mut storage = state.storage.lock().await;

    // Load current pending edits
    let mut pending_edits_state = storage
        .load_pending_edits_state()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Find and remove the edit
    let edit_index = pending_edits_state
        .pending_edits
        .iter()
        .position(|edit| edit.id == edit_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let pending_edit = pending_edits_state.pending_edits.remove(edit_index);

    // Save updated pending edits
    storage
        .save_pending_edits_state(&pending_edits_state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let message = if action == "approve" {
        // TODO: Apply the edit to Last.fm here
        format!(
            "Edit approved: {} - {}",
            pending_edit.original_artist_name, pending_edit.original_track_name
        )
    } else {
        format!(
            "Edit rejected: {} - {}",
            pending_edit.original_artist_name, pending_edit.original_track_name
        )
    };

    Ok(Json(ApiResponse {
        success: true,
        message,
    }))
}

async fn handle_rule_action<S: StateStorage, P: ScrubActionProvider>(
    State(state): State<WebInterfaceState<S, P>>,
    Path((id, action)): Path<(String, String)>,
) -> Result<Json<ApiResponse>, StatusCode> {
    let rule_id = id; // Now using String IDs directly

    if action != "approve" && action != "reject" {
        return Ok(Json(ApiResponse {
            success: false,
            message: "Invalid action. Use 'approve' or 'reject'".to_string(),
        }));
    }

    let mut storage = state.storage.lock().await;

    // Load current pending rules
    let mut pending_rules_state = storage
        .load_pending_rewrite_rules_state()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Find and remove the rule
    let rule_index = pending_rules_state
        .pending_rules
        .iter()
        .position(|rule| rule.id == rule_id)
        .ok_or(StatusCode::NOT_FOUND)?;

    let pending_rule = pending_rules_state.pending_rules.remove(rule_index);

    // Save updated pending rules
    storage
        .save_pending_rewrite_rules_state(&pending_rules_state)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let message = if action == "approve" {
        // Add to active rewrite rules
        let mut rewrite_rules_state = storage
            .load_rewrite_rules_state()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        rewrite_rules_state
            .rewrite_rules
            .push(pending_rule.rule.clone());

        storage
            .save_rewrite_rules_state(&rewrite_rules_state)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        format!("Rule approved and added: {}", pending_rule.reason)
    } else {
        format!("Rule rejected: {}", pending_rule.reason)
    };

    Ok(Json(ApiResponse {
        success: true,
        message,
    }))
}

async fn scrubber_status<S: StateStorage, P: ScrubActionProvider>(
    State(state): State<WebInterfaceState<S, P>>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let scrubber = state.scrubber.lock().await;
    let is_running = scrubber.is_running().await;

    Ok(Json(serde_json::json!({
        "is_running": is_running,
        "status": if is_running { "running" } else { "idle" }
    })))
}

async fn process_artist<S: StateStorage + 'static, P: ScrubActionProvider + 'static>(
    State(state): State<WebInterfaceState<S, P>>,
    Json(request): Json<ProcessArtistRequest>,
) -> Result<Json<ApiResponse>, StatusCode> {
    if request.artist_name.trim().is_empty() {
        return Ok(Json(ApiResponse {
            success: false,
            message: "Artist name cannot be empty".to_string(),
        }));
    }

    {
        let scrubber = state.scrubber.lock().await;
        // Check if scrubber is already running
        if scrubber.is_running().await {
            return Ok(Json(ApiResponse {
                success: false,
                message: "Scrubber is already running, please wait".to_string(),
            }));
        }
    }

    // Process artist using channel-based approach
    let artist_name = request.artist_name.trim().to_string();

    // Create oneshot channel for response
    let (response_tx, response_rx) = oneshot::channel();

    // Send processing request via channel
    let request = ProcessingRequest::ProcessArtist {
        artist_name: artist_name.clone(),
        response_tx,
    };

    if state.processing_tx.send(request).is_err() {
        return Ok(Json(ApiResponse {
            success: false,
            message: "Processing service is unavailable".to_string(),
        }));
    }

    // Wait for response from background worker
    match response_rx.await {
        Ok(Ok(())) => Ok(Json(ApiResponse {
            success: true,
            message: format!("Successfully processed artist '{artist_name}'"),
        })),
        Ok(Err(e)) => Ok(Json(ApiResponse {
            success: false,
            message: format!("Failed to process artist '{artist_name}': {e}"),
        })),
        Err(_) => Ok(Json(ApiResponse {
            success: false,
            message: "Processing service did not respond".to_string(),
        })),
    }
}

// Background worker task that processes requests
pub async fn processing_worker<S: StateStorage + 'static, P: ScrubActionProvider + 'static>(
    mut receiver: mpsc::UnboundedReceiver<ProcessingRequest>,
    scrubber: Arc<Mutex<ScrobbleScrubber<S, P>>>,
) {
    while let Some(request) = receiver.recv().await {
        match request {
            ProcessingRequest::ProcessArtist {
                artist_name,
                response_tx,
            } => {
                log::info!("Got process artist request '{artist_name}'");
                // Process in blocking context since it's not Send
                let scrubber_clone = scrubber.clone();
                let artist_name_clone = artist_name.clone();

                let result = tokio::task::spawn_blocking(move || {
                    tokio::runtime::Handle::current().block_on(async move {
                        let mut scrubber = scrubber_clone.lock().await;
                        scrubber.process_artist(&artist_name_clone).await
                    })
                })
                .await;

                let response = match result {
                    Ok(Ok(())) => {
                        log::info!("Successfully processed artist '{artist_name}'");
                        Ok(())
                    }
                    Ok(Err(e)) => {
                        log::error!("Failed to process artist '{artist_name}': {e}");
                        Err(e.to_string())
                    }
                    Err(e) => {
                        log::error!("Task error processing artist '{artist_name}': {e}");
                        Err(format!("Task error: {e}"))
                    }
                };

                // Send response back (ignore if receiver dropped)
                let _ = response_tx.send(response);
            }
        }
    }
}

pub async fn start_web_server<
    S: StateStorage + Send + Sync + 'static,
    P: ScrubActionProvider + Send + Sync + 'static,
>(
    storage: Arc<Mutex<S>>,
    scrubber: Arc<Mutex<ScrobbleScrubber<S, P>>>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Create processing channel
    let (processing_tx, processing_rx) = mpsc::unbounded_channel();

    // Start background worker
    let worker_scrubber = scrubber.clone();
    tokio::spawn(async move {
        processing_worker(processing_rx, worker_scrubber).await;
    });

    let state = WebInterfaceState {
        storage,
        scrubber,
        processing_tx,
    };

    let app = create_router().with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;

    log::info!("Web interface available at http://localhost:{port}");

    axum::serve(listener, app).await?;

    Ok(())
}

async fn fetch_tracks_for_workshop<S: StateStorage, P: ScrubActionProvider>(
    State(state): State<WebInterfaceState<S, P>>,
    Json(request): Json<FetchTracksRequest>,
) -> Result<Json<Vec<TrackInfo>>, StatusCode> {
    log::info!("=== FETCH TRACKS REQUEST RECEIVED ===");
    log::info!("Artist: {:?}, Limit: {:?}", request.artist, request.limit);

    let limit = request.limit.unwrap_or(100);

    let lastfm_tracks = match &request.artist {
        Some(artist_name) => {
            log::info!("Fetching tracks for artist: {artist_name}");
            let mut scrubber = state.scrubber.lock().await;
            scrubber
                .fetch_artist_tracks(artist_name, limit)
                .await
                .map_err(|e| {
                    log::error!("Failed to fetch artist tracks: {e}");
                    StatusCode::INTERNAL_SERVER_ERROR
                })?
        }
        None => {
            log::info!("Fetching recent tracks");
            let mut scrubber = state.scrubber.lock().await;
            scrubber.fetch_recent_tracks(limit).await.map_err(|e| {
                log::error!("Failed to fetch recent tracks: {e}");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
        }
    };

    // Convert lastfm_edit::Track to TrackInfo
    let tracks: Vec<TrackInfo> = lastfm_tracks
        .into_iter()
        .map(|track| TrackInfo {
            name: track.name,
            artist: track.artist,
            album: track.album,
            playcount: track.playcount,
            timestamp: track.timestamp,
        })
        .collect();

    log::info!(
        "Fetched {} real tracks for workshop (artist: {:?})",
        tracks.len(),
        request.artist
    );
    Ok(Json(tracks))
}

async fn test_rules_on_tracks<S: StateStorage, P: ScrubActionProvider>(
    State(_state): State<WebInterfaceState<S, P>>,
    Json(request): Json<TestRulesRequest>,
) -> Result<Json<TestRulesResult>, StatusCode> {
    log::info!(
        "Testing {} rules on {} tracks",
        request.rules.len(),
        request.tracks.len()
    );

    let mut track_results = Vec::new();

    for track in &request.tracks {
        let mut would_change = false;
        let mut new_track_name = None;
        let mut new_artist_name = None;
        let mut rules_applied = Vec::new();

        let mut current_track_name = track.name.clone();
        let current_artist_name = track.artist.clone();

        // Apply each rule to see what changes
        for rule in &request.rules {
            // Test track name rule if it exists
            if let Some(track_rule) = &rule.track_name {
                if let Ok(result) = track_rule.apply(&current_track_name) {
                    if result != current_track_name {
                        current_track_name = result;
                        rules_applied.push("Track name rewrite".to_string());
                        would_change = true;
                        new_track_name = Some(current_track_name.clone());
                    }
                }
            }

            // Test artist name rule if it exists
            if let Some(artist_rule) = &rule.artist_name {
                if let Ok(result) = artist_rule.apply(&current_artist_name) {
                    if result != current_artist_name {
                        rules_applied.push("Artist name rewrite".to_string());
                        would_change = true;
                        new_artist_name = Some(result);
                    }
                }
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

    let affected_count = track_results.iter().filter(|r| r.would_change).count();
    log::info!("Rule testing complete: {affected_count} tracks would change");

    Ok(Json(TestRulesResult { track_results }))
}
