#!/usr/bin/env rust-script

//! Test script to analyze Last.fm session validation responses
//! 
//! This script tests different authentication scenarios:
//! 1. Valid session (using saved session data)
//! 2. Invalid/corrupted session cookies
//! 3. No session at all
//! 
//! Usage: rust-script session_test.rs

use reqwest::Client;
use serde_json;
use std::fs;

const TEST_URL: &str = "https://www.last.fm/settings/subscription/automatic-edits/tracks";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Last.fm Session Validation Test ===\n");
    
    let client = Client::new();
    
    // Test 1: No session at all
    println!("🔍 Test 1: No session cookies");
    test_no_session(&client).await?;
    
    println!("\n{}\n", "=".repeat(50));
    
    // Test 2: Invalid/corrupted session
    println!("🔍 Test 2: Invalid/corrupted session cookies");
    test_invalid_session(&client).await?;
    
    println!("\n{}\n", "=".repeat(50));
    
    // Test 3: Try to use saved session if available
    println!("🔍 Test 3: Attempt with saved session (if available)");
    if let Some(session_data) = try_load_saved_session() {
        test_with_saved_session(&client, &session_data).await?;
    } else {
        println!("❌ No saved session found. Please run scrobble-scrubber first to create a session.");
        println!("   Then re-run this test script.");
    }
    
    println!("\n=== Analysis Complete ===");
    Ok(())
}

async fn test_no_session(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    println!("Making request with no authentication...");
    
    let response = client
        .get(TEST_URL)
        .header("User-Agent", "Mozilla/5.0 (compatible; scrobble-scrubber-test)")
        .send()
        .await?;
    
    println!("Status: {}", response.status());
    println!("Headers:");
    for (name, value) in response.headers() {
        if name.as_str().to_lowercase().contains("location") || 
           name.as_str().to_lowercase().contains("set-cookie") ||
           name.as_str().to_lowercase().contains("www-authenticate") {
            println!("  {}: {:?}", name, value);
        }
    }
    
    let final_url = response.url().to_string();
    println!("Final URL: {}", final_url);
    
    let body = response.text().await?;
    analyze_response_body(&body, "No Session");
    
    Ok(())
}

async fn test_invalid_session(client: &Client) -> Result<(), Box<dyn std::error::Error>> {
    println!("Making request with corrupted session cookies...");
    
    let response = client
        .get(TEST_URL)
        .header("User-Agent", "Mozilla/5.0 (compatible; scrobble-scrubber-test)")
        .header("Cookie", "session_cookie=invalid_session_data_12345; csrf_token=fake_csrf_token")
        .send()
        .await?;
    
    println!("Status: {}", response.status());
    println!("Headers:");
    for (name, value) in response.headers() {
        if name.as_str().to_lowercase().contains("location") || 
           name.as_str().to_lowercase().contains("set-cookie") ||
           name.as_str().to_lowercase().contains("www-authenticate") {
            println!("  {}: {:?}", name, value);
        }
    }
    
    let final_url = response.url().to_string();
    println!("Final URL: {}", final_url);
    
    let body = response.text().await?;
    analyze_response_body(&body, "Invalid Session");
    
    Ok(())
}

async fn test_with_saved_session(
    client: &Client, 
    session_data: &SavedSessionData
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Making request with saved session cookies...");
    println!("Username: {}", session_data.username);
    println!("Session cookies count: {}", session_data.cookies.len());
    
    // Build cookie header from session data
    let cookie_header = session_data.cookies.join("; ");
    println!("Cookie header length: {} chars", cookie_header.len());
    
    let mut request_builder = client
        .get(TEST_URL)
        .header("User-Agent", "Mozilla/5.0 (compatible; scrobble-scrubber-test)")
        .header("Cookie", &cookie_header);
    
    // Add CSRF token if available
    if let Some(csrf_token) = &session_data.csrf_token {
        println!("Adding CSRF token: {}", &csrf_token[..std::cmp::min(csrf_token.len(), 20)]);
        request_builder = request_builder.header("X-CSRFToken", csrf_token);
    }
    
    let response = request_builder.send().await?;
    
    println!("Status: {}", response.status());
    println!("Headers:");
    for (name, value) in response.headers() {
        if name.as_str().to_lowercase().contains("location") || 
           name.as_str().to_lowercase().contains("set-cookie") ||
           name.as_str().to_lowercase().contains("www-authenticate") {
            println!("  {}: {:?}", name, value);
        }
    }
    
    let final_url = response.url().to_string();
    println!("Final URL: {}", final_url);
    
    let body = response.text().await?;
    analyze_response_body(&body, "Saved Session");
    
    Ok(())
}

