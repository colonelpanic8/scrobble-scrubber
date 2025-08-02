use lastfm_edit::{LastFmEditClient, MockLastFmEditClient};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    env_logger::init();
    
    // Create a mock client for testing
    let client = MockLastFmEditClient::new();
    
    println!("Searching for albums containing 'remaster'...");
    
    // Search for albums containing "remaster"
    let mut album_search = client.search_albums("remaster");
    let mut count = 0;
    let limit = 50;
    
    while let Some(album) = album_search.next().await? {
        if count >= limit {
            break;
        }
        
        println!("Album: {} by {}", album.name, album.artist);
        
        // Get tracks for this album to see remaster patterns
        let mut track_iterator = client.album_tracks(&album.artist, &album.name);
        let mut track_count = 0;
        let track_limit = 10; // Limit tracks per album for readability
        
        while let Some(track) = track_iterator.next().await? {
            if track_count >= track_limit {
                break;
            }
            
            if track.name.to_lowercase().contains("remaster") {
                println!("  -> Track: {}", track.name);
            }
            track_count += 1;
        }
        
        count += 1;
        if count % 10 == 0 {
            println!("Processed {} albums so far...", count);
        }
    }
    
    println!("Search completed. Found {} albums.", count);
    Ok(())
}