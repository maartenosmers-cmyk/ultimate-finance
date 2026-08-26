# Building the Android + Windows app (Compose Multiplatform)

One Kotlin codebase (`composeApp/`) → two native targets:
- **Windows desktop** — JVM app, packages to `.msi`/`.exe`
- **Android** — APK / installable

## Toolchain on this machine (already installed)

| Tool | Location |
|---|---|
| JDK 21 (Corretto) | `C:\tools\jdk21.0.12_9` |
| Gradle 8.12 | `C:\tools\gradle-8.12\bin\gradle.bat` |
| Android SDK | `C:\tools\android-sdk` (platform-35, build-tools 35.0.0) |

`gradle.properties` pins `org.gradle.java.home`; `local.properties` (gitignored)
pins `sdk.dir`. Set `JAVA_HOME=C:\tools\jdk21.0.12_9` in your shell.

## Windows desktop

```powershell
cd apps/kmp
C:\tools\gradle-8.12\bin\gradle.bat :composeApp:run                  # dev run
C:\tools\gradle-8.12\bin\gradle.bat :composeApp:createDistributable
# → composeApp\build\compose\binaries\main\app\UltimateFinance\UltimateFinance.exe
```

✅ Verified building + launching on this machine.

## Android

```powershell
cd apps/kmp
C:\tools\gradle-8.12\bin\gradle.bat :composeApp:assembleDebug
# → composeApp\build\outputs\apk\debug\composeApp-debug.apk   (8.7 MB)
adb install composeApp\build\outputs\apk\debug\composeApp-debug.apk
```

✅ Verified building on this machine. Install on a device/emulator, or drag the
APK onto an emulator window.

## Pointing the app at the API

Settings screen → Server URL. On the same PC: `http://localhost:8080`.
From an Android device on your LAN: `http://<WINDOWS-LAN-IP>:8080`
(open the port with the `netsh` firewall rule from `apps/ios/BUILD.md`).

Flow: sign up → **Connections → Add test bank** → transactions appear on Home.
Manual transactions: **+ Tx** on Home (manual accounts only).

## Architecture

```
composeApp/src/
  commonMain/kotlin/finance/     Models, ApiClient (expect/actual Http), AppState, all Compose UI
  androidMain/                   MainActivity, manifest, Http/Today actuals
  desktopMain/                   Window entry, Http/Today actuals
```

Both targets are JVM, so the `Http` and `today()` actuals are identical
implementations; the split exists because common code can't touch `java.*`.

## Build tips for this network

First dependency sync downloads ~1 GB from Maven Central / Google — this
connection throttles long transfers, so if a download stalls use chunked
resume (`curl -C -` in a retry loop; see `C:\tools\fetch-loop.ps1`).
Subsequent builds are incremental and fast.
