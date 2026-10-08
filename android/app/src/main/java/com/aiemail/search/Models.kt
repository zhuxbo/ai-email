package com.aiemail.search

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable data class Account(val id: String, val email: String, @SerialName("display_name") val displayName: String = "", val folders: List<String> = emptyList())
@Serializable data class SyncStatus(@SerialName("account_id") val accountId: String, val status: String, @SerialName("last_synced_at") val lastSyncedAt: String? = null, val error: String? = null)
@Serializable data class AccountResponse(val accounts: List<Account> = emptyList(), val sync: List<SyncStatus> = emptyList(), @SerialName("can_write") val canWrite: Boolean = false)
@Serializable data class MailMessage(
    val id: String,
    @SerialName("account_id") val accountId: String = "",
    @SerialName("account_email") val accountEmail: String = "",
    val folder: String = "",
    val subject: String = "",
    @SerialName("from_address") val fromAddress: String = "",
    @SerialName("to_addresses") val toAddresses: List<String> = emptyList(),
    @SerialName("received_at") val receivedAt: String = "",
    @SerialName("body_preview") val bodyPreview: String = "",
    val flags: List<String> = emptyList(),
    @SerialName("body_text") val bodyText: String = "",
    @SerialName("cc_addresses") val ccAddresses: List<String> = emptyList(),
    @SerialName("message_id") val messageId: String? = null,
    val references: List<String> = emptyList(),
)
@Serializable data class MessagePage(val items: List<MailMessage> = emptyList(), val total: Int = 0, val limit: Int = 30, val offset: Int = 0, val sync: List<SyncStatus> = emptyList())
class ApiException(message: String) : Exception(message)
interface MailApi {
    suspend fun accounts(): AccountResponse
    suspend fun messages(query: String, account: String?, offset: Int): MessagePage
    suspend fun message(id: String): MailMessage
}
