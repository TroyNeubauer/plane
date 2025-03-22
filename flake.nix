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
            minicom
            #udev
            pkg-config
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ udev ];

          shellHook = ''
            ${if pkgs.stdenv.isLinux then "LD_LIBRARY_PATH=${pkgs.udev}/lib" else ""}
          '';
        };
      in
      {
        devShells.default = shell;
      });
}
