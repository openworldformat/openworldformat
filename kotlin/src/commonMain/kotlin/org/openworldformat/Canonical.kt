// Entry identity: canonical JSON plus SHA-256, both in common code —
// no java.security, no platform digest, the same bytes everywhere the
// package runs (JVM and Android). Spec: spec/session.md "Entry
// identity, forks and branches" — implementations computing content
// hashes MUST serialize the entry canonically (no whitespace, keys
// sorted alphabetically).

package org.openworldformat

import kotlin.math.abs
import kotlin.math.floor
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

/**
 * Canonical JSON: no whitespace, object keys sorted recursively
 * (alphabetically), arrays in order, standard string escaping,
 * integral numbers without a decimal part.
 *
 * Cross-language hash equality holds for integer-valued JSON; float
 * formatting may differ between languages, so fractional numbers
 * should not be hashed across references (the format's own
 * integer-valued fields — revisions, timestamps, ids, entity counts —
 * all canonicalize identically everywhere).
 *
 * kotlinx.serialization's own `toString` is not canonical: it keeps
 * insertion order and formats numbers as parsed, so this walks the
 * [JsonElement] tree itself.
 */
fun canonicalJson(value: JsonElement): String {
    val sb = StringBuilder()
    appendCanonical(sb, value)
    return sb.toString()
}

private fun appendCanonical(sb: StringBuilder, value: JsonElement) {
    when (value) {
        is JsonObject -> {
            sb.append('{')
            var first = true
            for (key in value.keys.sorted()) {
                if (!first) sb.append(',')
                first = false
                appendString(sb, key)
                sb.append(':')
                appendCanonical(sb, value.getValue(key))
            }
            sb.append('}')
        }
        is JsonArray -> {
            sb.append('[')
            var first = true
            for (element in value) {
                if (!first) sb.append(',')
                first = false
                appendCanonical(sb, element)
            }
            sb.append(']')
        }
        is JsonNull -> sb.append("null")
        is JsonPrimitive -> appendPrimitive(sb, value)
    }
}

private fun appendPrimitive(sb: StringBuilder, p: JsonPrimitive) {
    if (p.isString) {
        appendString(sb, p.content)
        return
    }
    val content = p.content
    val d = content.toDoubleOrNull()
    // Integral numbers lose the decimal part: 7.0 → 7, 1e2 → 100 —
    // the form every reference agrees on. Anything else (or anything
    // the double can't round-trip) passes through as written.
    if (d != null && d.isFinite() && d == floor(d) && abs(d) < 9.007199254740992E15) {
        sb.append(d.toLong())
    } else {
        sb.append(content)
    }
}

/** JSON string escaping: the two mandatory escapes, the five short
 *  forms, `\u00xx` for the remaining control characters — everything
 *  else (non-ASCII included) passes through as UTF-8. */
private fun appendString(sb: StringBuilder, s: String) {
    sb.append('"')
    for (c in s) {
        when {
            c == '"' -> sb.append("\\\"")
            c == '\\' -> sb.append("\\\\")
            c == '\b' -> sb.append("\\b")
            c == '\u000C' -> sb.append("\\f")
            c == '\n' -> sb.append("\\n")
            c == '\r' -> sb.append("\\r")
            c == '\t' -> sb.append("\\t")
            c < ' ' -> {
                sb.append("\\u00")
                val hex = "0123456789abcdef"
                sb.append(hex[(c.code ushr 4) and 0xf])
                sb.append(hex[c.code and 0xf])
            }
            else -> sb.append(c)
        }
    }
    sb.append('"')
}

// ---------------------------------------------------------------------------
// SHA-256 — FIPS 180-4, hand-rolled: commonMain can't touch
// java.security.MessageDigest, and the digest is small and fixed.
// ---------------------------------------------------------------------------

/** The round constants — the first 32 bits of the fractional parts of
 *  the cube roots of the first 64 primes (FIPS 180-4 §4.2.2), held as
 *  Longs so every value reads as its published hex. */
private val SHA256_K = longArrayOf(
    0x428a2f98L, 0x71374491L, 0xb5c0fbcfL, 0xe9b5dba5L, 0x3956c25bL, 0x59f111f1L, 0x923f82a4L, 0xab1c5ed5L,
    0xd807aa98L, 0x12835b01L, 0x243185beL, 0x550c7dc3L, 0x72be5d74L, 0x80deb1feL, 0x9bdc06a7L, 0xc19bf174L,
    0xe49b69c1L, 0xefbe4786L, 0x0fc19dc6L, 0x240ca1ccL, 0x2de92c6fL, 0x4a7484aaL, 0x5cb0a9dcL, 0x76f988daL,
    0x983e5152L, 0xa831c66dL, 0xb00327c8L, 0xbf597fc7L, 0xc6e00bf3L, 0xd5a79147L, 0x06ca6351L, 0x14292967L,
    0x27b70a85L, 0x2e1b2138L, 0x4d2c6dfcL, 0x53380d13L, 0x650a7354L, 0x766a0abbL, 0x81c2c92eL, 0x92722c85L,
    0xa2bfe8a1L, 0xa81a664bL, 0xc24b8b70L, 0xc76c51a3L, 0xd192e819L, 0xd6990624L, 0xf40e3585L, 0x106aa070L,
    0x19a4c116L, 0x1e376c08L, 0x2748774cL, 0x34b0bcb5L, 0x391c0cb3L, 0x4ed8aa4aL, 0x5b9cca4fL, 0x682e6ff3L,
    0x748f82eeL, 0x78a5636fL, 0x84c87814L, 0x8cc70208L, 0x90befffaL, 0xa4506cebL, 0xbef9a3f7L, 0xc67178f2L,
).map { it.toInt() }.toIntArray()

