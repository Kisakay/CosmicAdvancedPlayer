self:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.programs.cosmic-advanced-player;
in
{
  options.programs.cosmic-advanced-player = {
    enable = lib.mkEnableOption "Cosmic Advanced Player, a native music player for Linux";

    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.default;
      defaultText = lib.literalExpression "cosmic-advanced-player.packages.\${system}.default";
      description = "The Cosmic Advanced Player package (prebuilt release binary by default).";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
