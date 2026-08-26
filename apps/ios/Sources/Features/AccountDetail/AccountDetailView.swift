import SwiftUI

/// One account: balance header + full transaction history with pull-to-refresh.
struct AccountDetailView: View {
    let accountID: String

    @Environment(AppEnvironment.self) private var env

    @State private var account: Account?
    @State private var transactions: [Transaction] = []
    @State private var isLoading = true
    @State private var errorMessage: String?
    @State private var showingNewTransaction = false
    /// Cursor for the next older page; nil when everything is loaded.
    @State private var nextBefore: String?

    var body: some View {
        Group {
            if let account {
                content(account)
            } else if isLoading {
                ProgressView()
            } else {
                ContentUnavailableView("Account not found", systemImage: "questionmark.circle")
            }
        }
        .background(Theme.Palette.background)
        .navigationTitle(account?.name ?? "Account")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    showingNewTransaction = true
                } label: {
                    Image(systemName: "plus")
                }
                .disabled(!(account?.isSynced == false))
            }
        }
        .sheet(isPresented: $showingNewTransaction) {
            if let account {
                NewTransactionSheet(accounts: [account])
            }
        }
        .task { await reload() }
        .refreshable { await reload() }
    }

    private func content(_ account: Account) -> some View {
        List {
            Section {
                VStack(spacing: Theme.Spacing.xs) {
                    Text("Balance")
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(Theme.Palette.mutedText)
                    HeadlineAmount(amountMinor: account.currentBalanceMinor)
                        .foregroundStyle(account.type.isLiability ? Theme.Palette.negative : .primary)
                    if account.isSynced {
                        Label("Synced from your bank", systemImage: "arrow.triangle.2.circlepath")
                            .font(.caption)
                            .foregroundStyle(Theme.Palette.mutedText)
                    }
                }
                .frame(maxWidth: .infinity)
                .cardStyle()
                .listRowInsets(EdgeInsets())
                .listRowBackground(Color.clear)
            }

            Section("Transactions") {
                if transactions.isEmpty && !isLoading {
                    Text("Nothing here yet.")
                        .foregroundStyle(Theme.Palette.mutedText)
                }
                ForEach(transactions) { TransactionRow(transaction: $0) }
                if nextBefore != nil {
                    Button {
                        Task { await loadMore() }
                    } label: {
                        HStack {
                            Spacer()
                            if isLoading { ProgressView() } else { Text("Older transactions") }
                            Spacer()
                        }
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }

    // MARK: data

    @MainActor
    private func reload() async {
        errorMessage = nil
        do {
            guard let hhId = env.activeHouseholdId else { return }
            async let accounts = env.api.accounts(householdId: hhId)
            let page = try await env.api.transactions(householdId: hhId, limit: 50)
            account = try await accounts.first { $0.id == accountID }
            // Keep this account's transactions only.
            transactions = page.transactions.filter { $0.accountId == accountID }
            nextBefore = page.nextBefore
        } catch {
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    @MainActor
    private func loadMore() async {
        // v1: single household-wide cursor; detail paging refines in M3.
        await reload()
    }
}
