#!/usr/bin/env node

// Node.js example for testing WASM bindings with real Last.fm credentials
// This bypasses CORS issues by running in Node.js instead of the browser

const { LastFmEditClient } = require('./pkg-node/scrobble_scrubber_js.js');

async function testLastFmWasm() {
    console.log('🎵 Testing Scrobble Scrubber WASM with Last.fm');

    // Get credentials from environment variables (set by direnv)
    const username = process.env.SCROBBLE_SCRUBBER_LASTFM_USERNAME;
    const password = process.env.SCROBBLE_SCRUBBER_LASTFM_PASSWORD;

    if (!username || !password) {
        console.error('❌ Missing environment variables:');
        console.error('   SCROBBLE_SCRUBBER_LASTFM_USERNAME');
        console.error('   SCROBBLE_SCRUBBER_LASTFM_PASSWORD');
        console.error('   Make sure you\'re in the project directory with direnv loaded.');
        process.exit(1);
    }

    console.log(`📋 Loaded credentials for user: ${username}`);

    // Create WASM client
    const client = new LastFmEditClient();

    try {
        // Set credentials
        console.log('🔐 Setting credentials...');
        client.set_credentials(username, password);

        // Test authentication
        console.log('🔑 Testing authentication...');
        const authResult = await client.test_auth();

        console.log('📊 Authentication result:', {
            success: authResult.success,
            message: authResult.message
        });

        if (!authResult.success) {
            console.error('❌ Authentication failed');
            return;
        }

        console.log('✅ Authentication successful!');

        // Test recent tracks
        console.log('\\n📻 Loading recent tracks...');
        const recentTracks = await client.get_recent_tracks(5);

        if (recentTracks && recentTracks.length > 0) {
            console.log(`✅ Found ${recentTracks.length} recent tracks:`);
            recentTracks.forEach((track, index) => {
                console.log(`  ${index + 1}. "${track.name}" by ${track.artist}`);
                if (track.album) {
                    console.log(`     Album: ${track.album}`);
                }
                console.log(`     Plays: ${track.playcount}`);
                console.log('');
            });
        } else {
            console.log('⚠️  No recent tracks found');
        }

        // Test artist tracks (use an artist from the recent tracks if available)
        if (recentTracks && recentTracks.length > 0) {
            const artistName = recentTracks[0].artist;
            console.log(`🎤 Loading tracks for artist: ${artistName}...`);
            const artistTracks = await client.get_artist_tracks(artistName, 3);

            if (artistTracks && artistTracks.length > 0) {
                console.log(`✅ Found ${artistTracks.length} tracks by ${artistName}:`);
                artistTracks.forEach((track, index) => {
                    console.log(`  ${index + 1}. "${track.name}"`);
                    if (track.album) {
                        console.log(`     Album: ${track.album}`);
                    }
                    console.log(`     Plays: ${track.playcount}`);
                    console.log('');
                });
            } else {
                console.log(`⚠️  No tracks found for artist: ${artistName}`);
            }
        }

        console.log('🎉 WASM Last.fm integration test completed successfully!');

    } catch (error) {
        console.error('❌ Error during WASM test:', error.message);
        console.error('Full error:', error);
    }
}

// Run the test
testLastFmWasm().catch(console.error);
