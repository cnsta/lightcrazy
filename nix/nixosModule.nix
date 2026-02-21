self:
{
  config,
  pkgs,
  lib,
  ...
}:
let
  inherit (lib.options) mkEnableOption mkPackageOption mkOption;
  inherit (lib.types) int str;
  inherit (lib) mkIf mkMerge optionalAttrs;
  cfg = config.hardware.lightcrazy;
in
{
  options.hardware.lightcrazy = {
    enable = mkEnableOption "LightCrazy: Pulsar X2 CrazyLight Control — installs the package and udev rules";

    package = mkPackageOption pkgs "lightcrazy" { } // {
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.lightcrazy;
    };

    service = {
      enable = mkEnableOption ''
        LightCrazy systemd user service.

        Runs `lightcrazy` (tray mode) as a systemd user service that starts
        automatically with your graphical session. The tray icon gives access
        to battery status and can launch the settings panel via your terminal.

        Requires `hardware.lightcrazy.enable = true`.
      '';

      threshold = mkOption {
        type = int;
        default = 10;
        description = "Battery percentage threshold for low-battery desktop notifications.";
      };

      interval = mkOption {
        type = int;
        default = 60;
        description = "Battery check interval in seconds.";
      };

      terminal = mkOption {
        type = str;
        default = "";
        example = "alacritty";
        description = ''
          Terminal emulator binary to use when opening the settings panel from
          the tray icon. When set, this overrides $TERMINAL and the built-in
          detection order (kitty, alacritty, wezterm, ghostty, foot, konsole,
          gnome-terminal, xterm).

          If left empty, the service will attempt to detect the terminal via
          $TERMINAL and $TERM from the user manager environment, then fall back
          to the built-in list. Because systemd user services have a restricted
          PATH, setting this explicitly is recommended if auto-detection fails.
        '';
      };
    };
  };

  config = mkIf cfg.enable (mkMerge [
    {
      environment.systemPackages = [ cfg.package ];

      # Grant the current user access to the HID interface without root.
      # TAG+="uaccess" makes logind grant seat-local access automatically.
      services.udev.extraRules = ''
        # Pulsar X2 CrazyLight (wired)
        SUBSYSTEM=="usb",  ATTRS{idVendor}=="3710", ATTRS{idProduct}=="3414", MODE="0666", TAG+="uaccess"
        # Pulsar 8K Dongle (wireless)
        SUBSYSTEM=="usb",  ATTRS{idVendor}=="3710", ATTRS{idProduct}=="5406", MODE="0666", TAG+="uaccess"
        # hidraw nodes — required for non-root HID access via hidapi
        KERNEL=="hidraw*", ATTRS{idVendor}=="3710", ATTRS{idProduct}=="3414", MODE="0666", TAG+="uaccess"
        KERNEL=="hidraw*", ATTRS{idVendor}=="3710", ATTRS{idProduct}=="5406", MODE="0666", TAG+="uaccess"
      '';
    }

    (mkIf cfg.service.enable {
      systemd.user.services.lightcrazy = {
        description = "LightCrazy Battery Tray";
        wantedBy = [ "graphical-session.target" ];
        partOf = [ "graphical-session.target" ];
        after = [ "graphical-session.target" ];

        environment = {
          PULSAR_BATTERY_THRESHOLD = toString cfg.service.threshold;
          PULSAR_CHECK_INTERVAL = toString cfg.service.interval;
        }
        // optionalAttrs (cfg.service.terminal != "") {
          TERMINAL = cfg.service.terminal;
        };

        serviceConfig = {
          Type = "simple";
          ExecStart = "${cfg.package}/bin/lightcrazy";
          Restart = "on-failure";
          RestartSec = "5s";

          ExecSearchPath = [
            "/etc/profiles/per-user/%u/bin" # per-user profile (NixOS home-manager / users.users)
            "/run/current-system/sw/bin" # NixOS system packages
            "/nix/var/nix/profiles/default/bin" # default Nix profile
            "/usr/local/bin" # conventional fallback
            "/usr/bin"
            "/bin"
          ];

          PassEnvironment = [
            "WAYLAND_DISPLAY"
            "DISPLAY"
            "XAUTHORITY"
            "XDG_RUNTIME_DIR"
            "DBUS_SESSION_BUS_ADDRESS"
            "TERMINAL"
            "TERM"
          ];
        };
      };
    })
  ]);
}
