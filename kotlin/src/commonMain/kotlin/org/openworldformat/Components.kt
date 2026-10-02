// Typed views over an entity's component fields.
//
// The fold treats components as opaque (they ride in `fields`); an
// app wants them typed. These accessors decode on demand and stay
// must-ignore-lenient: a component they don't know is simply absent
// from the typed view, never an error.

package org.openworldformat

import kotlinx.serialization.json.JsonElement

/** Transform in world space (or parent-relative if parented). */
data class WorldTransform(
    val position: Vec3?,
    val rotationDegrees: Vec3?,
    val scale: Vec3?,
    val visible: Boolean?,
) {
    companion object {
        operator fun invoke(json: JsonElement?): WorldTransform? {
            val o = json?.obj ?: return null
            return WorldTransform(
                position = Vec3.from(o["position"]),
                rotationDegrees = Vec3.from(o["rotation_degrees"]),
                scale = Vec3.from(o["scale"]),
                visible = o["visible"]?.bool,
            )
        }
    }
}

/** A parametric primitive, externally tagged:
 *  `{"Sphere": {"radius": 0.75}}`, `{"Cuboid": {"x": 1, "y": 1, "z": 1}}`, …
 *  Unknown shapes decode to null — the viewer profile's must-ignore. */
sealed class Shape {
    data class Cuboid(val x: Double, val y: Double, val z: Double) : Shape()
    data class Sphere(val radius: Double) : Shape()
    data class Cylinder(val radius: Double, val height: Double) : Shape()
    data class Cone(val radius: Double, val height: Double) : Shape()
    data class Capsule(val radius: Double, val halfLength: Double) : Shape()
    data class Torus(val majorRadius: Double, val minorRadius: Double) : Shape()
    data class Plane(val x: Double, val z: Double) : Shape()
    data class Pyramid(val baseX: Double, val baseZ: Double, val height: Double) : Shape()
    data class Tetrahedron(val radius: Double) : Shape()
    data class Icosahedron(val radius: Double) : Shape()
    data class Wedge(val x: Double, val y: Double, val z: Double) : Shape()

    companion object {
        operator fun invoke(json: JsonElement?): Shape? {
            val o = json?.obj ?: return null
            if (o.size != 1) return null
            val (tag, params) = o.entries.first()
            fun d(k: String, fallback: Double = 1.0): Double = params.obj?.get(k)?.dbl ?: fallback
            return when (tag) {
                "Cuboid" -> Cuboid(d("x"), d("y"), d("z"))
                "Sphere" -> Sphere(d("radius", 0.5))
                "Cylinder" -> Cylinder(d("radius", 0.5), d("height"))
                "Cone" -> Cone(d("radius", 0.5), d("height"))
                "Capsule" -> Capsule(d("radius", 0.5), d("half_length"))
                "Torus" -> Torus(d("major_radius", 0.5), d("minor_radius", 0.2))
                "Plane" -> Plane(d("x"), d("z"))
                "Pyramid" -> Pyramid(d("base_x"), d("base_z"), d("height"))
                "Tetrahedron" -> Tetrahedron(d("radius", 0.5))
                "Icosahedron" -> Icosahedron(d("radius", 0.5))
                "Wedge" -> Wedge(d("x"), d("y"), d("z"))
                else -> null
            }
        }
    }
}

/** A surface: color (rgba, linear), emissive, and the PBR knobs. */
data class MaterialDef(
    val color: List<Double>?,
    val emissive: List<Double>?,
    val roughness: Double?,
    val metallic: Double?,
    val transparency: Double?,
    val doubleSided: Boolean?,
    /** `base_color_texture` and friends: the asset-referencing fields. */
    val fields: kotlinx.serialization.json.JsonObject,
) {
    companion object {
        private val known = setOf("color", "emissive", "roughness", "metallic", "transparency", "double_sided")

        operator fun invoke(json: JsonElement?): MaterialDef? {
            val o = json?.obj ?: return null
            return MaterialDef(
                color = o["color"]?.arr?.mapNotNull { it.dbl },
                emissive = o["emissive"]?.arr?.mapNotNull { it.dbl },
                roughness = o["roughness"]?.dbl,
                metallic = o["metallic"]?.dbl,
                transparency = o["transparency"]?.dbl,
                doubleSided = o["double_sided"]?.bool,
                fields = kotlinx.serialization.json.JsonObject(o.filterKeys { it !in known }),
            )
        }
    }
}

/** A light: directional, point or spot. */
data class LightDef(
    val lightType: String?,
    val color: List<Double>?,
    val intensity: Double?,
    val direction: List<Double>?,
    val range: Double?,
    val angleDegrees: Double?,
    val shadows: Boolean?,
) {
    companion object {
        operator fun invoke(json: JsonElement?): LightDef? {
            val o = json?.obj ?: return null
            return LightDef(
                lightType = o["light_type"]?.str,
                color = o["color"]?.arr?.mapNotNull { it.dbl },
                intensity = o["intensity"]?.dbl,
                direction = o["direction"]?.arr?.mapNotNull { it.dbl },
                range = o["range"]?.dbl,
                angleDegrees = o["angle_degrees"]?.dbl,
                shadows = o["shadows"]?.bool,
            )
        }
    }
}

/** The entity's transform, if it declares one. */
val WorldEntity.transform: WorldTransform?
    get() = WorldTransform(fields["transform"])

/** The entity's parametric shape, if it declares one. */
val WorldEntity.shape: Shape?
    get() = Shape(fields["shape"])

/** The entity's material, if it declares one. */
val WorldEntity.material: MaterialDef?
    get() = MaterialDef(fields["material"])

/** The entity's light, if it declares one. */
val WorldEntity.light: LightDef?
    get() = LightDef(fields["light"])
