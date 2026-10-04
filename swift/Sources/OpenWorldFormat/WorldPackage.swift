// Loading a `.world` package folder: the manifest, the log, and the
// state declaration, as one value. A zip is the transport form —
// unpack it first. Spec: spec/package.md.

import Foundation

/// A loaded package: everything a viewer or inspector needs first.
public struct WorldPackage: Sendable {
    /// The folder this package was loaded from, if any.
    public let directory: URL?
    /// The world now: `manifest.json`, the state at the tip of `main`.
    /// A viewer draws this and needs nothing else.
    public let manifest: WorldManifest
    /// The state the log folds from: `snapshots/base.json`, or the head
    /// itself when the log holds no edits.
    public let base: WorldManifest
    /// Parsed log entries, in file order. A torn final line is skipped
    /// — the writer's crash is not the reader's (spec/session.md).
    public let entries: [LogEntry]
    public let stateDocument: StateDocument?
    /// `package.json` (the transport metadata), if present.
    public let packageJSON: JSONValue?

    /// Load a head-first package folder: `manifest.json` (required),
    /// `snapshots/base.json` (required when the log holds edits),
    /// `ops.jsonl`, `state.json` and `package.json` (optional).
    public init(directory: URL) throws {
        let fm = FileManager.default
        func text(_ name: String) throws -> String? {
            let url = directory.appendingPathComponent(name)
            guard fm.fileExists(atPath: url.path) else { return nil }
            return try String(contentsOf: url, encoding: .utf8)
        }

        guard let manifestText = try text("manifest.json") else {
            throw OpenWorldFormatError.parse("no manifest.json in \(directory.lastPathComponent)")
        }
        self.directory = directory
        self.manifest = try parseManifest(manifestText)

        if let logText = try text("ops.jsonl") {
            var entries: [LogEntry] = []
            let lines = logText.split(separator: "\n", omittingEmptySubsequences: true)
            for (n, line) in lines.enumerated() {
                do {
                    entries.append(try parseLogLine(String(line)))
                } catch {
                    // The last line may be torn mid-write; anything
                    // earlier is real corruption.
                    if n == lines.count - 1 { break }
                    throw error
                }
            }
            self.entries = entries
        } else {
            self.entries = []
        }

        if let baseText = try text(BASE_SNAPSHOT) {
            self.base = try parseManifest(baseText)
        } else if entries.contains(where: { !editOps($0).isEmpty }) {
            throw OpenWorldFormatError.parse(
                "the log holds edits but there is no \(BASE_SNAPSHOT) to fold them from")
        } else {
            self.base = manifest
        }

        self.stateDocument = try text("state.json").map { try StateDocument(json: JSONValue(parsing: $0)) }
        self.packageJSON = try text("package.json").map { try JSONValue(parsing: $0) }
    }

    /// The world at the tip of `main` (`refs.main`, else the last entry):
    /// the fold of its path over the base. For a consistent package this
    /// is `manifest` again — `verify()` checks that it is.
    public func folded() throws -> FoldState {
        if entries.isEmpty { return try foldLog(base, []) }
        return try foldPath(base, entries, tip: packageJSON?["refs"]?["main"]?.string)
    }

    /// The history of this package's log (tips, branches).
    public func history() throws -> History {
        try buildHistory(entries)
    }

    /// The state values at head revision, over the declaration.
    public func stateValues() -> StateFoldResult {
        foldState(stateDocument, entries)
    }
}

// MARK: - Snapshots and compaction

/// A snapshot's filename (spec/session.md, "Snapshots"):
/// `snapshots/entry-<id>.json` for a log whose entries carry ids,
/// `snapshots/rev-<N>.json` for a linear log without them. Characters
/// a filename can't hold — anything outside `[A-Za-z0-9._-]` — fold to
/// `_`. Snapshots are derived, never authoritative; this names them,
/// it doesn't write them.
public func snapshotFilename(entryId: String?, revision: Int) -> String {
    guard let entryId else { return "snapshots/rev-\(revision).json" }
    let allowed = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789._-"
    let sanitized = String(entryId.map { allowed.contains($0) ? $0 : "_" })
    return "snapshots/entry-\(sanitized).json"
}

/// Compaction's `package.json`: the same object with `base_revision` at
/// the head the folded manifest now holds (spec/session.md,
/// "Snapshots").
///
/// The routine this drives is the host app's, because it moves files:
/// copy `manifest.json` (already the head) to `snapshots/base.json`,
/// rename `ops.jsonl` to `ops.archive.jsonl` (or delete it), and start a
/// fresh empty `ops.jsonl`. This package is string-based — it returns the
/// updated JSON and moves nothing.
public func compactPackage(_ packageJson: JSONValue, headRevision: Int) -> JSONValue {
    var o = packageJson.object ?? [:]
    o["base_revision"] = .number(Double(headRevision))
    return .object(o)
}
