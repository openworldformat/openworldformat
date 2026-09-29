# Schema

`world.schema.json` is the **normative** data model of the world
document (L0): entities, shapes, materials, lights, environment, audio,
behaviors, triggers, tours, avatar. It is generated from the reference
type definitions (schemars over the reference Rust crate) — never
hand-edited. Where prose and schema disagree, the schema wins and the
prose gets fixed.

- Manifest schema version: **3**
- Draft spec: **0.1**

The session log's entry envelope and op kinds ([spec/session.md](../spec/session.md))
are specified in prose until the state document settles; the `state.json`
schema is an open item ([spec/README.md](../spec/README.md)).

## Using it

Validate a manifest before rendering:

```bash
npx ajv-cli validate -s world.schema.json -d ../conformance/shapes.json
```

License: Apache-2.0, generated from the LocalGPT reference types.
