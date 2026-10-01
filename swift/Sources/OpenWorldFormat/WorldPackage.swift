// Loading a `.world` package folder: the manifest, the log, and the
// state declaration, as one value. A zip is the transport form —
// unpack it first. Spec: spec/package.md.

import Foundation

/// A loaded package: everything a viewer or inspector needs first.
public struct WorldPackage: Sendable {
    /// The folder this package was loaded from, if any.
    public let directory: URL?
    public let manifest: WorldManifest
    /// Parsed log entries, in file order. A torn final line is skipped
    /// — the writer's crash is not the reader's (spec/session.md).
    public let entries: [LogEntry]
    public let stateDocument: StateDocument?
    /// `package.json` (the transport metadata), if present.
    public let packageJSON: JSONValue?

    /// Load a package folder: `manifest.json` (required), `ops.jsonl`
    /// (optional), `state.json` and `package.json` (optional).
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

        self.stateDocument = try text("state.json").map { try StateDocument(json: JSONValue(parsing: $0)) }
        self.packageJSON = try text("package.json").map { try JSONValue(parsing: $0) }
    }

    /// The world at head revision: the fold of the whole log.
    public func folded() throws -> FoldState {
        try foldLog(manifest, entries)
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
