use musicbrainz_rs::entity::recording::{Recording, RecordingSearchQuery};
use musicbrainz_rs::Search;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing MusicBrainz API with structured queries...\n");

    // Test data from the logs - simple case first
    let track_name = "Don't Stop Me Now";
    let artist_name = "Queen";

    println!("=== Testing: '{track_name}' by '{artist_name}' ===");

    // Test different query builder approaches
    let test_queries = [
        (
            "Only artist",
            RecordingSearchQuery::query_builder()
                .artist(artist_name)
                .build(),
        ),
        (
            "Only recording",
            RecordingSearchQuery::query_builder()
                .recording(track_name)
                .build(),
        ),
        (
            "Artist + Recording",
            RecordingSearchQuery::query_builder()
                .recording(track_name)
                .and()
                .artist(artist_name)
                .build(),
        ),
        (
            "Using artist_name field",
            RecordingSearchQuery::query_builder()
                .recording(track_name)
                .and()
                .artist_name(artist_name)
                .build(),
        ),
    ];

    for (i, (description, query_string)) in test_queries.iter().enumerate() {
        println!("Query {}: {} - {:?}", i + 1, description, query_string);

        match Recording::search(query_string.clone()).execute().await {
            Ok(results) => {
                println!("  ✅ Success! Found {} results", results.entities.len());
                if !results.entities.is_empty() {
                    let first = &results.entities[0];
                    println!(
                        "  First result: '{}' by '{}'",
                        first.title,
                        first
                            .artist_credit
                            .as_ref()
                            .and_then(|ac| ac.first())
                            .map(|ac| ac.artist.name.as_str())
                            .unwrap_or("Unknown")
                    );
                }
            }
            Err(e) => {
                println!("  ❌ Failed: {e}");
            }
        }
        println!();
    }

    Ok(())
}
