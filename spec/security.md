# Security and Privacy Considerations

The Open World Format is code-adjacent by design. Behaviors run from load, triggers fire on `click` and `proximity`, and third-party packages ship meshes, textures, and audio by hash. Content-addressing verifies a file is the one the author meant, but not that the author meant well.

## 1. Robustness vs. Safety

The "must-ignore" rule is a robustness rule, not a safety rule. It states what a reader does with unknown content to prevent crashes on version-skew. It does *not* mitigate what untrusted content can do to a system. 

Mitigation is the loader's job. Implementations parsing third-party `.world` packages MUST defend against:

- **Resource Exhaustion:** Unbounded instance expansions (e.g., an `instance_of` pointing to a creation that recursively instances itself) or excessively deep hierarchy trees that blow the parser's stack.
- **Decompression Bombs:** Maliciously crafted glTF meshes or PNG textures designed to consume all available memory.
- **Path Traversal:** Asset references pointing to absolute paths or containing `../` sequences attempting to read outside the package's `assets/` directory.

## 2. Privacy: Input Logs are Behavioral Recordings

The session log (`ops.jsonl`) captures `input` operations natively. These operations log a named visitor's position, gaze, and clicks at ~10 Hz. 

When a save game (the base world plus a player's log) is circulated, it is not merely a record of the world's state—it is a high-fidelity behavioral recording of the player. Tools that package worlds for distribution SHOULD provide an option to strip `input` operations, unless the player explicitly intends to share a replay.

Additionally, `meta.prompt` (or its equivalent in `ext-provenance`) carries whatever the author typed to generate the world. This field is preserved into every distributed copy and may inadvertently leak personal text or sensitive generation inputs.

## 3. Authenticity and Identity

The session log tracks operations via the `author` field, but this is an unauthenticated free-form object. While the package integrity hashes (`log_sha256`) prove the file hasn't been altered since the hash was generated, they do not prove *who* wrote it. Implementations MUST NOT rely on the `author` field for security or authorization decisions unless an external cryptographic layer (e.g., signed commits, multiplayer server authority) validates the identity.

## 4. Live Authoring Surfaces

An authority that serves a local API for authors ([the package](package.md), "The live folder") opens a door into the user's machine:

- **Bind to loopback only** and **require the token** from `.live/endpoint.json` on every call. A web page in the user's browser can send requests to `localhost`; the token, readable only by processes that can read the package folder, is what keeps a page from editing the world.
- **`.live/` is never transported**: exports, zips and git exclude it. An endpoint file that travels hands its token to whoever receives the package.
- **Ingestion is a loader too.** Asset paths in a batch are checked as the package's are (no absolute paths, nothing that leaves `assets/`), with a size bound, before anything is stored.
- **An author is not authenticated by its name.** A batch's `author` is what the sender says, as everywhere in the log (section 3).
