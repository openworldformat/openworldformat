// The shared merge corpus (conformance/merge/, spec/session.md "The
// merge rules, exactly"): every case runs — remap table, rewritten
// entries and merged head all compared against the committed expected
// results. The other four references run the same cases in their own
// suites, so the five merges cannot drift apart.

import test from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";

import { foldLog, mergeBranch, toManifest, manifestText } from "../src/index.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const caseDir = path.join(root, "conformance/merge/cases");

const cases = readdirSync(caseDir)
  .filter((f) => f.endsWith(".json"))
  .sort()
  .map((f) => JSON.parse(readFileSync(path.join(caseDir, f), "utf8")));

test("the merge corpus is present and covers the hand-written rules", () => {
  const names = new Set(cases.map((c) => c.name));
  for (const required of ["spent-id", "modify-world", "batch", "names"]) {
    assert.ok(names.has(required), `missing hand-written case ${required}`);
  }
  assert.ok(cases.length >= 100, "the corpus holds the generated cases too");
});

for (const mergeCase of cases) {
  test(`merge case ${mergeCase.name}`, () => {
    const state = foldLog(mergeCase.base, mergeCase.main);
    const { entries, remapped } = mergeBranch(state, mergeCase.branch);

    assert.deepEqual([...remapped], mergeCase.expected.remapped, "the remap table");
    assert.deepEqual(entries, mergeCase.expected.entries, "the rewritten entries");

    const head = foldLog(mergeCase.base, [...mergeCase.main, ...entries]);
    assert.equal(manifestText(toManifest(head)), mergeCase.expected.head, "the merged head, as canonical text");
  });
}
