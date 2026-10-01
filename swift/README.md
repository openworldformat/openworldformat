# openworldformat (SwiftPM)

The Open World Format's Apple-surface package. The core — parse a
`.world` manifest, fold its session log — is pure Foundation, no
dependencies, and deliberately **no renderer**: this is the format's
fourth reference implementation, the one an iOS, iPadOS, visionOS or
macOS app imports. Rendering is the app's job ([worldwalk] is the
viewer); physics is the Rust crate's job (its reference solver).

It is also the fold contract's fourth independently-built
implementation, sharing no code with the JS, Python or Rust ones —
every conformance world the other references fold, this package folds
too, with the same entity counts pinned in its tests.

[worldwalk]: https://github.com/openworldformat/worldwalk

## Add

```swift
// Package.swift
.package(url: "https://github.com/openworldformat/openworldformat", from: "0.1.0")
```

or Xcode → File → Add Package Dependencies → the same URL, then
`import OpenWorldFormat`.

## Use

```swift
import OpenWorldFormat

let package = try WorldPackage(directory: worldURL)   // a .world folder
let state = try package.folded()                      // the world at head revision
state.name                                           // "hello-world"
state.entities                                       // base + every applied edit
state.appliedEdits                                   // how many edits the log held

for entity in state.entities {
    let position = entity.transform?.position        // SIMD3<Double>
    let shape = entity.shape                         // .sphere(radius: 0.75), …
    let material = entity.material                   // color, roughness, metallic, …
}

// Branches: the same log, a different tip.
let history = try package.history()                  // tips, children
let variant = try foldPath(package.manifest, package.entries, tip: "e5")

// Save-game state: typed fields, folded over the declaration.
let stateValues = package.stateValues().values       // ["score.tour": 1]
```

Piece by piece, if a package is more than folders for you:

```swift
let manifest = try parseManifest(manifestText)
let entries = try logText.split(separator: "\n")
    .filter { !$0.isEmpty }
    .map(String.init)
    .map(parseLogLine)
let world = try foldLog(manifest, entries)           // the world at head
```

## What it carries, what it doesn't

- **Carries** — the document types (identity typed, components and
  `ext-*` fields riding untouched per must-ignore), op classification
  (edits first, the compatibility rule), the fold (all-or-nothing per
  entry and per batch), branching histories (`buildHistory`,
  `foldPath`), and the state fold (declared, dotted-map-subkey and
  undeclared keys).
- **Doesn't** — the `ext-physics` solver (the Rust crate owns the
  reference one), schema validation (the JSON Schema is normative;
  AJV in CI is the checker), and any rendering, audio or asset
  resolution. The viewer profile's must-ignore is the reader's side.

## License

Apache-2.0, like the rest of the repository.
