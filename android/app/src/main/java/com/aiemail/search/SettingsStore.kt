package com.aiemail.search

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

@Serializable data class SavedSettings(val url: String = "", val token: String = "", val allowEmulatorHttp: Boolean = false)

class SettingsStore(context: Context) : PendingReplyStore {
    private val preferences = context.getSharedPreferences("connection", Context.MODE_PRIVATE)
    private fun cipher(): SecretCipher {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val key = store.getKey("mail_search_connection", null) as? SecretKey ?: KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
            init(KeyGenParameterSpec.Builder("mail_search_connection", KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setRandomizedEncryptionRequired(true).build())
        }.generateKey()
        return SecretCipher(key)
    }
    fun load(): SavedSettings? = preferences.getString("encrypted", null)?.let { Json.decodeFromString(cipher().decrypt(it)) }
    fun save(settings: SavedSettings) {
        check(preferences.edit().putString("encrypted", cipher().encrypt(Json.encodeToString(SavedSettings.serializer(), settings))).commit()) { "无法保存连接设置" }
    }
    override fun loadPending(): PendingReply? = preferences.getString("pending_reply", null)?.let { Json.decodeFromString(cipher().decrypt(it)) }
    override fun savePending(reply: PendingReply) {
        check(preferences.edit().putString("pending_reply", cipher().encrypt(Json.encodeToString(PendingReply.serializer(), reply))).commit()) { "无法保存回复记录" }
    }
    override fun clearPending() {
        check(preferences.edit().remove("pending_reply").commit()) { "无法清除回复记录" }
    }
    fun clear() {
        check(preferences.edit().remove("encrypted").commit()) { "无法清除连接设置" }
    }
}
