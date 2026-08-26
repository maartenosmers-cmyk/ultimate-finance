import SwiftUI

/// Signed minor-units text with income/expense coloring.
struct MoneyText: View {
    let amountMinor: Int64
    var currencyCode = "USD"
    var signedColoring = true

    private var formatted: String {
        let dollars = NSDecimalNumber(value: Double(amountMinor) / 100).decimalValue
        let f = NumberFormatter()
        f.numberStyle = .currency
        f.currencyCode = currencyCode
        return f.string(from: NSDecimalNumber(decimal: abs(dollars))) ?? "\(amountMinor)"
    }

    var body: some View {
        Text(prefix + formatted)
            .fontDesign(.rounded)
            .foregroundStyle(color)
    }

    private var prefix: String {
        guard signedColoring else { return amountMinor < 0 ? "-" : "" }
        return amountMinor >= 0 ? "+" : "-"
    }

    private var color: Color {
        guard signedColoring else { return .primary }
        if amountMinor > 0 { return Theme.Palette.accent }
        if amountMinor < 0 { return Theme.Palette.negative }
        return Theme.Palette.mutedText
    }
}

/// Big rounded number for net-worth style headers (no +/- coloring).
struct HeadlineAmount: View {
    let amountMinor: Int64

    var body: some View {
        Text(text)
            .font(.system(size: 40, weight: .bold, design: .rounded))
            .monospacedDigit()
            .contentTransition(.numericText())
    }

    private var text: String {
        let f = NumberFormatter()
        f.numberStyle = .currency
        f.currencyCode = "USD"
        f.maximumFractionDigits = abs(amountMinor % 100) == 0 ? 0 : 2
        let v = NSDecimalNumber(value: Double(amountMinor) / 100)
        return f.string(from: v) ?? "$\(amountMinor)"
    }
}

struct TransactionRow: View {
    let transaction: Transaction

    var body: some View {
        HStack(spacing: Theme.Spacing.m) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).lineLimit(1)
                Text(dateLabel).font(.caption).foregroundStyle(Theme.Palette.mutedText)
            }
            Spacer()
            MoneyText(amountMinor: transaction.amountMinor)
        }
        .padding(.vertical, 2)
    }

    private var title: String {
        transaction.merchantRaw ?? transaction.description ?? "Transaction"
    }

    private var dateLabel: String {
        guard let d = APIClient.displayDate(transaction.postedOn) else { return transaction.postedOn }
        return d.formatted(date: .abbreviated, time: .omitted)
    }
}