fn analyze_response_body(body: &str, test_name: &str) {
    println!("\n--- Response Analysis for {} ---", test_name);
    println!("Body length: {} characters", body.len());
    
    // Look for common authentication indicators
    let indicators = [
        ("login form", body.contains("login") && (body.contains("<form") || body.contains("password"))),
        ("sign in text", body.to_lowercase().contains("sign in")),
        ("authentication required", body.to_lowercase().contains("authentication")),
        ("unauthorized", body.to_lowercase().contains("unauthorized")),
        ("403 forbidden", body.contains("403")),
        ("settings page", body.to_lowercase().contains("settings") && body.to_lowercase().contains("automatic")),
        ("subscription content", body.to_lowercase().contains("subscription")),
        ("tracks edit", body.to_lowercase().contains("tracks") && body.to_lowercase().contains("edit")),
        ("user profile", body.contains("profile") || body.contains("user")),
        ("last.fm account", body.to_lowercase().contains("account")),
    ];
    
    println!("Content indicators:");
    for (name, found) in indicators {
        println!("  {}: {}", name, if found { "✓ FOUND" } else { "✗ not found" });
    }
    
    // Look for specific HTML elements that might indicate authentication state
    let html_indicators = [
        ("title tag", extract_title(body)),
        ("h1 headings", extract_headings(body, "h1")),
        ("h2 headings", extract_headings(body, "h2")),
    ];
    
    for (name, content) in html_indicators {
        if !content.is_empty() {
            println!("  {}: {}", name, content);
        }
    }
    
    // Show first 500 chars of body for manual inspection
    println!("\nFirst 500 characters of response:");
    println!("{}", &body[..std::cmp::min(body.len(), 500)]);
    
    if body.len() > 500 {
        println!("\nLast 200 characters of response:");
        let start = body.len().saturating_sub(200);
        println!("{}", &body[start..]);
    }
}

fn extract_title(body: &str) -> String {
    if let (Some(start), Some(end)) = (body.find("<title>"), body.find("</title>")) {
        if start < end {
            return body[start + 7..end].trim().to_string();
        }
    }
    "No title found".to_string()
}

fn extract_headings(body: &str, tag: &str) -> String {
    let mut headings = Vec::new();
    let start_tag = format!("<{}>", tag);
    let end_tag = format!("</{}>", tag);
    
    let mut search_pos = 0;
    while let Some(start) = body[search_pos..].find(&start_tag) {
        let abs_start = search_pos + start + start_tag.len();
        if let Some(end) = body[abs_start..].find(&end_tag) {
            let heading = body[abs_start..abs_start + end].trim();
            if !heading.is_empty() {
                headings.push(heading.to_string());
            }
            search_pos = abs_start + end + end_tag.len();
        } else {
            break;
        }
        
        // Limit to first 3 headings to avoid spam
        if headings.len() >= 3 {
            break;
        }
    }
    
    if headings.is_empty() {
        format!("No {} headings found", tag)
    } else {
        headings.join(" | ")
    }
}

#[derive(Debug)]
struct SavedSessionData {
    username: String,
    cookies: Vec<String>,
    csrf_token: Option<String>,
}

fn try_load_saved_session() -> Option<SavedSessionData> {
    // Try to find session files in common locations
    let possible_paths = [
        // XDG data directory
        dirs::data_dir().map(|d| d.join("scrobble-scrubber/users")),
        // Current directory fallback
        Some(std::path::PathBuf::from(".")),
    ];
    
    for base_path in possible_paths.into_iter().flatten() {
        if let Ok(entries) = fs::read_dir(&base_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let session_file = path.join("session.json");
                    if session_file.exists() {
                        if let Ok(session_data) = try_parse_session_file(&session_file) {
                            println!("✓ Found session file: {}", session_file.display());
                            return Some(session_data);
                        }
                    }
                }
            }
        }
    }
    
    // Also try direct session files in current directory (fallback naming)
    for entry in fs::read_dir(".").ok()?.flatten() {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.ends_with("_session.json") {
                if let Ok(session_data) = try_parse_session_file(&path) {
                    println!("✓ Found session file: {}", path.display());
                    return Some(session_data);
                }
            }
        }
    }
    
    None
}

fn try_parse_session_file(path: &std::path::Path) -> Result<SavedSessionData, Box<dyn std::error::Error>> {
    let content = fs::read_to_string(path)?;
    let json: serde_json::Value = serde_json::from_str(&content)?;
    
    // Extract session data
    let username = json["username"].as_str().unwrap_or("unknown").to_string();
    
    // Extract session cookies
    let session = &json["session"];
    let cookies = session["cookies"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    
    let csrf_token = session["csrf_token"].as_str().map(|s| s.to_string());
    
    Ok(SavedSessionData {
        username,
        cookies,
        csrf_token,
    })
}