#!/usr/bin/env python3
"""
Script to analyze remaster patterns and suggest new rules based on common patterns.
This is based on research of real Last.fm data patterns.
"""

import json
import re

# Common remaster patterns found in Last.fm data that might not be covered
additional_patterns = [
    # Album name patterns (these would go in album rules)
    {
        "name": "Remove Anniversary Edition from Album",
        "description": "Removes patterns like 'Album (50th Anniversary Edition)' from album names",
        "examples": ["Abbey Road (50th Anniversary Edition) → Abbey Road", "Pet Sounds (50th Anniversary Edition) → Pet Sounds"],
        "album_name": {
            "find": "^(.+?) \\(\\d+th Anniversary Edition\\)$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Deluxe Remaster Album",
        "description": "Removes patterns like 'Album (Deluxe Remaster)' from album names", 
        "examples": ["Dark Side of the Moon (Deluxe Remaster) → Dark Side of the Moon"],
        "album_name": {
            "find": "^(.+?) \\(Deluxe Remaster\\)$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Super Deluxe Edition Album",
        "description": "Removes patterns like 'Album (Super Deluxe Edition)' from album names",
        "examples": ["The White Album (Super Deluxe Edition) → The White Album"],
        "album_name": {
            "find": "^(.+?) \\(Super Deluxe Edition\\)$", 
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    # Track name patterns (additions to existing track rules)
    {
        "name": "Remove HD Remastered",
        "description": "Removes patterns like 'Song - HD Remastered'",
        "examples": ["Bohemian Rhapsody - HD Remastered → Bohemian Rhapsody"],
        "track_name": {
            "find": "^(.+?) - HD Remastered$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Hi-Res Remaster",
        "description": "Removes patterns like 'Song - Hi-Res Remaster'",
        "examples": ["Stairway to Heaven - Hi-Res Remaster → Stairway to Heaven"],
        "track_name": {
            "find": "^(.+?) - Hi-Res Remaster$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Stereo Remaster",
        "description": "Removes patterns like 'Song - Stereo Remaster' or 'Song - 2009 Stereo Remaster'",
        "examples": ["Come Together - 2009 Stereo Remaster → Come Together", "Here Comes the Sun - Stereo Remaster → Here Comes the Sun"],
        "track_name": {
            "find": "^(.+?) - (\\d{4} )?Stereo Remaster$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Mono Remaster",
        "description": "Removes patterns like 'Song - Mono Remaster' or 'Song - 2014 Mono Remaster'",
        "examples": ["I Want to Hold Your Hand - 2014 Mono Remaster → I Want to Hold Your Hand"],
        "track_name": {
            "find": "^(.+?) - (\\d{4} )?Mono Remaster$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Expanded Edition Track",
        "description": "Removes patterns like 'Song - Expanded Edition'",
        "examples": ["Norwegian Wood - Expanded Edition → Norwegian Wood"],
        "track_name": {
            "find": "^(.+?) - Expanded Edition$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Collector's Edition Track",
        "description": "Removes patterns like 'Song - Collector's Edition'",
        "examples": ["Love Me Do - Collector's Edition → Love Me Do"],
        "track_name": {
            "find": "^(.+?) - Collector's Edition$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Anniversary Remaster Track",
        "description": "Removes patterns like 'Song - 50th Anniversary Remaster'",
        "examples": ["Yesterday - 50th Anniversary Remaster → Yesterday", "Help! - 25th Anniversary Remaster → Help!"],
        "track_name": {
            "find": "^(.+?) - \\d+th Anniversary Remaster$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Special Edition Track",
        "description": "Removes patterns like 'Song - Special Edition'",
        "examples": ["Revolution - Special Edition → Revolution"],
        "track_name": {
            "find": "^(.+?) - Special Edition$",
            "replace": "$1"
        },
        "requires_confirmation": False
    },
    {
        "name": "Remove Year Digital Remaster with Extra Info",
        "description": "Removes patterns like 'Song - 2009 Digital Remaster / Extra Info'",
        "examples": ["The End - 2009 Digital Remaster / Medley → The End"],
        "track_name": {
            "find": "^(.+?) - \\d{4} Digital Remaster / .*$",
            "replace": "$1"
        },
        "requires_confirmation": False
    }
]

def load_current_rules():
    """Load the current default rules JSON file."""
    try:
        with open('app/assets/default_remaster_rules.json', 'r') as f:
            return json.load(f)
    except FileNotFoundError:
        print("Current rules file not found!")
        return None

def save_updated_rules(rules_data):
    """Save the updated rules back to the JSON file."""
    with open('app/assets/default_remaster_rules.json', 'w') as f:
        json.dump(rules_data, f, indent=2)
    print("Updated rules saved to app/assets/default_remaster_rules.json")

def analyze_and_update():
    """Analyze current rules and add new patterns."""
    current_rules = load_current_rules()
    if not current_rules:
        return
    
    print(f"Current rules count: {len(current_rules['rules'])}")
    
    # Check which patterns are already covered
    existing_patterns = set()
    for rule in current_rules['rules']:
        if 'track_name' in rule and 'find' in rule['track_name']:
            existing_patterns.add(rule['track_name']['find'])
    
    # Add new patterns that aren't already covered
    new_rules_added = 0
    for pattern in additional_patterns:
        pattern_key = None
        if 'track_name' in pattern:
            pattern_key = pattern['track_name']['find']
        elif 'album_name' in pattern:
            pattern_key = pattern['album_name']['find']
            
        if pattern_key and pattern_key not in existing_patterns:
            current_rules['rules'].append(pattern)
            new_rules_added += 1
            print(f"Added new rule: {pattern['name']}")
    
    if new_rules_added > 0:
        # Update version and description
        current_version = current_rules.get('version', '1.0')
        version_parts = current_version.split('.')
        version_parts[1] = str(int(version_parts[1]) + 1)
        current_rules['version'] = '.'.join(version_parts)
        
        current_rules['description'] = f"Comprehensive set of rewrite rules to remove remaster information from track and album names, based on analysis of 600+ real Last.fm tracks. Updated with {new_rules_added} additional patterns."
        
        save_updated_rules(current_rules)
        print(f"Added {new_rules_added} new rules. New version: {current_rules['version']}")
        print(f"Total rules now: {len(current_rules['rules'])}")
    else:
        print("No new rules needed - all patterns already covered!")

if __name__ == "__main__":
    analyze_and_update()