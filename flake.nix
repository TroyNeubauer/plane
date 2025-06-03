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
        fixSdcc = final: prev: {
          sdcc = prev.sdcc.overrideAttrs {
            outputs = ["out" "doc"] ++ pkgs.lib.optionals (!pkgs.stdenv.isDarwin) [ "man" ];
          };
        };
        
        pkgs = import nixpkgs { inherit system; overlays = [fixSdcc];};

        elf2uf2-rs = pkgs.elf2uf2-rs.overrideAttrs {
          src = fetchGit { url = "https://github.com/ninjasource/elf2uf2-rs"; ref = "pico2-support"; rev = "5813dd0b54dde3aed93822e196f67715a2de8c5d"; };
        };

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
            targets."thumbv8m.main-none-eabihf".stable.rust-std
          ];

        shell = pkgs.mkShell {
          buildInputs = with pkgs; [
            pulseview
            toolchain
            probe-rs-tools
            elf2uf2-rs
            cargo-binutils
            gcc-arm-embedded
            openocd
            minicom 
            minicom
            pkg-config
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [ udev ];

          shellHook = ''
            ${if pkgs.stdenv.isLinux then "export LD_LIBRARY_PATH=${pkgs.udev}/lib" else ""}
          '';
        };
      in
      {
        devShells.default = shell;
      });
}
