package finance

import kotlinx.serialization.Serializable

// Wire models — mirror services/api/src/models.rs exactly.

@Serializable
data class AuthResponse(val token: String, val user: User, val household: Household? = null)

@Serializable
data class User(val id: String, val email: String, val displayName: String, val createdAt: String = "")

@Serializable
data class Household(val id: String, val name: String, val currency: String = "USD")

@Serializable
data class MeResponse(val user: User, val households: List<HouseholdEntry> = emptyList())

@Serializable
data class HouseholdEntry(val household: Household, val role: String = "member")

@Serializable
enum class AccountType(val isLiability: Boolean = false) {
    checking, savings,
    @kotlinx.serialization.SerialName("credit_card") creditCard(true),
    brokerage, retirement,
    loan(true), mortgage(true),
    property, vehicle, cash, other;
}

@Serializable
data class Account(
    val id: String,
    val householdId: String,
    val connectionId: String? = null,
    val externalId: String? = null,
    val name: String,
    val type: AccountType = AccountType.other,
    val currency: String = "USD",
    val currentBalanceMinor: Long = 0,
) {
    val isSynced: Boolean get() = connectionId != null
}

@Serializable
data class AccountsResponse(val accounts: List<Account> = emptyList())

@Serializable
data class Transaction(
    val id: String,
    val accountId: String,
    val postedOn: String,
    val amountMinor: Long,
    val merchantRaw: String? = null,
    val description: String? = null,
)

@Serializable
data class TransactionsResponse(
    val transactions: List<Transaction> = emptyList(),
    val nextBefore: String? = null,
)

@Serializable
data class ConnectionDto(
    val id: String,
    val provider: String,
    val status: String = "pending",
    val institutionName: String? = null,
)

@Serializable
data class ConnectionsResponse(val connections: List<ConnectionDto> = emptyList())

@Serializable
data class MockConnectResponse(
    val connection: ConnectionDto,
    val accounts: List<Account> = emptyList(),
    val transactionsInserted: Int = 0,
)

// Requests

@Serializable
data class SignupRequest(val email: String, val password: String, val displayName: String)

@Serializable
data class LoginRequest(val email: String, val password: String)

@Serializable
data class CreateTransactionRequest(
    val accountId: String,
    val postedOn: String,
    val amountMinor: Long,
    val merchantRaw: String? = null,
)

@Serializable
data class MockConnectRequest(val householdId: String)
