#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    scrobble_scrubber::cli::run().await.map_err(|e| e.into())
}
