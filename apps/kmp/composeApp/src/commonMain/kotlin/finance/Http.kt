package finance

/** Minimal blocking HTTP result. Both targets are JVM, so actuals share one implementation. */
data class RawResponse(val status: Int, val body: String)

expect object Http {
    fun request(method: String, url: String, body: String?, headers: Map<String, String>): RawResponse
}
