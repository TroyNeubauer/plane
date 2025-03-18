{
  description = "Rust Embedded Setup for Raspberry Pi Pico with fenix and probe-rs-tools";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    fenix.url = "github:nix-community/fenix";
    fenix.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { self, nixpkgs, fenix, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system: 
      let
        pkgs = import nixpkgs { inherit system; };

        toolchain = with fenix.packages.${system};
          combine [
            stable.rustc
            stable.cargo
            stable.rustfmt
            stable.clippy
            stable.rust-analyzer
            stable.rust-std
            stable.rust-src
            # llvm-tools-preview
            targets.thumbv6m-none-eabi.stable.rust-std
          ];

        shell = pkgs.mkShell {
          buildInputs = with pkgs; [
            toolchain
            probe-rs-tools
            elf2uf2-rs
            cargo-binutils
            gcc-arm-embedded
            openocd
            minicom
            udev
            pkg-config
          ];

          LD_LIBRARY_PATH="${pkgs.udev}/lib";
        };
      in
      {
        devShells.default = shell;
      });
}
