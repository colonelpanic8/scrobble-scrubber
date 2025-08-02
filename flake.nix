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
              wasm-pack
              nodejs
              nodePackages.npm

              # For TUI development
              libiconv

              # Linux-specific dependencies for GUI applications
            ]
            ++ lib.optionals stdenv.isLinux [
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
            ++ lib.optionals (!stdenv.isDarwin) [
              # Image processing for icon generation (may not support all platforms)
              imagemagick
            ]
            ++ [
              # GitHub CLI for monitoring releases (cross-platform)
              gh
            ]
            ++ lib.optionals stdenv.isDarwin [
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

          shellHook = lib.optionalString stdenv.isLinux ''
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
            ++ lib.optionals stdenv.isDarwin [
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.SystemConfiguration
            ];

          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";
        };

        packages.default = self.packages.${system}.scrobble-scrubber;
      }
    );
}
