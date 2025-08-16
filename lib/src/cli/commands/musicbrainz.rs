use crate::musicbrainz::MusicBrainzScrubActionProvider;
use crate::scrub_action_provider::ScrubActionProvider;
use clap::Subcommand;
use lastfm_edit::Track;

use crate::musicbrainz::CompilationToCanonicalProvider;

#[derive(Subcommand, Debug, Clone)]
pub enum MusicBrainzCommands {
    /// Search for albums/releases for an artist
    AlbumSearch {
        /// Artist name to search for
        #[arg(short, long)]
        artist: String,

        /// Optional album name to filter results
        #[arg(short = 'A', long)]
        album: Option<String>,

        /// Maximum number of results to return
        #[arg(short = 'l', long, default_value = "10")]
        limit: usize,
    },

    /// Check if a track exists on the canonical release of an album
    TrackMatch {
        /// Artist name
        #[arg(short, long)]
        artist: String,

        /// Track title
        #[arg(short, long)]
        title: String,

        /// Album name to check against
        #[arg(short = 'A', long)]
        album: String,

        /// Show full track list of canonical release
        #[arg(long)]
        show_tracks: bool,

        /// Show all releases (not just canonical)
        #[arg(long)]
        show_all: bool,
    },

    /// Search for a track and show what MusicBrainz finds
    TrackSearch {
        /// Artist name
        #[arg(short, long)]
        artist: String,

        /// Track title
        #[arg(short, long)]
        title: String,

        /// Optional album name
        #[arg(short = 'A', long)]
        album: Option<String>,

        /// Maximum number of results
        #[arg(short = 'l', long, default_value = "5")]
        limit: usize,
    },

    /// Look up the canonical album for an artist/album pair
    CanonicalAlbum {
        /// Artist name
        #[arg(short, long)]
        artist: String,

        /// Album name
        #[arg(short = 'A', long)]
        album: String,

        /// Show all releases (not just the canonical one)
        #[arg(long)]
        show_all: bool,

        /// Show track list of the canonical release
        #[arg(long)]
        show_tracks: bool,
    },

    /// Show ranked releases for a track (for debugging compilation provider)
    RankReleases {
        /// Artist name
        #[arg(short, long)]
        artist: String,

        /// Track title
        #[arg(short, long)]
        title: String,

        /// Current album (optional, helps identify compilations)
        #[arg(short = 'A', long)]
        album: Option<String>,

        /// Output format (text or json)
        #[arg(short = 'f', long, default_value = "text")]
        format: String,
    },
}

impl MusicBrainzCommands {
    pub async fn execute(self) -> Result<(), Box<dyn std::error::Error>> {
        match self {
            Self::AlbumSearch {
                artist,
                album,
                limit,
            } => Self::search_albums(&artist, album.as_deref(), limit).await,
            Self::TrackMatch {
                artist,
                title,
                album,
                show_tracks,
                show_all,
            } => Self::check_track_match(&artist, &title, &album, show_tracks, show_all).await,
            Self::TrackSearch {
                artist,
                title,
                album,
                limit,
            } => Self::search_track(&artist, &title, album.as_deref(), limit).await,
            Self::CanonicalAlbum {
                artist,
                album,
                show_all,
                show_tracks,
            } => Self::show_canonical_album(&artist, &album, show_all, show_tracks).await,
            Self::RankReleases {
                artist,
                title,
                album,
                format,
            } => Self::rank_releases(&artist, &title, album.as_deref(), &format).await,
        }
    }

