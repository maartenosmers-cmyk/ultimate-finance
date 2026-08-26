import SwiftUI

/// Bank connections: connect the mock institution (dev) or sync existing ones.
/// Plaid Link SDK drops into this screen when sandbox keys are configured.
struct ConnectionsView: View {
    @Environment(AppEnvironment.self) private var env

    @State private var connections: [ConnectionDTO] = []
    @State private var isLoading = true
    @State private var isConnecting = false
    @State private var syncingIds: Set<String> = []
    @State private var message: String?

    var body: some View {
        List {
            Section("Connected Institutions") {
                if connections.isEmpty && !isLoading {
                    Text("Nothing connected yet.")
                        .foregroundStyle(Theme.Palette.mutedText)
                }
                ForEach(connections) { conn in
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(conn.institutionName ?? conn.provider.capitalized)
                            Text(statusLabel(conn.status))
                                .font(.caption)
                                .foregroundStyle(Theme.Palette.mutedText)
                        }
                        Spacer()
                        Button {
                            Task { await sync(conn.id) }
                        } label: {
                            Group {
                                if syncingIds.contains(conn.id) {
                                    ProgressView()
                                } else {
                                    Image(systemName: "arrow.triangle.2.circlepath")
                                }
                            }
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }

            Section {
                Button {
                    Task { await connectMock() }
                } label: {
                    HStack {
                        if isConnecting { ProgressView() } else {
                            Label("Add test bank", systemImage: "plus.square.on.square")
                        }
                    }
                }
                .disabled(isConnecting)
            } header: {
                Text("Add a connection")
            } footer: {
                VStack(alignment: .leading, spacing: Theme.Spacing.xs) {
                    Text("“Test bank” uses the deterministic mock institution — same pipeline as real banks.")
                    Text("Plaid Link goes here once sandbox keys are set server-side.")
                }
            }
        }
        .navigationTitle("Connections")
        .task { await load() }
        .refreshable { await load() }
    }

    private func statusLabel(_ status: String) -> String {
        switch status {
        case "connected": return "Connected"
        case "requires_reauth": return "Needs attention"
        case "error": return "Error"
        case "disconnected": return "Disconnected"
        default: return "Pending…"
        }
    }

    @MainActor
    private func load() async {
        guard let hhId = env.activeHouseholdId else { return }
        connections = (try? await env.api.connections(householdId: hhId)) ?? []
        isLoading = false
    }

    @MainActor
    private func connectMock() async {
        guard let hhId = env.activeHouseholdId else { return }
        isConnecting = true
        defer { isConnecting = false }
        do {
            let result = try await env.api.mockConnect(householdId: hhId)
            connections.append(result.connection)
            message = "Inserted \(result.transactionsInserted) transactions."
        } catch {
            message = error.localizedDescription
        }
    }

    @MainActor
    private func sync(_ id: String) async {
        syncingIds.insert(id)
        defer { syncingIds.remove(id) }
        _ = try? await env.api.sync(connectionId: id)
        await load()
    }
}
