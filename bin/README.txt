AidTerm adb binary directory
=============================

This directory is where the adb binary for Android device support lives. The
application resolves adb in this priority order:

  1. The `AIDTERM_ADB` environment variable (explicit override)
  2. The bundled `adb(.exe)` in this directory (shipped inside the app)
  3. Any adb found on PATH (fallback)

If adb is already on your PATH, nothing needs to be done here. To bundle adb
into release packages, run:

    npm run fetch-adb

which downloads Google's platform-tools (adb) for the current platform into
this directory. The binary is not committed to git (see .gitignore); the
placeholder files in here exist so release packaging (Tauri `bundle.resources`
and Electron `extraResources`) has something to copy even when adb is absent.
