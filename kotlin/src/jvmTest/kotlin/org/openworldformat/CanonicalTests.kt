// Canonical JSON, SHA-256 and entry identity — the vectors every
// reference agrees on byte-for-byte (the hash contract of
// spec/session.md "Entry identity, forks and branches").

package org.openworldformat

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals

class CanonicalTests {
    /** The cross-language golden vector: keys sorted recursively, no
     *  whitespace, arrays in order, integers without decimal part. */
    @Test
    fun canonicalJsonSortsKeysRecursivelyWithNoWhitespace() {
        val entry = """{"revision":7,"timestamp_ms":1790000000123,"author":{"peer":3,"name":"maya"},"ops":[{"SpawnEntity":{"entity":{"id":1,"name":"beacon"}}}],"parent":"e6"}"""
        assertEquals(
            """{"author":{"name":"maya","peer":3},"ops":[{"SpawnEntity":{"entity":{"id":1,"name":"beacon"}}}],"parent":"e6","revision":7,"timestamp_ms":1790000000123}""",
            canonicalJson(json(entry)),
        )
    }

    @Test
    fun canonicalJsonKeepsArrayOrderAndEscapesStrings() {
        // (Raw strings: one backslash is one backslash — the canonical
        // form escapes the quote and the newline back into the text.)
        assertEquals(
            """{"a":"quote \" and \n","b":[3,1,2],"z":{}}""",
            canonicalJson(json("""{"b": [3, 1, 2], "a": "quote \" and \n", "z": {}}""")),
        )
    }

    @Test
    fun canonicalJsonPrintsIntegralNumbersWithoutTheirDecimalPart() {
        assertEquals("""{"a":1,"b":7,"c":-3}""", canonicalJson(json("""{"a": 1.0, "b": 7, "c": -3.0}""")))
        // Fractional numbers keep their digits — and their
        // cross-language caveat (float formatting may differ).
        assertEquals("""{"x":0.5}""", canonicalJson(json("""{"x": 0.5}""")))
    }

    @Test
    fun canonicalJsonPassesNonAsciiThroughAsUtf8() {
        assertEquals("""{"名":"灯"}""", canonicalJson(json("""{"名": "灯"}""")))
    }

    /** The FIPS 180-4 test vectors — the hand-rolled digest's ground truth. */
    @Test
    fun sha256MatchesTheFipsVectors() {
        assertEquals(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            sha256Hex(""),
        )
        assertEquals(
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            sha256Hex("abc"),
        )
        assertEquals(
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
            sha256Hex("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        )
    }

    /** The golden entry id: the `id` field is excluded from the hash
     *  input (parent included), so the same entry on two forks hashes
     *  equal — whatever id each side assigns it. */
    @Test
    fun computeEntryIdHashesTheEntryWithoutItsId() {
        val line = """{"id":"e7","revision":7,"timestamp_ms":1790000000123,"author":{"peer":3,"name":"maya"},"ops":[{"SpawnEntity":{"entity":{"id":1,"name":"beacon"}}}],"parent":"e6"}"""
        assertEquals(
            "sha256:4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2",
            computeEntryId(parseLogLine(line)),
        )
        // A different id, the same content: the same hash.
        val otherId = line.replace("\"id\":\"e7\"", "\"id\":\"zzz\"")
        assertEquals(computeEntryId(parseLogLine(line)), computeEntryId(parseLogLine(otherId)))
        // And a different parent really is different content.
        val otherParent = line.replace("\"parent\":\"e6\"", "\"parent\":\"e9\"")
        assertNotEquals(computeEntryId(parseLogLine(line)), computeEntryId(parseLogLine(otherParent)))
    }

    /** Absent keys stay absent — a minimal entry hashes its minimal
     *  canonical form (`{"ops":[{"DeleteEntity":{"id":1}}],"revision":1}`,
     *  sha256 precomputed over exactly those bytes). */
    @Test
    fun computeEntryIdOmitsAbsentKeys() {
        val minimal = LogEntry(revision = 1, ops = listOf(json("""{"DeleteEntity": {"id": 1}}""")))
        assertEquals(
            "sha256:140c194f3fc6b4b6d708ec36b13a214bedc2ca130f5d6d8f0874aaff7fb4fcf4",
            computeEntryId(minimal),
        )
    }
}

/** Hex for the tests' sake — [sha256] returns bytes, hashes read hex. */
private fun sha256Hex(text: String): String {
    val digest = sha256(text.encodeToByteArray())
    return digest.joinToString("") { b -> "%02x".format(b) }
}
