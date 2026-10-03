"""The 0.2.0 spec-draft alignment, pinned: identity, immediate name
binding, the computed inverse, branch merges, packages, strict reading.

The golden vectors here are the ones every reference implementation
carries — five languages agreeing byte-for-byte on what an entry
canonicalizes to and hashes to is the cross-check that makes content
ids portable. Everything else pins the rules the spec states and the
fold must keep: names bind at ingestion (never later), undo appends
the inverse, merges rewrite colliding ids, compaction changes nothing
observable.
"""

import copy
import json
import tempfile
import unittest
from pathlib import Path

from openworldformat import (
    EDIT_KEYS,
    EXT_PROVENANCE_FIELDS,
    MAX_ENTITY_ID,
    REGISTERED_EXTENSIONS,
    WorldFormatError,
    canonical_json,
    classify_op,
    compact,
    compact_package,
    compute_entry_id,
    compute_inverse,
    ext_provenance,
    fold_log,
    fold_state,
    merge_branch,
    op_kind_shape_ok,
    parse_log_line,
    parse_manifest,
    snapshot_filename,
)

ROOT = Path(__file__).resolve().parents[2]
HELLO = ROOT / "examples" / "hello-world"


def entry_of(ops, revision=1, timestamp_ms=0):
    return parse_log_line(json.dumps({
        "revision": revision, "author": {"name": "t"}, "ops": ops,
        "timestamp_ms": timestamp_ms,
    }))


def mini_manifest():
    """One ground plane under a name — the smallest world that folds."""
    return {
        "version": 3,
        "meta": {"name": "mini"},
        "entities": [
            {"id": 1, "name": "ground", "shape": {"Plane": {"x": 10.0, "z": 10.0}}},
        ],
    }


# The golden entry: canonicalizes and hashes to what every language
# agrees on (spec/session.md "Entry identity").
GOLDEN_ENTRY = {
    "revision": 7,
    "timestamp_ms": 1790000000123,
    "author": {"peer": 3, "name": "maya"},
    "ops": [{"SpawnEntity": {"entity": {"id": 1, "name": "beacon"}}}],
    "parent": "e6",
}
GOLDEN_CANONICAL = (
    '{"author":{"name":"maya","peer":3},'
    '"ops":[{"SpawnEntity":{"entity":{"id":1,"name":"beacon"}}}],'
    '"parent":"e6","revision":7,"timestamp_ms":1790000000123}'
)
GOLDEN_ENTRY_ID = (
    "sha256:4b754af615abd0b36a11f6bec2753eedb7be6492080d8ed8ba6e50464dfaffa2"
)


class ConstantsTest(unittest.TestCase):
    def test_the_id_ceiling_is_the_json_safe_integer(self):
        self.assertEqual(MAX_ENTITY_ID, 2 ** 53 - 1)
        self.assertEqual(MAX_ENTITY_ID, 9007199254740991)

    def test_an_id_above_the_ceiling_is_refused_at_spawn(self):
        with self.assertRaisesRegex(
            WorldFormatError, "exceeds the id ceiling 9007199254740991"
        ):
            fold_log(mini_manifest(), [entry_of([
                {"SpawnEntity": {"entity": {
                    "id": MAX_ENTITY_ID + 1, "name": "too big",
                }}},
            ])])

    def test_an_id_at_the_ceiling_is_the_last_legal_id(self):
        state = fold_log(mini_manifest(), [entry_of([
            {"SpawnEntity": {"entity": {"id": MAX_ENTITY_ID, "name": "edge"}}},
        ])])
        self.assertIn(MAX_ENTITY_ID, state["by_id"])

    def test_the_registry_names_the_five_registered_extensions(self):
        self.assertEqual(REGISTERED_EXTENSIONS, (
            "ext-physics",
            "ext-strict-determinism",
            "ext-visibility",
            "ext-cinematography",
            "ext-provenance",
        ))


