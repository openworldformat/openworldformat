// Assemble the live demo's inputs into static/: the conformance worlds,
// the example package's manifest, and the reference renderer. The copies
// are gitignored — edit them where they live (../conformance, ../examples,
// ../js/src/render.js) and re-assemble, the same rule localgpt.world
// runs on. vendor/three (committed) is self-hosted so the site makes no
// third-party requests.
import { cp, mkdir, readdir, rm } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const repo = path.resolve(root, '..');

await rm(path.join(root, 'static/conformance'), { recursive: true, force: true });
await rm(path.join(root, 'static/viewer'), { recursive: true, force: true });
await mkdir(path.join(root, 'static/conformance/assets'), { recursive: true });
await mkdir(path.join(root, 'static/viewer'), { recursive: true });

// The conformance worlds and their textures.
const worlds = (await readdir(path.join(repo, 'conformance')))
  .filter((f) => f.endsWith('.json'));
for (const world of worlds) {
  await cp(path.join(repo, 'conformance', world), path.join(root, 'static/conformance', world));
}
await cp(path.join(repo, 'conformance/assets/textures'), path.join(root, 'static/conformance/assets/textures'), { recursive: true });

// The example package's manifest — its head, which a viewer reads as is.
await cp(
  path.join(repo, 'examples/hello-world/manifest.json'),
  path.join(root, 'static/conformance/hello-world.json'),
);

// The reference renderer.
await cp(path.join(repo, 'js/src/render.js'), path.join(root, 'static/viewer/world-viewer.js'));

console.log(`assembled ${worlds.length} conformance worlds, the example manifest, and the renderer`);
