# The Rust reference (placeholder)

What belongs here: a format-only Rust workspace — the data model
(`world.schema.json` as serde types), the fold (`fold_log`, `fold_path`,
`fold_state`), the session op vocabulary, and the `ext-physics`
reference solver. Published as `openworldformat` on crates.io, mirroring
the npm package.

Where it lives today: LocalGPT's `world-types`, `world-sync` and
`world-physics` crates (`../localgpt/crates/`), which hold the same
surfaces plus application concerns, and run this repository's
conformance (worlds and outcome assertions) in their CI. That
arrangement is why the extension registry counts two implementers.

What triggers the extraction: a Rust consumer that isn't LocalGPT —
the repo's own rule, add things when a second user needs them. Until
then, publishing the `localgpt-*` crates as the format's reference
would tie the format's release cadence to one producer's app, which is
the coupling this repository exists to prevent.

When it lands, CI grows a `rust` job: `cargo test` over
`../conformance` and `../conformance/outcomes`, the same teeth the JS
job has.