class CanonicalIdentityTest(unittest.TestCase):
    def test_canonical_json_sorts_keys_recursively_and_sheds_whitespace(self):
        self.assertEqual(canonical_json(GOLDEN_ENTRY), GOLDEN_CANONICAL)
        self.assertEqual(
            canonical_json({"b": 1, "a": [2, {"d": 3, "c": 4}]}),
            '{"a":[2,{"c":4,"d":3}],"b":1}',
        )

    def test_compute_entry_id_matches_the_golden_digest(self):
        self.assertEqual(compute_entry_id(GOLDEN_ENTRY), GOLDEN_ENTRY_ID)

    def test_the_entry_id_excludes_the_id_and_hashes_the_rest(self):
        one = dict(GOLDEN_ENTRY, id="sha256:aaaa")
        two = dict(GOLDEN_ENTRY, id="sha256:bbbb")
        self.assertEqual(compute_entry_id(one), compute_entry_id(two))
        different = copy.deepcopy(GOLDEN_ENTRY)
        different["ops"][0]["SpawnEntity"]["entity"]["name"] = "beacon2"
        self.assertNotEqual(
            compute_entry_id(different), compute_entry_id(GOLDEN_ENTRY)
        )

    def test_a_parsed_line_hashes_to_the_same_id(self):
        # The reader's ``classified`` annotation is not content: a line
        # hashed after parse_log_line names the same entry as the bare
        # dict hashed before it.
        line = json.dumps(dict(GOLDEN_ENTRY, id="line-id"))
        self.assertEqual(
            compute_entry_id(parse_log_line(line)),
            compute_entry_id(GOLDEN_ENTRY),
        )

    def test_compute_entry_id_leaves_the_entry_untouched(self):
        before = copy.deepcopy(GOLDEN_ENTRY)
        compute_entry_id(GOLDEN_ENTRY)
        self.assertEqual(GOLDEN_ENTRY, before)


class ShapeCollisionTest(unittest.TestCase):
    def test_every_edit_kind_is_pascalcase(self):
        for kind in EDIT_KEYS:
            self.assertTrue(kind[:1].isupper(), kind)
            self.assertTrue(op_kind_shape_ok(kind), kind)

    def test_every_history_kind_is_lowercase_and_shape_ok(self):
        for kind in ("tool", "input", "state", "clock", "merge"):
            self.assertEqual(kind, kind.lower())
            self.assertTrue(op_kind_shape_ok(kind), kind)

    def test_a_lowercased_edit_spelling_is_neither_edit_nor_ok(self):
        self.assertFalse(op_kind_shape_ok("spawnentity"))
        # And the classifier agrees by shape: edits are recognized
        # PascalCase-first, so a lowercase impostor is just unknown.
        self.assertEqual(classify_op({"spawnentity": {}})["kind"], "unknown")

    def test_extension_names_sit_outside_the_collision_rule(self):
        self.assertFalse(op_kind_shape_ok("ext-physics"))


