import SwiftUI

/// Sign in / create account, segmented.
struct AuthView: View {
    @Environment(AppEnvironment.self) private var env

    enum Mode: String, CaseIterable {
        case signIn = "Sign In"
        case signUp = "Create Account"
    }

    @State private var mode: Mode = .signIn
    @State private var email = ""
    @State private var password = ""
    @State private var displayName = ""
    @State private var isWorking = false
    @State private var errorMessage: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: Theme.Spacing.xl) {
                    header
                    Picker("", selection: $mode) {
                        ForEach(Mode.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                    }
                    .pickerStyle(.segmented)

                    form
                    if let errorMessage {
                        Text(errorMessage)
                            .font(.callout)
                            .foregroundStyle(Theme.Palette.negative)
                            .multilineTextAlignment(.center)
                    }
                }
                .padding(Theme.Spacing.xl)
            }
            .scrollDismissesKeyboard(.interactively)
            .background(Theme.Palette.background)
            .navigationTitle("Welcome")
        }
    }

    private var header: some View {
        VStack(spacing: Theme.Spacing.s) {
            Image(systemName: "chart.line.uptrend.xyaxis")
                .font(.system(size: 44, weight: .semibold))
                .foregroundStyle(Theme.Palette.accent)
            Text("Your money, modeled.")
                .font(.title2.bold())
        }
        .padding(.top, Theme.Spacing.xxl)
    }

    private var form: some View {
        VStack(spacing: Theme.Spacing.m) {
            TextField("Email", text: $email)
                .textContentType(.emailAddress)
                .keyboardType(.emailAddress)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .textFieldStyle(.roundedBorder)

            if mode == .signUp {
                TextField("Your name", text: $displayName)
                    .textContentType(.name)
                    .textFieldStyle(.roundedBorder)
            }

            SecureField("Password (8+ characters)", text: $password)
                .textContentType(mode == .signUp ? .newPassword : .password)
                .textFieldStyle(.roundedBorder)

            Button(action: { Task { await submit() } }) {
                HStack {
                    if isWorking { ProgressView().tint(.white) }
                    Text(mode == .signUp ? "Create Account" : "Sign In")
                        .fontWeight(.semibold)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 6)
            }
            .buttonStyle(.borderedProminent)
            .tint(Theme.Palette.accent)
            .disabled(isWorking || !canSubmit)
        }
    }

    private var canSubmit: Bool {
        !email.trimmingCharacters(in: .whitespaces).isEmpty && password.count >= 8
            && (mode == .signIn || !displayName.trimmingCharacters(in: .whitespaces).isEmpty)
    }

    private func submit() async {
        isWorking = true
        errorMessage = nil
        defer { isWorking = false }
        do {
            let client = env.api
            let response: AuthResponse
            switch mode {
            case .signUp:
                response = try await client.signUp(
                    email: email,
                    password: password,
                    displayName: displayName
                )
            case .signIn:
                response = try await client.logIn(email: email, password: password)
            }
            env.signIn(response: response)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
