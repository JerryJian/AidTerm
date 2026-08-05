/**
 * @type {import('electron-builder').Configuration}
 */
module.exports = () => {
  const target = process.env.BUILD_TARGET || process.argv.find(a => a.startsWith('--linux') ? 'linux' : a.startsWith('--mac') ? 'mac' : a.startsWith('--win') ? 'win' : null) || 'win'

  const isWin = target === 'win'
  const isLinux = target === 'linux'
  const isMac = target === 'mac'

  const platformExclude = isWin
    ? ['darwin-*', 'linux-*', 'android-*', 'win32-arm64', 'win32-ia32']
    : isLinux
      ? ['darwin-*', 'win32-*', 'android-*']
      : ['linux-*', 'win32-*', 'android-*']

  return {
    appId: 'com.aidterm.app',
    productName: 'AidTerm',
    executableName: 'aidterm',
    directories: {
      output: './out'
    },
    files: [
      'dist/**/*',
      'node_modules/**/*',
      '!node_modules/**/{deps,scripts,node-addon-api,build/deps,build/Release/obj}/**',
      `!node_modules/**/prebuilds/{${platformExclude.join(',')}}/**`,
      `!node_modules/**/third_party/**/{${isWin ? 'win10-arm64' : isLinux ? 'win10-x64,win10-arm64' : 'win10-x64,win10-arm64'}}/**`,
      `!node_modules/**/bin/{${platformExclude.join(',')}}*}/**`,
      '!node_modules/**/*.{md,ts,map,cc,vcxproj,vcxproj.filters,sln,iobj,ipdb,lib,exp,tlog,recipe,xml,gypi,bat,c}',
      '!node_modules/**/buildcheck.*',
      '!node_modules/**/.eslint*',
      '!node_modules/**/*.{test,spec}.js',
      '!node_modules/**/install.js',
    ],
    asarUnpack: [
      'node_modules/node-pty/lib/**',
      'node_modules/node-pty/prebuilds/**',
      'node_modules/node-pty/build/Release/**',
      'node_modules/node-pty/third_party/**',
      'node_modules/cpu-features/lib/**',
      'node_modules/cpu-features/build/Release/**',
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
