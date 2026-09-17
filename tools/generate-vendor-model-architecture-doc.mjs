#!/usr/bin/env node
// generate-vendor-model-architecture-doc.mjs —— 由模型目录生成 vendor/模型架构文档
//
// 产出（两份内容一致，TECH 版把标题替换为 owner 行）：
//   docs/vendor-model-architecture.md
//   docs/architecture/tech/TECH-vendor-model-architecture.md
//
// 口径（generated，勿手改）：
//   - Vendor Summary：一 vendor 一行。Protocols/Regions 取该 vendor 全部 region 的并集；
//     Client APIs 取该 vendor 各 region 中「最强」的 supportStatus
//     （supported > partial > convert > unsupported）。
//   - Model Architecture by Region：仅收录 shelfState === "listed" 的模型，按 modelId 升序。
//     Context = contextTokens / 1000 向下取整 + "K"（无 contextTokens 记 N/A）。
//     Modalities = model.capabilities 原序逗号连接（无则 N/A）。
//     Pricing = 该模型 official 侧 llm_input_token / llm_output_token 的 unitPrice，
//     保留 2 位小数；任一缺失记 N/A。分时/分档变体不展开（取 rateVariant=standard 优先）。
//   - Statistics Summary：四张表，计数对象为 vendor 行（25 个 vendor），
//     Capability Support 取 vendor.json#/capabilities 的并集。
//
// 用法：
//   node tools/generate-vendor-model-architecture-doc.mjs           # 写盘
//   node tools/generate-vendor-model-architecture-doc.mjs --check    # 只校验是否最新
import { existsSync, readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const root = process.cwd();
const readJson = (p) => JSON.parse(readFileSync(p, 'utf8'));
const modelsRoot = join(root, 'models');

const CLIENT_APIS = [
  ['claude_code', 'CC'],
  ['codex', 'CX'],
  ['gemini_cli', 'GC'],
];
const STATUS_RANK = { supported: 4, partial: 3, convert: 2, unsupported: 1 };

// ---------- 装载目录 ----------
const vendors = new Map(); // vendorCode -> { regions: Map<regionCode, bundle> }
for (const vendorCode of readdirSync(modelsRoot).sort()) {
  const vendorDir = join(modelsRoot, vendorCode);
  if (!statSync(vendorDir).isDirectory()) continue;
  const regions = new Map();
  for (const regionCode of readdirSync(vendorDir).sort()) {
    const regionDir = join(vendorDir, regionCode);
    if (!statSync(regionDir).isDirectory()) continue;
    const modelsDir = join(regionDir, 'models');
    const vendorPath = join(regionDir, 'vendor.json');
    if (!existsSync(modelsDir) || !existsSync(vendorPath)) continue;

    const models = readdirSync(modelsDir)
      .filter((f) => f.endsWith('.json'))
      .map((f) => readJson(join(modelsDir, f)));

    const pricingDir = join(regionDir, 'pricing');
    const pricingFiles = existsSync(pricingDir) ? readdirSync(pricingDir).filter((f) => f.endsWith('.json')) : [];
    const pricing = new Map();
    let currency = null;
    for (const f of pricingFiles) {
      const doc = readJson(join(pricingDir, f));
      pricing.set(doc.modelId ?? f.replace(/\.json$/, ''), doc);
      if (!currency && typeof doc.currency === 'string') currency = doc.currency;
    }

    regions.set(regionCode, {
      vendor: readJson(vendorPath),
      models,
      pricing,
      pricingFileCount: pricingFiles.length,
      currency: currency ?? (regionCode === 'cn' ? 'CNY' : 'USD'),
    });
  }
  if (regions.size > 0) vendors.set(vendorCode, { regions });
}

// ---------- 取值 helpers ----------
const REGION_LABEL = {
  cn: 'CN',
  global: 'GLOBAL',
};
const regionLabel = (code) => REGION_LABEL[code] ?? code.toUpperCase();
const regionOrder = (code) => (code === 'cn' ? 0 : code === 'global' ? 1 : 2);

function contextCell(model) {
  if (!Number.isFinite(model.contextTokens) || model.contextTokens <= 0) return 'N/A';
  return `${Math.floor(model.contextTokens / 1000)}K`;
}

function modalitiesCell(model) {
  const caps = Array.isArray(model.capabilities) ? model.capabilities : [];
  return caps.length > 0 ? caps.join(', ') : 'N/A';
}

function priceCell(pricingDoc) {
  if (!pricingDoc) return 'N/A';
  const pick = (meterCode) => {
    const rows = (pricingDoc.prices ?? []).filter(
      (p) => p.meterCode === meterCode && p.priceSide === 'official',
    );
    if (rows.length === 0) return null;
    const standard = rows.find((p) => p.rateVariant === 'standard');
    return Number((standard ?? rows[0]).unitPrice);
  };
  const input = pick('llm_input_token');
  const output = pick('llm_output_token');
  if (input === null || output === null) return 'N/A';
  const currency = pricingDoc.currency ?? '';
  return `${currency} ${input.toFixed(2)} / ${output.toFixed(2)}`.trim();
}

function strongestStatus(regions, clientApiCode) {
  let best = 'unsupported';
  let bestRank = 0;
  for (const b of regions.values()) {
    const status = b.vendor.clientApiCompatibility?.[clientApiCode]?.supportStatus;
    if (status && (STATUS_RANK[status] ?? 0) > bestRank) {
      best = status;
      bestRank = STATUS_RANK[status] ?? 0;
    }
  }
  return best;
}

// ---------- 文档正文 ----------
const meta = readJson(join(root, 'sdkwork-models.json'));
const generatedDate = String(meta.generatedAt ?? '').slice(0, 10);
const lines = [];
const push = (...xs) => lines.push(...xs);

push(
  `Generated: ${generatedDate}`,
  '',
  '> 本文件由 `tools/generate-vendor-model-architecture-doc.mjs` 从模型目录生成，请勿手改。',
  '> 口径：模型表仅收录 `shelfState = listed` 的模型；`Context` = `contextTokens / 1000` 向下取整；',
  '> `Pricing` 取 official 侧 `llm_input_token` / `llm_output_token` 单价（缺则 N/A），分时/分档变体不展开；',
  '> `Modalities` 取 `model.capabilities` 原序；`Client APIs` 取该 vendor 各 region 中最强的 `supportStatus`。',
  '',
  '## Vendor Summary',
  '',
  '| # | Vendor Code | Display Name | Open Source | Protocols | Regions | Client APIs |',
  '|---|-------------|--------------|-------------|-----------|---------|-------------|',
);

let index = 0;
for (const [vendorCode, { regions }] of [...vendors.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  index += 1;
  const first = regions.values().next().value.vendor;
  const protocols = [...new Set([...regions.values()].flatMap((b) => b.vendor.supportedProtocols ?? []))].sort();
  const regionCodes = [...regions.keys()].sort((a, b) => regionOrder(a) - regionOrder(b));
  const clientCells = CLIENT_APIS.map(([code, short]) => `${short}:${strongestStatus(regions, code)}`).join(' / ');
  push(
    `| ${index} | ${vendorCode} | ${first.displayName} | ${first.openSource ? 'Yes' : 'No'} | `
    + `${protocols.join(', ')} | ${regionCodes.join(', ')} | ${clientCells} |`,
  );
}

push('', '## Model Architecture by Region', '');
for (const [vendorCode, { regions }] of [...vendors.entries()].sort((a, b) => a[0].localeCompare(b[0]))) {
  const first = regions.values().next().value.vendor;
  push(`### ${first.displayName} (${vendorCode})`, '');
  for (const [regionCode, bundle] of [...regions.entries()].sort((a, b) => regionOrder(a[0]) - regionOrder(b[0]))) {
    push(`**Region: ${regionLabel(regionCode)}** (${bundle.currency})`, '');
    const listed = bundle.models
      .filter((m) => m.shelfState === 'listed')
      .sort((a, b) => a.modelId.localeCompare(b.modelId));
    push('| Model ID | Context | Modalities | Pricing (Input/Output) |', '|----------|---------|------------|------------------------|');
    for (const m of listed) {
      push(`| ${m.modelId} | ${contextCell(m)} | ${modalitiesCell(m)} | ${priceCell(bundle.pricing.get(m.modelId))} |`);
    }
    push('');
  }
}

// ---------- 统计 ----------
const regionCodes = [...new Set([...vendors.values()].flatMap((v) => [...v.regions.keys()]))]
  .sort((a, b) => regionOrder(a) - regionOrder(b));
push('## Statistics Summary', '', '### Vendor Count by Region', '', '| Region | Vendors | Models | Pricing Files |', '|--------|---------|--------|---------------|');
for (const code of regionCodes) {
  const bundles = [...vendors.values()].map((v) => v.regions.get(code)).filter(Boolean);
  const modelCount = bundles.reduce((n, b) => n + b.models.length, 0);
  const pricingCount = bundles.reduce((n, b) => n + b.pricingFileCount, 0);
  push(`| ${regionLabel(code)} | ${bundles.length} | ${modelCount} | ${pricingCount} |`);
}

push('', '### Client API Support', '', '| API | Supported | Partial | Convert | Unsupported |', '|-----|-----------|---------|---------|-------------|');
for (const [code] of CLIENT_APIS) {
  const tally = { supported: 0, partial: 0, convert: 0, unsupported: 0 };
  for (const { regions } of vendors.values()) tally[strongestStatus(regions, code)] += 1;
  push(`| ${code} | ${tally.supported} | ${tally.partial} | ${tally.convert} | ${tally.unsupported} |`);
}

push('', '### Protocol Support', '', '| Protocol | Vendors |', '|----------|---------|');
const protocolTally = new Map();
for (const { regions } of vendors.values()) {
  for (const p of new Set([...regions.values()].flatMap((b) => b.vendor.supportedProtocols ?? []))) {
    protocolTally.set(p, (protocolTally.get(p) ?? 0) + 1);
  }
}
for (const [p, n] of [...protocolTally.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))) {
  push(`| ${p} | ${n} |`);
}

push('', '### Capability Support', '', '| Capability | Vendors |', '|------------|---------|');
const capabilityTally = new Map();
for (const { regions } of vendors.values()) {
  for (const c of new Set([...regions.values()].flatMap((b) => b.vendor.capabilities ?? []))) {
    capabilityTally.set(c, (capabilityTally.get(c) ?? 0) + 1);
  }
}
for (const [c, n] of [...capabilityTally.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))) {
  push(`| ${c} | ${n} |`);
}
push('');

