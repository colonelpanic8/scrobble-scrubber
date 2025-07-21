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
use uuid::Uuid;

use crate::persistence::StateStorage;
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

    let html = format!(
        r#"
<!DOCTYPE html>
<html>
<head>
    <title>Scrobble Scrubber Dashboard</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 20px; }}
        .section {{ margin-bottom: 30px; padding: 20px; border: 1px solid #ddd; }}
        .btn {{ padding: 8px 16px; margin: 4px; background: #007bff; color: white; border: none; cursor: pointer; }}
        .btn.danger {{ background: #dc3545; }}
        .item {{ margin: 10px 0; padding: 10px; background: #f9f9f9; }}
    </style>
</head>
<body>
    <h1>🎵 Scrobble Scrubber Dashboard</h1>

    <div class="section">
        <h2>Pending Edits ({})</h2>
        {}
    </div>

    <div class="section">
        <h2>Pending Rules ({})</h2>
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

    <script>
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
    </script>
</body>
</html>
    "#,
        pending_edits.len(),
        pending_edits
            .iter()
            .take(5)
            .map(|edit| {
                format!(
                    r#"
                <div class="item">
                    <strong>{} - {}</strong><br>
                    <small>ID: {}</small><br>
                    <button class="btn" onclick="handleEdit('{}', 'approve')">Approve</button>
                    <button class="btn danger" onclick="handleEdit('{}', 'reject')">Reject</button>
                </div>
            "#,
                    edit.original_artist_name, edit.original_track_name, edit.id, edit.id, edit.id
                )
            })
            .collect::<String>(),
        pending_rules.len(),
        pending_rules
            .iter()
            .take(5)
            .map(|rule| {
                format!(
                    r#"
                <div class="item">
                    <strong>{}</strong><br>
                    <small>Example: {} - {}</small><br>
                    <button class="btn" onclick="handleRule('{}', 'approve')">Approve</button>
                    <button class="btn danger" onclick="handleRule('{}', 'reject')">Reject</button>
                </div>
            "#,
                    rule.reason,
                    rule.example_artist_name,
                    rule.example_track_name,
                    rule.id,
                    rule.id
                )
            })
            .collect::<String>()
    );

    Ok(Html(html))
}

async fn handle_edit_action<S: StateStorage, P: ScrubActionProvider>(
    State(state): State<WebInterfaceState<S, P>>,
    Path((id, action)): Path<(String, String)>,
) -> Result<Json<ApiResponse>, StatusCode> {
    let edit_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

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
    let rule_id = Uuid::parse_str(&id).map_err(|_| StatusCode::BAD_REQUEST)?;

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
