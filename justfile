fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

readme:
    #!/usr/bin/env bash
    set -euo pipefail

    # Extract rustdoc comments from lib.rs and convert to markdown
    echo "Generating README.md from rustdoc..."

    # Use cargo doc to generate docs, then extract the main module doc
    cargo doc --no-deps --document-private-items --quiet

    # Extract the rustdoc content and convert it to README format
    sed -n '/^\/\/!/p' lib/src/lib.rs | \
    sed 's/^\/\/! \?//' | \
    sed 's/^\/\/!$//' > README.md

    echo "README.md generated successfully!"

clippy:
    cargo clippy --all-targets --all-features -- -D warnings

checks:
    just fmt
    just fmt-check
    just clippy
    cargo test --all

# Update cargoHash for the Dioxus app package in flake.nix
update-cargo-hash:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "🔄 Updating cargoHash for app package..."

    # Set cargoHash to empty string to trigger hash mismatch
    sed -i 's/cargoHash = ".*";/cargoHash = "";/' flake.nix

    echo "🏗️  Building with empty hash to get the correct one..."
    # Try to build and capture the error output
    if OUTPUT=$(nix build .#app 2>&1); then
        echo "✅ Build succeeded, no hash update needed"
        exit 0
    else
        # Extract the correct hash from the error message
        NEW_HASH=$(echo "$OUTPUT" | grep "got:" | sed 's/.*got: *//')

        if [ -z "$NEW_HASH" ]; then
            echo "❌ Could not extract hash from build output"
            echo "Build output:"
            echo "$OUTPUT"
            exit 1
        fi

        echo "📝 Found new hash: $NEW_HASH"

        # Update flake.nix with the correct hash using | as delimiter to avoid issues with / in hash
        sed -i "s|cargoHash = \"\";|cargoHash = \"$NEW_HASH\";|" flake.nix

        echo "✅ Updated flake.nix with new cargoHash"
        echo "🔨 Verifying the build..."

        # Verify the build works with the new hash
        if nix build .#app; then
            echo "✅ Build successful with new hash!"
        else
            echo "❌ Build still failing with new hash"
            exit 1
        fi
    fi

# Generate all required icon formats from a source image
generate-icons SOURCE_IMAGE:
    #!/usr/bin/env bash
    set -euo pipefail

    SOURCE="{{SOURCE_IMAGE}}"
    ICONS_DIR="app/assets/icons"

    echo "🎨 Generating icons from: $SOURCE"

    # Check if source file exists
    if [[ ! -f "$SOURCE" ]]; then
        echo "❌ Error: Source image '$SOURCE' not found"
        exit 1
    fi

    # Create icons directory
    mkdir -p "$ICONS_DIR"

    # Check if ImageMagick is available
    if ! command -v convert &> /dev/null; then
        echo "❌ Error: ImageMagick 'convert' command not found"
        echo "Please ensure ImageMagick is installed and available in PATH"
        exit 1
    fi

    echo "📐 Generating PNG icons at various sizes..."

    # Generate PNG icons for Linux and various sizes (ensure 32-bit RGBA)
    for size in 16 24 32 64 128 256; do
        echo "  📏 Creating ${size}x${size}.png"
        convert "$SOURCE" -resize "${size}x${size}" -depth 8 -type TrueColorAlpha "$ICONS_DIR/${size}x${size}.png"
    done

    # Generate 2x versions for high-DPI displays
    echo "  📏 Creating 128x128@2x.png (256x256)"
    cp "$ICONS_DIR/256x256.png" "$ICONS_DIR/128x128@2x.png"

    echo "🍎 Generating macOS .icns file..."
    # Create icns for macOS (requires multiple sizes embedded)
    if command -v png2icns &> /dev/null; then
        # Use png2icns if available (better quality)
        png2icns "$ICONS_DIR/icon.icns" "$ICONS_DIR/16x16.png" "$ICONS_DIR/32x32.png" "$ICONS_DIR/128x128.png" "$ICONS_DIR/256x256.png"
    else
        # Fallback to ImageMagick
        echo "  ℹ️  Using ImageMagick for .icns (consider installing png2icns for better results)"
        convert "$ICONS_DIR/16x16.png" "$ICONS_DIR/32x32.png" "$ICONS_DIR/128x128.png" "$ICONS_DIR/256x256.png" "$ICONS_DIR/icon.icns"
    fi

    echo "🪟 Generating Windows .ico file..."
    # Create ico for Windows (embed multiple sizes)
    convert "$ICONS_DIR/16x16.png" "$ICONS_DIR/24x24.png" "$ICONS_DIR/32x32.png" "$ICONS_DIR/64x64.png" "$ICONS_DIR/icon.ico"

    echo "✅ Icon generation complete!"
    echo ""
    echo "📁 Generated files in $ICONS_DIR:"
    ls -la "$ICONS_DIR"
    echo ""
    echo "🔧 To use these icons, update your app/Dioxus.toml:"
    echo 'icon = ['
    echo '  "assets/icons/32x32.png",'
    echo '  "assets/icons/128x128.png",'
    echo '  "assets/icons/128x128@2x.png",'
    echo '  "assets/icons/icon.icns",'
    echo '  "assets/icons/icon.ico"'
    echo ']'

release bump_type="patch":
    #!/usr/bin/env bash
    set -euo pipefail

    echo "🚀 Releasing new {{bump_type}} version for workspace packages..."

    # Check if cargo-edit is installed
    if ! command -v cargo-set-version &> /dev/null; then
        echo "❌ cargo-edit is not installed. Installing..."
        cargo install cargo-edit
    fi

    # Get current version from workspace
    current_version=$(grep "^version" Cargo.toml | head -1 | cut -d'"' -f2)
    echo "📦 Current workspace version: $current_version"

    # Bump version in workspace root
    echo "⬆️  Bumping {{bump_type}} version in workspace..."
    cargo set-version --workspace --bump {{bump_type}}

    # Get new version
    new_version=$(grep "^version" Cargo.toml | head -1 | cut -d'"' -f2)
    echo "📦 New version: $new_version"

    # Run checks to ensure everything still works
    echo "🔍 Running checks..."
    just checks

    # Build release versions for lib and cli
    echo "🔨 Building release versions..."
    echo "  📚 Building library..."
    cargo build --release -p scrobble-scrubber
    echo "  🖥️  Building CLI..."
    cargo build --release -p scrobble-scrubber-cli

    # Add all changes
    git add .

    # Create commit with auto-generated message
    echo "💾 Committing changes..."
    git commit -m "Bump version to $new_version

    🤖 Generated with [Claude Code](https://claude.ai/code)

    Co-Authored-By: Claude <noreply@anthropic.com>"

    # Create git tag
    echo "🏷️  Creating tag v$new_version..."
    git tag "v$new_version"

    echo "✅ Release v$new_version ready!"
    echo "📤 To publish, run:"
    echo "   git push origin master"
    echo "   git push origin v$new_version"
    echo "   just publish-crates"

# Full release with automatic push and publish
# Usage: just publish [patch|minor|major]
publish bump_type="patch":
    #!/usr/bin/env bash
    set -euo pipefail

    # Run the release process
    just release {{bump_type}}

    # Get the new version for confirmation
    new_version=$(grep "^version" Cargo.toml | head -1 | cut -d'"' -f2)

    echo "🌐 Publishing release v$new_version..."

    # Push commits and tags
    echo "📤 Pushing to remote..."
    git push origin master
    git push origin "v$new_version"

    # Publish packages to crates.io
    just publish-crates

    echo "🎉 Release v$new_version published successfully!"

# Publish workspace packages to crates.io in the correct order
publish-crates:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "📦 Publishing packages to crates.io..."

    # First publish the library package (scrobble-scrubber)
    echo "📚 Publishing library package..."
    cd lib && cargo publish --allow-dirty
    
    # Wait a bit for crates.io to process the library
    echo "⏳ Waiting for crates.io to process library package..."
    sleep 30
    
    # Then publish the CLI package (scrobble-scrubber-cli)
    echo "🖥️  Publishing CLI package..."
    cd ../cli && cargo publish --allow-dirty
    
    echo "✅ All packages published successfully!"

# Dry run of publishing to see what would be published
publish-dry-run:
    #!/usr/bin/env bash
    set -euo pipefail

    echo "🔍 Dry run of publishing packages..."

    echo "📚 Library package (scrobble-scrubber):"
    cd lib && cargo publish --dry-run --allow-dirty
    
    echo ""
    echo "🖥️  CLI package (scrobble-scrubber-cli):"
    cd ../cli && cargo publish --dry-run --allow-dirty
    
    echo ""
    echo "✅ Dry run complete. Review the output above before publishing."
