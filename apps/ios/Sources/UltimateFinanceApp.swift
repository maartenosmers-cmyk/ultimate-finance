import SwiftUI

@main
struct UltimateFinanceApp: App {
    @State private var env = AppEnvironment()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environment(env)
        }
    }
}

/// Switches between auth and the main experience once session state resolves.
private struct RootView: View {
    @Environment(AppEnvironment.self) private var env

    var body: some View {
        if env.isRestoringSession {
            ZStack {
                Theme.Palette.background.ignoresSafeArea()
                Image(systemName: "chart.line.uptrend.xyaxis")
                    .font(.system(size: 48, weight: .semibold))
                    .foregroundStyle(Theme.Palette.accent)
            }
            .task { await env.restoreSession() }
        } else if env.isSignedIn {
            HomeView()
        } else {
            AuthView()
        }
    }
}
