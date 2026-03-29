{
  description = "jj-hunk";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
    forAllSystems = nixpkgs.lib.genAttrs systems;
  in let
    cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
    version = "${cargoToml.package.version}+${self.shortRev or self.dirtyShortRev or "unknown"}";
  in {
    packages = forAllSystems (system: let
      pkgs = import nixpkgs {inherit system;};
    in {
      default = pkgs.rustPlatform.buildRustPackage {
        pname = "jj-hunk";
        version = version;
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
      };
    });

    devShells = forAllSystems (system: let
      pkgs = import nixpkgs {inherit system;};
    in {
      default = pkgs.mkShell {
        packages = with pkgs; [
          cargo
          clippy
          rustc
          rustfmt
        ];
      };
    });
  };
}
