package com.duskcue.fire.personalization

import com.duskcue.tv.api.TvFirePlaybackAuthorization
import java.time.Instant
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.decodeFromJsonElement
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test

class ServerFirePlaybackAuthorizationSourceTest {
    private val now = 1_000L
    private fun response(): TvFirePlaybackAuthorization {
        val fixture = requireNotNull(javaClass.classLoader?.getResource("catalog-authorization.json")).readText()
        return Json.decodeFromJsonElement(Json.parseToJsonElement(fixture).jsonObject.getValue("accepted"))
    }

    @Test
    fun unknownConsentDoesNotFetchOrExposeServerKeys() = runBlocking {
        var requests = 0
        val source = ServerFirePlaybackAuthorizationSource({ false }, { requests += 1; response() }, { now })
        assertNull(source.authorize("item", "session"))
        assertEquals(0, requests)
    }

    @Test
    fun serverEligibilityCannotReplaceCustomerConsent() = runBlocking {
        var consent = true
        val source = ServerFirePlaybackAuthorizationSource({ consent }, { consent = false; response() }, { now })
        assertNull(source.authorize("item", "session"))
    }

    @Test
    fun acceptsOnlyAnEligibleCompleteCurrentServerLease() = runBlocking {
        var reply = response()
        val source = ServerFirePlaybackAuthorizationSource({ true }, { reply }, { now })
        assertNotNull(source.authorize("item", "session"))
        listOf(
            response().copy(eligible = false),
            response().copy(catalog_content_id = null),
            response().copy(opaque_profile_key = null),
            response().copy(mapping_revision = 0),
            response().copy(expires_at = "invalid"),
            response().copy(expires_at = Instant.ofEpochMilli(now).toString()),
            response().copy(expires_at = Instant.ofEpochMilli(now + 60_001).toString()),
        ).forEach {
            reply = it
            assertNull(source.authorize("item", "session"))
        }
    }
}
