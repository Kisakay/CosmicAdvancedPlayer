{
  description = "Cosmic Advanced Player — a beautiful native music player for Linux, built with Rust & libcosmic";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      # `default` is the prebuilt binary of the latest GitHub release (nothing to compile),
      # pinned in nix/release.json by the release workflow. Before the first release, or
      # with `cosmic-advanced-player-source`, the player is built from source.
      packages = forAllSystems (
        pkgs:
        let
          system = pkgs.stdenv.hostPlatform.system;
          release = nixpkgs.lib.importJSON ./nix/release.json;
          hasBinary = release.version != null && release.hashes ? ${system};
          source = pkgs.callPackage ./nix/package.nix { };
          binary = pkgs.callPackage ./nix/bin.nix { inherit release; };
        in
        {
          cosmic-advanced-player-source = source;
          cosmic-advanced-player = if hasBinary then binary else source;
          default = self.packages.${system}.cosmic-advanced-player;
        }
        // nixpkgs.lib.optionalAttrs hasBinary { cosmic-advanced-player-bin = binary; }
      );

      apps = forAllSystems (pkgs: rec {
        cosmic-advanced-player = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.default}/bin/cosmic-advanced-player";
        };
        default = cosmic-advanced-player;
      });

      overlays.default = final: _prev: {
        cosmic-advanced-player = self.packages.${final.stdenv.hostPlatform.system}.cosmic-advanced-player;
      };

      nixosModules.default = import ./nix/module.nix self;
      nixosModules.cosmic-advanced-player = self.nixosModules.default;
      homeManagerModules.default = import ./nix/hm-module.nix self;

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.cosmic-advanced-player-source ];
          packages = with pkgs; [
            cargo
            rustc
            rustfmt
            clippy
            rust-analyzer
            just
          ];
          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (
            with pkgs;
            [
              wayland
              libxkbcommon
              vulkan-loader
              libGL
              alsa-lib
              libx11
              libxcursor
              libxi
              libxrandr
            ]
          );
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
