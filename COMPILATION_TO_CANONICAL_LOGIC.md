# Compilation to Canonical Provider - Logic Documentation

## Purpose
The `CompilationToCanonicalProvider` is designed to suggest moving tracks from compilation albums (like "Greatest Hits", "Now That's What I Call Music", etc.) to their original studio albums. For example, if you have "Bohemian Rhapsody" scrobbled from "Greatest Hits", it should suggest moving it to "A Night at the Opera".

## Current Implementation (Crude Heuristics)

### How It Works

1. **For each track being analyzed:**
   - Look up the track in MusicBrainz by artist + title
   - Get all releases (albums) that contain this recording
   - Filter the releases to find the "canonical" one

2. **The "Canonical Release" Selection Process:**

   **Step 1: Filter OUT these types of releases:**
   - ❌ **Bootlegs** - Releases with `status = "Bootleg"` (unofficial recordings)
   - ❌ **Promotional** - Releases with `status = "Promotion"`  
   - ❌ **Pseudo-releases** - Releases with `status = "PseudoRelease"`
   - ❌ **Live albums** - Albums with titles containing "live at", "live in", "concert", "unplugged", etc.
   - ❌ **Compilations** - Albums with titles containing:
     - "greatest", "best of", "collection", "essential"
     - "anthology", "ultimate", "hits", "singles"
     - "soundtrack", "ost", "various artists"
   - ❌ **Singles** - Releases where the title matches the track name
   - ❌ **Various Artists releases** - Where artist credit is "Various Artists", "VA", etc.

   **Step 2: From remaining releases, pick the EARLIEST by release date**

3. **Only suggest a change if:**
   - A canonical release was found
   - It's different from the current album

## Problems with This Approach

### 1. Title-Based Heuristics Are Unreliable
- **False Positives:** "The Beatles Box" is detected as non-compilation (doesn't contain our keywords)
- **False Negatives:** An album called "Live and Let Die" might be filtered as a live album
- **Language Issues:** Non-English compilations like "Grandes Éxitos" won't be detected

### 2. "Earliest Release" Doesn't Mean "Original Album"
The current logic assumes the earliest release is the original, but this fails for:
- **Reissued singles** that predate the album
- **Regional releases** (Japanese release might be earlier but not canonical)
- **Box sets** released early in an artist's career
- **Other compilations** that don't match our keyword list

### 3. MusicBrainz Data Limitations
- **Missing release status:** Many releases don't have status field populated
- **No compilation flag:** MusicBrainz doesn't directly mark compilations vs studio albums
- **Release group info needed:** The real solution requires fetching release groups (additional API calls)

## What We SHOULD Be Doing

### Proper MusicBrainz Approach

1. **Use Release Groups:**
   ```
   Recording -> Release Group -> Primary Type
   ```
   - Release groups have a `primary-type` field: "Album", "Single", "EP", "Compilation", "Soundtrack", "Live"
   - This would definitively identify compilations

2. **Use Secondary Types:**
   - Release groups also have `secondary-types` like "Compilation", "Live", "Soundtrack", "Remix"
   - Much more reliable than title parsing

3. **Respect Artist Intent:**
   - Some "greatest hits" are considered canonical by the artist
   - Some tracks were only released on compilations
   - Need to handle these edge cases

### Example of Current Failures

**Input:** "Come Together" by The Beatles from "Abbey Road" (1969)
**Current Output:** Suggests "1962–1970: The Best Of" because:
- It's not detected as a compilation (no keywords match)
- It has an earlier date in the database
- Our heuristics fail

**Expected:** No suggestion (already on the original album)

## Short-term Improvements (Without API Changes)

1. **Expand keyword lists** for compilation detection
2. **Check album artist** - if different from track artist, likely compilation
3. **Track count heuristic** - compilations often have 20+ tracks from different albums
4. **Year span check** - if album contains tracks spanning many years, likely compilation
5. **Whitelist known studio albums** for major artists

## Long-term Solution

1. **Fetch release group data** from MusicBrainz
2. **Use proper type fields** instead of title heuristics
3. **Cache release group lookups** to minimize API calls
4. **Build a learning system** that improves based on user corrections
5. **Allow user-defined rules** for specific artists/albums

## Current Workarounds

The provider is still useful for obvious cases:
- ✅ "Now That's What I Call Music" -> Original albums
- ✅ "Greatest Hits" -> Original albums (when keyword matches)
- ✅ Soundtracks -> Original albums
- ❌ Edge cases and non-English compilations
- ❌ Box sets and reissues

## Conclusion

The current implementation uses crude pattern matching on album titles and basic date sorting. While it works for obvious compilations, it fails on edge cases and can suggest inappropriate moves (like from studio album to a different compilation). The proper solution requires using MusicBrainz's release group types, but that would require additional API calls and significant refactoring.