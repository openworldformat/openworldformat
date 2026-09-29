// Headless render check, the conformance suite's teeth: serve the built
// site, open world.html?src=conformance/<world> in Chromium with software
// WebGL, and fail on any page or console error, a missing canvas, or a
// viewer that reports no entities. "Compliance means rendering these" —
// this is what makes that sentence testable. (Pattern: localgpt.world's
// check-worlds.mjs.)
//
//   npm ci && npx playwright install --with-deps chromium && npm run check
import { createServer } from 'node:http';
import { readFile, readdir } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from 'playwright';

const root = fileURLToPath(new URL('../public/', import.meta.url));
const types = {
  '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript',
  '.json': 'application/json', '.css': 'text/css', '.svg': 'image/svg+xml',
  '.png': 'image/png',
};

// The viewer fetches JSON and ES modules, so it needs a server, not file://.
const server = createServer(async (req, res) => {
  const path = normalize(decodeURIComponent(new URL(req.url, 'http://localhost').pathname));
  const file = join(root, path === '/' ? 'index.html' : path);
  if (!file.startsWith(root)) { res.writeHead(403); res.end(); return; }
  try {
    const body = await readFile(file);
    res.writeHead(200, { 'content-type': types[extname(file)] || 'application/octet-stream' });
    res.end(body);
  } catch {
    res.writeHead(404); res.end();
  }
});
await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
const base = `http://127.0.0.1:${server.address().port}`;

const worlds = (await readdir(join(root, 'conformance'))).filter((f) => f.endsWith('.json')).sort();

const browser = await chromium.launch({
  args: ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
});
const page = await browser.newPage({ viewport: { width: 960, height: 600 } });
const errors = [];
page.on('pageerror', (e) => errors.push(`pageerror: ${e.message}`));
page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text()}`); });

let failed = 0;
for (const world of worlds) {
  errors.length = 0;
  await page.goto(`${base}/world.html?src=conformance/${world}`, { waitUntil: 'load' });
  let info = null;
  try {
    await page.waitForFunction(
      () => window.owfViewer && window.owfViewer.sceneInfo().entityCount > 0,
      null, { timeout: 15000 },
    );
    info = await page.evaluate(() => window.owfViewer.sceneInfo());
  } catch {
    // Reported through `info` below.
  }
  const canvas = await page.evaluate(() => Boolean(document.querySelector('#scene canvas')));
  const ok = Boolean(info) && canvas && errors.length === 0;
  if (!ok) failed += 1;
  console.log(
    `${ok ? 'ok  ' : 'FAIL'} ${world}: entities=${info?.entityCount ?? 0}`
    + ` tours=${info?.tourCount ?? 0} triggers=${info?.triggerCount ?? 0}`
    + ` canvas=${canvas} errors=${errors.length}`,
  );
  for (const error of errors) console.log(`      ${error.slice(0, 300)}`);
}

await browser.close();
server.close();
if (failed) {
  console.log(`${failed} of ${worlds.length} worlds failed`);
  process.exit(1);
}
console.log(`${worlds.length} worlds rendered`);
