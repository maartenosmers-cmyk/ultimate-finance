import Foundation

enum APIError: LocalizedError {
    case invalidURL
    case transport(Error)
    case status(code: Int, message: String)

    var errorDescription: String? {
        switch self {
        case .invalidURL:
            return "The server URL looks wrong. Check Settings."
        case .transport(let e):
            return "Can't reach the server. \(e.localizedDescription)"
        case .status(401, _):
            return "Session expired. Sign in again."
        case .status(_, let message):
            return message
        }
    }
}

/// Thin async/await client over URLSession. One method per endpoint; no
/// generic endpoint soup so call sites stay greppable.
struct APIClient {
    var baseURL: URL
    var token: String?

    private let decoder = JSONDecoder()
    private let encoder = JSONEncoder()

    private struct EmptyBody: Encodable {}

    // MARK: plumbing

    private func baseRequest<B: Encodable>(
        _ method: String,
        _ path: String,
        body: B?
    ) throws -> URLRequest {
        guard let url = URL(string: path, relativeTo: baseURL) else { throw APIError.invalidURL }
        var req = URLRequest(url: url)
        req.httpMethod = method
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        if let token {
            req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        if let body {
            req.httpBody = try encoder.encode(body)
        }
        return req
    }

    private func request(_ method: String, _ path: String) throws -> URLRequest {
        try baseRequest(method, path, body: Optional<EmptyBody>.none)
    }

    private func request<B: Encodable>(_ method: String, _ path: String, jsonBody: B) throws -> URLRequest {
        try baseRequest(method, path, body: Optional(jsonBody))
    }

    private func send(_ req: URLRequest) async throws -> Data {
        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await URLSession.shared.data(for: req)
        } catch {
            throw APIError.transport(error)
        }
        let statusCode = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard (200..<300).contains(statusCode) else {
            let message = decodeErrorMessage(data) ?? "Request failed (\(statusCode))"
            throw APIError.status(code: statusCode, message: message)
        }
        return data
    }

    private func decodeErrorMessage(_ data: Data) -> String? {
        struct Envelope: Decodable {
            struct Err: Decodable { let message: String }
            let error: Err
        }
        return (try? JSONDecoder().decode(Envelope.self, from: data))?.error.message
    }

    private func get<T: Decodable>(_ path: String) async throws -> T {
        let data = try await send(try request("GET", path))
        return try decoder.decode(T.self, from: data)
    }

    private func post<T: Decodable>(_ path: String) async throws -> T {
        let data = try await send(try request("POST", path))
        return try decoder.decode(T.self, from: data)
    }

    private func post<T: Decodable, B: Encodable>(_ path: String, jsonBody: B) async throws -> T {
        let data = try await send(try request("POST", path, jsonBody: jsonBody))
        return try decoder.decode(T.self, from: data)
    }

    // MARK: endpoints

    func signUp(email: String, password: String, displayName: String) async throws -> AuthResponse {
        try await post(
            "/api/v1/auth/signup",
            jsonBody: SignupRequest(email: email, password: password, displayName: displayName)
        )
    }

    func logIn(email: String, password: String) async throws -> AuthResponse {
        // Login returns {token, user}; the household arrives via /me.
        struct LoginResponse: Decodable { let token: String; let user: User }
        let r: LoginResponse = try await post(
            "/api/v1/auth/login",
            jsonBody: LoginRequest(email: email, password: password)
        )
        return AuthResponse(token: r.token, user: r.user, household: nil)
    }

    /// Fire-and-forget: a stale/invalid token still counts as logged out.
    func logOut() async {
        _ = try? await send(try request("POST", "/api/v1/auth/logout"))
    }

    func me() async throws -> MeResponse {
        try await get("/api/v1/me")
    }

    func accounts(householdId: String) async throws -> [Account] {
        try await get("/api/v1/accounts?householdId=\(householdId)").accounts
    }

    func transactions(householdId: String, limit: Int = 100) async throws -> TransactionsResponse {
        try await get("/api/v1/transactions?householdId=\(householdId)&limit=\(limit)")
    }

    @discardableResult
    func createTransaction(
        accountId: String,
        date: Date,
        amountMinor: Int64,
        merchant: String?
    ) async throws -> Transaction {
        struct Wrapper: Decodable { let transaction: Transaction }
        let w: Wrapper = try await post(
            "/api/v1/transactions",
            jsonBody: CreateTransactionRequest(
                accountId: accountId,
                postedOn: Self.apiDate(date),
                amountMinor: amountMinor,
                merchantRaw: merchant
            )
        )
        return w.transaction
    }

    func connections(householdId: String) async throws -> [ConnectionDTO] {
        try await get("/api/v1/connections?householdId=\(householdId)").connections
    }

    @discardableResult
    func mockConnect(householdId: String) async throws -> MockConnectResponse {
        struct Req: Encodable { let householdId: String }
        return try await post("/api/v1/connections/mock-connect", jsonBody: Req(householdId: householdId))
    }

    /// Returns how many new transactions arrived.
    @discardableResult
    func sync(connectionId: String) async throws -> Int {
        struct Response: Decodable { let transactionsInserted: Int }
        let r: Response = try await post("/api/v1/connections/\(connectionId)/sync")
        return r.transactionsInserted
    }

    // MARK: date + money helpers

    static func apiDate(_ date: Date) -> String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        f.timeZone = TimeZone.current
        return f.string(from: date)
    }

    static func displayDate(_ apiDateString: String) -> Date? {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd"
        return f.date(from: apiDateString)
    }

    /// Signed cents → `Decimal` dollars.
    static func minorToDollars(_ minor: Int64) -> Decimal {
        Decimal(minor) / 100
    }

    /// User-typed dollars → integer minor units, half-up like bank statements.
    static func minorUnits(fromDollars dollars: Decimal) -> Int64 {
        var cents = dollars * 100
        var rounded = Decimal()
        NSDecimalRound(&rounded, &cents, 0, .plain)
        return NSDecimalNumber(decimal: rounded).int64Value
    }
}
