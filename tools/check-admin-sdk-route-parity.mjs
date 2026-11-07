#!/usr/bin/env node
/**
 * Parity gate for the model-catalog admin API surface.
 *
 * Three inventories must describe exactly the same operations:
 *   1. the generated TypeScript SDK operations consumed by the console
 *   2. the owner route descriptors (handler + service method registration)
 *   3. the HTTP route manifest bound by the Web Framework
 *
 * A drift in any direction is a real outage: a SDK call without a manifest
 * entry answers `404 route-not-in-manifest`, while a manifest entry without a
 * handler advertises a route that cannot be served. The platform cloud gateway
 * composes these manifests through `ApiModuleRegistry::try_compose`, so the
 * check complements (but does not replace) the Rust composition tests.
 *
 * Usage: node tools/check-admin-sdk-route-parity.mjs [--root <repo-root>]
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import process from 'node:process';
import { fileURLToPath } from 'node:url';

const args = process.argv.slice(2);
const rootFlag = args.indexOf('--root');
const repoRoot = path.resolve(
  rootFlag >= 0 && args[rootFlag + 1] ? args[rootFlag + 1] : path.join(path.dirname(fileURLToPath(import.meta.url)), '..'),
);

const surfaces = [
  {
    name: 'backend',
    sdk: 'sdks/sdkwork-models-backend-sdk/sdkwork-models-backend-sdk-typescript/composed/operations.ts',
    descriptors: 'crates/sdkwork-routes-catalog-backend-api/src/routes.rs',
    manifest: 'crates/sdkwork-routes-catalog-backend-api/src/http_route_manifest.rs',
    prefix: '/backend/v3/api/ai',
  },
  {
    name: 'app',
    sdk: 'sdks/sdkwork-models-app-sdk/sdkwork-models-app-sdk-typescript/composed/operations.ts',
    descriptors: 'crates/sdkwork-routes-catalog-app-api/src/routes.rs',
    manifest: 'crates/sdkwork-routes-catalog-app-api/src/http_route_manifest.rs',
    prefix: '/app/v3/api/ai',
  },
];

const read = (relative) => {
  const file = path.join(repoRoot, relative);
  try {
    return readFileSync(file, 'utf8');
  } catch (error) {
    throw new Error(`cannot read ${relative}: ${error.message}`);
  }
};

/** Parameter names differ by convention between axum and OpenAPI; compare shapes. */
function normalizePath(template) {
  return template.replace(/\{[^{}]*\}/g, '{}');
}

const operationKey = (method, pathTemplate) => `${method.toUpperCase()} ${normalizePath(pathTemplate)}`;

function parseSdk(source) {
  const operations = new Map();
  const pattern = /"([A-Za-z0-9_.]+)":\s*\{\s*method:\s*"([A-Z]+)",\s*path:\s*"([^"]+)"\s*\}/g;
  for (const match of source.matchAll(pattern)) {
    operations.set(operationKey(match[2], match[3]), { operationId: match[1], path: match[3] });
  }
  return operations;
}

function parseDescriptors(source) {
  const operations = new Map();
  const pattern = /method:\s*"([A-Z]+)",\s*path:\s*"([^"]+)",\s*operation_id:\s*"([^"]+)",\s*handler:\s*"([^"]+)"/g;
  for (const match of source.matchAll(pattern)) {
    operations.set(operationKey(match[1], match[2]), {
      operationId: match[3],
      path: match[2],
      handler: match[4],
    });
  }
  return operations;
}

function parseManifest(source) {
  const operations = new Map();
  const methods = { Get: 'GET', Post: 'POST', Put: 'PUT', Patch: 'PATCH', Delete: 'DELETE' };
  const pattern = /HttpRoute::[a-z_]+\(\s*HttpMethod::(\w+),\s*"([^"]+)",\s*[A-Z_]+,\s*"([^"]+)"/g;
  for (const match of source.matchAll(pattern)) {
    const method = methods[match[1]];
    if (!method) {
      throw new Error(`unknown HttpMethod variant ${match[1]}`);
    }
    operations.set(operationKey(method, match[2]), { operationId: match[3], path: match[2] });
  }
  return operations;
}

const problems = [];
const report = [];

for (const surface of surfaces) {
  const sdk = parseSdk(read(surface.sdk));
  const descriptors = parseDescriptors(read(surface.descriptors));
  const manifest = parseManifest(read(surface.manifest));

  report.push(
    `${surface.name}: sdk=${sdk.size} descriptors=${descriptors.size} manifest=${manifest.size}`,
  );

  for (const [label, source, target] of [
    ['SDK -> descriptors', sdk, descriptors],
    ['descriptors -> manifest', descriptors, manifest],
    ['manifest -> SDK', manifest, sdk],
  ]) {
    for (const [key, value] of source) {
      if (!target.has(key)) {
        problems.push(`${surface.name} ${label}: ${key} (${value.operationId}) is missing`);
      }
    }
  }

  for (const [label, source, target] of [
    ['SDK/descriptors', sdk, descriptors],
    ['descriptors/manifest', descriptors, manifest],
  ]) {
    for (const [key, value] of source) {
      const peer = target.get(key);
      if (peer && peer.operationId !== value.operationId) {
        problems.push(
          `${surface.name} ${label}: ${key} operationId mismatch (${value.operationId} vs ${peer.operationId})`,
        );
      }
    }
  }

  for (const [key, value] of sdk) {
    if (!value.path.startsWith(surface.prefix)) {
      problems.push(`${surface.name} SDK operation ${key} leaves the ${surface.prefix} surface`);
    }
  }
}

console.log(report.join('\n'));
if (problems.length > 0) {
  console.error(`\nmodel catalog admin API parity FAILED (${problems.length}):`);
  for (const problem of problems) console.error(`  - ${problem}`);
  process.exit(1);
}
console.log('\nmodel catalog admin API parity OK: SDK, handlers and manifests agree');
