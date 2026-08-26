import Foundation
import Observation
import Security

/// Minimal keychain-backed token store with a UserDefaults fallback so dev
/// flows never hard-fail on simulator keychain quirks.
enum TokenStore {
    private static let service = "io.ultimatefinance.session"
    private static let account = "api-token"
    private static let fallbackKey = "api-token-fallback"

    static func save(_ token: String) {
        let data = Data(token.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
        var add = query
        add[kSecValueData as String] = data
        let status = SecItemAdd(add as CFDictionary, nil)
        if status != errSecSuccess {
            UserDefaults.standard.set(token, forKey: fallbackKey)
        }
    }

    static func load() -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ]
        var result: AnyObject?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecSuccess, let data = result as? Data, let token = String(data: data, encoding: .utf8) {
            return token
        }
        return UserDefaults.standard.string(forKey: fallbackKey)
    }

    static func clear() {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
        SecItemDelete(query as CFDictionary)
        UserDefaults.standard.removeObject(forKey: fallbackKey)
    }
}

/// Root observable container: session + active household + shared API client.
@Observable
@MainActor
final class AppEnvironment {
    // Persisted settings.
    @ObservationIgnored private static let serverURLKey = "server-url"

    private(set) var serverURL: URL {
        didSet { UserDefaults.standard.set(serverURL.absoluteString, forKey: Self.serverURLKey) }
    }

    private(set) var token: String?
    private(set) var user: User?
    private(set) var households: [Household] = []
    private(set) var isRestoringSession = true

    var activeHouseholdId: String? { households.first?.id }

    /// Rebuild the client whenever credentials/server change.
    var api: APIClient { APIClient(baseURL: serverURL, token: token) }

    var isSignedIn: Bool { token != nil && user != nil }

    init() {
        if let raw = UserDefaults.standard.string(forKey: Self.serverURLKey),
           let url = URL(string: raw) {
            serverURL = url
        } else {
            serverURL = URL(string: "http://localhost:8080")!
        }
        token = TokenStore.load()
    }

    /// Try a silent /me against the stored token at launch.
    func restoreSession() async {
        guard let token else {
            isRestoringSession = false
            return
        }
        do {
            let me = try await api.me()
            user = me.user
            households = me.households.map(\.household)
        } catch {
            // Bad/expired token or unreachable server → start signed out.
            signOutLocally()
        }
        isRestoringSession = false
    }

    func signIn(response: AuthResponse) {
        token = response.token
        user = response.user
        TokenStore.save(response.token)
        Task { await refreshHouseholds() }
    }

    func refreshHouseholds() async {
        guard let me = try? await api.me() else { return }
        user = me.user
        households = me.households.map(\.household)
    }

    func updateServerURL(_ string: String) throws {
        guard let url = URL(string: string.trimmingCharacters(in: .whitespacesAndNewlines)),
              url.scheme == "http" || url.scheme == "https",
              url.host() != nil else {
            throw APIError.invalidURL
        }
        serverURL = url
    }

    func signOutLocally() {
        token = nil
        user = nil
        households = []
        TokenStore.clear()
    }

    func signOut() async {
        await api.logOut()
        signOutLocally()
    }
}
