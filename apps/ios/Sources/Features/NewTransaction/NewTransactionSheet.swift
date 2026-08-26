import SwiftUI

/// Add a manual transaction: expense/income toggle, amount in dollars,
/// date picker, merchant.
struct NewTransactionSheet: View {
    let accounts: [Account]

    @Environment(AppEnvironment.self) private var env
    @Environment(\.dismiss) private var dismiss

    enum Direction: String, CaseIterable {
        case expense = "Expense"
        case income = "Income"
    }

    @State private var accountId = ""
    @State private var direction: Direction = .expense
    @State private var amountText = ""
    @State private var merchant = ""
    @State private var date = Date()
    @State private var isSaving = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            Form {
                if accounts.isEmpty {
                    Section {
                        Label(
                            "All your accounts are bank-synced — balances come from the institution.",
                            systemImage: "info.circle"
                        )
                        .font(.callout)
                        .foregroundStyle(Theme.Palette.mutedText)
                    }
                }

                Section("Details") {
                    Picker("Account", selection: $accountId) {
                        ForEach(accounts) { Text($0.name).tag($0.id) }
                    }
                    Picker("Flow", selection: $direction) {
                        ForEach(Direction.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                    }
                    .pickerStyle(.segmented)

                    HStack {
                        Text(currencySymbol)
                        TextField("0.00", text: $amountText)
                            .keyboardType(.decimalPad)
                            .font(.title3.monospacedDigit())
                    }

                    TextField("Merchant (optional)", text: $merchant)
                        .textInputAutocapitalization(.words)

                    DatePicker("Date", selection: $date, displayedComponents: .date)
                }

                if let errorMessage {
                    Section { Text(errorMessage).foregroundStyle(Theme.Palette.negative) }
                }
            }
            .navigationTitle("New Transaction")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if isSaving {
                        ProgressView()
                    } else {
                        Button("Save") { Task { await save() } }
                            .disabled(!canSave)
                    }
                }
            }
            .onAppear {
                if accountId.isEmpty, let first = accounts.first {
                    accountId = first.id
                }
            }
        }
    }

    private var currencySymbol: String {
        var f = NumberFormatter()
        f.numberStyle = .currency
        f.currencyCode = "USD"
        return f.currencySymbol ?? "$"
    }

    private var typedDollars: Decimal? {
        Decimal(string: amountText.replacingOccurrences(of: ",", with: "."))
    }

    private var canSave: Bool {
        guard let d = typedDollars, d > 0, !accountId.isEmpty else { return false }
        return true
    }

    private func save() async {
        guard let dollars = typedDollars else { return }
        var minor = APIClient.minorUnits(fromDollars: dollars)
        if direction == .expense { minor = -minor }
        isSaving = true
        errorMessage = nil
        defer { isSaving = false }
        do {
            _ = try await env.api.createTransaction(
                accountId: accountId,
                date: date,
                amountMinor: minor,
                merchant: merchant.trimmingCharacters(in: .whitespaces).isEmpty ? nil : merchant
            )
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
