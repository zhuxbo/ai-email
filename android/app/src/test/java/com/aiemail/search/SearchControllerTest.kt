package com.aiemail.search

import kotlinx.coroutines.*
import kotlinx.coroutines.test.*
import org.junit.Assert.*
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SearchControllerTest {
    @Test fun oldResponseCannotOverwriteNewSearchEvenIfTransportIgnoresCancellation() = runTest {
        val api = object : FakeApi() {
            override suspend fun messages(query: String, account: String?, offset: Int): MessagePage {
                if (query == "old") withContext(NonCancellable) { delay(100) }
                return MessagePage(listOf(MailMessage(id = query, subject = query)), 1)
            }
        }
        val controller = SearchController(api, backgroundScope)
        controller.search("old", null)
        runCurrent()
        controller.search("new", "work")
        runCurrent()
        advanceTimeBy(150)
        runCurrent()
        assertEquals("new", controller.state.value.items.single().id)
        assertFalse(controller.state.value.loading)
    }
    @Test fun changingQueryResetsPaginationAndFailedPageCanRetry() = runTest {
        val requests = mutableListOf<Pair<String, Int>>()
        var failPage = true
        val api = object : FakeApi() {
            override suspend fun messages(query: String, account: String?, offset: Int): MessagePage {
                requests += query to offset
                if (offset > 0 && failPage) { failPage = false; throw ApiException("暂不可用") }
                return MessagePage(listOf(MailMessage(id = "$query-$offset")), 2)
            }
        }
        val controller = SearchController(api, backgroundScope)
        controller.search("first", null); runCurrent()
        controller.loadMore(); runCurrent()
        assertEquals(1, controller.state.value.items.size)
        assertNotNull(controller.state.value.error)
        controller.loadMore(); runCurrent()
        assertEquals(2, controller.state.value.items.size)
        controller.search("second", null); runCurrent()
        assertEquals(listOf("first" to 0, "first" to 1, "first" to 1, "second" to 0), requests)
        assertEquals("second-0", controller.state.value.items.single().id)
    }
    @Test fun editingInputCancelsPendingSearchBeforeSubmittingAgain() = runTest {
        val api = object : FakeApi() {
            override suspend fun messages(query: String, account: String?, offset: Int): MessagePage {
                withContext(NonCancellable) { delay(100) }
                return MessagePage(listOf(MailMessage(id = query)), 1)
            }
        }
        val controller = SearchController(api, backgroundScope)
        controller.search("old", null); runCurrent()
        assertTrue(controller.state.value.loading)
        controller.invalidateSearch()
        advanceTimeBy(150); runCurrent()
        assertTrue(controller.state.value.items.isEmpty())
        assertFalse(controller.state.value.loading)
    }
    @Test fun overlappingPagesDoNotDuplicateMessagesOrRepeatOffsets() = runTest {
        val offsets = mutableListOf<Int>()
        val api = object : FakeApi() {
            override suspend fun messages(query: String, account: String?, offset: Int): MessagePage {
                offsets += offset
                val id = if (offset < 2) "shared" else "last"
                return MessagePage(listOf(MailMessage(id = id)), 3)
            }
        }
        val controller = SearchController(api, backgroundScope)
        controller.search("", null); runCurrent()
        controller.loadMore(); runCurrent()
        assertEquals(listOf("shared"), controller.state.value.items.map { it.id })
        controller.loadMore(); runCurrent()
        assertEquals(listOf(0, 1, 2), offsets)
        assertEquals(listOf("shared", "last"), controller.state.value.items.map { it.id })
    }
    @Test fun cancelledDetailCannotReappearAfterClosing() = runTest {
        val api = object : FakeApi() {
            override suspend fun message(id: String): MailMessage {
                withContext(NonCancellable) { delay(100) }
                return MailMessage(id = id)
            }
        }
        val controller = SearchController(api, backgroundScope)
        controller.openMessage("old"); runCurrent()
        assertEquals("old", controller.state.value.selectedId)
        controller.closeMessage()
        advanceTimeBy(150); runCurrent()
        assertNull(controller.state.value.selectedId)
        assertNull(controller.state.value.detail)
    }
}

open class FakeApi : MailApi {
    override suspend fun accounts() = AccountResponse()
    override suspend fun messages(query: String, account: String?, offset: Int) = MessagePage()
    override suspend fun message(id: String) = MailMessage(id = id)
}
