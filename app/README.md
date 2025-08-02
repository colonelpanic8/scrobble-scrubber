# Scrobble Scrubber App

A desktop application for managing and cleaning Last.fm scrobbles with support for rewrite rules, pending edits, and automated processing.

## Project Structure

```
app/
├─ assets/           # Application assets (icons, styles, etc.)
├─ src/
│  ├─ main.rs       # Application entry point
│  ├─ components/   # UI components
│  ├─ tray.rs       # System tray functionality
│  ├─ icons.rs      # Window icon configuration
│  └─ ...           # Other modules
├─ Cargo.toml       # Rust dependencies and configuration
└─ Dioxus.toml      # Dioxus framework configuration
```

### Tailwind
1. Install npm: https://docs.npmjs.com/downloading-and-installing-node-js-and-npm
2. Install the Tailwind CSS CLI: https://tailwindcss.com/docs/installation
3. Run the following command in the root of the project to start the Tailwind CSS compiler:

```bash
npx tailwindcss -i ./tailwind.css -o ./assets/tailwind.css --watch
```

### Serving Your App

Run the following command in the root of your project to start developing with the default platform:

```bash
dx serve --platform desktop
```

## Building and Releases

### Development Build
```bash
dx build --platform desktop
```

### Release Build
```bash
dx build --release --platform desktop
```

### Creating Distribution Packages

The project supports automated release builds for multiple platforms. See the [Release Documentation](../.github/RELEASE.md) for detailed instructions.

#### Manual Package Creation

For Linux:
```bash
dx bundle --release --platform linux --package-types deb,appimage
```

For macOS:
```bash
dx bundle --release --platform macos --package-types macos,dmg
```

For Windows:
```bash
dx bundle --release --platform windows --package-types msi,nsis
```

### Automated Releases

The project includes GitHub Actions workflows for automated cross-platform builds:

1. **Tag-based releases**: Push a git tag (e.g., `v1.0.0`) to trigger automatic builds and GitHub release creation
2. **Branch-based builds**: Push to a `release/*` branch to build artifacts without creating a release

See [Release Documentation](../.github/RELEASE.md) for complete details.

### Code Quality

Before committing, ensure code quality:
```bash
cargo fmt --all                                    # Format code
cargo clippy --all-targets --all-features -- -D warnings  # Lint code
dx build --release                                 # Test build
```

