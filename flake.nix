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

              # Rust toolchain and Dioxus CLI for macOS
              (rust-bin.stable.latest.default.override {
                extensions = [ "rust-src" ];
              })
              dioxus-cli
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
        packages.app = pkgs.rustPlatform.buildRustPackage {
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
              libappindicator-gtk3
              xdotool
            ];

          # Skip the default checks
          doCheck = false;

          # Override the build phase to build the binary (skip bundling for Nix)
          buildPhase = ''
            runHook preBuild

            # Ensure we're in the app directory
            cd app

            # Copy assets for the build
            mkdir -p assets
            cp -r ${./app/assets}/* assets/

            # Build the application binary (no packaging needed for Nix)
            dx build --release

            echo "Build phase completed"

            # Return to source root
            cd ..

            runHook postBuild
          '';

          # Install the bundled app
          installPhase = ''
            runHook preInstall

            mkdir -p $out/bin

            # Install the binary from the dx build output
            echo "Current directory: $(pwd)"
            echo "Looking for built binary..."
            find . -name "scrobble-scrubber-app" -type f 2>/dev/null | head -10

            # The binary is created by dx build in the target directory
            BINARY_PATH="target/release/scrobble-scrubber-app"
            if [ -f "$BINARY_PATH" ]; then
              echo "Found binary at: $BINARY_PATH"
              cp "$BINARY_PATH" $out/bin/scrobble-scrubber-app
              chmod +x $out/bin/scrobble-scrubber-app
            else
              echo "ERROR: Binary not found at expected location: $BINARY_PATH"
              echo "Contents of target/release directory:"
              ls -la target/release/ 2>/dev/null || echo "No target/release directory found"
              echo "Searching for any scrobble-scrubber-app binary:"
              find . -name "scrobble-scrubber-app" -type f 2>/dev/null || echo "No binary found anywhere"
              exit 1
            fi

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
