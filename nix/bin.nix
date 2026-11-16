# Prebuilt binary from the GitHub release, patched for NixOS.
# Releases are immutable, so the hashes pinned in release.json never change.
{
  lib,
  stdenv,
  fetchurl,
  autoPatchelfHook,
  makeWrapper,
  alsa-lib,
  libxkbcommon,
  libGL,
  vulkan-loader,
  wayland,
  libx11,
  libxcursor,
  libxi,
  libxrandr,
  libxcb,
  cosmic-icons,
  release ? lib.importJSON ./release.json,
}:

let
  inherit (release) version;
  arch = stdenv.hostPlatform.parsed.cpu.name;
  # Libraries opened with dlopen() at runtime (graphics & windowing).
  runtimeLibs = [
    libxkbcommon
    libGL
    vulkan-loader
    wayland
    libx11
    libxcursor
    libxi
    libxrandr
    libxcb
  ];
in
stdenv.mkDerivation {
  pname = "cosmic-advanced-player";
  inherit version;

  src = fetchurl {
    url = "https://github.com/Kisakay/CosmicAdvancedPlayer/releases/download/v${version}/cosmic-advanced-player-${version}-linux-${arch}.tar.gz";
    hash = release.hashes.${stdenv.hostPlatform.system};
  };

  nativeBuildInputs = [
    autoPatchelfHook
    makeWrapper
  ];

  # The graphics stack may be linked directly (DT_NEEDED) as well as dlopen()ed.
  buildInputs = [
    alsa-lib
    stdenv.cc.cc.lib
  ]
  ++ runtimeLibs;

  dontConfigure = true;
  dontBuild = true;

  installPhase = ''
    runHook preInstall
    mkdir -p $out
    cp -r bin share $out/
    runHook postInstall
  '';

  postFixup = ''
    wrapProgram $out/bin/cosmic-advanced-player \
      --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath runtimeLibs} \
      --suffix XDG_DATA_DIRS : $out/share:${cosmic-icons}/share
  '';

  doInstallCheck = true;
  installCheckPhase = ''
    $out/bin/cosmic-advanced-player --version | grep -q ${version}
  '';

  meta = {
    description = "A beautiful native music player for Linux (prebuilt binary)";
    homepage = "https://github.com/Kisakay/CosmicAdvancedPlayer";
    license = lib.licenses.gpl3Only;
    mainProgram = "cosmic-advanced-player";
    platforms = [
      "x86_64-linux"
      "aarch64-linux"
    ];
    sourceProvenance = [ lib.sourceTypes.binaryNativeCode ];
  };
}
