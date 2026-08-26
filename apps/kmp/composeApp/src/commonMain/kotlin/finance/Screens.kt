package finance

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.launch

// ---------------------------------------------------------------- shared bits

@Composable
fun MoneyText(amountMinor: Long, signed: Boolean = true, color: Color = Color.Unspecified) {
    val text = remember(amountMinor, signed) {
        buildString {
            if (amountMinor < 0) append("-") else if (signed && amountMinor > 0) append("+")
            append(ApiClient.minorToDollarsString(kotlin.math.abs(amountMinor)))
        }
    }
    val tint = when {
        color != Color.Unspecified -> color
        signed && amountMinor > 0 -> Palette.Emerald
        signed && amountMinor < 0 -> Palette.Red
        else -> LocalContentColor.current
    }
    Text(text, color = tint, fontWeight = FontWeight.SemiBold, fontSize = 15.sp)
}

// ------------------------------------------------------------------- auth

@Composable
fun AuthScreen(state: AppState) {
    val scope = rememberCoroutineScope()
    var mode by remember { mutableStateOf(0) } // 0 sign in, 1 sign up
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }

    Column(
        modifier = Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(32.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text("Your money, modeled.", fontSize = 26.sp, fontWeight = FontWeight.Bold)
        Spacer(Modifier.height(8.dp))
        Text("Ultimate Finance", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Medium)
        Spacer(Modifier.height(28.dp))

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            FilterChip(selected = mode == 0, onClick = { mode = 0 }, label = { Text("Sign In") })
            FilterChip(selected = mode == 1, onClick = { mode = 1 }, label = { Text("Create Account") })
        }
        Spacer(Modifier.height(16.dp))

        OutlinedTextField(email, { email = it }, label = { Text("Email") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
        if (mode == 1) {
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(name, { name = it }, label = { Text("Your name") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
        }
        Spacer(Modifier.height(8.dp))
        OutlinedTextField(
            password, { password = it },
            label = { Text("Password (8+)") },
            visualTransformation = PasswordVisualTransformation(),
            modifier = Modifier.fillMaxWidth(), singleLine = true,
        )
        Spacer(Modifier.height(16.dp))

        Button(
            onClick = {
                scope.launch {
                    if (mode == 1) state.signUp(email.trim(), password, name.trim())
                    else state.logIn(email.trim(), password)
                }
            },
            enabled = !state.busy && email.isNotBlank() && password.length >= 8 && (mode == 0 || name.isNotBlank()),
            modifier = Modifier.fillMaxWidth().height(48.dp),
        ) {
            if (state.busy) CircularProgressIndicator(modifier = Modifier.size(20.dp), strokeWidth = 2.dp)
            else Text(if (mode == 1) "Create Account" else "Sign In", fontWeight = FontWeight.SemiBold)
        }
        state.error?.let {
            Spacer(Modifier.height(12.dp))
            Text(it, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        }
    }
}

// ------------------------------------------------------------------- home

@Composable
fun HomeScreen(state: AppState) {
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) { state.loadHome() }

    LazyColumn(
        modifier = Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background),
        contentPadding = PaddingValues(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { NetWorthCard(state) }
        item {
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = { state.screen = Screen.Connections }) { Text("Connections") }
                OutlinedButton(onClick = { state.screen = Screen.Settings }) { Text("Settings") }
                Spacer(Modifier.weight(1f))
                Button(onClick = {
                    val manual = state.accounts.firstOrNull { !it.isSynced }
                    if (manual != null) state.pendingNewTxAccount = manual.id
                }) { Text("+ Tx") }
            }
        }
        if (state.accounts.any { !it.type.isLiability }) {
            item { SectionLabel("Assets") }
            items(state.accounts.filter { !it.type.isLiability }, key = { it.id }) { AccountCard(state, it) }
        }
        if (state.accounts.any { it.type.isLiability }) {
            item { SectionLabel("Liabilities") }
            items(state.accounts.filter { it.type.isLiability }, key = { it.id }) { AccountCard(state, it) }
        }
        item { SectionLabel("Recent Activity") }
        if (state.transactions.isEmpty()) {
            item { Text("No transactions yet — connect a bank above.", color = Color.Gray) }
        } else {
            items(state.transactions.take(10), key = { it.id }) { TxRow(it) }
        }
    }

    state.pendingNewTxAccount?.let { id ->
        state.accounts.firstOrNull { it.id == id }?.let { account ->
            NewTransactionDialog(state, account) { state.pendingNewTxAccount = null }
        }
    }
}

@Composable
private fun SectionLabel(text: String) {
    Text(text.uppercase(), fontSize = 12.sp, color = Color.Gray, fontWeight = FontWeight.SemiBold)
}

@Composable
private fun NetWorthCard(state: AppState) {
    Column(
        modifier = Modifier.fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(16.dp))
            .padding(20.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("NET WORTH", fontSize = 12.sp, color = Color.Gray, letterSpacing = 1.sp)
        Spacer(Modifier.height(4.dp))
        Text(
            ApiClient.minorToDollarsString(state.netWorthMinor),
            fontSize = 38.sp, fontWeight = FontWeight.Bold,
        )
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(24.dp)) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text("Assets", fontSize = 11.sp, color = Color.Gray)
                MoneyText(state.assetsMinor, signed = false, color = Palette.Emerald)
            }
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text("Debts", fontSize = 11.sp, color = Color.Gray)
                MoneyText(state.liabilitiesMinor, signed = false, color = Palette.Red)
            }
        }
    }
}

