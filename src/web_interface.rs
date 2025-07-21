use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
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

#[derive(Serialize, Deserialize)]
struct ApiResponse {
    success: bool,
    message: String,
}

pub struct WebInterfaceState<S: StateStorage, P: ScrubActionProvider> {
    pub storage: Arc<Mutex<S>>,
    pub scrubber: Arc<Mutex<ScrobbleScrubber<S, P>>>,
}

impl<S: StateStorage, P: ScrubActionProvider> Clone for WebInterfaceState<S, P> {
    fn clone(&self) -> Self {
        Self {
            storage: self.storage.clone(),
            scrubber: self.scrubber.clone(),
        }
    }
}

pub fn create_router<S: StateStorage + 'static, P: ScrubActionProvider + 'static>(
) -> Router<WebInterfaceState<S, P>> {
    Router::new()
        .route("/", get(dashboard))
        .route("/api/edits/:id/:action", post(handle_edit_action))
        .route("/api/rules/:id/:action", post(handle_rule_action))
        .route("/api/scrubber/status", get(scrubber_status))
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

pub async fn start_web_server<S: StateStorage + 'static, P: ScrubActionProvider + 'static>(
    storage: Arc<Mutex<S>>,
    scrubber: Arc<Mutex<ScrobbleScrubber<S, P>>>,
    port: u16,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let state = WebInterfaceState { storage, scrubber };

    let app = create_router().with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}")).await?;

    log::info!("Web interface available at http://localhost:{port}");

    axum::serve(listener, app).await?;

    Ok(())
}
