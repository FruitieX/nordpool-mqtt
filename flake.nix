{
  description = "nordpool-mqtt";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }: flake-utils.lib.eachDefaultSystem (system:
    let
      overlays = [ rust-overlay.overlays.default ];
      pkgs = import nixpkgs { inherit system overlays; };
      rustToolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
      muslCc = pkgs.pkgsCross.musl64.stdenv.cc;
    in {
      devShells.default = pkgs.mkShell {
        name = "nordpool-mqtt";
        nativeBuildInputs = [
          rustToolchain
          pkgs.stdenv.cc
          muslCc
        ];

        # aws-lc-sys needs a musl C compiler for the static release build
        CC_x86_64_unknown_linux_musl = "${muslCc}/bin/${muslCc.targetPrefix}cc";
        CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER = "${muslCc}/bin/${muslCc.targetPrefix}cc";
      };
    }
  );
}
