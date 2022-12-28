{ pkgs
, testPkgs
, lanzabooteModule
}:

let
  inherit (pkgs) lib;

  commonModule = { lib, ... }: {
    imports = [
      lanzabooteModule
    ];

    virtualisation = {
      useBootLoader = true;
      useEFIBoot = true;
      useSecureBoot = true;
    };

    boot.loader.efi = {
      canTouchEfiVariables = true;
    };
    boot.lanzaboote = {
      enable = true;
      enrollKeys = lib.mkDefault true;
      pkiBundle = ../../pki;
    };
  };

  mkSecureBootTest =
    { name
    , nodes ? { }
    , machine ? { }
    , testScript
    , broken ? false
    }: testPkgs.nixosTest {
      inherit name testScript;
      meta = { inherit broken; };
      nodes = {
        machine = _: {
          imports = [
            commonModule
            machine
          ];
        };
      } // nodes;
    };

  # Execute a boot test that is intended to fail.
  #
  mkUnsignedTest = { name, path, appendCrap ? false }: mkSecureBootTest {
    inherit name;
    testScript = ''
      import json
      import os.path
      bootspec = None

      def convert_to_esp(store_file_path):
          store_dir = os.path.basename(os.path.dirname(store_file_path))
          filename = os.path.basename(store_file_path)
          return f'/boot/EFI/nixos/{store_dir}-{filename}.efi'

      machine.start()
      bootspec = json.loads(machine.succeed("cat /run/current-system/boot.json")).get('v1')
      assert bootspec is not None, "Unsupported bootspec version!"
      src_path = ${path.src}
      dst_path = ${path.dst}
      machine.succeed(f"cp -rf {src_path} {dst_path}")
    '' + lib.optionalString appendCrap ''
      machine.succeed(f"echo Foo >> {dst_path}")
    '' +
    ''
      machine.succeed("sync")
      machine.crash()
      machine.start()
      machine.wait_for_console_text("Hash mismatch")
    '';
  };
in
{
  # TODO: user mode: OK
  # TODO: how to get in: {deployed, audited} mode ?
  lanzaboote-boot = mkSecureBootTest {
    name = "signed-files-boot-under-secureboot";
    testScript = ''
      machine.start()
      assert "Secure Boot: enabled (user)" in machine.succeed("bootctl status")
    '';
  };

  lanzaboote-boot-under-sd-stage1 = mkSecureBootTest {
    name = "signed-files-boot-under-secureboot-systemd-stage-1";
    machine = { ... }: {
      boot.initrd.systemd.enable = true;
    };
    testScript = ''
      machine.start()
      assert "Secure Boot: enabled (user)" in machine.succeed("bootctl status")
    '';
  };

  # So, this is the responsibility of the lanzatool install
  # to run the append-initrd-secret script
  # This test assert that lanzatool still do the right thing
  # preDeviceCommands should not have any root filesystem mounted
  # so it should not be able to find /etc/iamasecret, other than the
  # initrd's one.
  # which should exist IF lanzatool do the right thing.
  lanzaboote-with-initrd-secrets = mkSecureBootTest {
    name = "signed-files-boot-with-secrets-under-secureboot";
    machine = { ... }: {
      boot.initrd.secrets = {
        "/etc/iamasecret" = (pkgs.writeText "iamsecret" "this is a very secure secret");
      };

      boot.initrd.preDeviceCommands = ''
        grep "this is a very secure secret" /etc/iamasecret
      '';
    };
    testScript = ''
      machine.start()
      assert "Secure Boot: enabled (user)" in machine.succeed("bootctl status")
    '';
  };

  # The initrd is not directly signed. Its hash is embedded
  # into lanzaboote. To make integrity verification fail, we
  # actually have to modify the initrd. Appending crap to the
  # end is a harmless way that would make the kernel still
  # accept it.
  is-initrd-secured = mkUnsignedTest {
    name = "unsigned-initrd-do-not-boot-under-secureboot";
    path = {
      src = "bootspec.get('initrd')";
      dst = "convert_to_esp(bootspec.get('initrd'))";
    };
    appendCrap = true;
  };

  is-kernel-secured = mkUnsignedTest {
    name = "unsigned-kernel-do-not-boot-under-secureboot";
    path = {
      src = "bootspec.get('kernel')";
      dst = "convert_to_esp(bootspec.get('kernel'))";
    };
  };
  specialisation-works = mkSecureBootTest {
    name = "specialisation-still-boot-under-secureboot";
    machine = { pkgs, ... }: {
      specialisation.variant.configuration = {
        environment.systemPackages = [
          pkgs.efibootmgr
        ];
      };
    };
    testScript = ''
      machine.start()
      print(machine.succeed("ls -lah /boot/EFI/Linux"))
      print(machine.succeed("cat /run/current-system/boot.json"))
      # TODO: make it more reliable to find this filename, i.e. read it from somewhere?
      machine.succeed("bootctl set-default nixos-generation-1-specialisation-variant.efi")
      machine.succeed("sync")
      machine.fail("efibootmgr")
      machin.crash()
      machine.start()
      print(machine.succeed("bootctl"))
      # We have efibootmgr in this specialisation.
      machine.succeed("efibootmgr")
    '';
  };

  # This test is supposed to produce more generations than specified in the
  # configurationLimit and assert that when the limit is reached only the
  # correct numbers of generations remain on the ESP. This test mostly serves
  # as a stub for someone else to figure it out. Additionally, there are no
  # tests in nixpkgs that verify that the configurationLimit garbage collection
  # works.
  _config-limit = mkSecureBootTest {
    name = "lanzaboote-respects-config-limit";
    # Currently this test is broken because you cannot easily run nixos-rebuild
    # with an out-of-tree module in a VM test. Just calling
    # switch-to-configuration does not produce the links in
    # /nix/var/nix/profiles which are needed for lanzatool to install
    # generations. Only nixos-rebuild produces the right paths.
    broken = true;

    nodes = {
      gen0 = _: {
        imports = [ commonModule ];
        boot.lanzaboote.configurationLimit = 2;
        boot.lanzaboote.enrollKeys = false;
      };
      gen1 = _: {
        imports = [ commonModule ];
        networking.hostName = "wowMuchChange";
        boot.lanzaboote.configurationLimit = 2;
        boot.lanzaboote.enrollKeys = false;
      };
    };

    testScript = { nodes, ... }:
      let
        system0 = nodes.gen0.config.system.build.toplevel;
        system1 = nodes.gen1.config.system.build.toplevel;
      in
      ''
        machine.start()

        print(machine.succeed("nix-env -p /nix/var/nix/profiles/system --set ${system0}"))
        print(machine.succeed("${system0}/bin/switch-to-configuration boot"))

        print(machine.succeed("nix-env -p /nix/var/nix/profiles/system --set ${system1}"))
        print(machine.succeed("${system1}/bin/switch-to-configuration boot"))
      '';
  };
}
