// The authoring outcome suite (conformance/authoring/cases.json) through
// this reference's `ingest`: each batch over the base world, and what must
// come back — mirrors rust/tests/authoring_outcomes.rs, the normative
// runner. `ops[].path` is a JSON pointer into the committed op,
// `world[].path` one into the named entity; numbers compare by value.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { foldLog, ingest, parseManifest } from "../src/index.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const suite = JSON.parse(readFileSync(path.join(root, "conformance/authoring/cases.json"), "utf8"));

/** A JSON pointer into a value (`/a/0/b`), or undefined where it leads nowhere.
 * @param {any} value @param {string} pointer */
function at(value, pointer) {
  let node = value;
  for (const raw of pointer.split("/").slice(1)) {
    const key = raw.replaceAll("~1", "/").replaceAll("~0", "~");
    if (Array.isArray(node)) node = node[Number(key)];
    else if (node !== null && typeof node === "object" && key in node) node = node[key];
    else return undefined;
  }
  return node;
}

/** Numbers compare by value — the suite writes 2, a float field reads back 2.0.
 * @param {any} a @param {any} b */
function same(a, b) {
  if (typeof a === "number" && typeof b === "number") {
    return Math.abs(a - b) <= 1e-6 * Math.max(Math.abs(a), Math.abs(b), 1);
  }
  if (Array.isArray(a) && Array.isArray(b)) return a.length === b.length && a.every((x, i) => same(x, b[i]));
  if (a && b && typeof a === "object" && typeof b === "object") {
    const keys = Object.keys(a);
    return keys.length === Object.keys(b).length && keys.every((k) => k in b && same(a[k], b[k]));
  }
  return a === b;
}

/** @param {string} name @param {any} state @param {any[]} predicates */
function checkWorld(name, state, predicates) {
  for (const p of predicates) {
    const id = state.nameToId.get(p.entity);
    if (p.absent === true) {
      assert.equal(id, undefined, `${name}: entity '${p.entity}' should be absent`);
      continue;
    }
    assert.notEqual(id, undefined, `${name}: no entity '${p.entity}'`);
    const entity = state.byId.get(id);
    const got = at(entity, p.path);
    assert.notEqual(got, undefined, `${name}: '${p.entity}' has nothing at ${p.path}: ${JSON.stringify(entity)}`);
    assert.ok(same(got, p.equals), `${name}: '${p.entity}' at ${p.path} is ${JSON.stringify(got)}, expected ${JSON.stringify(p.equals)}`);
  }
}

test("the authoring outcomes hold", () => {
  const base = foldLog(parseManifest(JSON.stringify(suite.base)), []);
  assert.ok(suite.cases.length > 0);
  for (const c of suite.cases) {
    const { name, expect } = c;
    const done = ingest(base, c.batch);
    if (expect.result === "committed") {
      assert.equal(done.ok, true, `${name}: refused: ${JSON.stringify(/** @type {any} */ (done).errors)}`);
      if (!done.ok) continue;
      assert.equal(expect.errors_contain, undefined, `${name}: committed, but the suite expects an error`);
      for (const want of expect.warnings_contain ?? []) {
        assert.ok(done.warnings.some((w) => w.includes(want)), `${name}: no warning contains ${JSON.stringify(want)}: ${JSON.stringify(done.warnings)}`);
      }
      for (const [n, id] of Object.entries(expect.spawned ?? {})) {
        assert.equal(done.spawned[n], id, `${name}: spawned ${n} -> ${JSON.stringify(done.spawned)}`);
      }
      for (const p of expect.ops ?? []) {
        const op = done.ops[p.op];
        const got = at(op, p.path);
        assert.notEqual(got, undefined, `${name}: op ${p.op} has nothing at ${p.path}: ${JSON.stringify(op)}`);
        assert.ok(same(got, p.equals), `${name}: op ${p.op} at ${p.path} is ${JSON.stringify(got)}, expected ${JSON.stringify(p.equals)}`);
      }
      checkWorld(name, done.state, expect.world ?? []);
    } else {
      assert.equal(done.ok, false, `${name}: committed, expected refused`);
      if (done.ok) continue;
      for (const want of expect.errors_contain ?? []) {
        assert.ok(done.errors.some((e) => e.includes(want)), `${name}: no error contains ${JSON.stringify(want)}: ${JSON.stringify(done.errors)}`);
      }
      checkWorld(name, base, expect.world ?? []);
    }
  }
});
