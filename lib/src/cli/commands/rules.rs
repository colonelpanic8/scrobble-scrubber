use crate::persistence::StateStorage;
use crate::rewrite::{load_comprehensive_default_rules, RewriteRule, SdRule};
use lastfm_edit::{LastFmError, Result};
use std::collections::HashSet;
use std::io::{self, Write};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Show current active rewrite rules
pub async fn show_active_rules(
    storage: &Arc<Mutex<crate::persistence::FileStorage>>,
) -> Result<()> {
    println!("📝 Active Rewrite Rules");
    println!("=====================");

    let rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    if rules_state.rewrite_rules.is_empty() {
        println!("No rewrite rules configured");
        return Ok(());
    }

    println!("Found {} rewrite rules:", rules_state.rewrite_rules.len());

    for (i, rule) in rules_state.rewrite_rules.iter().enumerate() {
        println!(
            "  Rule {}: {}",
            i + 1,
            rule.name.as_deref().unwrap_or(&format!("Rule #{}", i + 1))
        );

        if let Some(track_rule) = &rule.track_name {
            println!(
                "    Track: '{}' → '{}'",
                track_rule.find, track_rule.replace
            );
        }
        if let Some(artist_rule) = &rule.artist_name {
            println!(
                "    Artist: '{}' → '{}'",
                artist_rule.find, artist_rule.replace
            );
        }
        if let Some(album_rule) = &rule.album_name {
            println!(
                "    Album: '{}' → '{}'",
                album_rule.find, album_rule.replace
            );
        }
        if let Some(album_artist_rule) = &rule.album_artist_name {
            println!(
                "    Album Artist: '{}' → '{}'",
                album_artist_rule.find, album_artist_rule.replace
            );
        }

        if rule.requires_confirmation || rule.requires_musicbrainz_confirmation {
            println!("    Options:");
            if rule.requires_confirmation {
                println!("      - Requires user confirmation");
            }
            if rule.requires_musicbrainz_confirmation {
                println!("      - Requires MusicBrainz confirmation");
            }
        }

        println!();
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
/// Add a new rewrite rule
pub async fn add_rewrite_rule(
    storage: &Arc<Mutex<crate::persistence::FileStorage>>,
    name: Option<&str>,
    track_find: Option<&str>,
    track_replace: Option<&str>,
    artist_find: Option<&str>,
    artist_replace: Option<&str>,
    album_find: Option<&str>,
    album_replace: Option<&str>,
    album_artist_find: Option<&str>,
    album_artist_replace: Option<&str>,
    flags: Option<&str>,
    require_confirmation: bool,
    require_musicbrainz_confirmation: bool,
) -> Result<()> {
    println!("📝 Adding New Rewrite Rule");
    println!("==========================");

    // Validate that at least one field is specified
    let has_track_rule = track_find.is_some() && track_replace.is_some();
    let has_artist_rule = artist_find.is_some() && artist_replace.is_some();
    let has_album_rule = album_find.is_some() && album_replace.is_some();
    let has_album_artist_rule = album_artist_find.is_some() && album_artist_replace.is_some();

    if !has_track_rule && !has_artist_rule && !has_album_rule && !has_album_artist_rule {
        println!("❌ Error: At least one field rule must be specified");
        println!("   Use --track-find/--track-replace, --artist-find/--artist-replace,");
        println!("   --album-find/--album-replace, or --album-artist-find/--album-artist-replace");
        return Ok(());
    }

    // Validate that find/replace pairs are complete
    if (track_find.is_some() && track_replace.is_none())
        || (track_find.is_none() && track_replace.is_some())
    {
        println!("❌ Error: Track rule requires both --track-find and --track-replace");
        return Ok(());
    }
    if (artist_find.is_some() && artist_replace.is_none())
        || (artist_find.is_none() && artist_replace.is_some())
    {
        println!("❌ Error: Artist rule requires both --artist-find and --artist-replace");
        return Ok(());
    }
    if (album_find.is_some() && album_replace.is_none())
        || (album_find.is_none() && album_replace.is_some())
    {
        println!("❌ Error: Album rule requires both --album-find and --album-replace");
        return Ok(());
    }
    if (album_artist_find.is_some() && album_artist_replace.is_none())
        || (album_artist_find.is_none() && album_artist_replace.is_some())
    {
        println!("❌ Error: Album artist rule requires both --album-artist-find and --album-artist-replace");
        return Ok(());
    }

    // Create SdRule helper function
    let create_sd_rule = |find: &str, replace: &str, flags: Option<&str>| -> SdRule {
        SdRule {
            find: find.to_string(),
            replace: replace.to_string(),
            flags: flags.map(|f| f.to_string()),
        }
    };

    // Create the new rule
    let new_rule = RewriteRule {
        name: name.map(|n| n.to_string()),
        track_name: if has_track_rule {
            Some(create_sd_rule(
                track_find.unwrap(),
                track_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        artist_name: if has_artist_rule {
            Some(create_sd_rule(
                artist_find.unwrap(),
                artist_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        album_name: if has_album_rule {
            Some(create_sd_rule(
                album_find.unwrap(),
                album_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        album_artist_name: if has_album_artist_rule {
            Some(create_sd_rule(
                album_artist_find.unwrap(),
                album_artist_replace.unwrap(),
                flags,
            ))
        } else {
            None
        },
        requires_confirmation: require_confirmation,
        requires_musicbrainz_confirmation: require_musicbrainz_confirmation,
        musicbrainz_release_filters: None, // Use default filters, can be configured later if needed
    };

    // Load existing rules
    let mut rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    // Add the new rule
    rules_state.rewrite_rules.push(new_rule.clone());

    // Save updated rules
    storage
        .lock()
        .await
        .save_rewrite_rules_state(&rules_state)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save rewrite rules: {e}"
            )))
        })?;

    // Display what was added
    println!("✅ Successfully added rewrite rule:");
    println!("   Name: {}", name.unwrap_or("(unnamed)"));
    if let Some(track_rule) = &new_rule.track_name {
        println!("   Track: '{}' → '{}'", track_rule.find, track_rule.replace);
    }
    if let Some(artist_rule) = &new_rule.artist_name {
        println!(
            "   Artist: '{}' → '{}'",
            artist_rule.find, artist_rule.replace
        );
    }
    if let Some(album_rule) = &new_rule.album_name {
        println!("   Album: '{}' → '{}'", album_rule.find, album_rule.replace);
    }
    if let Some(album_artist_rule) = &new_rule.album_artist_name {
        println!(
            "   Album Artist: '{}' → '{}'",
            album_artist_rule.find, album_artist_rule.replace
        );
    }
    if let Some(flags) = flags {
        println!("   Flags: {flags}");
    }
    if require_confirmation {
        println!("   Requires confirmation: yes");
    }
    if require_musicbrainz_confirmation {
        println!("   Requires MusicBrainz confirmation: yes");
    }

    Ok(())
}

/// Remove a rewrite rule
pub async fn remove_rewrite_rule(
    storage: &Arc<Mutex<crate::persistence::FileStorage>>,
    index: Option<usize>,
    name: Option<&str>,
    all: bool,
) -> Result<()> {
    println!("🗑️ Removing Rewrite Rule");
    println!("========================");

    // Load existing rules
    let mut rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    if rules_state.rewrite_rules.is_empty() {
        println!("❌ No rewrite rules found to remove");
        return Ok(());
    }

    // Handle different removal methods
    if all {
        // Remove all rules (with confirmation)
        println!(
            "⚠️ This will remove ALL {} rewrite rules.",
            rules_state.rewrite_rules.len()
        );
        print!("Are you sure you want to continue? (y/N): ");
        io::stdout().flush().unwrap();

        let mut input = String::new();
        io::stdin().read_line(&mut input).map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!("Failed to read input: {e}")))
        })?;

        if input.trim().to_lowercase() != "y" && input.trim().to_lowercase() != "yes" {
            println!("❌ Operation cancelled");
            return Ok(());
        }

        rules_state.rewrite_rules.clear();
        println!("✅ Removed all rewrite rules");
    } else if let Some(rule_index) = index {
        // Remove by index (1-based)
        if rule_index == 0 || rule_index > rules_state.rewrite_rules.len() {
            println!("❌ Invalid rule index: {rule_index}");
            println!("   Valid range: 1-{}", rules_state.rewrite_rules.len());
            return Ok(());
        }

        let removed_rule = rules_state.rewrite_rules.remove(rule_index - 1);
        println!(
            "✅ Removed rule #{rule_index}: {}",
            removed_rule.name.as_deref().unwrap_or("(unnamed)")
        );
    } else if let Some(rule_name) = name {
        // Remove by name
        let original_len = rules_state.rewrite_rules.len();
        rules_state
            .rewrite_rules
            .retain(|rule| rule.name.as_deref() != Some(rule_name));

        let removed_count = original_len - rules_state.rewrite_rules.len();
        if removed_count == 0 {
            println!("❌ No rule found with name: '{rule_name}'");
            return Ok(());
        } else if removed_count == 1 {
            println!("✅ Removed rule: '{rule_name}'");
        } else {
            println!("✅ Removed {removed_count} rules with name: '{rule_name}'");
        }
    } else {
        // No removal method specified
        println!("❌ Error: Must specify --index, --name, or --all");
        println!("   Use 'show-rules' to see rule indices and names");
        return Ok(());
    }

    // Save updated rules
    storage
        .lock()
        .await
        .save_rewrite_rules_state(&rules_state)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save rewrite rules: {e}"
            )))
        })?;

    println!("Remaining rules: {}", rules_state.rewrite_rules.len());
    Ok(())
}

