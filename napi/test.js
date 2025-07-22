#!/usr/bin/env node

// Native Node.js addon test - real HTTP requests work!
const { LastFmEditClient } = require('./index.js');

async function testNativeAddon() {
    console.log('🎵 Testing Native Scrobble Scrubber Node.js Addon');

    // Get credentials from environment variables
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

    // Create native client
    const client = new LastFmEditClient();

    try {
        // Set credentials
        console.log('🔐 Setting credentials...');
        client.setCredentials(username, password);

        // Test authentication with REAL HTTP requests!
        console.log('🔑 Testing authentication (REAL API CALL)...');
        const authResult = await client.testAuth();

        console.log('📊 Authentication result:', {
            success: authResult.success,
            message: authResult.message
        });

        if (!authResult.success) {
            console.error('❌ Authentication failed');
            return;
        }

        console.log('✅ Authentication successful!');

        // Test recent tracks with REAL API calls!
        console.log('\n📻 Loading recent tracks (REAL API CALL)...');
        const recentTracks = await client.getRecentTracks(5);

        if (recentTracks && recentTracks.length > 0) {
            console.log(`✅ Found ${recentTracks.length} real recent tracks:`);
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

        // Test artist tracks with REAL API calls!
        if (recentTracks && recentTracks.length > 0) {
            const artistName = recentTracks[0].artist;
            console.log(`🎤 Loading tracks for artist: ${artistName} (REAL API CALL)...`);
            const artistTracks = await client.getArtistTracks(artistName, 3);

            if (artistTracks && artistTracks.length > 0) {
                console.log(`✅ Found ${artistTracks.length} real tracks by ${artistName}:`);
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

        console.log('🎉 Native addon Last.fm integration test completed successfully!');
        console.log('🚀 All HTTP requests were REAL API calls to Last.fm!');

    } catch (error) {
        console.error('❌ Error during native addon test:', error.message);
        console.error('Full error:', error);
    }
}

// Run the test
testNativeAddon().catch(console.error);