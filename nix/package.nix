{
  lib,
  udev,
  pkg-config,
  rustPlatform,
  dbus,
  makeWrapper,
  rev ? "dirty",
}:
let
  cargoToml = lib.importTOML ../Cargo.toml;
  runtimeDeps = [
    udev
    dbus
  ];
in
rustPlatform.buildRustPackage {
  pname = "lightcrazy";
  version = "${cargoToml.package.version}-${rev}";

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../src
      ../Cargo.lock
      ../Cargo.toml
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;
  strictDeps = true;

  nativeBuildInputs = [
    pkg-config
    rustPlatform.bindgenHook
    makeWrapper
  ];

  buildInputs = runtimeDeps;

  postInstall = ''
    for bin in $out/bin/*; do
      wrapProgram $bin \
        --prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeDeps}"
    done
  '';

  meta = {
    description = "LightCrazy: Control software for Pulsar X2 CrazyLight gaming mouse";
    longDescription = ''
      LightCrazy provides a terminal UI and system tray for
      configuring Pulsar X2 CrazyLight gaming mice — DPI, polling rate, LOD,
      debounce, battery monitoring, and more.
    '';
    homepage = "https://github.com/cnsta/lightcrazy";
    license = lib.licenses.mit;
    mainProgram = "lightcrazy";
    platforms = lib.platforms.linux;
  };
}
