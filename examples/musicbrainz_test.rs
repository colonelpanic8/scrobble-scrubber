use musicbrainz_rs::entity::recording::Recording;
use musicbrainz_rs::Search;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing MusicBrainz API queries...\n");

    // Test data from the logs
    let test_tracks = vec![
        ("Don't Stop Me Now", "Queen"),
        ("Orange Color Queen", "Ty Segall"),
        ("Style", "Taylor Swift"),
    ];

    for (track_name, artist_name) in test_tracks {
        println!("=== Testing: '{}' by '{}' ===", track_name, artist_name);
        
        // Test different query formats
        let queries = vec![
            format!("recording:\"{}\" AND artist:\"{}\"", track_name, artist_name),
            format!("recording:\"{}\" AND artistname:\"{}\"", track_name, artist_name),
            format!("\"{}\" AND artist:\"{}\"", track_name, artist_name),
            format!("\"{}\" AND artistname:\"{}\"", track_name, artist_name),
            format!("{} AND artist:{}", track_name, artist_name),
            format!("recording:{} AND artist:{}", track_name, artist_name),
        ];

        for (i, query) in queries.iter().enumerate() {
            println!("Query {}: {}", i + 1, query);
            
            match Recording::search(query.clone()).execute().await {
                Ok(results) => {
                    println!("  ✅ Success! Found {} results", results.entities.len());
                    if !results.entities.is_empty() {
                        let first = &results.entities[0];
                        println!("  First result: '{}' by '{}'", 
                                first.title,
                                first.artist_credit.as_ref()
                                    .and_then(|ac| ac.first())
                                    .map(|ac| &ac.artist.name)
                                    .unwrap_or("Unknown"));
                    }
                }
                Err(e) => {
                    println!("  ❌ Failed: {}", e);
                }
            }
        }
        println!();
    }

    Ok(())
}