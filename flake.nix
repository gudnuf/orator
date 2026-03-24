{
  description = "orator - Push-to-talk voice-to-text for macOS";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
  };

  outputs = { self, nixpkgs, crane, ... }:
    let
      supportedSystems = [ "aarch64-darwin" "x86_64-darwin" ];
      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      # Kroko streaming zipformer model from HuggingFace.
      # We fetch the individual model files instead of cloning the repo
      # to avoid git-lfs dependency and keep the download small.
      modelFiles = system:
        let pkgs = nixpkgs.legacyPackages.${system};
        in pkgs.stdenv.mkDerivation {
          pname = "sherpa-onnx-kroko-model";
          version = "2025-08-06";

          # Use fetchurl for each model file from HuggingFace
          srcs = [
            (pkgs.fetchurl {
              url = "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/main/encoder.onnx";
              hash = "sha256-1IgcV0SdWB4HcP1T+mbC/cbNFn2S7OfHFeYD3vyW2dQ=";
              name = "encoder.onnx";
            })
            (pkgs.fetchurl {
              url = "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/main/decoder.onnx";
              hash = "sha256-RVujhGb86NWlfn22ijI7aEB5yk2eHdk6dA2bJCmq47E=";
              name = "decoder.onnx";
            })
            (pkgs.fetchurl {
              url = "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/main/joiner.onnx";
              hash = "sha256-1Ab2FnNjUOKn3z45OYt46y/Bosppc6GdOFP6MifiW1I=";
              name = "joiner.onnx";
            })
            (pkgs.fetchurl {
              url = "https://huggingface.co/csukuangfj/sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06/resolve/main/tokens.txt";
              hash = "sha256-OW2+tfSFiHVpBxYIT1TpDTOWedC6PmtbWE89dYklTS0=";
              name = "tokens.txt";
            })
          ];

          sourceRoot = ".";
          unpackPhase = "true";  # fetchurl files don't need unpacking

          installPhase = ''
            mkdir -p $out
            for src in $srcs; do
              # fetchurl names files by hash; use the 'name' attribute via symlink name
              local name=$(stripHash "$src")
              cp "$src" "$out/$name"
            done
          '';
        };
    in
    {
      packages = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          craneLib = crane.mkLib pkgs;
          model = modelFiles system;
          orator = pkgs.callPackage ./nix/package.nix {
            inherit craneLib;
            src = craneLib.cleanCargoSource self;
            infoPlist = ./nix/Info.plist;
            modelSrc = model;
            hotwordsFile = ./hotwords.txt;
          };
        in {
          default = orator;
          inherit orator;
          model = model;
        }
      );

      # Overlay for use in other flakes (e.g., nix-config)
      overlays.default = final: prev:
        let
          craneLib = crane.mkLib final;
          model = (modelFiles final.system);
        in {
          orator = final.callPackage ./nix/package.nix {
            inherit craneLib;
            src = craneLib.cleanCargoSource self;
            infoPlist = ./nix/Info.plist;
            modelSrc = model;
            hotwordsFile = ./hotwords.txt;
          };
        };

      # Home Manager module
      homeManagerModules.default = import ./nix/home-module.nix;

      devShells = forAllSystems (system:
        let
          pkgs = nixpkgs.legacyPackages.${system};

          sherpa-onnx-prebuilt = pkgs.stdenv.mkDerivation rec {
            pname = "sherpa-onnx-prebuilt";
            version = "1.12.31";

            src = pkgs.fetchurl {
              url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v${version}/sherpa-onnx-v${version}-osx-universal2-shared.tar.bz2";
              hash = "sha256-3d3kIXy+jqbqIP+tt0jKCIGBly+dlkUHROQWJ6cdtec=";
            };

            sourceRoot = "sherpa-onnx-v${version}-osx-universal2-shared";

            installPhase = ''
              mkdir -p $out
              cp -r lib $out/
              cp -r include $out/
            '';

            nativeBuildInputs = pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.darwin.sigtool
            ];

            fixupPhase = ''
              for f in $out/lib/*.dylib; do
                install_name_tool -id "$out/lib/$(basename $f)" "$f" 2>/dev/null || true
              done
              onnx_lib=$(ls $out/lib/libonnxruntime.*.dylib | head -1)
              onnx_name=$(basename "$onnx_lib")
              for target in libsherpa-onnx-c-api.dylib libsherpa-onnx-cxx-api.dylib; do
                install_name_tool -change "@rpath/$onnx_name" "$out/lib/$onnx_name" \
                  "$out/lib/$target" 2>/dev/null || true
              done
              for f in $out/lib/*.dylib; do
                codesign -fs - "$f" 2>/dev/null || true
              done
            '';
          };
        in {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              rustc
              cargo
              rustfmt
              clippy
              rust-analyzer
              sherpa-onnx-prebuilt
              git-lfs
            ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
              pkgs.apple-sdk_15
            ];

            shellHook = ''
              export SHERPA_ONNX_LIB_DIR="${sherpa-onnx-prebuilt}/lib"
              export DYLD_LIBRARY_PATH="${sherpa-onnx-prebuilt}/lib:''${DYLD_LIBRARY_PATH:-}"
              echo "orator dev shell ready"
              echo "  cargo build    - build the project"
              echo "  cargo run      - run (needs models/ directory)"
            '';
          };
        }
      );
    };
}
