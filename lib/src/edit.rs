use lastfm_edit::{EditResponse, LastFmEditClient, ScrobbleEdit};
use std::time::Duration;

/// Result type for edit operations
pub type EditResult = Result<EditResponse, String>;

/// Apply an edit in dry-run mode (logs the edit but doesn't execute it)
pub async fn dry_run_edit(edit: &ScrobbleEdit) -> EditResult {
    log::info!("DRY RUN: Would apply edit: {edit}");
    // Return a mock successful response for dry run
    Ok(EditResponse {
        individual_results: Vec::new(),
    })
}

/// Actually apply an edit to Last.fm using a client with timeout
pub async fn apply_edit_to_lastfm(
    client: &dyn LastFmEditClient,
    edit: &ScrobbleEdit,
    timeout: Duration,
) -> EditResult {
    match tokio::time::timeout(timeout, client.edit_scrobble(edit)).await {
        Ok(Ok(result)) => {
            log::info!("Successfully applied edit to Last.fm: {edit}");
            Ok(result)
        }
        Ok(Err(e)) => {
            log::error!("Failed to apply edit to Last.fm: {e}");
            Err(format!("Failed to apply edit to Last.fm: {e}"))
        }
        Err(_) => {
            log::error!("Timeout applying edit to Last.fm after {timeout:?}");
            Err(format!(
                "Timeout applying edit to Last.fm after {timeout:?}"
            ))
        }
    }
}

/// Create a pending edit (logs the intent but doesn't apply immediately)
/// This is used when confirmation is required
pub async fn create_pending_edit(edit: &ScrobbleEdit) -> EditResult {
    log::info!("Creating pending edit for confirmation: {edit}");
    // Return a mock response indicating the edit is pending
    Ok(EditResponse {
        individual_results: Vec::new(),
    })
}
