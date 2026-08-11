AidTerm runtime binaries directory (../bin)
===========================================

The `bin/` directory next to this one holds the runtime binaries that get
bundled into release packages:

  - `adb(.exe)`           Android Debug Bridge (fetched by `npm run fetch-adb`)
  - `AdbWinApi.dll`       adb dependency on Windows (same fetch)
  - `AdbWinUsbApi.dll`    adb dependency on Windows (same fetch)
  - `scrcpy-server.jar`   ADB screen casting (fetched by `npm run fetch-scrcpy`)

The application resolves adb in this priority order:

  1. The `AIDTERM_ADB` environment variable (explicit override)
  2. The bundled `adb(.exe)` in `bin/` (shipped inside the app)
  3. Any adb found on PATH (fallback)

If adb is already on your PATH, nothing needs to be done here. To bundle adb
into release packages, run:

    npm run fetch-adb

which downloads Google's platform-tools for the current platform into `bin/`
(plus the AdbWin*.dll dependencies on Windows). Run `npm run fetch-scrcpy` for
the screen-cast server jar.

These download scripts live in `scripts/` (build-time only, NOT bundled).
`bin/` contains only runtime files, so it can be copied into packages as-is;
only the `*.mjs`/`README.txt` files used to live here are excluded by keeping
them in `scripts/`. The binaries are not committed to git (see .gitignore);
`.gitkeep` keeps the directory present for release packaging even when adb is
absent.
