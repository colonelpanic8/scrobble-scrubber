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
              # Core Rust toolchain
              rust-bin.stable.latest.default

              # System dependencies for reqwest/openssl
              pkg-config
              openssl

              # Dioxus CLI for app development
              dioxus-cli
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              # macOS specific dependencies for Dioxus desktop apps
              darwin.apple_sdk.frameworks.Security
              darwin.apple_sdk.frameworks.CoreFoundation
              darwin.apple_sdk.frameworks.SystemConfiguration
              darwin.apple_sdk.frameworks.WebKit
              darwin.apple_sdk.frameworks.AppKit
            ]
            ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
              # Linux specific dependencies for Dioxus desktop apps
              gtk3
              webkitgtk_4_1
              librsvg
              libsoup_3
              libappindicator-gtk3
              xdotool  # For libxdo needed by tray-icon and muda
            ];

          # Environment variables
          PKG_CONFIG_PATH = "${pkgs.openssl.dev}/lib/pkgconfig";

          # For OpenSSL on some systems
          OPENSSL_DIR = "${pkgs.openssl.dev}";
          OPENSSL_LIB_DIR = "${pkgs.openssl.out}/lib";
          OPENSSL_INCLUDE_DIR = "${pkgs.openssl.dev}/include";

          shellHook = pkgs.lib.optionalString pkgs.stdenv.isLinux ''
            # Add library paths for system tray functionality on Linux
            export LD_LIBRARY_PATH="${pkgs.libappindicator-gtk3}/lib:${pkgs.gtk3}/lib:$LD_LIBRARY_PATH"
          '';
        };
      }
    );
}