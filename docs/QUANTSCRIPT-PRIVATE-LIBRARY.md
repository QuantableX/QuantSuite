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

Forge parameter versions stay in `indicators/versions/<base key>/` (see
Versions below). Optional `indicators/library.json` contains `certification`,
`certification_tf` and `legacy_variants` tables. They default to empty on a new
installation. Script version history remains in the existing user
`modules/script/script.db`.

There is no first-run seeding, fallback to installed indicator implementations,
or automatic import from an old user library. Removing a script keeps it removed.
The installer owns only the README in this folder; private files are not update
resources. Back up the private folder separately before uninstalling or moving.

## Versions

Every indicator exists in up to five versions, each its own registry key:

| Role | Key | What it is |
|---|---|---|
| Standard | `<key>` | the script's own defaults; every variable adjustable |
| Optimized (general) | `<key>_opt` | the parameters the indicator was used with before versions existed, frozen |
| Optimized 1H / 4H / 1D | `<key>_opt_1h`, `<key>_opt_4h`, `<key>_opt_1d` | the forge's pick for 1h, 4h and daily bots |

A version other than the Standard is a JSON file (format 2) in
`indicators/versions/<key>/<version key>.json` holding its parameters, its
evidence per track and the signature of the script and its `REQUIRES`
closure. When the script changes, its versions become unavailable until the
forge writes them again (QuantScript → Forge → Optimize timeframes). A
Standard's evidence file (`<key>.json`) records the Standard's verdicts only.

Where an indicator is used, QuantSystems and QuantAlgo pick a version by key,
and every variable of any version can be changed there (the parameter form):
QuantSystems stores only the values that differ from the version
(`indicator.params`), QuantAlgo freezes them into the new strategy.

## The Collection

QuantScript → Collection installs indicators from a catalog. Every
installation knows the public catalog
[QuantScript-Collection-Public](https://github.com/QuantableX/QuantScript-Collection-Public)
— no account or token needed; its files are read from GitHub's raw host at a
pinned commit. Further sources are a private GitHub repository in the same
format (access controlled by its owner) or a local folder (its `FORMAT.md`
describes the format). Your own scripts stay next to the installed ones.
Installing one indicator brings its `REQUIRES` closure and
all its versions. Every file is checked against the catalog's sha256; the new
scripts are tried with your library in a sandbox (`python -m
smithery.quantscript stage-check`) before anything is written; a file the
Collection did not install is replaced only after you confirm, and its old
content stays in the script history. `indicators/collection.json` records
what the Collection installed (source, version, commit, file hashes); removing
an item is refused while another installed item requires it.

Sources live in `modules/script/script.db`; Settings → QuantScript →
Collection adds, edits and tests them. A private GitHub repository needs a
fine-grained token with Repository access to that repository and
Contents: Read-only. The token is kept in the operating system's credential
store (Windows Credential Manager) under "QuantSuite QuantScript Collection"
and is never written to settings, logs, events or command results — the
interface only shows whether one is stored. Catalog files are cached per
source and commit under `modules/script/collection/`.

None of this ships: the installer contains no catalog, no cache, no
`collection.json`, no version files and no script declaring `REGISTER`
(`npm run check:distribution` reads every bundled Python file to make sure).

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
