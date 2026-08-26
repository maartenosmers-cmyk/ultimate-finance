import SwiftUI

/// Server endpoint + session management.
struct SettingsView: View {
    @Environment(AppEnvironment.self) private var env
    @Environment(\.dismiss) private var dismiss

    @State private var serverURLText = ""
    @State private var urlMessage: String?
    @State private var confirmSignOut = false

    var body: some View {
        Form {
            Section {
                TextField("http://192.168.1.50:8080", text: $serverURLText)
                    .keyboardType(.URL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                Button("Apply") { applyURL() }
                if let urlMessage {
                    Text(urlMessage).font(.caption).foregroundStyle(urlMessage.hasPrefix("✓") ? Theme.Palette.accent : Theme.Palette.negative)
                }
            } header: {
                Text("Server URL")
            } footer: {
                Text("The Rust API on your Windows host. From a VM use the host's LAN IP, not localhost.")
            }

            Section("Account") {
                LabeledContent("Signed in as", value: env.user?.email ?? "—")
                Button(role: .destructive) {
                    confirmSignOut = true
                } label: {
                    Text("Sign Out")
                }
                .confirmationDialog(
                    "Sign out of Ultimate Finance?",
                    isPresented: $confirmSignOut,
                    titleVisibility: .visible
                ) {
                    Button("Sign Out", role: .destructive) {
                        Task {
                            await env.signOut()
                            dismiss()
                        }
                    }
                }
            }

            Section("About") {
                LabeledContent("Version", value: "0.1.0")
                LabeledContent("API", value: "M2 aggregation build")
            }
        }
        .navigationTitle("Settings")
        .onAppear { serverURLText = env.serverURL.absoluteString }
    }

    private func applyURL() {
        do {
            try env.updateServerURL(serverURLText)
            urlMessage = "✓ Applied — reload Home to use it."
        } catch {
            urlMessage = "That doesn't look like a valid URL."
        }
    }
}
