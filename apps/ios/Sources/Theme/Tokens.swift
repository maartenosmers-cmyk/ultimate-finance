import SwiftUI

/// Design tokens: one place to retune the whole app. Dark-first finance
/// aesthetic with an emerald accent; adapts to light mode automatically.
enum Theme {
    enum Palette {
        static let accent = Color(red: 0.10, green: 0.78, blue: 0.55)      // emerald
        static let negative = Color(red: 0.95, green: 0.35, blue: 0.38)    // warm red
        static let background = Color(uiColor: .systemGroupedBackground)
        static let card = Color(uiColor: .secondarySystemGroupedBackground)
        static let mutedText = Color.secondary
    }

    enum Spacing {
        static let xs: CGFloat = 4
        static let s: CGFloat = 8
        static let m: CGFloat = 12
        static let l: CGFloat = 16
        static let xl: CGFloat = 24
        static let xxl: CGFloat = 32
    }

    enum Radius {
        static let card: CGFloat = 16
    }
}

extension View {
    /// Standard elevated-card treatment used across screens.
    func cardStyle() -> some View {
        self
            .padding(Theme.Spacing.l)
            .background(Theme.Palette.card, in: RoundedRectangle(cornerRadius: Theme.Radius.card))
    }
}
