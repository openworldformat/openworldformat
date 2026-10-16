# Authoring outcomes

`cases.json` is the authoring step's conformance (spec/session.md,
"Authoring"): a base world, and batches an author sends to an authority
over it, each with what must come back — whether it commits or is
refused, which names the batch spawned and the ids they got, what the
committed ops carry (names bound to ids in every marked field, merged
patches carried whole), what the world holds after, and what the receipt
warns about.

It is implementation-neutral: run each batch through your ingest and check
the predicates. `ops[].path` is a JSON pointer into the committed op as
JSON, `world[].path` one into the named entity as JSON (`absent` says the
entity must not exist); numbers compare by value. A refused batch changes
nothing, so its `world` predicates hold against the base.

Runners: the Rust crate's `rust/tests/authoring_outcomes.rs` (over
`openworldformat::authoring::ingest`) and LocalGPT's live authority
(`crates/world-agent/tests/authoring_outcomes.rs`, over a vendored copy).
The first cases are the class a cold agent run found a live authority
getting wrong — a behavior target sent by name committed as the name, and
the package stopped verifying against its own history — which no earlier
suite could have caught, since none exercised authoring.
