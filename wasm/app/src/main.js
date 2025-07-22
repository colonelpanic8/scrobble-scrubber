import init, { 
    create_track, 
    create_simple_rule, 
    test_rule_applies, 
    apply_rule_to_track,
    validate_regex,
    test_regex,
    LastFmEditClient
} from '../../pkg/scrobble_scrubber_js.js';

let wasmModule;
let lastfmEditClient;

async function initWasm() {
    try {
        wasmModule = await init();
        lastfmEditClient = new LastFmEditClient();
        console.log('WASM module loaded successfully');
        return true;
    } catch (error) {
        console.error('Failed to load WASM module:', error);
        return false;
    }
}

// Global functions for the HTML
window.createRule = async function() {
    if (!wasmModule) {
        showError('ruleResult', 'WASM module not loaded');
        return;
    }

    try {
        const field = document.getElementById('ruleField').value;
        const find = document.getElementById('findPattern').value;
        const replace = document.getElementById('replaceWith').value;
        const isLiteral = document.getElementById('isLiteral').checked;

        if (!find) {
            showError('ruleResult', 'Find pattern is required');
            return;
        }

        const ruleJson = create_simple_rule(field, find, replace, isLiteral);
        const rule = JSON.parse(ruleJson);
        
        document.getElementById('ruleJson').value = JSON.stringify(rule, null, 2);
        showSuccess('ruleResult', 'Rule created successfully!\\n' + JSON.stringify(rule, null, 2));
    } catch (error) {
        showError('ruleResult', 'Error creating rule: ' + error.message);
    }
};

window.testRegex = async function() {
    if (!wasmModule) {
        showError('regexResult', 'WASM module not loaded');
        return;
    }

    try {
        const pattern = document.getElementById('regexPattern').value;
        const text = document.getElementById('testText').value;
        const replacement = document.getElementById('replacement').value;

        if (!pattern || !text) {
            showError('regexResult', 'Pattern and text are required');
            return;
        }

        // First validate the regex
        const validation = validate_regex(pattern);
        if (!validation.valid) {
            showError('regexResult', 'Invalid regex: ' + validation.error);
            return;
        }

        const result = test_regex(pattern, text, replacement);
        if (result.success) {
            const message = `Matched: ${result.matched}\\nResult: "${result.result}"`;
            showSuccess('regexResult', message);
        } else {
            showError('regexResult', 'Regex error: ' + result.error);
        }
    } catch (error) {
        showError('regexResult', 'Error testing regex: ' + error.message);
    }
};

window.testRule = async function() {
    if (!wasmModule) {
        showError('testResult', 'WASM module not loaded');
        return;
    }

    try {
        // Create track
        const track = {
            name: document.getElementById('trackName').value,
            artist: document.getElementById('trackArtist').value,
            album: document.getElementById('trackAlbum').value || null,
            playcount: parseInt(document.getElementById('trackPlaycount').value) || 0,
            timestamp: null
        };

        const ruleJson = document.getElementById('ruleJson').value;
        
        if (!ruleJson.trim()) {
            showError('testResult', 'Rule JSON is required');
            return;
        }

        // Test if rule applies
        const trackJson = JSON.stringify(track);
        const applies = test_rule_applies(ruleJson, trackJson);
        
        if (!applies) {
            showInfo('testResult', 'Rule does not apply to this track');
            return;
        }

        // Apply the rule
        const result = apply_rule_to_track(ruleJson, trackJson);
        
        if (result.changed) {
            let changes = [];
            const edit = result.edit;
            
            if (edit.track_name !== edit.track_name_original) {
                changes.push(`Track: "${edit.track_name_original}" → "${edit.track_name}"`);
            }
            if (edit.artist_name !== edit.artist_name_original) {
                changes.push(`Artist: "${edit.artist_name_original}" → "${edit.artist_name}"`);
            }
            if (edit.album_name !== edit.album_name_original) {
                changes.push(`Album: "${edit.album_name_original}" → "${edit.album_name}"`);
            }
            if (edit.album_artist_name !== edit.album_artist_name_original) {
                changes.push(`Album Artist: "${edit.album_artist_name_original}" → "${edit.album_artist_name}"`);
            }
            
            if (changes.length > 0) {
                showSuccess('testResult', 'Rule applied successfully!\\n\\nChanges:\\n' + changes.join('\\n'));
            } else {
                showInfo('testResult', 'Rule applies but no changes were made');
            }
        } else {
            showInfo('testResult', 'Rule applies but no changes were made');
        }
    } catch (error) {
        showError('testResult', 'Error testing rule: ' + error.message);
    }
};

function showError(elementId, message) {
    const element = document.getElementById(elementId);
    element.textContent = message;
    element.className = 'result error';
}

function showSuccess(elementId, message) {
    const element = document.getElementById(elementId);
    element.textContent = message;
    element.className = 'result success';
}

function showInfo(elementId, message) {
    const element = document.getElementById(elementId);
    element.textContent = message;
    element.className = 'result';
}

// Mock data function
window.useMockData = function() {
    showSuccess('loginResult', 'Using mock data for demonstration');
    document.getElementById('loginSection').style.display = 'none';
    document.getElementById('tracksSection').style.display = 'block';
};

