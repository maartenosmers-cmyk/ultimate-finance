package finance

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue

sealed class Screen {
    data object Auth : Screen()
    data object Home : Screen()
    data object Connections : Screen()
    data object Settings : Screen()
    data class AccountDetail(val accountId: String) : Screen()
}

/** Single observable container driving the whole UI (no DI, no VM framework). */
class AppState {
    var serverUrl by mutableStateOf("http://localhost:8080")
    var token by mutableStateOf<String?>(null)
    var user by mutableStateOf<User?>(null)
    var householdId by mutableStateOf<String?>(null)

    var screen by mutableStateOf<Screen>(Screen.Auth)
    var busy by mutableStateOf(false)
    var error by mutableStateOf<String?>(null)

    var accounts by mutableStateOf<List<Account>>(emptyList())
    var transactions by mutableStateOf<List<Transaction>>(emptyList())
    var connections by mutableStateOf<List<ConnectionDto>>(emptyList())
    var detailTransactions by mutableStateOf<List<Transaction>>(emptyList())
    /** Non-null → show the new-transaction dialog for this account id. */
    var pendingNewTxAccount by mutableStateOf<String?>(null)

    val api: ApiClient get() = ApiClient(serverUrl, token)
    val isSignedIn: Boolean get() = token != null && user != null

    private fun fail(e: Throwable) {
        error = (e as? ApiException)?.message ?: e.message ?: "Something went wrong"
    }

    // ---- auth ----

    suspend fun restore() {
        // No persistence in v1: every launch starts at Auth.
        screen = Screen.Auth
    }

    suspend fun signUp(email: String, password: String, displayName: String) {
        busy = true; error = null
        try {
            val r = api.signUp(email, password, displayName)
            token = r.token; user = r.user; householdId = r.household?.id
            screen = Screen.Home
            loadHome()
        } catch (e: Exception) { fail(e) }
        busy = false
    }

    suspend fun logIn(email: String, password: String) {
        busy = true; error = null
        try {
            val r = api.logIn(email, password)
            token = r.token; user = r.user
            val me = api.me()
            householdId = me.households.firstOrNull()?.household?.id
            screen = Screen.Home
            loadHome()
        } catch (e: Exception) { fail(e) }
        busy = false
    }

    suspend fun logOut() {
        api.logOut()
        token = null; user = null; householdId = null
        accounts = emptyList(); transactions = emptyList(); connections = emptyList()
        screen = Screen.Auth
    }

    // ---- data ----

    suspend fun loadHome(syncFirst: Boolean = false) {
        val hh = householdId ?: return
        try {
            if (syncFirst) {
                connections = api.connections(hh)
                connections.filter { it.status == "connected" }.forEach { runCatching { api.sync(it.id) } }
            }
            coroutineLoad(hh)
        } catch (e: Exception) { fail(e) }
    }

    private suspend fun coroutineLoad(hh: String) {
        val a = api.accounts(hh)
        val t = api.transactions(hh, limit = 30)
        accounts = a
        transactions = t.transactions
        connections = api.connections(hh)
    }

    suspend fun openAccount(accountId: String) {
        detailTransactions = transactions.filter { it.accountId == accountId }
        screen = Screen.AccountDetail(accountId)
    }

    suspend fun addTransaction(accountId: String, amountMinor: Long, merchant: String?) {
        busy = true; error = null
        try {
            api.createTransaction(accountId, ApiClient.todayString(), amountMinor, merchant)
            loadHome()
            if (screen is Screen.AccountDetail) openAccount(accountId)
        } catch (e: Exception) { fail(e) }
        busy = false
    }

    suspend fun mockConnect() {
        val hh = householdId ?: return
        busy = true; error = null
        try {
            val r = api.mockConnect(hh)
            connections = connections + r.connection
            loadHome()
        } catch (e: Exception) { fail(e) }
        busy = false
    }

    suspend fun syncConnection(id: String) {
        try {
            api.sync(id)
            loadHome()
        } catch (e: Exception) { fail(e) }
    }

    // ---- derived ----

    val netWorthMinor: Long get() = accounts.sumOf { it.currentBalanceMinor }
    val assetsMinor: Long get() = accounts.filter { !it.type.isLiability }.sumOf { it.currentBalanceMinor }
    val liabilitiesMinor: Long get() = accounts.filter { it.type.isLiability }.sumOf { it.currentBalanceMinor }
}