@Composable
private fun AccountCard(state: AppState, account: Account) {
    val scope = rememberCoroutineScope()
    Column(
        modifier = Modifier.fillMaxWidth()
            .background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp))
            .clickable { scope.launch { state.openAccount(account.id) } }
            .padding(16.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(account.name, fontWeight = FontWeight.Medium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (account.isSynced) Text("Synced", fontSize = 11.sp, color = Color.Gray)
            }
            MoneyText(
                account.currentBalanceMinor,
                signed = false,
                color = if (account.type.isLiability) Palette.Red else LocalContentColor.current,
            )
        }
    }
}

@Composable
private fun TxRow(tx: Transaction) {
    Row(modifier = Modifier.fillMaxWidth().padding(vertical = 6.dp)) {
        Column(Modifier.weight(1f)) {
            Text(tx.merchantRaw ?: tx.description ?: "Transaction", maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(tx.postedOn, fontSize = 11.sp, color = Color.Gray)
        }
        MoneyText(tx.amountMinor)
    }
}

// ------------------------------------------------------------------ detail

@Composable
fun AccountDetailScreen(state: AppState, accountId: String) {
    val scope = rememberCoroutineScope()
    val account = state.accounts.firstOrNull { it.id == accountId }

    LaunchedEffect(accountId) {
        // Refresh this account's history from the server.
        state.householdId?.let { hh ->
            runCatching {
                state.transactions = state.api.transactions(hh, limit = 100).transactions
                state.detailTransactions = state.transactions.filter { it.accountId == accountId }
            }
        }
    }

    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(16.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("← Back", modifier = Modifier.clickable { state.screen = Screen.Home }.padding(end = 16.dp))
            Text(account?.name ?: "Account", fontWeight = FontWeight.Bold, fontSize = 20.sp)
        }
        Spacer(Modifier.height(16.dp))
        Column(
            modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(16.dp)).padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text("BALANCE", fontSize = 11.sp, color = Color.Gray)
            Text(
                ApiClient.minorToDollarsString(account?.currentBalanceMinor ?: 0),
                fontSize = 32.sp, fontWeight = FontWeight.Bold,
            )
        }
        Spacer(Modifier.height(12.dp))
        if (account != null && !account.isSynced) {
            var show by remember { mutableStateOf(false) }
            Button(onClick = { show = true }, modifier = Modifier.fillMaxWidth()) { Text("Add Transaction") }
            if (show) NewTransactionDialog(state, account) { show = false }
        }
        Spacer(Modifier.height(12.dp))
        LazyColumn(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            items(state.detailTransactions, key = { it.id }) { TxRow(it) }
        }
    }
}

// -------------------------------------------------------------- new txn

