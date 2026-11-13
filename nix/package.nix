{ lib, stdenv, rustPlatform, llvmPackages_21, fetchurl, runCommand, unzip
, pkg-config, cmake, python3, makeWrapper, openssl, dbus, fontconfig, freetype
, glib, libxkbcommon, wayland, libGL, vulkan-loader, libdrm, mesa
, alsa-lib, libpulseaudio, pipewire, libx11, libxcursor, libxi, libxrandr, libxcb
, libxkbfile, libxfixes, libxdamage, libxcomposite, libxext, src
}:
let
  # Keep documentation, captures, local tooling and flake bookkeeping out of
  # the application source hash. Cargo sources and vendored patches stay in it.
  source = lib.cleanSourceWith {
    inherit src;
    filter = path: _type:
      let
        relative = lib.removePrefix "${src}/" (toString path);
        top = builtins.head (lib.splitString "/" relative);
      in (toString path) == (toString src) || builtins.elem top [
        "Cargo.toml" "Cargo.lock" ".cargo" "crates" "scripts" "macos" "vendor" "LICENSE"
      ];
  };
  triples = {
    aarch64-darwin = "mac-arm64-release";
    aarch64-linux = "linux-arm64-release";
    x86_64-linux = "linux-x64-release";
  };
  triple = triples.${stdenv.hostPlatform.system};
  hashes = import ./webrtc-sources.nix;
  archive = fetchurl {
    url = "https://github.com/livekit/rust-sdks/releases/download/webrtc-a545de4/webrtc-${triple}.zip";
    hash = hashes.${triple};
  };
  # Rust bindings otherwise download this archive from an ordinary build script.
  # Supply the fixed-output input explicitly so the app build stays offline.
  webrtc = runCommand "livekit-webrtc-a545de4-${triple}" {
    nativeBuildInputs = [ unzip ];
  } ''
    mkdir -p "$out"
    unzip -q ${archive} -d "$out"
    test -f "$out/${triple}/webrtc.ninja"
    test -f "$out/${triple}/desktop_capture.ninja"
  '';
  linuxLibraries = [
    dbus fontconfig freetype glib openssl libxkbcommon wayland libGL
    vulkan-loader libdrm mesa alsa-lib libpulseaudio pipewire
    libx11 libxcursor libxi libxrandr libxcb
    libxkbfile libxfixes libxdamage libxcomposite libxext
  ];
  # The pinned WebRTC libc++ headers require clang >= 21 on Linux.
  buildRustPackage = rustPlatform.buildRustPackage.override {
    stdenv = if stdenv.hostPlatform.isLinux then llvmPackages_21.stdenv else stdenv;
  };
  version = (builtins.fromTOML (builtins.readFile "${source}/Cargo.toml")).workspace.package.version;
  target = stdenv.hostPlatform.rust.rustcTarget;
in buildRustPackage {
  pname = "twayblade";
  inherit version;
  src = source;
  strictDeps = true;
  cargoLock.lockFile = "${source}/Cargo.lock";
  nativeBuildInputs = [ pkg-config cmake python3 makeWrapper ];
  buildInputs = lib.optionals stdenv.hostPlatform.isLinux linuxLibraries;
  LK_CUSTOM_WEBRTC = "${webrtc}/${triple}";
  TWAYBLADE_PACKAGE_MANAGER = "nix";
  OPENSSL_NO_VENDOR = "1";
  MACOSX_DEPLOYMENT_TARGET = "14.0";

  # GUI/service tests belong to the development/CI checks, not an isolated
  # package builder without a desktop, keyring, media grants or test services.
  doCheck = false;
  buildPhase = ''
    runHook preBuild
    export CARGO_TARGET_DIR="$TMPDIR/twayblade-target"
    ${lib.optionalString stdenv.hostPlatform.isLinux "python3 "}scripts/cargo-pruned build --release --locked --offline -p twayblade \
      --target ${target} -j "$NIX_BUILD_CORES"
    runHook postBuild
  '';
  installPhase = ''
    runHook preInstall
    mkdir -p "$out/bin" "$out/share/licenses/twayblade"
    cp LICENSE "$out/share/licenses/twayblade/"
    cp crates/media/licenses/libwebrtc-a545de4.md "$out/share/licenses/twayblade/"
    cp "${webrtc}/${triple}/LICENSE.md" "$out/share/licenses/twayblade/libwebrtc-full.md"
  '' + lib.optionalString stdenv.hostPlatform.isDarwin ''
    app="$out/Applications/Twayblade.app"
    mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources/licenses"
    install -m755 "$CARGO_TARGET_DIR/${target}/release/twayblade" "$app/Contents/MacOS/twayblade"
    cp macos/Info.plist "$app/Contents/Info.plist"
    substituteInPlace "$app/Contents/Info.plist" \
      --replace-fail com.twayblade.desktop.development com.twayblade.desktop \
      --replace-fail '<string>0.1.0</string>' '<string>${version}</string>'
    cp "$out/share/licenses/twayblade/"* "$app/Contents/Resources/licenses/"
    ln -s "$app/Contents/MacOS/twayblade" "$out/bin/twayblade"
  '' + lib.optionalString stdenv.hostPlatform.isLinux ''
    install -m755 "$CARGO_TARGET_DIR/${target}/release/twayblade" "$out/bin/twayblade"
    wrapProgram "$out/bin/twayblade" --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath linuxLibraries}
    mkdir -p "$out/share/applications"
    cat > "$out/share/applications/twayblade.desktop" <<EOF
    [Desktop Entry]
    Name=Twayblade
    Comment=Native Matrix messaging and calls
    Exec=$out/bin/twayblade
    Type=Application
    Terminal=false
    Categories=Network;InstantMessaging;
    EOF
  '' + ''
    runHook postInstall
  '';
  passthru = { inherit webrtc; };
  meta = {
    description = "Native GPUI Matrix client with encrypted MatrixRTC calls";
    homepage = "https://github.com/zerw0/twayblade";
    license = lib.licenses.mit;
    mainProgram = "twayblade";
    platforms = builtins.attrNames triples;
  };
}
