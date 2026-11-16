{
  lib,
  rustPlatform,
  libcosmicAppHook,
  just,
  pkg-config,
  alsa-lib,
  fontconfig,
  freetype,
  libxkbcommon,
  wayland,
  stdenv,
}:

let
  cargoToml = lib.importTOML ../Cargo.toml;
in
rustPlatform.buildRustPackage {
  pname = "cosmic-advanced-player";
  inherit (cargoToml.package) version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../src
      ../i18n
      ../i18n.toml
      ../resources
      ../justfile
      ../LICENSE
    ];
  };

  cargoLock = {
    lockFile = ../Cargo.lock;
    allowBuiltinFetchGit = true;
  };

  nativeBuildInputs = [
    just
    pkg-config
    libcosmicAppHook
  ];

  buildInputs = [
    alsa-lib
    fontconfig
    freetype
    libxkbcommon
    wayland
  ];

  dontUseJustBuild = true;
  dontUseJustCheck = true;

  justFlags = [
    "--set"
    "prefix"
    (placeholder "out")
    "--set"
    "cargo-target-dir"
    "target/${stdenv.hostPlatform.rust.cargoShortTarget}"
  ];

  meta = {
    description = cargoToml.package.description;
    homepage = "https://github.com/Kisakay/CosmicAdvancedPlayer";
    license = lib.licenses.mit;
    mainProgram = "cosmic-advanced-player";
    platforms = lib.platforms.linux;
  };
}
