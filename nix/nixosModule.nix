self:
{
  config,
  pkgs,
  lib,
  ...
}:
let
  inherit (lib.options) mkEnableOption mkPackageOption;
  inherit (lib) mkIf mkMerge;
  cfg = config.hardware.lightcrazy;
in
{
  options.hardware.lightcrazy = {
    enable = mkEnableOption "LightCrazy — installs the package and udev rules for the Pulsar X2 CrazyLight";

    package = mkPackageOption pkgs "lightcrazy" { } // {
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.lightcrazy;
    };

    service.enable = mkEnableOption ''
      Systemd user service.

      Runs `lightcrazy` in tray mode as a systemd user service, started
      automatically with your graphical session. Battery check interval,
      notification threshold, and other preferences are configured from
      within the app itself (`lightcrazy --options`).

      Requires `hardware.lightcrazy.enable = true`.
    '';
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
        # Interface 0, presented as a mouse; suppress keyboard classification
        SUBSYSTEM=="input", ATTRS{idVendor}=="3710", ATTRS{idProduct}=="5406", ATTRS{bInterfaceNumber}=="00", ENV{ID_INPUT_KEYBOARD}="0", ENV{ID_INPUT_KEY}="0"
        # Interface 2, presented as a keyboard/consumer control; suppress entirely
        SUBSYSTEM=="input", ATTRS{idVendor}=="3710", ATTRS{idProduct}=="5406", ATTRS{bInterfaceNumber}=="02", ENV{ID_INPUT_KEYBOARD}="0", ENV{ID_INPUT_KEY}="0", ENV{ID_INPUT_MOUSE}="0"
      '';
    }

    (mkIf cfg.service.enable {
      systemd.user.services.lightcrazy = {
        description = "lightcrazy tray";
        wantedBy = [ "graphical-session.target" ];
        partOf = [ "graphical-session.target" ];
        after = [ "graphical-session.target" ];

        serviceConfig = {
          Type = "simple";
          ExecStart = "${cfg.package}/bin/lightcrazy";
          Restart = "on-failure";
          RestartSec = "5s";

          ExecSearchPath = [
            "/etc/profiles/per-user/%u/bin"
            "/run/current-system/sw/bin"
            "/nix/var/nix/profiles/default/bin"
            "/usr/local/bin"
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
