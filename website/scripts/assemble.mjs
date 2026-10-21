// Assemble the live demo's inputs into static/: the conformance worlds,
// the example package's manifest, the reference renderer, and the brand
// files (the favicon, the touch icon, the header mark, the social image).
// The copies are gitignored — edit them where they live (../conformance,
// ../examples, ../js/src/render.js, ../brand) and re-assemble, the same
// rule localgpt.world runs on. vendor/three (committed) is self-hosted so
// the site makes no third-party requests.
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

// The reference renderer, and the sibling modules it imports (the
// extension module, and the prism's geometry).
await cp(path.join(repo, 'js/src/render.js'), path.join(root, 'static/viewer/world-viewer.js'));
await cp(path.join(repo, 'js/src/cinematography.js'), path.join(root, 'static/viewer/cinematography.js'));
await cp(path.join(repo, 'js/src/prism.js'), path.join(root, 'static/viewer/prism.js'));

// The brand files the templates reference (base.html).
await rm(path.join(root, 'static/brand'), { recursive: true, force: true });
await mkdir(path.join(root, 'static/brand'), { recursive: true });
const brand = ['mark.svg', 'mark-mono.svg', 'tile.svg', 'lockup.svg', 'lockup-white.svg', 'favicon-32.png', 'favicon-16.png', 'apple-touch-icon.png', 'social-preview.png'];
for (const f of brand) {
  await cp(path.join(repo, 'brand', f), path.join(root, 'static/brand', f));
}

console.log(`assembled ${worlds.length} conformance worlds, the example manifest, the renderer, and ${brand.length} brand files`);