    /// Search for albums using the provider's search method
    async fn search_albums(
        artist: &str,
        album_filter: Option<&str>,
        limit: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Searching for albums by '{artist}'...\n");

        // Use the provider's search method
        let releases = MusicBrainzScrubActionProvider::search_album_releases(artist, album_filter)
            .await
            .map_err(|e| format!("Provider error: {e}"))?;

        if releases.is_empty() {
            println!("No releases found for artist '{artist}'");
            if let Some(album) = album_filter {
                println!("  Album filter: '{album}'");
            }
            return Ok(());
        }

        // Take up to limit results
        let display_releases: Vec<_> = releases.iter().take(limit).collect();

        println!(
            "Found {} releases (showing {}):\n",
            releases.len(),
            display_releases.len()
        );

        // Group releases by album title to show canonical selection
        let mut album_groups: std::collections::HashMap<
            String,
            Vec<&musicbrainz_rs::entity::release::Release>,
        > = std::collections::HashMap::new();
        for release in &display_releases {
            album_groups
                .entry(release.title.clone())
                .or_default()
                .push(release);
        }

        // Create a provider instance to use its preference settings
        let provider = MusicBrainzScrubActionProvider::new(0.8, 20);

        for (album_title, group) in album_groups.iter() {
            println!("📀 Album: {album_title}");

            // Convert group to owned vec for the provider method
            let group_owned: Vec<_> = group.iter().map(|r| (*r).clone()).collect();

            // Use provider's method to select canonical
            let canonical = provider.select_canonical_release(&group_owned);

            for release in group {
                let is_canonical = canonical
                    .as_ref()
                    .map(|c| c.id == release.id)
                    .unwrap_or(false);
                let canonical_marker = if is_canonical { " ⭐ [CANONICAL]" } else { "" };

                print!("   ");
                if let Some(date) = &release.date {
                    print!("{} ", date.0);
                } else {
                    print!("???? ");
                }

                if let Some(country) = &release.country {
                    print!("[{country}] ");
                } else {
                    print!("[??] ");
                }

                if let Some(disamb) = &release.disambiguation {
                    print!("- {disamb} ");
                }

                println!("{canonical_marker}");
            }
            println!();
        }

        Ok(())
    }

    /// Check if a track exists on the canonical release (using the actual provider logic)
    async fn check_track_match(
        artist: &str,
        title: &str,
        album: &str,
        show_tracks: bool,
        show_all: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create the provider with the same settings as production
        let provider = MusicBrainzScrubActionProvider::new(0.8, 20);

        println!(
            "Checking if '{title}' by '{artist}' exists on canonical release of '{album}'...\n"
        );

        // Use the provider's actual verification method
        let exists = provider
            .verify_track_exists_on_canonical_release(artist, title, Some(album))
            .await
            .map_err(|e| format!("Provider error: {e}"))?;

        // For detailed output, use the provider's search to show what was selected
        if show_tracks || show_all {
            use musicbrainz_rs::entity::release::Release;
            use musicbrainz_rs::Fetch;

            // Use the provider's search method
            let releases =
                MusicBrainzScrubActionProvider::search_album_releases(artist, Some(album))
                    .await
                    .map_err(|e| format!("Provider error: {e}"))?;

            if !releases.is_empty() {
                // Use provider's method to select canonical release
                let canonical = provider.select_canonical_release(&releases).unwrap();

                println!("🎯 CANONICAL RELEASE SELECTED BY PROVIDER:");
                println!("   Title: {}", canonical.title);
                println!(
                    "   Date: {}",
                    canonical
                        .date
                        .as_ref()
                        .map(|d| d.0.as_str())
                        .unwrap_or("unknown")
                );
                println!(
                    "   Country: {}",
                    canonical.country.as_ref().unwrap_or(&"??".to_string())
                );
                if let Some(disamb) = &canonical.disambiguation {
                    println!("   Disambiguation: {disamb}");
                }
                println!("   MusicBrainz ID: {}", canonical.id);
                println!();

                if show_tracks {
                    // Fetch the full release with recordings
                    let full_release = Release::fetch()
                        .id(&canonical.id)
                        .with_recordings()
                        .execute()
                        .await?;

                    println!("📀 TRACK LIST:");
                    if let Some(media) = &full_release.media {
                        for (disc_idx, medium) in media.iter().enumerate() {
                            if media.len() > 1 {
                                println!("\n  Disc {}:", disc_idx + 1);
                            }
                            if let Some(tracks) = &medium.tracks {
                                for track in tracks {
                                    let track_title = track.title.as_str();
                                    let is_match = track_title.eq_ignore_ascii_case(title);

                                    if is_match {
                                        println!("   {}. {} ✅ FOUND", track.position, track_title);
                                    } else {
                                        println!("   {}. {}", track.position, track_title);
                                    }
                                }
                            }
                        }
                    }
                    println!();
                }

                if show_all {
                    println!("📚 ALL RELEASES ({} total):", releases.len());
                    for release in &releases {
                        let is_canonical = release.id == canonical.id;
                        let marker = if is_canonical { " ← CANONICAL" } else { "" };
                        println!(
                            "   {} - {} ({}){}{}",
                            release
                                .date
                                .as_ref()
                                .map(|d| d.0.as_str())
                                .unwrap_or("????"),
                            release.title,
                            release.country.as_ref().unwrap_or(&"??".to_string()),
                            release
                                .disambiguation
                                .as_ref()
                                .map(|d| format!(" [{d}]"))
                                .unwrap_or_default(),
                            marker
                        );
                    }
                    println!();
                }
            }
        }

        // Show the provider's result
        println!("{}", "=".repeat(80));
        println!("PROVIDER RESULT:");
        if exists {
            println!("✅ Track '{title}' EXISTS on the canonical release");
            println!("   This track would PASS MusicBrainz confirmation");
        } else {
            println!("❌ Track '{title}' is NOT on the canonical release");
            println!("   This track would FAIL MusicBrainz confirmation");
            println!("   (likely a bonus track from a special edition or regional release)");
        }
        println!("{}", "=".repeat(80));

        Ok(())
    }

