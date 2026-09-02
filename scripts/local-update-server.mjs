// Local-only update server for BiliTools development builds.
//
// It binds to 127.0.0.1 and serves the Tauri updater manifest plus the signed
// bundle artifacts produced under src-tauri/target/release/bundle.

import fs from 'node:fs';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const tauriDir = path.join(root, 'src-tauri');
const configPath = path.join(tauriDir, 'tauri.conf.json');
const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));
const port = Number(process.env.LOCAL_UPDATE_PORT || 45678);
const artifactRoot = path.resolve(
  process.env.LOCAL_UPDATE_ROOT ||
    path.join(tauriDir, 'target', 'release', 'bundle'),
);

function isSafePath(urlPath) {
  try {
    const clean = decodeURIComponent(urlPath).replace(/^[/\\]+/, '');
    const target = path.resolve(artifactRoot, clean);
    const rel = path.relative(artifactRoot, target);
    return !(rel.startsWith('..') || path.isAbsolute(rel));
  } catch {
    return false;
  }
}

function findUpdaterArtifacts() {
  const candidates = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(full);
      } else if (fs.existsSync(`${full}.sig`)) {
        candidates.push({ artifact: full, sig: `${full}.sig` });
      }
    }
  };

  if (fs.existsSync(artifactRoot)) {
    walk(artifactRoot);
  }

  candidates.sort(
    (a, b) =>
      fs.statSync(b.artifact).mtimeMs - fs.statSync(a.artifact).mtimeMs,
  );
  return candidates.find((candidate) => fs.existsSync(candidate.sig));
}

function latestManifest() {
  const artifact = findUpdaterArtifacts();
  if (!artifact) {
    throw new Error(
      'No signed updater artifact found. Run scripts/build-local-update.ps1 first.',
    );
  }

  const signature = fs.readFileSync(artifact.sig, 'utf8').trim();
  const rel = path.relative(artifactRoot, artifact.artifact).replace(/\\/g, '/');
  const url = `http://127.0.0.1:${port}/${rel}`;

  return {
    version: config.version,
    notes:
      process.env.LOCAL_UPDATE_NOTES ||
      'Local development update for this machine.',
    pub_date: new Date().toISOString(),
    platforms: {
      'windows-x86_64': {
        url,
        signature,
      },
    },
  };
}

function contentType(filePath) {
  if (filePath.endsWith('.json')) return 'application/json';
  if (filePath.endsWith('.zip')) return 'application/zip';
  if (filePath.endsWith('.sig')) return 'text/plain';
  if (filePath.endsWith('.exe')) return 'application/octet-stream';
  return 'application/octet-stream';
}

const server = http.createServer((req, res) => {
  const { pathname } = new URL(req.url, `http://127.0.0.1:${port}`);

  if (req.method === 'GET' && pathname === '/latest.json') {
    try {
      const manifest = latestManifest();
      const body = JSON.stringify(manifest, null, 2);
      res.writeHead(200, {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
      });
      res.end(body);
    } catch (error) {
      const body = JSON.stringify({ error: error.message }, null, 2);
      res.writeHead(503, {
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
      });
      res.end(body);
    }
    return;
  }

  let decodedPath;
  try {
    decodedPath = decodeURIComponent(pathname);
  } catch {
    res.writeHead(400);
    res.end();
    return;
  }

  if (!isSafePath(decodedPath)) {
    res.writeHead(404);
    res.end();
    return;
  }

  const filePath = path.resolve(
    artifactRoot,
    decodedPath.replace(/^[/\\]+/, ''),
  );
  if (!fs.existsSync(filePath) || !fs.statSync(filePath).isFile()) {
    res.writeHead(404);
    res.end();
    return;
  }

  const stat = fs.statSync(filePath);
  res.writeHead(200, {
    'Content-Type': contentType(filePath),
    'Content-Length': stat.size,
  });
  fs.createReadStream(filePath).pipe(res);
});

server.listen(port, '127.0.0.1', () => {
  console.log(`Local update server: http://127.0.0.1:${port}/latest.json`);
});

for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, () => {
    server.close(() => process.exit(0));
  });
}
