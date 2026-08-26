package finance

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.json.Json

class ApiException(message: String, val statusCode: Int) : Exception(message)

private val json = Json { ignoreUnknownKeys = true; explicitNulls = false }

/** Thin suspend client over the expect/actual Http. One method per endpoint. */
class ApiClient(val baseURL: String, val token: String? = null) {

    private fun headers(withAuth: Boolean = true): Map<String, String> =
        buildMap {
            put("Content-Type", "application/json")
            if (withAuth && token != null) put("Authorization", "Bearer $token")
        }

    private suspend fun <T> call(
        serializer: KSerializer<T>,
        method: String,
        path: String,
        body: String? = null,
        auth: Boolean = true,
    ): T = withContext(Dispatchers.IO) {
        val res = Http.request(method, baseURL.trimEnd('/') + path, body, headers(auth))
        if (res.status !in 200..299) {
            val msg = runCatching {
                json.decodeFromString(ErrorEnvelope.serializer(), res.body).error.message
            }.getOrNull() ?: "Request failed (${res.status})"
            throw ApiException(msg, res.status)
        }
        json.decodeElement(serializer, res.body)
    }

    private fun <T> Json.decodeElement(serializer: KSerializer<T>, raw: String): T =
        decodeFromString(serializer, raw)

    private suspend fun <T> get(s: KSerializer<T>, path: String): T = call(s, "GET", path)
    private suspend fun <T> post(s: KSerializer<T>, path: String, body: String? = null, auth: Boolean = true): T =
        call(s, "POST", path, body, auth)

    // ---- endpoints ----

    suspend fun signUp(email: String, password: String, displayName: String): AuthResponse =
        post(
            AuthResponse.serializer(), "/api/v1/auth/signup",
            json.encodeToString(SignupRequest.serializer(), SignupRequest(email, password, displayName)),
            auth = false,
        )

    suspend fun logIn(email: String, password: String): AuthResponse {
        val raw = post(
            LoginWire.serializer(), "/api/v1/auth/login",
            json.encodeToString(LoginRequest.serializer(), LoginRequest(email, password)),
            auth = false,
        )
        return AuthResponse(raw.token, raw.user)
    }

    @kotlinx.serialization.Serializable
    private data class LoginWire(val token: String, val user: User)

    suspend fun logOut() {
        withContext(Dispatchers.IO) {
            runCatching { Http.request("POST", baseURL.trimEnd('/') + "/api/v1/auth/logout", null, headers()) }
        }
    }

    suspend fun me(): MeResponse = get(MeResponse.serializer(), "/api/v1/me")

    suspend fun accounts(householdId: String): List<Account> =
        get(AccountsResponse.serializer(), "/api/v1/accounts?householdId=$householdId").accounts

    suspend fun transactions(householdId: String, limit: Int = 100): TransactionsResponse =
        get(TransactionsResponse.serializer(), "/api/v1/transactions?householdId=$householdId&limit=$limit")

    suspend fun createTransaction(
        accountId: String, date: String, amountMinor: Long, merchant: String?,
    ): Transaction {
        val w = post(
            TxWrapper.serializer(), "/api/v1/transactions",
            json.encodeToString(
                CreateTransactionRequest.serializer(),
                CreateTransactionRequest(accountId, date, amountMinor, merchant),
            ),
        )
        return w.transaction
    }

    @kotlinx.serialization.Serializable
    private data class TxWrapper(val transaction: Transaction)

    suspend fun connections(householdId: String): List<ConnectionDto> =
        get(ConnectionsResponse.serializer(), "/api/v1/connections?householdId=$householdId").connections

    suspend fun mockConnect(householdId: String): MockConnectResponse =
        post(
            MockConnectResponse.serializer(), "/api/v1/connections/mock-connect",
            json.encodeToString(MockConnectRequest.serializer(), MockConnectRequest(householdId)),
        )

    suspend fun sync(connectionId: String): Int {
        val w = post(SyncResult.serializer(), "/api/v1/connections/$connectionId/sync")
        return w.transactionsInserted
    }

    @kotlinx.serialization.Serializable
    private data class SyncResult(val transactionsInserted: Int)

    @kotlinx.serialization.Serializable
    private data class ErrorEnvelope(val error: Err)

    @kotlinx.serialization.Serializable
    private data class Err(val code: String = "", val message: String = "")

    // ---- formatting ----

    companion object {
        /** Signed cents → display string, e.g. -1234 → "-$12.34". */
        fun minorToDollarsString(minor: Long): String {
            val negative = minor < 0
            val cents = if (negative) -minor else minor
            val whole = cents / 100
            val frac = cents % 100
            val sign = if (negative) "-" else ""
            return if (frac == 0L) "$sign$$whole" else "$sign$$whole.${frac.toString().padStart(2, '0')}"
        }

        /** Today as `yyyy-MM-dd` (actual per platform). */
        fun todayString(): String = today()
    }
}

/** Platform clock access, kept out of common code. */
internal expect fun today(): String
