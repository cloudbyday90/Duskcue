# Desktop icons

These PNG, ICO and ICNS assets are generated from the existing shared [Duskcue app icon source](../../../../docs/branding/assets/app-icon.svg). The desktop bundle configuration explicitly lists every checked-in icon path, including the default `icon.png` needed when Linux and macOS compile a fresh checkout.

Regenerate with the installed Tauri CLI from the repository root:

```powershell
node clients/desktop/node_modules/@tauri-apps/cli/tauri.js icon docs/branding/assets/app-icon.svg --output .cache/desktop-icons
```

Copy `icon.png`, `32x32.png`, `64x64.png`, `128x128.png`, `128x128@2x.png`, `icon.ico` and `icon.icns` into this directory. The CLI also emits mobile and AppX variants; those generated outputs remain separate from this desktop asset set. See [Tauri's icon documentation](https://v2.tauri.app/develop/icons/) and [client packaging](../../../../docs/ci/CLIENT_PACKAGING.md).