fn rule_signature(rule: &RewriteRule) -> String {
    format!(
        "track:{:?}|artist:{:?}|album:{:?}|album_artist:{:?}",
        rule.track_name.as_ref().map(|r| (&r.find, &r.replace)),
        rule.artist_name.as_ref().map(|r| (&r.find, &r.replace)),
        rule.album_name.as_ref().map(|r| (&r.find, &r.replace)),
        rule.album_artist_name
            .as_ref()
            .map(|r| (&r.find, &r.replace))
    )
}

/// Enable all default rewrite rules, avoiding duplicates
pub async fn enable_default_rules(
    storage: &Arc<Mutex<crate::persistence::FileStorage>>,
) -> Result<()> {
    println!("📝 Enabling Default Rewrite Rules");
    println!("=================================");

    let default_rules = load_comprehensive_default_rules();

    let mut rules_state = storage
        .lock()
        .await
        .load_rewrite_rules_state()
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to load rewrite rules: {e}"
            )))
        })?;

    let existing_signatures: HashSet<String> = rules_state
        .rewrite_rules
        .iter()
        .map(rule_signature)
        .collect();

    let mut added_count = 0;
    let mut skipped_count = 0;

    for rule in default_rules {
        let signature = rule_signature(&rule);
        if existing_signatures.contains(&signature) {
            log::debug!(
                "Skipping duplicate rule: {}",
                rule.name.as_deref().unwrap_or("(unnamed)")
            );
            skipped_count += 1;
        } else {
            log::debug!(
                "Adding rule: {}",
                rule.name.as_deref().unwrap_or("(unnamed)")
            );
            rules_state.rewrite_rules.push(rule);
            added_count += 1;
        }
    }

    storage
        .lock()
        .await
        .save_rewrite_rules_state(&rules_state)
        .await
        .map_err(|e| {
            LastFmError::Io(std::io::Error::other(format!(
                "Failed to save rewrite rules: {e}"
            )))
        })?;

    println!("✅ Default rules processing complete:");
    println!("   Added: {added_count} new rules");
    println!("   Skipped: {skipped_count} duplicate rules");
    println!("   Total active rules: {}", rules_state.rewrite_rules.len());

    Ok(())
}
