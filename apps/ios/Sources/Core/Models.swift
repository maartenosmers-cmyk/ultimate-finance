import Foundation

// MARK: - Wire models (mirror services/api/src/models.rs exactly)

struct AuthResponse: Decodable {
    let token: String
    let user: User
    let household: Household?
}

struct User: Decodable, Identifiable, Hashable {
    let id: String
    let email: String
    let displayName: String
    let createdAt: String
}

struct Household: Decodable, Identifiable, Hashable {
    let id: String
    let name: String
    let currency: String
}

struct MeResponse: Decodable {
    let user: User
    let households: [HouseholdEntry]
}

struct HouseholdEntry: Decodable {
    let household: Household
    let role: String
}

enum AccountType: String, Decodable {
    case checking, savings
    case creditCard = "credit_card"
    case brokerage, retirement
    case loan, mortgage
    case property, vehicle, cash, other

    var isLiability: Bool {
        switch self {
        case .creditCard, .loan, .mortgage: return true
        default: return false
        }
    }
}

struct Account: Decodable, Identifiable, Hashable {
    let id: String
    let householdId: String
    let connectionId: String?
    let name: String
    let type: AccountType
    let currency: String
    let currentBalanceMinor: Int64

    var isSynced: Bool { connectionId != nil }
}

struct AccountsResponse: Decodable { let accounts: [Account] }

struct Transaction: Decodable, Identifiable, Hashable {
    let id: String
    let accountId: String
    let postedOn: String          // "2026-08-26"
    let amountMinor: Int64
    let merchantRaw: String?
    let description: String?
}

struct TransactionsResponse: Decodable {
    let transactions: [Transaction]
    let nextBefore: String?
}

struct ConnectionDTO: Decodable, Identifiable, Hashable {
    let id: String
    let provider: String
    let status: String
    let institutionName: String?
}

struct ConnectionsResponse: Decodable { let connections: [ConnectionDTO] }

struct MockConnectResponse: Decodable {
    let connection: ConnectionDTO
    let accounts: [Account]
    let transactionsInserted: Int
}

// MARK: - Requests

struct SignupRequest: Encodable {
    let email: String
    let password: String
    let displayName: String
}

struct LoginRequest: Encodable {
    let email: String
    let password: String
}

struct CreateAccountRequest: Encodable {
    let householdId: String
    let type: AccountType
    let name: String
    let currentBalanceMinor: Int64?
}

struct CreateTransactionRequest: Encodable {
    let accountId: String
    let postedOn: String
    let amountMinor: Int64
    let merchantRaw: String?
}
