package com.aiemail.search

import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

data class SearchState(
    val canWrite: Boolean = false, val accounts: List<Account> = emptyList(), val sync: List<SyncStatus> = emptyList(),
    val items: List<MailMessage> = emptyList(), val total: Int = 0, val nextOffset: Int = 0,
    val query: String = "", val accountId: String? = null,
    val loading: Boolean = false, val error: String? = null, val accountsError: String? = null,
    val selectedId: String? = null, val detail: MailMessage? = null,
    val detailLoading: Boolean = false, val detailError: String? = null,
)
class SearchController(private val api: MailApi, private val scope: CoroutineScope) {
    private val mutableState = MutableStateFlow(SearchState())
    val state = mutableState.asStateFlow()
    private var queryJob: Job? = null
    private var detailJob: Job? = null
    private var accountsJob: Job? = null
    private var queryGeneration = 0L
    private var detailGeneration = 0L

    fun refreshAccounts() {
        accountsJob?.cancel()
        accountsJob = scope.launch {
            try {
                val response = api.accounts()
                mutableState.update { it.copy(accounts = response.accounts, sync = response.sync, canWrite = response.canWrite, accountsError = null) }
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) { mutableState.update { it.copy(accountsError = error.userMessage()) } }
        }
    }
    fun invalidateSearch() {
        queryGeneration++
        queryJob?.cancel()
        mutableState.update { it.copy(loading = false) }
    }
    fun search(query: String, account: String?) {
        invalidateSearch()
        mutableState.update { it.copy(query = query, accountId = account, items = emptyList(), total = 0, nextOffset = 0, error = null) }
        fetchPage(0)
    }
    fun loadMore() {
        val current = state.value
        if (!current.loading && current.nextOffset < current.total) fetchPage(current.nextOffset)
    }
    private fun fetchPage(offset: Int) {
        val generation = queryGeneration
        val current = state.value
        mutableState.update { it.copy(loading = true, error = null) }
        queryJob = scope.launch {
            try {
                val response = api.messages(current.query, current.accountId, offset)
                if (generation == queryGeneration) mutableState.update {
                    it.copy(
                        items = (if (offset == 0) response.items else it.items + response.items).distinctBy { message -> message.id },
                        total = response.total,
                        nextOffset = if (response.items.isEmpty()) response.total else offset + response.items.size,
                        sync = response.sync, loading = false,
                    )
                }
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) {
                if (generation == queryGeneration) mutableState.update { it.copy(loading = false, error = error.userMessage()) }
            }
        }
    }
    fun openMessage(id: String) {
        detailGeneration++
        val generation = detailGeneration
        detailJob?.cancel()
        mutableState.update { it.copy(selectedId = id, detail = null, detailError = null, detailLoading = true) }
        detailJob = scope.launch {
            try {
                val result = api.message(id)
                if (generation == detailGeneration) mutableState.update { it.copy(detail = result, detailLoading = false) }
            } catch (error: CancellationException) { throw error }
            catch (error: Exception) {
                if (generation == detailGeneration) mutableState.update { it.copy(detailError = error.userMessage(), detailLoading = false) }
            }
        }
    }
    fun closeMessage() {
        detailGeneration++
        detailJob?.cancel()
        mutableState.update { it.copy(selectedId = null, detail = null, detailError = null, detailLoading = false) }
    }
    private fun Exception.userMessage() = if (this is ApiException) message ?: "请求失败，请重试。" else "请求失败，请检查连接后重试。"
}
