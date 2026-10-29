package app.pluto.mobile

// The remembered server: its address in the app's private files, its token
// encrypted with an AES-GCM key the Android Keystore holds (the key never
// leaves the Keystore; uninstalling the app deletes both). The file keeps
// the Dioxus app's format, so an update stays signed in: the address, then
// the IV and the ciphertext in hex, one per line.

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class Secret(context: Context) {
    private val file = File(context.filesDir, "server")

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }

    private fun lines(): List<String>? = runCatching { file.readLines() }.getOrNull()

    // The remembered address alone, to prefill the connection card.
    fun server(): String? = lines()?.firstOrNull()?.takeIf { it.isNotBlank() }

    // The remembered address and token.
    fun load(): Pair<String, String>? = runCatching {
        val lines = lines() ?: return null
        val server = lines.getOrNull(0)?.takeIf { it.isNotBlank() } ?: return null
        val iv = lines.getOrNull(1)?.let(::unhex) ?: return null
        val sealed = lines.getOrNull(2)?.let(::unhex) ?: return null
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
        val plain = cipher.doFinal(sealed)
        val token = String(plain, Charsets.UTF_8)
        plain.fill(0)
        server to token
    }.getOrNull()

    // Saves both; the error to show when the Keystore refused.
    fun save(server: String, token: String): String? = runCatching {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.doFinal(token.toByteArray(Charsets.UTF_8))
        file.writeText("$server\n${hex(cipher.iv)}\n${hex(sealed)}\n")
        null
    }.getOrElse { it.message ?: "The token could not be saved" }

    fun forget() {
        file.delete()
    }

    // Keeps the address, drops the sealed token.
    fun forgetToken() {
        server()?.let { file.writeText("$it\n") }
    }

    private companion object {
        const val ALIAS = "pluto-server-token"

        fun hex(bytes: ByteArray) = bytes.joinToString("") { "%02x".format(it) }

        fun unhex(text: String): ByteArray? {
            val t = text.trim()
            if (t.length % 2 != 0) return null
            return runCatching { ByteArray(t.length / 2) { t.substring(it * 2, it * 2 + 2).toInt(16).toByte() } }.getOrNull()
        }
    }
}