// Last.fm functions
window.loginToLastFm = async function() {
    if (!lastfmEditClient) {
        showError('loginResult', 'WASM module not loaded');
        return;
    }

    try {
        const username = document.getElementById('lastfmUsername').value;
        const password = document.getElementById('lastfmPassword').value;

        if (!username || !password) {
            showError('loginResult', 'Username and password are required');
            return;
        }

        lastfmEditClient.set_credentials(username, password);
        const authResult = await lastfmEditClient.test_auth();

        if (authResult.success) {
            showSuccess('loginResult', authResult.message);
            document.getElementById('loginSection').style.display = 'none';
            document.getElementById('tracksSection').style.display = 'block';
        } else {
            showError('loginResult', authResult.message);
        }
    } catch (error) {
        showError('loginResult', 'Login error: ' + error.message);
    }
};

window.loadRecentTracks = async function() {
    if (!lastfmEditClient) {
        showError('tracksResult', 'Not logged in');
        return;
    }

    try {
        const count = parseInt(document.getElementById('trackCount').value) || 10;
        const tracks = lastfmEditClient.get_mock_recent_tracks(count);
        
        displayTracks(tracks, 'tracksResult');
    } catch (error) {
        showError('tracksResult', 'Error loading tracks: ' + error.message);
    }
};

window.loadArtistTracks = async function() {
    if (!lastfmEditClient) {
        showError('artistTracksResult', 'Not logged in');
        return;
    }

    try {
        const artist = document.getElementById('artistName').value;
        const count = parseInt(document.getElementById('artistTrackCount').value) || 10;
        
        if (!artist) {
            showError('artistTracksResult', 'Artist name is required');
            return;
        }

        const tracks = lastfmEditClient.get_mock_artist_tracks(artist, count);
        displayTracks(tracks, 'artistTracksResult');
    } catch (error) {
        showError('artistTracksResult', 'Error loading artist tracks: ' + error.message);
    }
};

function displayTracks(tracks, elementId) {
    const element = document.getElementById(elementId);
    
    if (!tracks || tracks.length === 0) {
        element.textContent = 'No tracks found';
        element.className = 'result';
        return;
    }

    let html = `<div class="tracks-list">`;
    tracks.forEach((track, index) => {
        html += `
            <div class="track-item" onclick="loadTrackForTesting('${track.name}', '${track.artist}', '${track.album || ''}', ${track.playcount})">
                <div class="track-info">
                    <strong>${track.name}</strong><br>
                    by ${track.artist}<br>
                    ${track.album ? `from ${track.album}<br>` : ''}
                    <small>Plays: ${track.playcount}</small>
                </div>
            </div>
        `;
    });
    html += `</div>`;
    
    element.innerHTML = html;
    element.className = 'result success';
}

window.loadTrackForTesting = function(name, artist, album, playcount) {
    document.getElementById('trackName').value = name;
    document.getElementById('trackArtist').value = artist;
    document.getElementById('trackAlbum').value = album;
    document.getElementById('trackPlaycount').value = playcount;
    
    // Scroll to track tester
    document.querySelector('.track-tester').scrollIntoView({ behavior: 'smooth' });
};

// Process multiple tracks with rules
window.processTracksWithRules = async function() {
    if (!wasmModule) {
        showError('processResult', 'WASM module not loaded');
        return;
    }

    try {
        const rulesJson = document.getElementById('rulesJson').value;
        
        if (!rulesJson.trim()) {
            showError('processResult', 'Rules JSON is required');
            return;
        }

        // Get tracks from the current tracks display
        const tracksDisplay = document.getElementById('tracksResult');
        if (!tracksDisplay || !tracksDisplay.querySelector('.tracks-list')) {
            showError('processResult', 'No tracks loaded. Load some tracks first.');
            return;
        }

        showInfo('processResult', 'Processing tracks with rules...');

        // For now, use the loaded tracks from the display
        // In a real implementation, this would process the actual Last.fm tracks
        let sampleTracks = [
            {
                name: "Bohemian Rhapsody (2011 Remaster)",
                artist: "Queen",
                album: "A Night at the Opera (Deluxe Edition)",
                playcount: 42,
                timestamp: 1640995200
            }
        ];

        const tracksJson = JSON.stringify(sampleTracks);
        const result = process_tracks_with_rules(tracksJson, rulesJson);
        
        if (result.tracks_with_changes > 0) {
            showSuccess('processResult', `Processed ${result.processed_tracks} tracks, ${result.tracks_with_changes} would be changed`);
        } else {
            showInfo('processResult', `Processed ${result.processed_tracks} tracks, no changes needed`);
        }
    } catch (error) {
        showError('processResult', 'Error processing tracks: ' + error.message);
    }
};

// Initialize when the page loads
document.addEventListener('DOMContentLoaded', async () => {
    const loadingElement = document.createElement('div');
    loadingElement.textContent = 'Loading WASM module...';
    loadingElement.style.cssText = 'position: fixed; top: 10px; right: 10px; background: #007bff; color: white; padding: 10px; border-radius: 5px; z-index: 1000;';
    document.body.appendChild(loadingElement);

    const success = await initWasm();
    
    if (success) {
        loadingElement.textContent = '✓ WASM Ready';
        loadingElement.style.background = '#28a745';
        setTimeout(() => loadingElement.remove(), 2000);
    } else {
        loadingElement.textContent = '✗ WASM Failed';
        loadingElement.style.background = '#dc3545';
    }
});