class NameBindingTest(unittest.TestCase):
    def test_a_manifest_written_by_name_binds_against_the_complete_base(self):
        manifest = {
            "version": 3,
            "meta": {"name": "orbits"},
            "entities": [
                # Forward reference: the orbiter is declared before the
                # hub it circles — the base binds as a whole.
                {"id": 4, "name": "orbiter", "behaviors": [
                    {"Orbit": {"center": "hub", "radius": 3.0}},
                ]},
                {"id": 3, "name": "hub"},
                {"id": 8, "name": "watcher", "behaviors": [
                    {"LookAt": {"target": "orbiter"}},
                ]},
            ],
        }
        state = fold_log(manifest, [])
        orbiter = state["by_id"][4]
        watcher = state["by_id"][8]
        self.assertEqual(orbiter["behaviors"][0]["Orbit"]["center"], 3)
        self.assertEqual(watcher["behaviors"][0]["LookAt"]["target"], 4)

    def test_names_bind_at_ingestion_and_survive_a_later_rename(self):
        base = mini_manifest()
        state = fold_log(base, [
            entry_of([{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]),
            entry_of([{"SpawnEntity": {"entity": {
                "id": 901, "name": "satellite", "behaviors": [
                    {"Orbit": {"center": "a", "radius": 1.5}},
                ],
            }}}], revision=2, timestamp_ms=1),
            entry_of([{"ModifyEntity": {
                "id": 900, "patch": {"name": "b"},
            }}], revision=3, timestamp_ms=2),
        ])
        satellite = state["by_id"][901]
        # Bound to the id when entry 2 was ingested; the rename in
        # entry 3 can't break what entry 2 already resolved.
        self.assertEqual(satellite["behaviors"][0]["Orbit"]["center"], 900)

    def test_a_name_freed_by_rename_no_longer_binds(self):
        # The same sequence, read the other way: after the rename, "a"
        # names nothing — a reference written then is a refusal, which
        # is exactly the determinism delayed resolution would lose.
        with self.assertRaisesRegex(WorldFormatError, "no entity named 'a'"):
            fold_log(mini_manifest(), [
                entry_of([{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]),
                entry_of([{"ModifyEntity": {
                    "id": 900, "patch": {"name": "b"},
                }}], revision=2, timestamp_ms=1),
                entry_of([{"SpawnEntity": {"entity": {
                    "id": 901, "name": "satellite", "behaviors": [
                        {"Orbit": {"center": "a", "radius": 1.5}},
                    ],
                }}}], revision=3, timestamp_ms=2),
            ])

    def test_an_unknown_name_at_ingestion_fails_the_entry(self):
        with self.assertRaisesRegex(WorldFormatError, "no entity named 'ghost'"):
            fold_log(mini_manifest(), [entry_of([
                {"SpawnEntity": {"entity": {
                    "id": 901, "name": "satellite", "behaviors": [
                        {"LookAt": {"target": "ghost"}},
                    ],
                }}},
            ])])

    def test_a_name_freed_by_delete_no_longer_binds(self):
        with self.assertRaisesRegex(WorldFormatError, "no entity named 'a'"):
            fold_log(mini_manifest(), [
                entry_of([{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]),
                entry_of([{"DeleteEntity": {"id": 900}}], revision=2, timestamp_ms=1),
                entry_of([{"SpawnEntity": {"entity": {
                    "id": 901, "name": "satellite", "behaviors": [
                        {"Orbit": {"center": "a", "radius": 1.5}},
                    ],
                }}}], revision=3, timestamp_ms=2),
            ])

    def test_a_name_spawned_and_referenced_within_one_entry_binds(self):
        # An entry is atomic: the name exists by the time the entry's
        # references bind, even though both ride the same entry.
        state = fold_log(mini_manifest(), [entry_of([
            {"SpawnEntity": {"entity": {"id": 900, "name": "a"}}},
            {"SpawnEntity": {"entity": {
                "id": 901, "name": "satellite", "behaviors": [
                    {"Orbit": {"center": "a", "radius": 1.5}},
                ],
            }}},
        ])])
        satellite = state["by_id"][901]
        self.assertEqual(satellite["behaviors"][0]["Orbit"]["center"], 900)

    def test_modulation_targets_name_properties_and_are_never_bound(self):
        manifest = mini_manifest()
        manifest["entities"][0]["modulations"] = [
            {"target": "material.emissive", "signal": "energy", "amount": 0.5},
        ]
        state = fold_log(manifest, [])
        ground = state["by_id"][1]
        self.assertEqual(
            ground["modulations"][0]["target"], "material.emissive"
        )

    def test_a_modify_that_rewrites_behaviors_binds_the_new_names(self):
        state = fold_log(mini_manifest(), [
            entry_of([{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]),
            entry_of([{"SpawnEntity": {"entity": {"id": 901, "name": "s"}}}]),
            entry_of([{"ModifyEntity": {"id": 901, "patch": {
                "behaviors": [{"LookAt": {"target": "a"}}],
            }}}], revision=3, timestamp_ms=2),
        ])
        self.assertEqual(
            state["by_id"][901]["behaviors"][0]["LookAt"]["target"], 900
        )

    def test_folding_binds_its_own_copies_and_leaves_the_entries_alone(self):
        entries = [
            entry_of([{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]),
            entry_of([{"SpawnEntity": {"entity": {
                "id": 901, "name": "satellite", "behaviors": [
                    {"Orbit": {"center": "a", "radius": 1.5}},
                ],
            }}}], revision=2, timestamp_ms=1),
        ]
        before = copy.deepcopy(entries)
        state = fold_log(mini_manifest(), entries)
        # The binding wrote the document's entity, not the writer's op —
        # the same entries fold the same over any base.
        self.assertEqual(entries, before)
        self.assertEqual(
            state["by_id"][901]["behaviors"][0]["Orbit"]["center"], 900
        )


class StatePrecedenceTest(unittest.TestCase):
    def test_an_exact_declaration_wins_over_a_map_subkey(self):
        # Declared both ways, the exact key takes precedence — always
        # (spec/state.md): "inventory.rope" the field is set, and the
        # map "inventory" never sees a "rope" entry.
        state_doc = {
            "format_version": 1,
            "fields": {
                "inventory": {"type": "map", "initial": {}},
                "inventory.rope": {"type": "int", "initial": 0},
            },
        }
        folded = fold_state(state_doc, [entry_of([
            {"state": {"inventory.rope": 5}},
        ])])
        self.assertEqual(folded["values"]["inventory.rope"], 5)
        self.assertEqual(folded["values"]["inventory"], {})


class ModifyPatchTest(unittest.TestCase):
    def test_null_clears_a_field_absent_leaves_it_and_empty_changes_nothing(self):
        manifest = mini_manifest()
        manifest["entities"][0]["light"] = {
            "light_type": "point", "intensity": 40.0,
        }
        state = fold_log(manifest, [
            entry_of([{"ModifyEntity": {"id": 1, "patch": {"light": None}}}]),
            entry_of([{"ModifyEntity": {"id": 1, "patch": {}}}], revision=2),
        ])
        ground = state["by_id"][1]
        self.assertNotIn("light", ground)  # null = clear
        self.assertIn("shape", ground)  # absent = unchanged
        # And the empty patch changed nothing at all:
        emptied = copy.deepcopy(ground)
        state2 = fold_log(manifest, [
            entry_of([{"ModifyEntity": {"id": 1, "patch": {"light": None}}}]),
            entry_of([{"ModifyEntity": {"id": 1, "patch": {}}}], revision=2),
            entry_of([{"ModifyEntity": {"id": 1, "patch": {}}}], revision=3),
        ])
        self.assertEqual(state2["by_id"][1], emptied)


class ComputeInverseTest(unittest.TestCase):
    def test_deleting_a_tree_inverses_to_a_batch_of_spawns_parents_first(self):
        base = mini_manifest()
        spawn_entry = entry_of([
            {"SpawnEntity": {"entity": {
                "id": 500, "name": "tower",
                "transform": {"position": [1.0, 0.0, 0.0]},
                "shape": {"Cuboid": {"x": 1.0, "y": 4.0, "z": 1.0}},
            }}},
            {"SpawnEntity": {"entity": {
                "id": 501, "name": "lamp", "parent": 500,
                "light": {"light_type": "point", "intensity": 60.0},
            }}},
        ])
        state = fold_log(base, [spawn_entry])
        op = {"DeleteEntity": {"id": 500}}
        inverse = compute_inverse(op, state)
        tower = state["by_id"][500]
        lamp = state["by_id"][501]
        self.assertEqual(inverse, {"Batch": {"ops": [
            {"SpawnEntity": {"entity": copy.deepcopy(tower)}},
            {"SpawnEntity": {"entity": copy.deepcopy(lamp)}},
        ]}})
        # The parent is restored before the child that hangs off it.
        self.assertEqual(
            inverse["Batch"]["ops"][0]["SpawnEntity"]["entity"]["id"], 500
        )
        # And appending the inverse really does undo: the tree returns,
        # deep-equal, without the log ever rewinding.
        undone = fold_log(base, [
            spawn_entry,
            entry_of([op], revision=2, timestamp_ms=1),
            entry_of([inverse], revision=3, timestamp_ms=2),
        ])
        self.assertEqual(undone["by_id"][500], tower)
        self.assertEqual(undone["by_id"][501], lamp)

    def test_a_spawn_inverses_to_its_delete(self):
        state = fold_log(mini_manifest(), [])
        inverse = compute_inverse(
            {"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}, state
        )
        self.assertEqual(inverse, {"DeleteEntity": {"id": 900}})

    def test_a_modify_inverses_to_the_current_values_or_clearing_nones(self):
        manifest = mini_manifest()
        state = fold_log(manifest, [])
        inverse = compute_inverse({"ModifyEntity": {
            "id": 1,
            "patch": {"light": {"light_type": "point"}, "name": "floor",
                      "parent": 2},
        }}, state)
        # The entity has a name but no light: the inverse restores the
        # name and clears the light back off; parent included.
        self.assertEqual(inverse, {"ModifyEntity": {
            "id": 1, "patch": {"light": None, "name": "ground", "parent": None},
        }})

    def test_scene_setters_inverse_to_the_previous_scene_or_the_defaults(self):
        manifest = mini_manifest()
        manifest["environment"] = {"ambient_intensity": 300.0}
        manifest["camera"] = {"position": [0.0, 6.0, 18.0], "look_at": [0, 0, 0],
                              "fov_degrees": 50.0}
        manifest["ambience"] = [{"kind": "wind", "intensity": 0.4}]
        state = fold_log(manifest, [])
        self.assertEqual(compute_inverse(
            {"SetEnvironment": {"env": {}}}, state),
            {"SetEnvironment": {"env": {"ambient_intensity": 300.0}}},
        )
        self.assertEqual(compute_inverse(
            {"SetCamera": {"camera": {}}}, state),
            {"SetCamera": {"camera": manifest["camera"]}},
        )
        self.assertEqual(compute_inverse(
            {"SetAmbience": {"ambience": []}}, state),
            {"SetAmbience": {"ambience": [{"kind": "wind", "intensity": 0.4}]}},
        )

    def test_scene_setters_on_a_bare_world_inverse_to_the_format_defaults(self):
        state = fold_log(mini_manifest(), [])
        self.assertEqual(compute_inverse(
            {"SetEnvironment": {"env": {}}}, state),
            {"SetEnvironment": {"env": {}}},
        )
        self.assertEqual(compute_inverse(
            {"SetCamera": {"camera": {}}}, state),
            {"SetCamera": {"camera": {
                "position": [5, 5, 5], "look_at": [0, 0, 0],
                "fov_degrees": 45,
            }}},
        )
        self.assertEqual(compute_inverse(
            {"SetAmbience": {"ambience": []}}, state),
            {"SetAmbience": {"ambience": []}},
        )

    def test_audio_emitters_inverse_symmetrically(self):
        spawn = {"SpawnAudioEmitter": {
            "name": "wind", "audio": {"kind": "ambience", "volume": 0.5},
        }}
        remove = {"RemoveAudioEmitter": {"name": "wind"}}
        empty = fold_log(mini_manifest(), [])
        self.assertEqual(compute_inverse(spawn, empty),
                         {"RemoveAudioEmitter": {"name": "wind"}})
        state = fold_log(mini_manifest(), [entry_of([spawn])])
        self.assertEqual(compute_inverse(remove, state), spawn)

    def test_a_batch_inverses_in_reverse_each_against_its_own_moment(self):
        base = mini_manifest()
        batch = {"Batch": {"ops": [
            {"SpawnEntity": {"entity": {
                "id": 300, "name": "x",
                "transform": {"position": [2.0, 0.0, 0.0]},
            }}},
            {"ModifyEntity": {"id": 300, "patch": {
                "transform": {"position": [3.0, 0.0, 0.0]},
            }}},
        ]}}
        state = fold_log(base, [])
        inverse = compute_inverse(batch, state)
        # The modify's inverse was computed against the batch's own
        # trial — x as spawned, transform and all — then the spawns'
        # deletes come last-in, first-out.
        self.assertEqual(inverse, {"Batch": {"ops": [
            {"ModifyEntity": {"id": 300, "patch": {
                "transform": {"position": [2.0, 0.0, 0.0]},
            }}},
            {"DeleteEntity": {"id": 300}},
        ]}})
        undone = fold_log(base, [
            entry_of([batch]),
            entry_of([inverse], revision=2, timestamp_ms=1),
        ])
        self.assertNotIn(300, undone["by_id"])
        self.assertEqual(len(undone["entities"]), len(state["entities"]))

    def test_only_edits_invert_and_only_present_entities(self):
        state = fold_log(mini_manifest(), [])
        with self.assertRaisesRegex(WorldFormatError, "no inverse"):
            compute_inverse({"state": {"score.x": 1}}, state)
        with self.assertRaisesRegex(WorldFormatError, "no inverse"):
            compute_inverse({"TeleportEntity": {"id": 1}}, state)
        with self.assertRaisesRegex(WorldFormatError, "no entity 999"):
            compute_inverse({"DeleteEntity": {"id": 999}}, state)
        with self.assertRaisesRegex(WorldFormatError, "no entity 999"):
            compute_inverse({"ModifyEntity": {"id": 999, "patch": {}}}, state)


class MergeBranchTest(unittest.TestCase):
    def test_colliding_ids_are_reallocated_and_every_reference_follows(self):
        base = {
            "version": 3,
            "meta": {"name": "main"},
            "entities": [{"id": 5, "name": "anchor"}],
        }
        main_state = fold_log(base, [])
        branch = [
            entry_of([{"SpawnEntity": {"entity": {"id": 5, "name": "beacon"}}}],
                     revision=2),
            entry_of([{"SpawnEntity": {"entity": {
                "id": 6, "name": "child", "parent": 5,
                "behaviors": [{"Orbit": {"center": 5, "radius": 2.0}}],
            }}}], revision=3, timestamp_ms=1),
        ]
        merged = merge_branch(main_state, branch)
        # Main holds 5; the branch's 5 moves past everything either
        # side spawned — 6 is taken by the branch's own child.
        self.assertEqual(merged["remapped"], {5: 7})
        entries = merged["entries"]
        self.assertEqual(
            entries[0]["ops"][0]["SpawnEntity"]["entity"]["id"], 7
        )
        child = entries[1]["ops"][0]["SpawnEntity"]["entity"]
        self.assertEqual(child["parent"], 7)
        self.assertEqual(child["behaviors"][0]["Orbit"]["center"], 7)
        # And main + the rewritten branch folds clean:
        state = fold_log(base, entries)
        self.assertIn(7, state["by_id"])
        self.assertEqual(state["by_id"][6]["parent"], 7)

    def test_a_merge_without_collisions_changes_nothing_but_the_copy(self):
        branch = [
            entry_of([{"SpawnEntity": {"entity": {"id": 900, "name": "a"}}}]),
            entry_of([{"tool": "gen_spawn", "args": {"name": "a"}}],
                     revision=2, timestamp_ms=1),
        ]
        merged = merge_branch(fold_log(mini_manifest(), []), branch)
        self.assertEqual(merged["remapped"], {})
        self.assertEqual(merged["entries"], branch)

    def test_by_name_references_and_history_ops_pass_through_untouched(self):
        base = {
            "version": 3,
            "meta": {"name": "main"},
            "entities": [{"id": 5, "name": "anchor"}],
        }
        main_state = fold_log(base, [])
        branch = [
            entry_of([
                {"SpawnEntity": {"entity": {"id": 5, "name": "beacon"}}},
                {"SpawnEntity": {"entity": {
                    "id": 6, "name": "child",
                    "behaviors": [{"LookAt": {"target": "anchor"}}],
                }}},
                {"tool": "x", "args": {"remap-me": 5}},
            ], revision=2),
        ]
        merged = merge_branch(main_state, branch)
        ops = merged["entries"][0]["ops"]
        # The string reference is left for ingestion to bind — to the
        # trunk's anchor, not to any rewritten id.
        self.assertEqual(ops[1]["SpawnEntity"]["entity"]["behaviors"][0]
                         ["LookAt"]["target"], "anchor")
        self.assertEqual(ops[2], {"tool": "x", "args": {"remap-me": 5}})
        state = fold_log(base, merged["entries"])
        self.assertEqual(
            state["by_id"][6]["behaviors"][0]["LookAt"]["target"], 5
        )

    def test_fresh_ids_skip_what_the_branch_itself_spawned(self):
        base = {
            "version": 3,
            "meta": {"name": "main"},
            "entities": [{"id": 2, "name": "anchor"}],
        }
        branch = [
            entry_of([
                {"SpawnEntity": {"entity": {"id": 2, "name": "trunkish"}}},
                {"SpawnEntity": {"entity": {"id": 3, "name": "keeps-id"}}},
            ], revision=2),
        ]
        merged = merge_branch(fold_log(base, []), branch)
        # 3 is the branch's own, uncollided — the reallocation steps
        # over it rather than onto it.
        self.assertEqual(merged["remapped"], {2: 4})
        ops = merged["entries"][0]["ops"]
        self.assertEqual(ops[0]["SpawnEntity"]["entity"]["id"], 4)
        self.assertEqual(ops[1]["SpawnEntity"]["entity"]["id"], 3)
        self.assertIn(3, fold_log(base, merged["entries"])["by_id"])


class PackageHelpersTest(unittest.TestCase):
    def test_snapshot_filenames_follow_the_entry_else_the_revision(self):
        self.assertEqual(snapshot_filename("e3", 7), "snapshots/entry-e3.json")
        self.assertEqual(snapshot_filename(None, 12), "snapshots/rev-12.json")
        # A content id sanitizes down to a plain filename.
        self.assertEqual(
            snapshot_filename("sha256:ab/cd ef", 1),
            "snapshots/entry-sha256_ab_cd_ef.json",
        )

    def test_compact_package_moves_base_revision_and_touches_nothing_else(self):
        original = {
            "format_version": 1, "name": "w", "base_revision": 0,
            "head_revision": 4,
        }
        updated = compact_package(original, 4)
        self.assertEqual(updated["base_revision"], 4)
        self.assertEqual(original["base_revision"], 0)  # the original is left be
        self.assertEqual(updated["name"], "w")
        self.assertEqual(updated["head_revision"], 4)


def build_world():
    """A tiny recorded package: a base, four revisions — one spawn, one
    environment change, one history-only entry, one more spawn."""
    manifest = {
        "version": 3,
        "meta": {"name": "tiny", "tags": ["test"]},
        "camera": {"position": [0.0, 3.0, 8.0], "look_at": [0, 0, 0],
                   "fov_degrees": 60.0},
        "entities": [
            {"id": 1, "name": "ground", "shape": {"Plane": {"x": 10.0, "z": 10.0}}},
        ],
    }
    env = {"background_color": [0.1, 0.1, 0.2, 1.0], "ambient_intensity": 100.0}
    raw_entries = [
        {"revision": 1, "timestamp_ms": 10, "author": {"name": "t"}, "ops": [
            {"SpawnEntity": {"entity": {
                "id": 30, "name": "cabin",
                "shape": {"Cuboid": {"x": 2.0, "y": 2.0, "z": 2.0}},
            }}},
        ]},
        {"revision": 2, "timestamp_ms": 20, "author": {"name": "t"}, "ops": [
            {"SetEnvironment": {"env": env}},
        ]},
        {"revision": 2, "timestamp_ms": 30, "ops": [
            {"clock": {"playing": True, "position_s": 4.5}},
        ]},
        {"revision": 3, "timestamp_ms": 40, "author": {"name": "t"}, "ops": [
            {"SpawnEntity": {"entity": {"id": 31, "name": "shed"}}},
        ]},
    ]
    return manifest, env, raw_entries


class CompactTest(unittest.TestCase):
    def test_compaction_changes_nothing_observable_and_truncates_replay(self):
        manifest, env, raw_entries = build_world()
        with tempfile.TemporaryDirectory() as tmp:
            world = Path(tmp) / "tiny.world"
            world.mkdir()
            (world / "manifest.json").write_text(json.dumps(manifest))
            log_text = "\n".join(json.dumps(e) for e in raw_entries) + "\n"
            (world / "ops.jsonl").write_text(log_text)
            (world / "package.json").write_text(json.dumps({
                "format_version": 1, "name": "tiny",
                "base_revision": 0, "head_revision": 3,
            }))
            parsed = [parse_log_line(json.dumps(e)) for e in raw_entries]
            before = fold_log(parse_manifest(json.dumps(manifest)), parsed)

            result = compact(world)
            self.assertEqual(result, world)

            # The log is archived whole, and the fresh one is empty.
            self.assertEqual((world / "ops.archive.jsonl").read_text(), log_text)
            self.assertEqual((world / "ops.jsonl").read_text(), "")

            # base_revision moved to the head; the rest survived.
            package = json.loads((world / "package.json").read_text())
            self.assertEqual(package["base_revision"], 3)
            self.assertEqual(package["format_version"], 1)
            self.assertEqual(package["name"], "tiny")

            # The new manifest is the folded document: same entities,
            # same scene, next id past the highest.
            new_manifest = parse_manifest(
                (world / "manifest.json").read_text()
            )
            self.assertEqual(
                sorted(new_manifest["entities"], key=lambda e: e["id"]),
                sorted(before["entities"], key=lambda e: e["id"]),
            )
            self.assertEqual(new_manifest["environment"], env)  # the folded one
            self.assertEqual(new_manifest["camera"], manifest["camera"])
            self.assertEqual(new_manifest["meta"]["tags"], ["test"])
            self.assertEqual(new_manifest["next_entity_id"], 32)
            # And folding it with no log reaches the same world — the
            # compaction contract, held.
            after = fold_log(new_manifest, [])
            self.assertEqual(
                sorted(after["entities"], key=lambda e: e["id"]),
                sorted(before["entities"], key=lambda e: e["id"]),
            )
            self.assertEqual(after["environment"], env)

    def test_compaction_to_a_revision_folds_only_that_far(self):
        manifest, env, raw_entries = build_world()
        with tempfile.TemporaryDirectory() as tmp:
            world = Path(tmp) / "tiny.world"
            world.mkdir()
            (world / "manifest.json").write_text(json.dumps(manifest))
            (world / "ops.jsonl").write_text(
                "\n".join(json.dumps(e) for e in raw_entries) + "\n"
            )
            compact(world, head_revision=2)
            package = json.loads((world / "package.json").read_text())
            self.assertEqual(package["base_revision"], 2)
            new_manifest = parse_manifest(
                (world / "manifest.json").read_text()
            )
            names = {e["name"] for e in new_manifest["entities"]}
            self.assertIn("cabin", names)  # revision 1
            self.assertNotIn("shed", names)  # revision 3 never folded in
            self.assertEqual(new_manifest["environment"], env)  # revision 2

    def test_compaction_creates_a_minimal_package_json_when_absent(self):
        manifest, _env, raw_entries = build_world()
        with tempfile.TemporaryDirectory() as tmp:
            world = Path(tmp) / "tiny.world"
            world.mkdir()
            (world / "manifest.json").write_text(json.dumps(manifest))
            (world / "ops.jsonl").write_text(
                "\n".join(json.dumps(e) for e in raw_entries) + "\n"
            )
            compact(world)
            package = json.loads((world / "package.json").read_text())
            self.assertEqual(package, {"format_version": 1, "base_revision": 3})

    def test_compaction_of_a_package_with_no_log_is_a_no_op_fold(self):
        manifest, _env, _raw = build_world()
        with tempfile.TemporaryDirectory() as tmp:
            world = Path(tmp) / "tiny.world"
            world.mkdir()
            (world / "manifest.json").write_text(json.dumps(manifest))
            compact(world)
            package = json.loads((world / "package.json").read_text())
            self.assertEqual(package, {"format_version": 1, "base_revision": 0})
            self.assertEqual((world / "ops.jsonl").read_text(), "")
            self.assertFalse((world / "ops.archive.jsonl").exists())


class StrictModeTest(unittest.TestCase):
    def test_the_example_manifest_passes_strict_reading(self):
        text = (HELLO / "manifest.json").read_text()
        manifest = parse_manifest(text, strict=True)
        self.assertGreater(len(manifest["entities"]), 0)

    def test_strict_reading_accepts_registered_extensions_everywhere(self):
        text = json.dumps({
            "version": 3,
            "meta": {"name": "w", "ext-provenance": {"prompt": "a cabin"}},
            "entities": [
                {"id": 1, "name": "ground", "ext-physics": {"body": {}}},
            ],
            "ext-physics": {"gravity": [0.0, -9.8, 0.0]},
        })
        manifest = parse_manifest(text, strict=True)
        self.assertEqual(manifest["meta"]["ext-provenance"]["prompt"], "a cabin")

    def test_strict_reading_refuses_keys_the_schema_doesnt_hold(self):
        base = {
            "version": 3, "meta": {"name": "w"},
            "entities": [{"id": 1, "name": "ground"}],
        }
        with self.assertRaisesRegex(WorldFormatError, "isn't in the schema"):
            parse_manifest(json.dumps({**base, "physics": {}}), strict=True)
        with self.assertRaisesRegex(WorldFormatError, "entity 1"):
            parse_manifest(json.dumps({
                **base,
                "entities": [{"id": 1, "name": "ground", "velocity": [0, 0, 0]}],
            }), strict=True)

    def test_strict_reading_names_unregistered_extensions_for_what_they_are(self):
        base = {
            "version": 3, "meta": {"name": "w"},
            "entities": [{"id": 1, "name": "ground"}],
        }
        with self.assertRaisesRegex(WorldFormatError, "extension registry"):
            parse_manifest(
                json.dumps({**base, "ext-teleport": {}}), strict=True
            )
        with self.assertRaisesRegex(WorldFormatError, "extension registry"):
            parse_manifest(json.dumps({
                **base,
                "meta": {"name": "w", "ext-hindsight": {}},
            }), strict=True)
        with self.assertRaisesRegex(WorldFormatError, "extension registry"):
            parse_manifest(json.dumps({
                **base,
                "entities": [{"id": 1, "name": "ground", "ext-teleport": {}}],
            }), strict=True)

    def test_the_legacy_provenance_fields_are_pointed_at_their_extension(self):
        for field in EXT_PROVENANCE_FIELDS:
            with self.assertRaisesRegex(WorldFormatError, "ext-provenance"):
                parse_manifest(json.dumps({
                    "version": 3,
                    "meta": {"name": "w", field: "x"},
                    "entities": [{"id": 1, "name": "ground"}],
                }), strict=True)
        with self.assertRaisesRegex(WorldFormatError, "moved"):
            parse_manifest(json.dumps({
                "version": 3,
                "meta": {"name": "w", "biome": "alpine"},
                "entities": [{"id": 1, "name": "ground"}],
            }), strict=True)

    def test_non_strict_reading_tolerates_exactly_as_it_always_did(self):
        text = json.dumps({
            "version": 3,
            "meta": {"name": "w", "prompt": "a cabin", "ext-hindsight": {}},
            "entities": [{"id": 1, "name": "ground", "mystery": True}],
            "ext-teleport": {},
        })
        manifest = parse_manifest(text)  # must-ignore, and it still folds
        self.assertEqual(manifest["meta"]["prompt"], "a cabin")
        fold_log(manifest, [])

    def test_strict_log_lines_refuse_unknown_shapes_and_unregistered_extensions(self):
        good = json.dumps({
            "revision": 1, "author": {"name": "t"},
            "ops": [{"ext-physics": {"gravity": [0.0, -9.8, 0.0]}}],
        })
        self.assertEqual(parse_log_line(good, strict=True)["ops"][0]
                         ["ext-physics"]["gravity"][1], -9.8)
        with self.assertRaisesRegex(WorldFormatError, "strict mode"):
            parse_log_line(json.dumps({
                "revision": 1, "author": {"name": "t"},
                "ops": [{"spawnentity": {}}],
            }), strict=True)
        with self.assertRaisesRegex(WorldFormatError, "extension registry"):
            parse_log_line(json.dumps({
                "revision": 1, "author": {"name": "t"},
                "ops": [{"ext-teleport": {"to": [1, 1, 1]}}],
            }), strict=True)
        # And the tolerant default takes both in stride:
        parse_log_line(json.dumps({
            "revision": 1, "author": {"name": "t"},
            "ops": [{"spawnentity": {}}, {"ext-teleport": {}}],
        }))


class ProvenanceTest(unittest.TestCase):
    def test_the_five_fields_extract_from_the_extension_block(self):
        block = {
            "prompt": "a lantern by a path",
            "model": "generator-7",
            "generation_duration_ms": 4200,
            "biome": "temperate",
            "semantic_category": "landmark",
        }
        manifest = {"version": 3,
                    "meta": {"name": "w", "ext-provenance": block}}
        self.assertEqual(ext_provenance(manifest), block)
        self.assertEqual(EXT_PROVENANCE_FIELDS, (
            "prompt", "model", "generation_duration_ms",
            "biome", "semantic_category",
        ))

    def test_absent_provenance_is_none_and_unknown_keys_are_ignored(self):
        self.assertIsNone(ext_provenance({"version": 3, "meta": {"name": "w"}}))
        self.assertIsNone(ext_provenance({"version": 3}))
        manifest = {"version": 3, "meta": {"name": "w", "ext-provenance": {
            "prompt": "p", "future_field": "must-ignore",
        }}}
        self.assertEqual(ext_provenance(manifest), {"prompt": "p"})


if __name__ == "__main__":
    unittest.main()