    /// Search for a track using the provider's search method
    async fn search_track(
        artist: &str,
        title: &str,
        album: Option<&str>,
        limit: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create the provider
        let provider = MusicBrainzScrubActionProvider::new(0.8, limit);

        println!("Searching MusicBrainz for '{title}' by '{artist}'");
        if let Some(alb) = album {
            println!("Album filter: '{alb}'");
        }
        println!();

        // Use the provider's search method
        let results = provider
            .search_musicbrainz_multiple(artist, title, album)
            .await
            .map_err(|e| format!("Provider error: {e}"))?;

        if results.is_empty() {
            println!("No results found");
            return Ok(());
        }

        println!("Found {} results:\n", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!("{}. '{}' by '{}'", idx + 1, result.title, result.artist);
            if let Some(ref alb) = result.album {
                println!("   Album: {alb}");
            }
            println!("   Confidence: {:.2}%", result.confidence * 100.0);
            println!("   MBID: {}", result.mbid);
            if let Some(ref release_id) = result.release_id {
                println!("   Release ID: {release_id}");
            }
            println!();
        }

        // Test what the provider would suggest for a track
        if !results.is_empty() {
            let track = Track {
                name: title.to_string(),
                artist: artist.to_string(),
                album: album.map(String::from),
                album_artist: None,
                playcount: 1,
                timestamp: Some(0),
            };

            let suggestions = provider
                .analyze_tracks(&[track], None, None)
                .await
                .map_err(|e| format!("Provider error: {e}"))?;

            if !suggestions.is_empty() {
                println!("PROVIDER SUGGESTIONS:");
                for (track_idx, track_suggestions) in suggestions {
                    for suggestion in track_suggestions {
                        println!("   Track #{}: {:?}", track_idx, suggestion.suggestion);
                        if suggestion.requires_confirmation {
                            println!("   (requires confirmation)");
                        }
                    }
                }
            } else {
                println!("Provider would not suggest any corrections for this track.");
            }
        }

        Ok(())
    }

