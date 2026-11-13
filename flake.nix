{
  description = "Twayblade native Matrix client and development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/0bb2eedc20e4739f29e8045034f2828ad7054f8f";
  outputs = { self, nixpkgs }:
    let
      # The pinned Nixpkgs 26.11 no longer evaluates x86_64-darwin.
      systems = [ "aarch64-darwin" "x86_64-linux" "aarch64-linux" ];
    in {
      overlays.default = final: _previous: {
        twayblade = final.callPackage ./nix/package.nix { src = self; };
      };
      packages = nixpkgs.lib.genAttrs systems (system:
        let pkgs = import nixpkgs { inherit system; overlays = [ self.overlays.default ]; };
        in { default = pkgs.twayblade; twayblade = pkgs.twayblade; });
      apps = nixpkgs.lib.genAttrs systems (system: {
        default = {
          type = "app";
          program = "${self.packages.${system}.default}/bin/twayblade";
          meta.description = "Twayblade native Matrix client";
        };
      });
      devShells = nixpkgs.lib.genAttrs systems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = (pkgs.mkShell.override {
            stdenv = if pkgs.stdenv.hostPlatform.isLinux then pkgs.llvmPackages_21.stdenv else pkgs.stdenv;
          }) {
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.dbus pkgs.fontconfig pkgs.freetype pkgs.glib pkgs.openssl ];
            packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config cmake python3 nodejs openssl ]
              ++ lib.optionals stdenv.hostPlatform.isLinux [
                wayland libxkbcommon libGL vulkan-loader fontconfig freetype openssl
                libx11 libxcursor libxi libxrandr libxcb libxkbfile
              ];
            LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux
              (pkgs.lib.makeLibraryPath (with pkgs; [
                wayland libxkbcommon libGL vulkan-loader fontconfig freetype openssl
                libx11 libxcursor libxi libxrandr libxcb libxkbfile
              ]));
          };
        });
    };
}
