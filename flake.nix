{
  description = "pdvim, a minimal, keyboard-driven PDF reader";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
      ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});

      # What wgpu and winit dlopen at run time rather than link to, so the
      # linker never records them and they must be added to the RPATH.
      runtimeLibs =
        pkgs:
        lib.optionals pkgs.stdenv.hostPlatform.isLinux (
          with pkgs;
          [
            libGL
            libxkbcommon
            vulkan-loader
            wayland
            libx11
            libxcursor
            libxi
            libxrandr
            libxcb
          ]
        );
    in
    {
      packages = forAllSystems (pkgs: {
        default = pkgs.callPackage (
          {
            lib,
            stdenv,
            rustPlatform,
            pkg-config,
            installShellFiles,
            fontconfig,
            freetype,
            sqlite,
          }:
          rustPlatform.buildRustPackage {
            pname = "pdvim";
            version = (lib.importTOML ./Cargo.toml).package.version;

            # Only what the build and the tests read, so that editing
            # anything else (or a `result` link) does not rebuild.
            src = lib.fileset.toSource {
              root = ./.;
              fileset = lib.fileset.unions [
                ./Cargo.toml
                ./Cargo.lock
                ./src
                ./defaults.toml
                ./README.md
                ./docs/man
                ./fuzz/seeds
                ./packaging/pdvim.desktop
                ./packaging/pdvim.svg
              ];
            };

            cargoLock.lockFile = ./Cargo.lock;

            nativeBuildInputs = [
              pkg-config
              installShellFiles
              # bindgen, in mupdf-sys, needs libclang.
              rustPlatform.bindgenHook
            ];

            # rusqlite's "bundled" SQLite is overridden here: the nixpkgs cargo
            # setup hook sets LIBSQLITE3_SYS_USE_PKG_CONFIG, so it links this.
            buildInputs = [
              fontconfig
              freetype
              sqlite
            ];

            postInstall = ''
              installManPage docs/man/pdvim.1 docs/man/pdvim.5
              install -Dm644 packaging/pdvim.desktop -t $out/share/applications
              install -Dm644 packaging/pdvim.svg -t $out/share/icons/hicolor/scalable/apps
              install -Dm644 README.md defaults.toml -t $out/share/doc/pdvim
            '';

            # After fixup, which shrinks the RPATH to what is linked: add
            # what wgpu and winit dlopen.
            postFixup = lib.optionalString stdenv.hostPlatform.isLinux ''
              patchelf --add-rpath ${lib.makeLibraryPath (runtimeLibs pkgs)} $out/bin/pdvim
            '';

            meta = {
              description = "Minimal, keyboard-driven PDF reader with vi keybindings";
              homepage = "https://github.com/djmbritt/pdvim";
              license = lib.licenses.agpl3Plus;
              mainProgram = "pdvim";
              platforms = lib.platforms.linux ++ lib.platforms.darwin;
            };
          }
        ) { };
      });

      apps = forAllSystems (pkgs: {
        default = {
          type = "app";
          program = lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.default;
          meta.description = "Run pdvim";
        };
      });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = [ self.packages.${pkgs.stdenv.hostPlatform.system}.default ];
          packages = with pkgs; [
            cargo
            rustc
            clippy
            rustfmt
            rust-analyzer
          ];
          # Lets `cargo run` find what wgpu and winit dlopen.
          LD_LIBRARY_PATH = lib.makeLibraryPath (runtimeLibs pkgs);
        };
      });

      formatter = forAllSystems (pkgs: pkgs.nixfmt);
    };
}
