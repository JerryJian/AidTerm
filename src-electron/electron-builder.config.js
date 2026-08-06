/**
 * @type {import('electron-builder').Configuration}
 */
module.exports = () => {
  return {
    appId: 'com.aidterm.app',
    productName: 'AidTerm',
    executableName: 'aidterm',
    directories: {
      output: './out'
    },
    files: [
      'dist/**/*',
      'node_modules/**/*'
    ],
    asarUnpack: [
      'node_modules/node-pty/**',
      'node_modules/cpu-features/**',
      'node_modules/@serialport/bindings-cpp/**',
      'node_modules/ssh2/util/pagent.exe'
    ],
    extraResources: [
      {
        from: '../dist',
        to: 'dist'
      }
    ],
    linux: {
      target: ['AppImage', 'deb'],
      artifactName: 'AidTerm_electron_${version}_${arch}.${ext}',
      syncDesktopName: true,
      category: 'TerminalEmulator',
      maintainer: 'AidTerm <aidterm@users.noreply.github.com>'
    },
    mac: {
      target: ['dmg']
    },
    win: {
      target: ['nsis'],
      icon: 'icons/icon.ico'
    }
  }
}
