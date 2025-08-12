# MusicBrainz Confirmation for Deluxe Edition Removal

## Feature Overview

This feature adds MusicBrainz confirmation to rewrite rules, allowing you to safely remove "(Deluxe Edition)" from album names while ensuring the original version exists.

## How It Works

1. **Rule Configuration**: Rules can now have `requires_musicbrainz_confirmation: true`
2. **Application Process**:
   - Rule matches track metadata
   - Applies rewrite temporarily 
   - Searches MusicBrainz for the rewritten track
   - Only applies if found with sufficient confidence (default 70%)
   - Skips rule if not found in MusicBrainz

## Example Use Case

### Before:
```
Artist: Radiohead
Album: OK Computer (Deluxe Edition)
Track: Paranoid Android
```

### With MusicBrainz Confirmation Rule:
```json
{
  "name": "Remove Deluxe Edition with MusicBrainz Confirmation",
  "album_name": {
    "find": "^(.+?) \\(Deluxe Edition\\)$",
    "replace": "$1"
  },
  "requires_musicbrainz_confirmation": true
}
```

### Process:
1. Rule matches "OK Computer (Deluxe Edition)"
2. Creates temporary rewrite: "OK Computer" 
3. Searches MusicBrainz for "Radiohead - Paranoid Android - OK Computer"
4. If found → applies rewrite → "OK Computer"
5. If not found → skips rule → keeps "OK Computer (Deluxe Edition)"

### After (if original exists in MusicBrainz):
```
Artist: Radiohead
Album: OK Computer
Track: Paranoid Android
```

## Benefits

- **Safe Deluxe Edition Removal**: Only removes when original exists
- **Prevents Metadata Loss**: Keeps deluxe info if no standard version exists
- **Automatic Validation**: No manual checking required
- **Configurable Confidence**: Adjust threshold as needed

## Implementation Details

### New Fields in RewriteRule:
- `requires_musicbrainz_confirmation: bool` - Enable MusicBrainz validation

### New Functions:
- `MusicBrainzValidator::validate_track()` - Check if track exists
- `apply_all_rules_with_musicbrainz_validation()` - Apply rules with validation

### UI Updates:
- New checkbox in rule editor: "Require MusicBrainz confirmation for this rule"
- Tooltip explaining the feature

This feature is perfect for cleaning up deluxe editions while preserving metadata integrity!