/** SHA-256 (FIPS 180-4). Verified against the standard's test vectors. */
internal fun sha256(bytes: ByteArray): ByteArray {
    var h0 = 0x6a09e667.toInt()
    var h1 = 0xbb67ae85.toInt()
    var h2 = 0x3c6ef372.toInt()
    var h3 = 0xa54ff53a.toInt()
    var h4 = 0x510e527f.toInt()
    var h5 = 0x9b05688c.toInt()
    var h6 = 0x1f83d9ab.toInt()
    var h7 = 0x5be0cd19.toInt()

    // Padding: 0x80, zeros, then the bit length as a big-endian 64-bit.
    val bitLength = bytes.size.toLong() * 8
    val paddedSize = (bytes.size + 8) / 64 * 64 + 64
    val padded = ByteArray(paddedSize)
    bytes.copyInto(padded)
    padded[bytes.size] = 0x80.toByte()
    for (i in 0 until 8) {
        padded[paddedSize - 1 - i] = (bitLength ushr (8 * i)).toByte()
    }

    val w = IntArray(64)
    var block = 0
    while (block < paddedSize) {
        for (j in 0 until 16) {
            val base = block + j * 4
            w[j] = ((padded[base].toInt() and 0xff) shl 24) or
                ((padded[base + 1].toInt() and 0xff) shl 16) or
                ((padded[base + 2].toInt() and 0xff) shl 8) or
                (padded[base + 3].toInt() and 0xff)
        }
        for (j in 16 until 64) {
            val s0 = rotr(w[j - 15], 7) xor rotr(w[j - 15], 18) xor (w[j - 15] ushr 3)
            val s1 = rotr(w[j - 2], 17) xor rotr(w[j - 2], 19) xor (w[j - 2] ushr 10)
            w[j] = w[j - 16] + s0 + w[j - 7] + s1
        }
        var a = h0
        var b = h1
        var c = h2
        var d = h3
        var e = h4
        var f = h5
        var g = h6
        var h = h7
        for (j in 0 until 64) {
            val s1 = rotr(e, 6) xor rotr(e, 11) xor rotr(e, 25)
            val ch = (e and f) xor (e.inv() and g)
            val t1 = h + s1 + ch + SHA256_K[j] + w[j]
            val s0 = rotr(a, 2) xor rotr(a, 13) xor rotr(a, 22)
            val maj = (a and b) xor (a and c) xor (b and c)
            val t2 = s0 + maj
            h = g
            g = f
            f = e
            e = d + t1
            d = c
            c = b
            b = a
            a = t1 + t2
        }
        h0 += a
        h1 += b
        h2 += c
        h3 += d
        h4 += e
        h5 += f
        h6 += g
        h7 += h
        block += 64
    }

    val out = ByteArray(32)
    val words = intArrayOf(h0, h1, h2, h3, h4, h5, h6, h7)
    for (i in 0 until 32) {
        out[i] = ((words[i / 4] ushr (24 - 8 * (i % 4))).toByte())
    }
    return out
}

/** Rotate right — `ushr`/`shl` alone can't express it (Kotlin's shifts
 *  are mod 32), and the digest needs 6..25, never 0 or 32. */
private fun rotr(x: Int, n: Int): Int = (x ushr n) or (x shl (32 - n))

private fun hexLower(bytes: ByteArray): String {
    val hex = "0123456789abcdef"
    val sb = StringBuilder(bytes.size * 2)
    for (b in bytes) {
        val v = b.toInt() and 0xff
        sb.append(hex[(v ushr 4) and 0xf])
        sb.append(hex[v and 0xf])
    }
    return sb.toString()
}

/**
 * An entry's content id: the entry canonicalized **without its `id`
 * field** (the parent included — an id names the entry, it isn't of
 * it; absent keys stay absent), SHA-256 over the UTF-8 bytes,
 * lower-case hex, `sha256:`-prefixed. Two forks computing this on the
 * same entry agree byte-for-byte. Spec: spec/session.md.
 */
fun computeEntryId(entry: LogEntry): String {
    val hashed = buildJsonObject {
        put("revision", entry.revision)
        entry.author?.let { put("author", it) }
        entry.timestampMs?.let { put("timestamp_ms", timestampPrimitive(it)) }
        put("ops", JsonArray(entry.ops))
        entry.parent?.let { put("parent", it) }
        entry.message?.let { put("message", it) }
    }
    val digest = sha256(canonicalJson(hashed).encodeToByteArray())
    return "sha256:" + hexLower(digest)
}

/** A timestamp primitive that survives canonicalization: integral
 *  milliseconds become integers (Doubles stringify scientifically
 *  past 1e7, which would round-trip 1790000000123 into 1.79E12). */
private fun timestampPrimitive(ms: Double): JsonPrimitive =
    if (ms == floor(ms) && abs(ms) < 9.007199254740992E15) {
        JsonPrimitive(ms.toLong())
    } else {
        JsonPrimitive(ms)
    }
