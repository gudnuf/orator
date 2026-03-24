# Home Manager module for orator voice-to-text.
#
# Provides:
#   - The orator binary on PATH
#   - Optional launchd agent for auto-start
#   - Shell aliases (orator-restart, orator-log)
#   - Activation script that deploys .app bundle with stable codesign
{ config, lib, pkgs, ... }:

with lib;

let
  cfg = config.programs.orator;
  homeDir = config.home.homeDirectory;
  appDir = "${homeDir}/.local/Applications/Orator.app";
  appBin = "${appDir}/Contents/MacOS/orator";
  launchdLabel = "org.nix-community.home.orator";
in {
  options.programs.orator = {
    enable = mkEnableOption "orator push-to-talk voice-to-text";

    package = mkOption {
      type = types.package;
      default = pkgs.orator or (throw "orator package not found. Add orator overlay or input.");
      defaultText = literalExpression "pkgs.orator";
      description = "The orator package to use.";
    };

    autoStart = mkOption {
      type = types.bool;
      default = false;
      description = "Whether to start orator automatically via launchd.";
    };
  };

  config = mkIf cfg.enable {
    home.packages = [ cfg.package ];

    launchd.agents.orator = mkIf (cfg.autoStart && pkgs.stdenv.isDarwin) {
      enable = true;
      config = {
        ProgramArguments = [ appBin ];
        RunAtLoad = true;
        KeepAlive = true;
        StandardOutPath = "${homeDir}/Library/Logs/orator/stdout.log";
        StandardErrorPath = "${homeDir}/Library/Logs/orator/stderr.log";
        EnvironmentVariables = {
          HOME = homeDir;
          PATH = "/usr/bin:/bin:/usr/sbin:/sbin:${homeDir}/.local/bin";
        };
      };
    };

    programs.zsh.shellAliases = mkIf config.programs.zsh.enable {
      orator-restart = "launchctl kickstart -k gui/$(id -u)/${launchdLabel}";
      orator-log = "tail -f ~/Library/Logs/orator/stderr.log";
    };

    home.activation.oratorSetup = lib.hm.dag.entryAfter [ "writeBoundary" ] ''
      $DRY_RUN_CMD mkdir -p "$HOME/Library/Logs/orator"
      $DRY_RUN_CMD mkdir -p "$HOME/.local/bin"

      if [ -d "${cfg.package}/Applications/Orator.app" ]; then
        $DRY_RUN_CMD mkdir -p "$HOME/.local/Applications"
        $DRY_RUN_CMD chmod -R u+w "$HOME/.local/Applications/Orator.app" 2>/dev/null || true
        $DRY_RUN_CMD rm -rf "$HOME/.local/Applications/Orator.app"
        $DRY_RUN_CMD cp -R "${cfg.package}/Applications/Orator.app" "$HOME/.local/Applications/Orator.app"
        $DRY_RUN_CMD chmod -R u+w "$HOME/.local/Applications/Orator.app"
        $DRY_RUN_CMD /usr/bin/codesign --force --sign - --identifier "com.gudnuf.orator" "$HOME/.local/Applications/Orator.app"
        $DRY_RUN_CMD ln -sf "$HOME/.local/Applications/Orator.app/Contents/MacOS/orator" "$HOME/.local/bin/orator"
      else
        $DRY_RUN_CMD cp -f "${cfg.package}/bin/orator" "$HOME/.local/bin/orator"
      fi
    '';
  };
}