const body = lines.join('\n');
const title = '# SDKWork Models - Vendor & Model Architecture\n\n';
const ownerLine = '> Owner: SDKWork maintainers\n\n';

const outputs = [
  [join(root, 'docs', 'vendor-model-architecture.md'), title + body],
  [join(root, 'docs', 'architecture', 'tech', 'TECH-vendor-model-architecture.md'), ownerLine + body],
];

if (process.argv.includes('--check')) {
  let stale = 0;
  for (const [file, content] of outputs) {
    const current = existsSync(file) ? readFileSync(file, 'utf8') : '';
    // TECH 版历史上多一个行尾空行，比较时统一去掉行尾空白
    if (current.replace(/\s+$/, '') !== content.replace(/\s+$/, '')) {
      console.error(`stale: ${file.replace(`${root}\\`, '')}`);
      stale += 1;
    }
  }
  if (stale > 0) {
    console.error('运行 node tools/generate-vendor-model-architecture-doc.mjs 重新生成。');
    process.exitCode = 1;
  } else {
    console.log('vendor/model architecture docs are current');
  }
} else {
  for (const [file, content] of outputs) writeFileSync(file, content, 'utf8');
  console.log(`generated ${outputs.length} doc(s) from ${vendors.size} vendors / ${regionCodes.length} regions`);
}
