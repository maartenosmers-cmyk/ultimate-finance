import SwiftUI

/// Home: net worth headline + accounts grouped assets/liabilities + recent
/// activity. Pull to refresh re-syncs connections then reloads.
struct HomeView: View {
    @Environment(AppEnvironment.self) private var env

    @State private var accounts: [Account] = []
    @State private var recent: [Transaction] = []
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var showingNewTransaction = false

    var body: some View {
        NavigationStack {
            Group {
                if isLoading {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if let errorMessage {
                    ContentUnavailableView(
                        "Couldn't load",
                        systemImage: "wifi.exclamationmark",
                        description: Text(errorMessage)
                    )
                } else {
                    content
                }
            }
            .background(Theme.Palette.background)
            .navigationTitle("Home")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink { ConnectionsView() } label: {
                        Label("Connections", systemImage: "bankbuilding")
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    NavigationLink { SettingsView() } label: {
                        Label("Settings", systemImage: "gearshape")
                    }
                }
            }
            .sheet(isPresented: $showingNewTransaction) {
                NewTransactionSheet(accounts: manualAccounts)
            }
            .task { await load() }
            .refreshable { await load(syncConnections: true) }
        }
    }

    private var content: some View {
        List {
            Section {
                netWorthCard
                    .listRowInsets(EdgeInsets())
                    .listRowBackground(Color.clear)
            }

            if !assetAccounts.isEmpty {
                Section("Assets") {
                    ForEach(assetAccounts) { AccountRow(account: $0) }
                }
            }

            if !liabilityAccounts.isEmpty {
                Section("Liabilities") {
                    ForEach(liabilityAccounts) { AccountRow(account: $0) }
                }
            }

            if !manualAccounts.isEmpty || !accounts.isEmpty {
                Section {
                    Button {
                        showingNewTransaction = true
                    } label: {
                        Label("Add Transaction", systemImage: "plus.circle.fill")
                            .fontWeight(.semibold)
                            .foregroundStyle(Theme.Palette.accent)
                    }
                }
            }

            Section("Recent Activity") {
                if recent.isEmpty {
                    Text("No transactions yet — connect a bank or add one manually.")
                        .font(.callout)
                        .foregroundStyle(Theme.Palette.mutedText)
                } else {
                    ForEach(recent.prefix(8)) { TransactionRow(transaction: $0) }
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    private var netWorthCard: some View {
        VStack(spacing: Theme.Spacing.s) {
            Text("Net Worth")
                .font(.subheadline.weight(.medium))
                .foregroundStyle(Theme.Palette.mutedText)
            HeadlineAmount(amountMinor: netWorthMinor)
            HStack(spacing: Theme.Spacing.xl) {
                labeled("Assets", value: assetsMinor, color: Theme.Palette.accent)
                labeled("Debts", value: liabilitiesMinor, color: Theme.Palette.negative)
            }
        }
        .frame(maxWidth: .infinity)
        .cardStyle()
        .padding(.horizontal, Theme.Spacing.s)
        .padding(.vertical, Theme.Spacing.s)
    }

    private func labeled(_ title: String, value: Int64, color: Color) -> some View {
        VStack(spacing: 2) {
            Text(title).font(.caption2).foregroundStyle(Theme.Palette.mutedText)
            MoneyText(amountMinor: value, signedColoring: false)
                .font(.callout.weight(.semibold))
                .foregroundStyle(color)
        }
    }

    // MARK: derived

    private var assetAccounts: [Account] { accounts.filter { !$0.type.isLiability } }
    private var liabilityAccounts: [Account] { accounts.filter(\.type.isLiability) }
    private var manualAccounts: [Account] { accounts.filter { !$0.isSynced } }
    private var netWorthMinor: Int64 { accounts.map(\.currentBalanceMinor).reduce(0, +) }
    private var assetsMinor: Int64 { accounts.filter { !$0.type.isLiability }.map(\.currentBalanceMinor).reduce(0, +) }
    private var liabilitiesMinor: Int64 { accounts.filter(\.type.isLiability).map(\.currentBalanceMinor).reduce(0, +) }

    // MARK: data

    @MainActor
    private func load(syncConnections: Bool = false) async {
        errorMessage = nil
        guard let hhId = env.activeHouseholdId else {
            isLoading = false
            return
        }
        do {
            let api = env.api
            if syncConnections {
                let conns = try await api.connections(householdId: hhId)
                for c in conns where c.status == "connected" {
                    _ = try? await api.sync(connectionId: c.id)
                }
            }
            async let a = api.accounts(householdId: hhId)
            async let t = api.transactions(householdId: hhId, limit: 20)
            accounts = try await a
            recent = try await t.transactions
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }
}

private struct AccountRow: View {
    let account: Account

    var body: some View {
        NavigationLink {
            AccountDetailView(accountID: account.id)
        } label: {
            HStack {
                Image(systemName: icon)
                    .foregroundStyle(Theme.Palette.accent)
                    .frame(width: 28)
                VStack(alignment: .leading, spacing: 2) {
                    Text(account.name).lineLimit(1)
                    if account.isSynced {
                        Text("Synced")
                            .font(.caption2.weight(.semibold))
                            .foregroundStyle(Theme.Palette.mutedText)
                    }
                }
                Spacer()
                MoneyText(
                    amountMinor: account.currentBalanceMinor,
                    currencyCode: account.currency,
                    signedColoring: false
                )
                .foregroundStyle(account.type.isLiability ? Theme.Palette.negative : .primary)
            }
        }
    }

    private var icon: String {
        switch account.type {
        case .checking: return "banknote"
        case .savings: return "dollarsign.circle"
        case .creditCard: return "creditcard"
        case .brokerage, .retirement: return "chart.pie"
        case .loan, .mortgage: return "house"
        case .property: return "house.and.flag"
        case .vehicle: return "car"
        default: return "wallet.pass"
        }
    }
}
