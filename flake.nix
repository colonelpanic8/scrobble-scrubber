{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    self,
    nixpkgs,
    rust-overlay,
    flake-utils,
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        overlays = [(import rust-overlay)];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs;
            [
              # System dependencies for reqwest/openssl
              pkg-config
              openssl
              just

              # WASM development
              # wasm-pack
              # nodejs
              # nodePackages.npm
              claude-code
              # For TUI development
              libiconv

              # Linux-specific dependencies for GUI applications
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              # Tauri dependencies
              gtk3
              webkitgtk_4_1
              librsvg
              libsoup_3
              libappindicator-gtk3
              xdotool

              # Linux-specific tools
              # Dioxus development tools (only on Linux to avoid webkitgtk issues on macOS)
              dioxus-cli
              
              # AppImage tooling
              appimage-run
            ]
            ++ pkgs.lib.optionals (!pkgs.stdenv.isDarwin) [
              # Image processing for icon generation (may not support all platforms)
              imagemagick
            ]
            ++ [
              # GitHub CLI for monitoring releases (cross-platform)
              gh
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              # macOS specific dependencies
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.SystemConfiguration
            ];

          # Environment variables
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

          # For OpenSSL on some systems
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
          WEBKIT_DISABLE_DMABUF_RENDERER = 1;

          shellHook = pkgs.lib.optionalString pkgs.stdenv.isLinux ''
            # Add library paths for system tray functionality on Linux
            export LD_LIBRARY_PATH="${pkgs.libappindicator-gtk3}/lib:${pkgs.gtk3}/lib:$LD_LIBRARY_PATH"
          '';
        };


        # Optional: Define the package itself
        packages.scrobble-scrubber = pkgs.rustPlatform.buildRustPackage {
          pname = "scrobble-scrubber";
          version = "0.1.0";

          src = ./scrobble-scrubber;

          cargoLock = {
            lockFile = ./scrobble-scrubber/Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs;
            [
              openssl
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.SystemConfiguration
            ];

          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };

        # Dioxus app package
        packages.scrobble-scrubber-app = pkgs.rustPlatform.buildRustPackage {
          pname = "scrobble-scrubber-app";
          version = "0.1.0";

          src = let
            # Use gitignore.nix to respect .gitignore files
            gitignoreSource = pkgs.nix-gitignore.gitignoreSourcePure [ 
              "*.nix"
              "result"
              "result-*"
              ".envrc"
              ".direnv"
              "CLAUDE.md"
            ] ./.;
          in gitignoreSource;

          cargoHash = "sha256-wjJJOXZm2Y6od76lEj+who+ut5gaoYi5kTVZ8H1u38A=";

          nativeBuildInputs = with pkgs; [
            pkg-config
            dioxus-cli
            ccache
          ];

          buildInputs = with pkgs;
            [
              openssl
            ]
            ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.SystemConfiguration
              darwin.apple_sdk.frameworks.WebKit
              darwin.apple_sdk.frameworks.AppKit
            ]
            ++ lib.optionals stdenv.isLinux [
              gtk3
              webkitgtk_4_1
              librsvg
              libsoup_3
              libayatana-appindicator
            ];

          # Skip the default checks
          doCheck = false;

          # Override the build phase to use dx bundle
          buildPhase = ''
            runHook preBuild
            
            # Ensure we're in the app directory
            cd app
            
            # Copy assets for bundling
            mkdir -p assets
            cp -r ${./app/assets}/* assets/
            
            # Build and bundle the application
            # Only build the .app bundle, no DMG
            dx bundle --release --platform ${if pkgs.stdenv.isDarwin then "macos" else if pkgs.stdenv.isLinux then "linux" else "windows"} --package-types ${if pkgs.stdenv.isDarwin then "macos" else if pkgs.stdenv.isLinux then "appimage" else "msi"}
            
            echo "Bundle phase completed"
            
            # The bundle is created in the source root's target directory
            cd ..
            
            runHook postBuild
          '';

          # Install the bundled app
          installPhase = ''
            runHook preInstall
            
            mkdir -p $out
            
            # We're now back in the source root directory
            echo "Current directory: $(pwd)"
            echo "Looking for .app bundles..."
            find . -name "*.app" -type d 2>/dev/null | head -10
            
            # Platform-specific installation
            ${if pkgs.stdenv.isDarwin then ''
              # The app bundle is created at a specific path by dx bundle
              APP_PATH="target/dx/scrobble-scrubber-app/bundle/macos/bundle/macos/ScrobbleScrubberApp.app"
              if [ -d "$APP_PATH" ]; then
                echo "Found app at: $APP_PATH"
                cp -r "$APP_PATH" $out/
                
                # Create a wrapper script for easier execution
                mkdir -p $out/bin
                echo '#!/bin/sh' > $out/bin/scrobble-scrubber-app
                echo 'exec "'"$out"'"/ScrobbleScrubberApp.app/Contents/MacOS/scrobble-scrubber-app" "$@"' >> $out/bin/scrobble-scrubber-app
                chmod +x $out/bin/scrobble-scrubber-app
              else
                echo "ERROR: App bundle not found at expected location: $APP_PATH"
                echo "Contents of target directory:"
                find target -type d -name "*.app" 2>/dev/null || echo "No target directory found"
                exit 1
              fi
            '' else if pkgs.stdenv.isLinux then ''
              # Look for AppImage in the bundle output
              APP_PATH=$(find target/dx -name "*.AppImage" 2>/dev/null | head -1)
              if [ -n "$APP_PATH" ]; then
                mkdir -p $out/bin
                cp "$APP_PATH" $out/bin/scrobble-scrubber-app
                chmod +x $out/bin/scrobble-scrubber-app
              else
                echo "ERROR: AppImage not found!"
                exit 1
              fi
            '' else ''
              # Windows
              EXE_PATH=$(find target/dx -name "*.exe" -o -name "*.msi" 2>/dev/null | head -1)
              if [ -n "$EXE_PATH" ]; then
                mkdir -p $out/bin
                cp "$EXE_PATH" $out/bin/
              else
                echo "ERROR: Windows executable not found!"
                exit 1
              fi
            ''}
            
            runHook postInstall
          '';

          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";
        };

        packages.default = self.packages.${system}.scrobble-scrubber;
      }
    );
}
