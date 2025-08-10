use clap::Subcommand;
use lastfm_edit::Track;
use scrobble_scrubber::musicbrainz_provider::MusicBrainzScrubActionProvider;
use scrobble_scrubber::scrub_action_provider::ScrubActionProvider;

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
        }
    }

    /// Search for albums using the provider's search method
    async fn search_albums(
        artist: &str,
        album_filter: Option<&str>,
        limit: usize,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Searching for albums by '{}'...\n", artist);

        // Use the provider's search method
        let releases = MusicBrainzScrubActionProvider::search_album_releases(artist, album_filter)
            .await
            .map_err(|e| format!("Provider error: {}", e))?;

        if releases.is_empty() {
            println!("No releases found for artist '{}'", artist);
            if let Some(album) = album_filter {
                println!("  Album filter: '{}'", album);
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
        let prefer_non_japanese = provider.prefer_non_japanese_releases();

        for (album_title, group) in album_groups.iter() {
            println!("📀 Album: {}", album_title);

            // Convert group to owned vec for the provider method
            let group_owned: Vec<_> = group.iter().map(|r| (*r).clone()).collect();

            // Use provider's method to select canonical
            let canonical = MusicBrainzScrubActionProvider::select_canonical_release(
                &group_owned,
                prefer_non_japanese,
            );

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
                    print!("[{}] ", country);
                } else {
                    print!("[??] ");
                }

                if let Some(disamb) = &release.disambiguation {
                    print!("- {} ", disamb);
                }

                println!("{}", canonical_marker);
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
            "Checking if '{}' by '{}' exists on canonical release of '{}'...\n",
            title, artist, album
        );

        // Use the provider's actual verification method
        let exists = provider
            .verify_track_exists_on_canonical_release(artist, title, Some(album))
            .await
            .map_err(|e| format!("Provider error: {}", e))?;

        // For detailed output, use the provider's search to show what was selected
        if show_tracks || show_all {
            use musicbrainz_rs::entity::release::Release;
            use musicbrainz_rs::Fetch;

            // Use the provider's search method
            let releases =
                MusicBrainzScrubActionProvider::search_album_releases(artist, Some(album))
                    .await
                    .map_err(|e| format!("Provider error: {}", e))?;

            if !releases.is_empty() {
                // Use provider's method to select canonical release
                let canonical = MusicBrainzScrubActionProvider::select_canonical_release(
                    &releases,
                    provider.prefer_non_japanese_releases(),
                )
                .unwrap();

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
                    println!("   Disambiguation: {}", disamb);
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
                                .map(|d| format!(" [{}]", d))
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
            println!("✅ Track '{}' EXISTS on the canonical release", title);
            println!("   This track would PASS MusicBrainz confirmation");
        } else {
            println!("❌ Track '{}' is NOT on the canonical release", title);
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

        println!("Searching MusicBrainz for '{}' by '{}'", title, artist);
        if let Some(alb) = album {
            println!("Album filter: '{}'", alb);
        }
        println!();

        // Use the provider's search method
        let results = provider
            .search_musicbrainz_multiple(artist, title, album)
            .await
            .map_err(|e| format!("Provider error: {}", e))?;

        if results.is_empty() {
            println!("No results found");
            return Ok(());
        }

        println!("Found {} results:\n", results.len());
        for (idx, result) in results.iter().enumerate() {
            println!("{}. '{}' by '{}'", idx + 1, result.title, result.artist);
            if let Some(ref alb) = result.album {
                println!("   Album: {}", alb);
            }
            println!("   Confidence: {:.2}%", result.confidence * 100.0);
            println!("   MBID: {}", result.mbid);
            if let Some(ref release_id) = result.release_id {
                println!("   Release ID: {}", release_id);
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
                .map_err(|e| format!("Provider error: {}", e))?;

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
}
