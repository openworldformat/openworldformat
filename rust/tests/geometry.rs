//! The geometry parity fixture (conformance/geometry.json, spec/world.md
//! "The geometry of the shapes"), from the crate's side: the fixture's
//! `half_extents` are exactly what `Shape::local_aabb_half` returns, the
//! exact mesh bounds it states fit inside them, and every shape kind the
//! crate knows is in it. Renderers check the landmarks; the crate has no
//! meshes, so it pins the box every renderer's mesh must fit in.

use std::path::PathBuf;

use openworldformat::Shape;

fn fixture() -> serde_json::Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../conformance/geometry/shapes.json");
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn vec3(value: &serde_json::Value) -> [f32; 3] {
    let a: Vec<f32> = serde_json::from_value(value.clone()).unwrap();
    [a[0], a[1], a[2]]
}

#[test]
fn the_fixtures_half_extents_are_the_crates_and_the_bounds_fit_inside() {
    let doc = fixture();
    let shapes = doc["shapes"].as_array().unwrap();
    let mut kinds = Vec::new();
    for entry in shapes {
        let name = entry["name"].as_str().unwrap();
        let shape: Shape = serde_json::from_value(entry["shape"].clone())
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        kinds.push(shape.kind());
        let half = shape.local_aabb_half();
        let expected = vec3(&entry["half_extents"]);
        for axis in 0..3 {
            assert!(
                (half[axis] - expected[axis]).abs() <= 1e-6,
                "{name}: local_aabb_half is {half:?}, the fixture says {expected:?}"
            );
        }
        let min = vec3(&entry["bounds"]["min"]);
        let max = vec3(&entry["bounds"]["max"]);
        for axis in 0..3 {
            assert!(
                max[axis] <= half[axis] + 1e-6 && -min[axis] <= half[axis] + 1e-6,
                "{name}: the exact bounds {min:?}..{max:?} leave the box {half:?}"
            );
            assert!(min[axis] <= max[axis], "{name}: bounds inverted on axis {axis}");
        }
    }
    kinds.sort_unstable();
    kinds.dedup();
    assert_eq!(
        kinds,
        vec![
            "capsule",
            "cone",
            "cuboid",
            "cylinder",
            "icosahedron",
            "plane",
            "prism",
            "pyramid",
            "sphere",
            "tetrahedron",
            "torus",
            "wedge",
        ],
        "every shape kind has a fixture entry, each once"
    );
}