    /// Show the canonical album for an artist/album pair
    async fn show_canonical_album(
        artist: &str,
        album: &str,
        show_all: bool,
        show_tracks: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use musicbrainz_rs::entity::release::Release;
        use musicbrainz_rs::Fetch;

        // Create the provider with default settings
        let provider = MusicBrainzScrubActionProvider::new(0.8, 20);

        println!("Looking up canonical album for '{album}' by '{artist}'...\n");

        // Search for the album releases
        let releases = MusicBrainzScrubActionProvider::search_album_releases(artist, Some(album))
            .await
            .map_err(|e| format!("Provider error: {e}"))?;

        if releases.is_empty() {
            println!("❌ No releases found for album '{album}' by '{artist}'");
            return Ok(());
        }

        // Filter releases to only those matching the album name
        let matching_releases: Vec<_> = releases
            .iter()
            .filter(|r| r.title.eq_ignore_ascii_case(album))
            .cloned()
            .collect();

        if matching_releases.is_empty() {
            println!("❌ No exact matches found for album '{album}'");
            println!(
                "   Found {} releases with different titles:",
                releases.len()
            );
            for (idx, release) in releases.iter().take(5).enumerate() {
                println!("   {}. '{}'", idx + 1, release.title);
            }
            return Ok(());
        }

        // Use provider's method to select canonical release
        let canonical = provider
            .select_canonical_release(&matching_releases)
            .ok_or("Failed to select canonical release")?;

        println!("✅ CANONICAL ALBUM FOUND\n");
        println!("{}", "=".repeat(80));
        println!("📀 Album: {}", canonical.title);
        println!("🎤 Artist: {artist}");
        println!(
            "📅 Release Date: {}",
            canonical
                .date
                .as_ref()
                .map(|d| d.0.as_str())
                .unwrap_or("unknown")
        );
        println!(
            "🌍 Country: {}",
            canonical.country.as_ref().unwrap_or(&"Unknown".to_string())
        );

        if let Some(disamb) = &canonical.disambiguation {
            println!("📝 Disambiguation: {disamb}");
        }

        println!("🆔 Release UUID: {}", canonical.id);

        // Fetch additional metadata
        let full_release = Release::fetch()
            .id(&canonical.id)
            .with_recordings()
            .with_artist_credits()
            .with_labels()
            .execute()
            .await?;

        // Show status
        if let Some(status) = &full_release.status {
            println!("📊 Status: {status:?}");
        }

        // Show labels
        if let Some(label_info) = &full_release.label_info {
            if !label_info.is_empty() {
                println!("🏷️  Labels:");
                for info in label_info {
                    if let Some(label) = &info.label {
                        let catalog = info.catalog_number.as_deref().unwrap_or("N/A");
                        println!("   - {} (Catalog: {})", label.name, catalog);
                    }
                }
            }
        }

        // Show format
        if let Some(media) = &full_release.media {
            if let Some(first_medium) = media.first() {
                if let Some(format) = &first_medium.format {
                    println!("💿 Format: {format}");
                }
            }

            // Show total tracks
            let total_tracks: usize = media
                .iter()
                .filter_map(|m| m.tracks.as_ref().map(|t| t.len()))
                .sum();
            println!("🎵 Total Tracks: {total_tracks}");

            if media.len() > 1 {
                println!("💽 Discs: {}", media.len());
            }
        }

        // Show barcode if available
        if let Some(barcode) = &full_release.barcode {
            if !barcode.is_empty() {
                println!("📊 Barcode: {barcode}");
            }
        }

        println!("{}", "=".repeat(80));

        // Show track list if requested
        if show_tracks {
            println!("\n📜 TRACK LIST:");
            if let Some(media) = &full_release.media {
                for (disc_idx, medium) in media.iter().enumerate() {
                    if media.len() > 1 {
                        println!("\n  💿 Disc {}:", disc_idx + 1);
                        if let Some(title) = &medium.title {
                            println!("     Title: {title}");
                        }
                    }
                    if let Some(tracks) = &medium.tracks {
                        for track in tracks {
                            let duration = track
                                .length
                                .map(|ms| {
                                    let seconds = ms / 1000;
                                    let minutes = seconds / 60;
                                    let secs = seconds % 60;
                                    format!("{minutes}:{secs:02}")
                                })
                                .unwrap_or_else(|| "?:??".to_string());

                            println!("   {:3}. {} ({})", track.position, track.title, duration);
                        }
                    }
                }
            }
            println!();
        }

        // Show all releases if requested
        if show_all {
            println!(
                "\n📚 ALL RELEASES FOR THIS ALBUM ({} total):",
                matching_releases.len()
            );
            println!("{}", "-".repeat(80));
            for release in &matching_releases {
                let is_canonical = release.id == canonical.id;
                let marker = if is_canonical { " ⭐ [CANONICAL]" } else { "" };

                let date_str = release
                    .date
                    .as_ref()
                    .map(|d| d.0.as_str())
                    .unwrap_or("????");
                let country = release.country.as_deref().unwrap_or("??");
                let disamb = release
                    .disambiguation
                    .as_ref()
                    .map(|d| format!(" - {d}"))
                    .unwrap_or_default();

                println!("   {date_str} [{country}]{disamb}");
                println!("     UUID: {}{marker}", release.id);

                // Show why it wasn't selected if not canonical
                if !is_canonical {
                    if provider.should_exclude_release(release) {
                        println!("     ❌ Excluded by filter rules");
                    } else if provider.should_deprioritize_release(release) {
                        println!("     ⬇️  Deprioritized (likely Japanese release)");
                    } else if MusicBrainzScrubActionProvider::is_special_edition(release) {
                        println!("     📦 Special edition");
                    }
                }
            }
            println!("{}", "-".repeat(80));
        }

        Ok(())
    }

