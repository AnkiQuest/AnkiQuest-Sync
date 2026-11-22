# SPDX-License-Identifier: AGPL-3.0-or-later
{
  description = "AnkiQuest-Sync: workflow tools";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";

    # The dev shells, formatter and pinned toolchains of every AnkiQuest
    # repository.
    ankiquest-dev = {
      url = "github:AnkiQuest/AnkiQuest-Dev";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = {
    self,
    nixpkgs,
    ankiquest-dev,
  }: let
    forAllSystems = nixpkgs.lib.genAttrs ankiquest-dev.lib.systems;
  in {
    inherit (ankiquest-dev) formatter;

    # AnkiQuest-Dev's default shell (actionlint, jq, gh and alejandra) and
    # its pinned Rust toolchain, for the tools in tools/.
    devShells = forAllSystems (system: {
      default = ankiquest-dev.legacyPackages.${system}.mkShell {
        inputsFrom = [ankiquest-dev.devShells.${system}.default];
        packages = [ankiquest-dev.packages.${system}.rust];
      };
    });

    checks = forAllSystems (system: {
      format = ankiquest-dev.lib.formatCheck nixpkgs.legacyPackages.${system} self;
    });
  };
}
