{
  description = "Native Matrix client development shell";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/0bb2eedc20e4739f29e8045034f2828ad7054f8f";
  outputs = { self, nixpkgs }:
    let
      systems = [ "aarch64-darwin" "x86_64-darwin" "x86_64-linux" "aarch64-linux" ];
    in {
      devShells = nixpkgs.lib.genAttrs systems (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.mkShell {
            buildInputs = pkgs.lib.optionals pkgs.stdenv.hostPlatform.isLinux [ pkgs.dbus pkgs.fontconfig pkgs.freetype pkgs.openssl ];
            packages = with pkgs; [ cargo rustc rustfmt clippy pkg-config cmake python3 ]
              ++ lib.optionals stdenv.hostPlatform.isLinux [
                wayland libxkbcommon libGL vulkan-loader fontconfig freetype openssl
                xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb xorg.libxkbfile
              ];
            LD_LIBRARY_PATH = pkgs.lib.optionalString pkgs.stdenv.hostPlatform.isLinux
              (pkgs.lib.makeLibraryPath (with pkgs; [
                wayland libxkbcommon libGL vulkan-loader fontconfig freetype openssl
                xorg.libX11 xorg.libXcursor xorg.libXi xorg.libXrandr xorg.libxcb xorg.libxkbfile
              ]));
          };
        });
    };
}
