# Rewrite Rules Guide

Rewrite rules are the core pattern-based cleaning system in Scrobble Scrubber. They use regular expressions to find and replace problematic metadata patterns.

## Rule Structure

Each rewrite rule can target any combination of four metadata fields:
- **Track Name** - Song titles
- **Artist Name** - Performing artist
- **Album Name** - Album titles
- **Album Artist Name** - Album-level artist attribution

**Important**: For a rule to apply, **ALL** specified patterns must match. If you define patterns for both artist and album, the track must match both patterns or the rule won't trigger.

## Rule Format

Rules consist of **find/replace patterns** using rust regular expressions:

```
Find Pattern: ^(.+) - \d{4} Remaster$
Replace: $1
```

This rule finds tracks ending with "- [Year] Remaster" and replaces the entire title with just the captured song name.

## Capture Groups Explained

Capture groups are the key to powerful rewrite rules - they let you extract and reuse parts of the matched text. They're created using parentheses `()` in your find pattern.

**Basic Numbered Groups:**
```
Find: ^(.+) - (\d{4}) Remaster$
Replace: $1 (originally from $2)
```
- `(.+)` captures the song title as group 1
- `(\d{4})` captures the year as group 2
- Input: "Hotel California - 1976 Remaster"
- Output: "Hotel California (originally from 1976)"

**Multiple Groups Example:**
```
Find: ^(.+) [Ff]eat\. (.+) - (.+)$
Replace: $3 by $1 featuring $2
```
- Group 1: Main artist
- Group 2: Featured artist
- Group 3: Song title
- Input: "Taylor Swift feat. Ed Sheeran - Everything Has Changed"
- Output: "Everything Has Changed by Taylor Swift featuring Ed Sheeran"

**Named Capture Groups:**
```
Find: ^(?P<artist>.+) - (?P<song>.+) \((?P<year>\d{4})\)$
Replace: ${song} by ${artist}
```
- Uses `(?P<name>...)` syntax for named groups
- Reference with `${name}` in replacement
- Input: "The Beatles - Hey Jude (1968)"
- Output: "Hey Jude by The Beatles"

**Escaping Special Characters:**
- Use `\$` for literal dollar signs
- Use `\{` and `\}` for literal braces
- Use `\\` for literal backslashes

## Real Examples

**Remove Remaster Suffixes:**
- Find: `^(.+) - \d{4} Digital Remaster$`
- Replace: `$1`
- Input: "The Big Ship - 2004 Digital Remaster"
- Output: "The Big Ship"

**Normalize Featuring Formats:**
- Find: `(.+) [Ff]t\. (.+)`
- Replace: `$1 feat. $2`
- Input: "Artist ft. Other Artist"
- Output: "Artist feat. Other Artist"

**Complex Multi-Field Rule:**
```
Artist Name: ^Chris Thile$ → Chris Thile & Michael Daves
Album Name: Sleep With One Eye Open → Sleep With One Eye Open
Album Artist: .* → Chris Thile & Michael Daves
```

This rule demonstrates the "ALL patterns must match" requirement - it only applies when:
1. The artist is exactly "Chris Thile" AND
2. The album contains "Sleep With One Eye Open" AND
3. There is an album artist field (any value)

Only when all three conditions are met will the rule trigger and correct the collaboration attribution.

## Advanced Examples

### Remove Version Suffixes
```
Find: ^(.+?) \((Single|Album|Radio) Version\)$
Replace: $1
```
- Removes "(Single Version)", "(Album Version)", or "(Radio Version)"
- The `?` makes the capture non-greedy to handle multiple parentheses correctly

### Clean Up Whitespace
```
Find: \s+
Replace: " "
```
- Replaces multiple spaces, tabs, or newlines with a single space
- Apply to all metadata fields for comprehensive cleaning

### Artist Collaboration Fixes
```
Artist Name: ^(.+) & (.+)$
Replace: $1 feat. $2
Track Name: .*
Replace: $0
```
- Converts "Artist & Other" to "Artist feat. Other"
- The track name pattern `.*` with `$0` replacement means "match anything and keep it unchanged"
- This ensures the rule only applies to tracks that have both an artist collaboration AND any track name

### Remove Bonus Track Indicators
```
Find: ^(.+?) \(Bonus Track\)$
Replace: $1
```
- Removes "(Bonus Track)" suffixes from track names

### Year Extraction and Formatting
```
Find: ^(.+?) \((\d{4})\)$
Replace: $1 [$2]
```
- Changes "Song Title (2020)" to "Song Title [2020]"
- Converts parentheses to square brackets for year indicators

## Key Features

- **All Patterns Must Match**: For multi-field rules, every specified pattern must match for the rule to apply
- **Whole String Replacement**: When a pattern matches, the entire field is replaced
- **30+ Default Rules**: Ships with comprehensive rules for common issues
- **Custom Rules**: Create your own rules through the GUI or configuration

## Tips for Creating Rules

### Start Simple
Begin with single-field rules before attempting complex multi-field patterns:
```
# Good first rule
Find: ^(.+) - Remaster$
Replace: $1

# Complex rule to attempt later
Artist: ^(.+)$, Album: ^(.+) \(Deluxe\)$, Track: ^(.+)$
```

### Test Thoroughly
- Use the Rule Workshop in the GUI to test patterns against real data
- Start with dry run mode to preview changes
- Test edge cases and unusual formatting

### Use Non-Greedy Matching
When dealing with multiple similar patterns, use `?` to make captures non-greedy:
```
# Greedy (might capture too much)
Find: ^(.+) \(.+\)$

# Non-greedy (stops at first match)
Find: ^(.+?) \(.+\)$
```

### Escape Special Characters
Remember to escape regex special characters when you want literal matches:
```
# To match literal parentheses
Find: ^(.+) \(Live\)$

# To match literal dots
Find: ^(.+) feat\.(.+)$
```

### Consider Order of Operations
Rules are applied in order, so place more specific rules before general ones:
```
# Specific rule first
Find: ^(.+) - \d{4} Digital Remaster$
Replace: $1

# General rule second  
Find: ^(.+) - Remaster$
Replace: $1
```

## Common Regex Patterns

| Pattern | Meaning | Example |
|---------|---------|---------|
| `^` | Start of string | `^The` matches "The Beatles" |
| `$` | End of string | `Remaster$` matches "2020 Remaster" |
| `.` | Any character | `a.c` matches "abc", "axc" |
| `+` | One or more | `\d+` matches "123", "7" |
| `*` | Zero or more | `\s*` matches "", " ", "   " |
| `?` | Zero or one | `colou?r` matches "color", "colour" |
| `\d` | Any digit | `\d{4}` matches "2020" |
| `\s` | Any whitespace | `\s+` matches " ", "\t", "\n" |
| `[Ff]` | Character class | `[Ff]t` matches "ft" or "Ft" |
| `()` | Capture group | `(.+)` captures for `$1` |
| `\|` | OR operator | `(Single\|Album)` matches either |

## Troubleshooting

### Rule Not Applying
- Check that ALL field patterns match (common mistake)
- Verify regex syntax is correct
- Test pattern matching in isolation
- Ensure input data matches expected format

### Unexpected Replacements
- Check for greedy vs non-greedy matching
- Verify capture group numbering
- Test with edge cases and unusual formatting
- Use Rule Workshop to preview changes

### Performance Issues
- Avoid overly complex regex patterns
- Use literal string matching when possible
- Test rules with large datasets before deploying
- Consider breaking complex rules into simpler ones

---

*For more information on using rewrite rules in the Scrobble Scrubber interface, see the main [User Guide](USER_GUIDE.md).*