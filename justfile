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
