# Releasing QuantSuite

The public repository is `QuantableX/QuantSuite`. Its history starts at 1.0.0.
Keep private libraries, user databases, research output and signing keys outside
the repository. Publish only the `main` branch and explicit release tags.

## Build and publish

The GitHub Actions workflow builds Windows x64 (NSIS and MSI), macOS universal
(Intel and Apple Silicon), and Linux x64 (AppImage, Debian and RPM). Pull requests
and main-branch pushes build without release secrets. Only a `v*` tag publishes.
Linux builds use Ubuntu 24.04: the screen-capture dependency requires newer
PipeWire/SPA headers than Ubuntu 22.04 provides. Packages target Ubuntu 24.04+
or distributions with equivalent or newer system libraries.

1. Set the same version in the root and workspace `package.json` files, the npm
   lockfile, `Cargo.toml`, and `apps/src-tauri/tauri.conf.json`. Refresh the Cargo
   lockfile with `cargo check --workspace`.
2. Run `npm run check`, `npm run check:release`, `npm run test:distribution`,
   `npm run test:release` and `npm run build`.
3. Commit on `main`, push, then create and push the matching tag:

   ```sh
   git tag v1.0.1
   git push origin main
   git push origin v1.0.1
   ```

The tag must exactly match the application version. After every platform succeeds,
the release job collects installers and signatures, generates `latest.json` and
`SHA256SUMS.txt`, and publishes the complete release. It refuses missing updater
packages or signatures. Failed builds publish nothing; fix the failure before
retrying a tag. Never overwrite an already published release with different code.

## Signing and update channel

The repository Actions secret `TAURI_SIGNING_PRIVATE_KEY` holds the Tauri updater
private key. `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` is optional for encrypted keys.
The matching public key is committed in `tauri.conf.json`. Keep an offline backup
of the private key: changing or losing it breaks updates for installed versions.
Do not print secrets in build logs or commit them.

QuantSuite checks only:

```text
https://github.com/QuantableX/QuantSuite/releases/latest/download/latest.json
```

The signed updater uses NSIS on Windows, the universal application archive on
macOS, and AppImage on Linux. Both macOS architecture entries point to the same
universal archive. Debian/RPM users install the next package manually. Updates
are user initiated from the QuantSuite main menu; there are no module-local
update commands or background checks of legacy repositories.

The Rust backend retains the checked update, verifies signatures before
installation, serializes concurrent requests across windows, and rejects install
requests from development builds. Save work before choosing Install & restart.

Updater signing is separate from Windows Authenticode and Apple Developer ID
signing/notarization. The initial Windows packages have no Authenticode
certificate; macOS uses ad-hoc signing. Add platform certificates separately if
you want trusted first-run installation without OS security prompts.

## Local packaging

For an unsigned local installer:

```sh
npm run tauri:build -- --config apps/src-tauri/tauri.local-build.conf.json
```

For signed packaging, set `TAURI_SIGNING_PRIVATE_KEY` to the private key file path
or contents, then run `npm run tauri:build`. Always distribute installer packages,
not a copy of an installed folder containing a user's private library.