@Composable
fun NewTransactionDialog(state: AppState, account: Account, onDismiss: () -> Unit) {
    val scope = rememberCoroutineScope()
    var expense by remember { mutableStateOf(true) }
    var amount by remember { mutableStateOf("") }
    var merchant by remember { mutableStateOf("") }

    val parsed = amount.replace(",", ".").toDoubleOrNull()
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("New Transaction") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(selected = expense, onClick = { expense = true }, label = { Text("Expense") })
                    FilterChip(selected = !expense, onClick = { expense = false }, label = { Text("Income") })
                }
                OutlinedTextField(
                    amount, { amount = it },
                    label = { Text("Amount ($)") },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal),
                    singleLine = true,
                )
                OutlinedTextField(merchant, { merchant = it }, label = { Text("Merchant (optional)") }, singleLine = true)
            }
        },
        confirmButton = {
            Button(
                onClick = {
                    val dollars = parsed ?: return@Button
                    var minor = (dollars * 100).toLong()
                    if (expense) minor = -minor
                    scope.launch {
                        state.addTransaction(account.id, minor, merchant.ifBlank { null })
                        onDismiss()
                    }
                },
                enabled = parsed != null && parsed != 0.0,
            ) { Text("Save") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancel") } },
    )
}

// -------------------------------------------------------------- connections

@Composable
fun ConnectionsScreen(state: AppState) {
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) {
        state.householdId?.let { hh ->
            runCatching { state.connections = state.api.connections(hh) }
        }
    }
    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(16.dp)) {
        Text("← Back", modifier = Modifier.clickable { state.screen = Screen.Home })
        Spacer(Modifier.height(12.dp))
        Text("Connections", fontWeight = FontWeight.Bold, fontSize = 20.sp)
        Spacer(Modifier.height(12.dp))

        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.weight(1f)) {
            items(state.connections, key = { it.id }) { c ->
                Row(
                    modifier = Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surfaceVariant, RoundedCornerShape(12.dp)).padding(16.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Column(Modifier.weight(1f)) {
                        Text(c.institutionName ?: c.provider, fontWeight = FontWeight.Medium)
                        Text(c.status, fontSize = 12.sp, color = Color.Gray)
                    }
                    TextButton(onClick = { scope.launch { state.syncConnection(c.id) } }) {
                        Text("Sync")
                    }
                }
            }
        }

        Button(onClick = { scope.launch { state.mockConnect() } }, enabled = !state.busy, modifier = Modifier.fillMaxWidth()) {
            if (state.busy) CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
            else Text("Add test bank")
        }
        state.error?.let {
            Spacer(Modifier.height(8.dp))
            Text(it, color = MaterialTheme.colorScheme.error, fontSize = 13.sp)
        }
    }
}

// ---------------------------------------------------------------- settings

@Composable
fun SettingsScreen(state: AppState) {
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf(state.serverUrl) }
    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).padding(16.dp)) {
        Text("← Back", modifier = Modifier.clickable { state.screen = Screen.Home })
        Spacer(Modifier.height(12.dp))
        Text("Settings", fontWeight = FontWeight.Bold, fontSize = 20.sp)
        Spacer(Modifier.height(16.dp))
        Text("Signed in as ${state.user?.email ?: "—"}", color = Color.Gray)
        Spacer(Modifier.height(16.dp))
        OutlinedTextField(url, { url = it }, label = { Text("Server URL") }, modifier = Modifier.fillMaxWidth(), singleLine = true)
        Spacer(Modifier.height(8.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(onClick = {
                state.serverUrl = url.trim()
                state.error = null
            }) { Text("Apply") }
            OutlinedButton(onClick = { scope.launch { state.logOut() } }) {
                Text("Sign Out", color = MaterialTheme.colorScheme.error)
            }
        }
    }
}

// ------------------------------------------------------------- root

@Composable
fun FinanceApp(state: AppState) {
    FinanceTheme {
        when (val s = state.screen) {
            Screen.Auth -> AuthScreen(state)
            Screen.Home -> HomeScreen(state)
            Screen.Connections -> ConnectionsScreen(state)
            Screen.Settings -> SettingsScreen(state)
            is Screen.AccountDetail -> AccountDetailScreen(state, s.accountId)
        }
    }
}
