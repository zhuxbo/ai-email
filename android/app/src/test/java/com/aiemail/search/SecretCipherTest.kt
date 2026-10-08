package com.aiemail.search

import javax.crypto.KeyGenerator
import org.junit.Assert.*
import org.junit.Test

class SecretCipherTest {
    @Test fun encryptedSettingsRoundTripWithoutPlaintextAndUseFreshNonce() {
        val cipher = SecretCipher(KeyGenerator.getInstance("AES").apply { init(256) }.generateKey())
        val secret = "readonly-credential-secret"
        val encrypted = cipher.encrypt(secret)
        assertFalse(encrypted.contains(secret))
        assertNotEquals(encrypted, cipher.encrypt(secret))
        assertEquals(secret, cipher.decrypt(encrypted))
    }
    @Test fun damagedOrWrongKeyDataIsRejected() {
        fun cipher() = SecretCipher(KeyGenerator.getInstance("AES").apply { init(256) }.generateKey())
        val first = cipher()
        val encrypted = first.encrypt("token")
        assertThrows(Exception::class.java) { cipher().decrypt(encrypted) }
        assertThrows(Exception::class.java) { first.decrypt("invalid") }
    }
}
