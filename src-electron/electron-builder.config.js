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
      },
      {
        from: '../bin',
        to: 'bin',
        filter: ['**/*', '!**/*.mjs', '!**/README.txt']
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
      target: ['dmg'],
      artifactName: 'AidTerm_electron_${version}_${arch}.${ext}'
    },
    win: {
      target: ['nsis'],
      icon: 'icons/icon.ico',
      artifactName: 'AidTerm_electron_${version}_${arch}_setup.${ext}'
    }
  }
}
