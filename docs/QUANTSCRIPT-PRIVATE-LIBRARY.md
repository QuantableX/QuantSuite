# Private QuantScript library

Indicator implementations are user files, separate from the application runtime.
A newly built installer contains the engine, editor, script loader and neutral
new-script template. It contains no personal indicator implementations,
historical research scores, parameter variants, editor history or Python caches.

## Where to put scripts

- Installed Windows app: `<folder containing QuantSuite.exe>/QuantScript/indicators/`.
- Development: `<checkout>/QuantScript/indicators/`, even when the binary is under `target/debug`.
- Optional override for every consumer: set `QUANTSCRIPT_INDICATORS_DIR` before launching.
  This also supports installations where the software directory is read-only.

Drop `.py` files here and refresh the QuantScript library. A script declares
`REGISTER = {"my_key": MyIndicator}` and optionally `WARMUP = {"my_key": 400}`.
Keep dependencies in the same folder; existing relative `smithery.indicators`
imports continue to work. New indicator creates the usual EMA starter.
Settings shows the active path and provides Copy folder path. Restart running
bots and engines to load changed scripts.

Forge parameter variants stay in `indicators/versions/<base key>/`. Optional
`indicators/library.json` contains `certification`, `certification_tf` and
`legacy_variants` tables. They default to empty on a new installation. Script
version history remains in the existing user `modules/script/script.db`.

There is no first-run seeding, fallback to installed indicator implementations,
or automatic import from an old user library. Removing a script keeps it removed.
The installer owns only the README in this folder; private files are not update
resources. Back up the private folder separately before uninstalling or moving.

## Building and sharing

Run `npm run tauri:build -- --bundles nsis --config apps/src-tauri/tauri.local-build.conf.json`. Share only the newly built
`target/release/bundle/nsis/QuantSuite_1.0.0_x64-setup.exe`.
Previously built installers still contain their old resources. Copying a
populated installation directory also copies its private library.

`apps/src-tauri/tauri.conf.json` lists individual runtime files explicitly.
New Python files require an intentional packaging entry. Do not replace the
list with a folder copy or glob. `npm run check:distribution` runs before every
Tauri production build and rejects private library resources, indicator
implementations, research-only runners, caches and broad Python resource copies.

Validation:

```powershell
npm run test:distribution
npm run test:script
$env:PYTHONPATH = "$PWD/sidecars/python"
py -3 -m unittest discover -s sidecars/python/tests -p test_private_distribution.py -v
cargo test -p tauri-plugin-qs -p tauri-plugin-script --lib
```

The distribution test stages only the installer resource list into an isolated
software folder and tests empty startup, shared discovery, script checking,
edits, deletion and path overrides. Existing indicator research tests require
the private library and are not shipped to recipients.
