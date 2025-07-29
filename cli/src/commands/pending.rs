use clap::{Args, Subcommand};
use scrobble_scrubber::config::ScrobbleScrubberConfig;
use scrobble_scrubber::persistence::{FileStorage, PendingEdit, PendingEditsState, StateStorage};
use std::path::PathBuf;

#[derive(Args, Debug, Clone)]
pub struct PendingArgs {
    #[command(subcommand)]
    pub command: PendingCommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PendingCommands {
    /// List all pending edits
    List,
    /// Apply a pending edit by ID
    Apply {
        /// ID of the pending edit to apply
        id: String,
    },
    /// Reject a pending edit by ID (remove without applying)
    Reject {
        /// ID of the pending edit to reject
        id: String,
    },
    /// Clear all pending edits
    Clear,
}

pub async fn handle_pending_command(
    args: PendingArgs,
    data_dir: PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut storage = FileStorage::new(data_dir)?;

    match args.command {
        PendingCommands::List => list_pending_edits(&storage).await,
        PendingCommands::Apply { id } => apply_pending_edit(&mut storage, &id).await,
        PendingCommands::Reject { id } => reject_pending_edit(&mut storage, &id).await,
        PendingCommands::Clear => clear_pending_edits(&mut storage).await,
    }
}

async fn list_pending_edits(
    storage: &FileStorage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pending_edits_state = storage.load_pending_edits_state().await?;

    if pending_edits_state.pending_edits.is_empty() {
        println!("No pending edits found.");
        return Ok(());
    }

    println!(
        "Pending Edits ({}):",
        pending_edits_state.pending_edits.len()
    );
    println!("{}", "=".repeat(80));

    for edit in &pending_edits_state.pending_edits {
        print_pending_edit(edit);
        println!("{}", "-".repeat(80));
    }

    Ok(())
}

async fn apply_pending_edit(
    storage: &mut FileStorage,
    id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut pending_edits_state = storage.load_pending_edits_state().await?;

    let edit_index = pending_edits_state
        .pending_edits
        .iter()
        .position(|edit| edit.id == id)
        .ok_or_else(|| format!("Pending edit with ID '{id}' not found"))?;

    let pending_edit = pending_edits_state.pending_edits.remove(edit_index);

    // Save the updated pending edits state
    storage
        .save_pending_edits_state(&pending_edits_state)
        .await?;

    println!("Applied pending edit:");
    print_pending_edit(&pending_edit);

    let scrobble_edit = pending_edit.to_scrobble_edit();

    // Apply the edit directly with the authenticated client
    let config = ScrobbleScrubberConfig::load()?;
    let client = crate::create_authenticated_client(&config).await?;

    match client.edit_scrobble(&scrobble_edit).await {
        Ok(_) => {
            println!("✓ Successfully applied edit to Last.fm");
        }
        Err(e) => {
            // Re-add the edit back to pending if it failed
            let mut pending_edits_state = storage.load_pending_edits_state().await?;
            pending_edits_state.pending_edits.push(pending_edit);
            storage
                .save_pending_edits_state(&pending_edits_state)
                .await?;

            return Err(format!("Failed to apply edit to Last.fm: {e}").into());
        }
    }

    Ok(())
}

async fn reject_pending_edit(
    storage: &mut FileStorage,
    id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut pending_edits_state = storage.load_pending_edits_state().await?;

    let edit_index = pending_edits_state
        .pending_edits
        .iter()
        .position(|edit| edit.id == id)
        .ok_or_else(|| format!("Pending edit with ID '{id}' not found"))?;

    let pending_edit = pending_edits_state.pending_edits.remove(edit_index);

    // Save the updated pending edits state
    storage
        .save_pending_edits_state(&pending_edits_state)
        .await?;

    println!("Rejected pending edit:");
    print_pending_edit(&pending_edit);

    Ok(())
}

async fn clear_pending_edits(
    storage: &mut FileStorage,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let pending_edits_state = storage.load_pending_edits_state().await?;
    let count = pending_edits_state.pending_edits.len();

    if count == 0 {
        println!("No pending edits to clear.");
        return Ok(());
    }

    // Clear all pending edits
    let empty_state = PendingEditsState::default();
    storage.save_pending_edits_state(&empty_state).await?;

    println!("Cleared {count} pending edit(s).");

    Ok(())
}

fn print_pending_edit(edit: &PendingEdit) {
    println!("ID: {}", edit.id);
    println!(
        "Original Track: {} - {}",
        edit.original_artist_name, edit.original_track_name
    );

    if let Some(ref album) = edit.original_album_name {
        println!("Original Album: {album}");
    }

    if let Some(ref album_artist) = edit.original_album_artist_name {
        println!("Original Album Artist: {album_artist}");
    }

    // Show proposed changes
    let mut has_changes = false;

    if let Some(ref new_track) = edit.new_track_name {
        println!("New Track Name: {new_track}");
        has_changes = true;
    }

    if let Some(ref new_artist) = edit.new_artist_name {
        println!("New Artist Name: {new_artist}");
        has_changes = true;
    }

    if let Some(ref new_album) = edit.new_album_name {
        println!("New Album Name: {new_album}");
        has_changes = true;
    }

    if let Some(ref new_album_artist) = edit.new_album_artist_name {
        println!("New Album Artist: {new_album_artist}");
        has_changes = true;
    }

    if !has_changes {
        println!("(No changes - removal edit)");
    }

    if let Some(timestamp) = edit.timestamp {
        if let Some(datetime) = chrono::DateTime::from_timestamp(timestamp as i64, 0) {
            println!("Timestamp: {}", datetime.format("%Y-%m-%d %H:%M:%S UTC"));
        }
    }
}
