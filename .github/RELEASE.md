# Release Process

This document explains how to create releases for the Scrobble Scrubber application.

## Automated Release Process

The project uses GitHub Actions to automatically build and release the application for multiple platforms.

### Creating a Release

#### Method 1: Git Tag (Recommended)

1. Ensure your changes are committed and pushed to the main branch
2. Create and push a version tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
3. The GitHub Actions workflow will automatically:
   - Build the application for Linux, macOS, and Windows
   - Create appropriate package formats for each platform
   - Create a GitHub release with all the built artifacts

#### Method 2: Release Branch

1. Create a release branch:
   ```bash
   git checkout -b release/v1.0.0
   git push origin release/v1.0.0
   ```
2. This will trigger builds but will NOT create a GitHub release
3. Artifacts will be available as workflow artifacts for testing

### Supported Package Formats

The automated build process creates the following package formats:

#### Linux
- **`.deb`** - Debian/Ubuntu package
- **`.AppImage`** - Universal Linux application bundle

#### macOS
- **`.app`** - macOS application bundle
- **`.dmg`** - macOS disk image installer

#### Windows
- **`.msi`** - Windows Installer package
- **`.exe`** - NSIS installer

### Version Numbering

Use semantic versioning (semver) for tags:
- `v1.0.0` - Major release
- `v1.0.1` - Patch release
- `v1.1.0` - Minor release
- `v1.0.0-beta.1` - Pre-release (will be marked as prerelease)

### Testing Releases

Before creating a public release:

1. Use the release branch method to test builds
2. Download and test the artifacts on target platforms
3. Once satisfied, create the actual release tag

### Manual Release (if needed)

If you need to create a release manually:

```bash
# Navigate to the app directory
cd app

# Build for specific platform
dx bundle --release --platform linux --package-types deb,appimage
dx bundle --release --platform macos --package-types macos,dmg
dx bundle --release --platform windows --package-types msi,nsis
```

### Troubleshooting

#### Build Failures

1. **Linux dependencies**: The workflow installs required system dependencies automatically
2. **macOS code signing**: Currently not configured - may need manual setup for distribution
3. **Windows**: Should work out of the box with the MSVC toolchain

#### Missing Artifacts

If artifacts are missing from the release:
1. Check the GitHub Actions logs for build errors
2. Verify the bundle command completed successfully
3. Check that the artifact upload step found the expected files

### Dependencies

The build process requires:
- Rust toolchain (automatically installed)
- Dioxus CLI (automatically installed)
- Platform-specific system dependencies (automatically installed)

### Customizing the Release Process

To modify the release process:
1. Edit `.github/workflows/release.yml`
2. Update package types, platforms, or build options as needed
3. Test changes using a release branch before applying to main

### Security Notes

- The workflow uses `GITHUB_TOKEN` which is automatically provided
- No additional secrets are required for basic releases
- For code signing (macOS/Windows), additional secrets would need to be configured