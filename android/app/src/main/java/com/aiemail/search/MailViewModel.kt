package com.aiemail.search

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow

class MailViewModel(application: Application) : AndroidViewModel(application) {
    private val store = SettingsStore(application)
    private var connectionScope: CoroutineScope? = null
    val settings = MutableStateFlow(SavedSettings())
    val settingsError = MutableStateFlow<String?>(null)
    val controller = MutableStateFlow<SearchController?>(null)
    val replyController = MutableStateFlow<ReplyController?>(null)
    init {
        try { store.load()?.let { settings.value = it; connect(it) } }
        catch (_: Exception) { settingsError.value = "无法读取已保存的连接，请重新填写设置。" }
    }
    fun save(value: SavedSettings): Boolean = try {
        Connection.create(value.url, value.token, BuildConfig.DEBUG, value.allowEmulatorHttp)
        store.save(value)
        settings.value = value
        settingsError.value = null
        connect(value)
        true
    } catch (error: IllegalArgumentException) {
        settingsError.value = error.message
        false
    } catch (_: Exception) {
        settingsError.value = "无法安全保存设置，请重试。"
        false
    }
    fun disconnect() {
        try {
            store.clear()
            connectionScope?.cancel()
            controller.value = null
            replyController.value = null
            settings.value = SavedSettings()
            settingsError.value = null
        } catch (_: Exception) { settingsError.value = "无法清除设置，请重试。" }
    }
    private fun connect(value: SavedSettings) {
        val connection = Connection.create(value.url, value.token, BuildConfig.DEBUG, value.allowEmulatorHttp)
        connectionScope?.cancel()
        val scope = CoroutineScope(viewModelScope.coroutineContext + Job(viewModelScope.coroutineContext[Job]))
        connectionScope = scope
        val api = HttpMailApi(connection)
        val newController = SearchController(api, scope)
        val reply = ReplyController(api, scope, connection.url, store)
        replyController.value = reply
        reply.restore()
        controller.value = newController
        newController.refreshAccounts()
        newController.search("", null)
    }
}
