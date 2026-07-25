package com.duskcue.tv.session

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import java.security.KeyStore
import java.util.UUID
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json

private const val secureSessionStoreName = "duskcue_tv_session"
private const val secureSessionKeyAlias = "duskcue.tv.session.v1"
private val Context.tvSessionDataStore: DataStore<Preferences> by preferencesDataStore(name = secureSessionStoreName)
private val encryptedSessionKey = stringPreferencesKey("encrypted_session")

@Serializable
data class SavedServer(
    val origin: String,
    val last_used_at: Long,
)

@Serializable
data class PersistedAccountSession(
    val origin: String,
    val token: String,
    val user_id: String,
    val username: String,
    val display_name: String,
    val role: String,
    val active_profile_id: String,
    val profile_selection_required: Boolean,
)

@Serializable
data class PersistedWatchNextMapping(
    val scope_hash: String,
    val platform_content_id: String,
    val media_item_id: String,
    val series_id: String? = null,
    val surface_item_id: String,
    val program_id: Long,
    val fingerprint: String,
    val source_fingerprint: String = fingerprint,
)

@Serializable
data class PersistedWatchNextSuppression(
    val scope_hash: String,
    val platform_content_id: String,
    val fingerprint: String,
)

@Serializable
data class PersistedWatchNextArtwork(
    val scope_hash: String,
    val platform_content_id: String,
    val source_hash: String,
    val cache_key: String,
    val etag: String? = null,
)

@Serializable
data class SecureTvState(
    val device_id: String,
    val known_servers: List<SavedServer> = emptyList(),
    val session: PersistedAccountSession? = null,
    val watch_next_mappings: List<PersistedWatchNextMapping> = emptyList(),
    val watch_next_suppressions: List<PersistedWatchNextSuppression> = emptyList(),
    val watch_next_artwork: List<PersistedWatchNextArtwork> = emptyList(),
    val pending_watch_next_program_ids: List<Long> = emptyList(),
)

interface TvSessionStore {
    suspend fun current(): SecureTvState
    suspend fun replace(value: SecureTvState)
}

class SecureSessionStore(
    context: Context,
    private val cipher: AndroidKeystoreCipher = AndroidKeystoreCipher(),
    private val json: Json = Json { ignoreUnknownKeys = true },
) : TvSessionStore {
    private val dataStore = context.applicationContext.tvSessionDataStore

    val state: Flow<SecureTvState> = dataStore.data.map { preferences ->
        preferences[encryptedSessionKey]
            ?.let(cipher::decryptOrNull)
            ?.let { encoded -> runCatching { json.decodeFromString<SecureTvState>(encoded) }.getOrNull() }
            ?: freshState()
    }

    override suspend fun current(): SecureTvState {
        val encrypted = dataStore.data.first()[encryptedSessionKey] ?: return freshState()
        val decoded = cipher.decryptOrNull(encrypted)?.let { value ->
            runCatching { json.decodeFromString<SecureTvState>(value) }.getOrNull()
        }
        if (decoded != null) {
            return decoded
        }
        dataStore.edit { it.remove(encryptedSessionKey) }
        return freshState()
    }

    override suspend fun replace(value: SecureTvState) {
        val encrypted = cipher.encrypt(json.encodeToString(SecureTvState.serializer(), value))
        dataStore.edit { it[encryptedSessionKey] = encrypted }
    }

    private fun freshState(): SecureTvState = SecureTvState(device_id = UUID.randomUUID().toString())
}

class AndroidKeystoreCipher(private val alias: String = secureSessionKeyAlias) {
    fun encrypt(value: String): String {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        val encrypted = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val payload = ByteArray(1 + cipher.iv.size + encrypted.size)
        payload[0] = cipher.iv.size.toByte()
        cipher.iv.copyInto(payload, destinationOffset = 1)
        encrypted.copyInto(payload, destinationOffset = 1 + cipher.iv.size)
        return Base64.encodeToString(payload, Base64.NO_WRAP)
    }

    fun decryptOrNull(value: String): String? = runCatching {
        val payload = Base64.decode(value, Base64.NO_WRAP)
        val ivLength = payload.firstOrNull()?.toInt() ?: 0
        require(ivLength in 12..32 && payload.size > ivLength + 1)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), javax.crypto.spec.GCMParameterSpec(128, payload.copyOfRange(1, 1 + ivLength)))
        cipher.doFinal(payload.copyOfRange(1 + ivLength, payload.size)).toString(Charsets.UTF_8)
    }.getOrNull()

    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        val existing = keyStore.getKey(alias, null) as? SecretKey
        if (existing != null) {
            return existing
        }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(
                alias,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setKeySize(256)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }
}
