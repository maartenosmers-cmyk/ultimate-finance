# Building the iOS app (macOS VM)

One-time setup, then it's Cmd+R.

## 1. Prerequisites on macOS

```bash
# Xcode 15.4+ (16.x recommended) from the App Store or developer.apple.com
xcode-select --install

# XcodeGen generates the .xcodeproj from project.yml
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
brew install xcodegen
```

## 2. Generate + open the project

```bash
cd apps/ios
xcodegen          # → UltimateFinance.xcodeproj
open UltimateFinance.xcodeproj
```

Pick an iPhone simulator, press **Cmd+R**.

## 3. Point the app at your Windows API server

The Rust API runs on the Windows host (`cargo run` in `services/api`, binds
`0.0.0.0:8080`). From inside the VM, `localhost` is the VM itself — use the
**host's** LAN IP instead:

1. On Windows find the IP: `ipconfig` → look at your active adapter's IPv4.
2. Allow the port through Windows Firewall (admin PowerShell):
   ```powershell
   netsh advfirewall firewall add rule name="Ultimate Finance API" dir=in action=allow protocol=TCP localport=8080
   ```
3. In the app: **Settings → Server URL** → `http://<WINDOWS-IP>:8080`
   (e.g. `http://192.168.1.50:8080`). The default of `localhost:8080` works if
   you ever run the API on the Mac itself.

> Parallels note: with "Shared Network" mode the host is often reachable at
> `http://10.211.55.2:8080`. VMware NAT usually puts the host at `x.x.x.1`
> of the VM subnet.

## 4. Flow

Sign up → you land on **Home** with a starter household → **Connections →
Add test bank** pulls the mock institution through the real sync pipeline →
transactions appear. Add manual transactions from any account detail screen.

## Notes

- ATS allows plain HTTP for dev; TLS terminates at a reverse proxy later.
- Plaid Link SDK integration is intentionally not wired yet — the backend
  endpoints exist (`link-token`, `exchange`) and will light up once the Link
  iOS SDK package is added and sandbox keys are set server-side.
- App source is pure SwiftUI + Swift Concurrency (iOS 17+). No third-party deps.
