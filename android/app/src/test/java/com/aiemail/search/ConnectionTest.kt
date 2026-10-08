package com.aiemail.search

import org.junit.Assert.*
import org.junit.Test

class ConnectionTest {
    @Test fun releaseRejectsCleartextEvenWhenOptedIn() {
        assertThrows(IllegalArgumentException::class.java) { Connection.create("http://10.0.2.2:8080", "token", false, true) }
    }
    @Test fun emulatorHttpRequiresDebugAndExplicitOptIn() {
        assertEquals("http://10.0.2.2:8080/", Connection.create("http://10.0.2.2:8080", "token", true, true).url)
        assertThrows(IllegalArgumentException::class.java) { Connection.create("http://10.0.2.2:8080", "token", true, false) }
        assertThrows(IllegalArgumentException::class.java) { Connection.create("http://example.com", "token", true, true) }
    }
    @Test fun rejectsEmbeddedCredentialsQueriesAndInvalidTokens() {
        listOf("https://me:pass@example.com", "https://example.com?q=secret", "https://example.com#secret").forEach {
            assertThrows(IllegalArgumentException::class.java) { Connection.create(it, "token", false, false) }
        }
        listOf("", " ", "secret\nInjected: value").forEach {
            assertThrows(IllegalArgumentException::class.java) { Connection.create("https://example.com", it, false, false) }
        }
    }
    @Test fun normalizesBasePathWithoutDroppingIt() {
        assertEquals("https://example.com/mail/", Connection.create(" https://example.com/mail ", "token", false, false).url)
    }
}
