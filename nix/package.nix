# Crane-based build for orator.
#
# Uses statically-linked sherpa-onnx libraries. The sherpa-onnx-sys build
# script expects a pre-downloaded archive in SHERPA_ONNX_ARCHIVE_DIR (since
# the nix sandbox blocks network access during builds).
#
# Produces a macOS .app bundle at $out/Applications/Orator.app for stable
# codesign identity (TCC permissions survive rebuilds).
{
  craneLib,
  src,
  lib,
  stdenv,
  fetchurl,
  apple-sdk_15 ? null,
  darwinMinVersionHook ? null,
  darwin ? null,
  infoPlist ? null,
  modelSrc ? null,
  hotwordsFile ? null,
}:

let
  # sherpa-onnx-sys 1.12.32 (the native binding crate pulled by sherpa-onnx 1.12.31)
  # constructs the archive filename from its own CARGO_PKG_VERSION (1.12.32).
  sysVersion = "1.12.32";

  # Pre-download the static lib archive that sherpa-onnx-sys expects.
  # The build.rs will find it via SHERPA_ONNX_ARCHIVE_DIR and unpack it
  # instead of trying to download it (which fails in the nix sandbox).
  sherpa-onnx-archive = {
    "aarch64-darwin" = fetchurl {
      url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sysVersion}/sherpa-onnx-v${sysVersion}-osx-arm64-static-lib.tar.bz2";
      hash = "sha256-yqH9J7CgGSLIhBIMQbcJMRMEDNPPol8n9osgzvFVCK8=";
    };
    "x86_64-darwin" = fetchurl {
      url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${sysVersion}/sherpa-onnx-v${sysVersion}-osx-x64-static-lib.tar.bz2";
      # This hash will need updating when someone builds on x86_64-darwin
      hash = lib.fakeHash;
    };
  }.${stdenv.system} or (throw "Unsupported system: ${stdenv.system}");

  # Stage the archive with its expected filename so the build script finds it
  sherpa-onnx-archive-dir = stdenv.mkDerivation {
    pname = "sherpa-onnx-archive-dir";
    version = sysVersion;
    dontUnpack = true;
    installPhase = let
      archiveName = {
        "aarch64-darwin" = "sherpa-onnx-v${sysVersion}-osx-arm64-static-lib.tar.bz2";
        "x86_64-darwin" = "sherpa-onnx-v${sysVersion}-osx-x64-static-lib.tar.bz2";
      }.${stdenv.system};
    in ''
      mkdir -p $out
      cp ${sherpa-onnx-archive} "$out/${archiveName}"
    '';
  };

  commonArgs = {
    inherit src;
    pname = "orator";
    version = "0.2.0";
    strictDeps = true;

    nativeBuildInputs = lib.optionals stdenv.isDarwin [
      darwin.sigtool
    ];

    buildInputs = lib.optionals stdenv.isDarwin [
      apple-sdk_15
      (darwinMinVersionHook "13.0")
    ];

    env = lib.optionalAttrs stdenv.isDarwin {
      # Tell sherpa-onnx-sys build script where to find the pre-downloaded archive
      SHERPA_ONNX_ARCHIVE_DIR = "${sherpa-onnx-archive-dir}";
      NIX_LDFLAGS = toString [
        "-framework CoreAudio"
        "-framework AudioToolbox"
        "-framework AudioUnit"
        "-framework CoreGraphics"
        "-framework ApplicationServices"
        "-framework AppKit"
        "-framework CoreFoundation"
        "-framework QuartzCore"
        "-framework IOKit"
      ];
    };
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

in
craneLib.buildPackage (commonArgs // {
  inherit cargoArtifacts;

  postInstall = lib.optionalString stdenv.isDarwin ''
    # .app bundle
    ${lib.optionalString (infoPlist != null) ''
      APP="$out/Applications/Orator.app/Contents"
      mkdir -p "$APP/MacOS"
      cp "$out/bin/orator" "$APP/MacOS/orator"
      cp "${infoPlist}" "$APP/Info.plist"
    ''}

    # Bundle model files
    ${lib.optionalString (modelSrc != null) ''
      MODEL_DIR="$out/share/orator/models/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06"
      mkdir -p "$MODEL_DIR"
      cp "${modelSrc}"/encoder.onnx "$MODEL_DIR/"
      cp "${modelSrc}"/decoder.onnx "$MODEL_DIR/"
      cp "${modelSrc}"/joiner.onnx "$MODEL_DIR/"
      cp "${modelSrc}"/tokens.txt "$MODEL_DIR/"
    ''}

    # Bundle hotwords
    mkdir -p "$out/share/orator"
    ${lib.optionalString (hotwordsFile != null) ''
      cp "${hotwordsFile}" "$out/share/orator/hotwords.txt"
    ''}

    # Wrapper script that sets ORATOR_DATA_DIR
    mv "$out/bin/orator" "$out/bin/.orator-unwrapped"
    cat > "$out/bin/orator" <<WRAPPER
#!/bin/bash
export ORATOR_DATA_DIR="\''${ORATOR_DATA_DIR:-$out/share/orator}"
exec "$out/bin/.orator-unwrapped" "\$@"
WRAPPER
    chmod +x "$out/bin/orator"
  '';

  meta = with lib; {
    description = "Push-to-talk voice-to-text for macOS";
    license = licenses.mit;
    platforms = platforms.darwin;
    mainProgram = "orator";
  };
})