    /// Show ranked releases for a track
    async fn rank_releases(
        artist: &str,
        title: &str,
        current_album: Option<&str>,
        format: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Create the compilation provider
        let provider = CompilationToCanonicalProvider::new();

        println!("🔍 Ranking releases for '{title}' by '{artist}'");
        if let Some(album) = current_album {
            println!("   Current album: '{album}'");
        }
        println!();

        // Get ranked releases
        let ranked_releases = provider
            .rank_releases_for_recording(artist, title, current_album)
            .await
            .map_err(|e| format!("Failed to rank releases: {e}"))?;

        if ranked_releases.is_empty() {
            println!("❌ No releases found");
            return Ok(());
        }

        // Output based on format
        match format {
            "json" => {
                // JSON output for programmatic use
                let json = serde_json::to_string_pretty(&ranked_releases)?;
                println!("{json}");
            }
            _ => {
                // Text output for human reading
                println!("📊 RELEASE RANKINGS");
                println!("{}", "=".repeat(80));
                println!(
                    "Found {} releases, ranked from best to worst:\n",
                    ranked_releases.len()
                );

                for release in &ranked_releases {
                    // Rank indicator
                    let rank_emoji = match release.rank {
                        1 => "🥇",
                        2 => "🥈",
                        3 => "🥉",
                        _ => "  ",
                    };

                    println!("{} Rank #{}: {}", rank_emoji, release.rank, release.title);
                    println!("   Artist: {}", release.artist);
                    println!("   Reason: {}", release.rank_reason);

                    if let Some(date) = &release.date {
                        println!("   Date: {date}");
                    }

                    if let Some(country) = &release.country {
                        println!("   Country: {country}");
                    }

                    if let Some(status) = &release.status {
                        println!("   Status: {status}");
                    }

                    if let Some(primary) = &release.primary_type {
                        println!("   Primary Type: {primary:?}");
                    }

                    if !release.secondary_types.is_empty() {
                        let secondary_types: Vec<String> = release.secondary_types.iter().map(|st| format!("{st:?}")).collect();
                        println!("   Secondary Types: {}", secondary_types.join(", "));
                    }

                    if release.is_compilation {
                        println!("   ⚠️  COMPILATION");
                    }

                    if release.is_various_artists {
                        println!("   ⚠️  VARIOUS ARTISTS");
                    }

                    if let Some(disamb) = &release.disambiguation {
                        println!("   Note: {disamb}");
                    }

                    println!("   MBID: {}", release.release_id);
                    println!();
                }

                // Summary
                println!("{}", "=".repeat(80));
                println!("📋 SUMMARY");

                // What would the provider suggest?
                if let Some(current) = current_album {
                    let current_is_compilation = ranked_releases
                        .iter()
                        .find(|r| r.title.eq_ignore_ascii_case(current))
                        .map(|r| r.is_compilation)
                        .unwrap_or(false);

                    if current_is_compilation {
                        // Find the best non-compilation
                        let best_non_compilation = ranked_releases.iter().find(|r| {
                            !r.is_compilation
                                && !r.is_various_artists
                                && !r.title.eq_ignore_ascii_case(current)
                        });

                        if let Some(best) = best_non_compilation {
                            println!(
                                "✅ Provider would suggest: '{}' → '{}'",
                                current, best.title
                            );
                            println!(
                                "   Reason: Moving from compilation to {}",
                                best.primary_type.as_ref().map(|pt| format!("{pt:?}")).unwrap_or_else(|| "studio release".to_string())
                            );
                        } else {
                            println!("❌ Provider would NOT suggest a change");
                            println!("   Reason: No suitable non-compilation release found");
                        }
                    } else {
                        println!("✅ Provider would NOT suggest a change");
                        println!("   Reason: '{current}' is not a compilation");
                    }
                } else {
                    // Just show the best release
                    if let Some(best) = ranked_releases.first() {
                        if !best.is_compilation {
                            println!("✅ Best release: '{}'", best.title);
                        } else {
                            println!("⚠️  Best release is a compilation: '{}'", best.title);
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
