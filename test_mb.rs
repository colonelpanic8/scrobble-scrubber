// Simple MusicBrainz test - run with: rustc --edition 2021 test_mb.rs && ./test_mb
use std::process::Command;

fn main() {
    println!("Testing MusicBrainz API with curl...\n");

    let track_name = "Don't Stop Me Now";
    let artist_name = "Queen";

    // Test a simple MusicBrainz query using curl
    let query = format!("recording:\"{}\" AND artist:\"{}\"", track_name, artist_name);
    let encoded_query = urlencoding::encode(&query);
    let url = format!("https://musicbrainz.org/ws/2/recording?query={}&fmt=json", encoded_query);
    
    println!("Testing URL: {}", url);
    
    let output = Command::new("curl")
        .arg("-s")
        .arg(&url)
        .output();
        
    match output {
        Ok(result) => {
            let response = String::from_utf8_lossy(&result.stdout);
            if response.len() > 200 {
                println!("Response (first 200 chars): {}...", &response[..200]);
            } else {
                println!("Response: {}", response);
            }
        }
        Err(e) => {
            println!("Curl failed: {}", e);
        }
    }
}