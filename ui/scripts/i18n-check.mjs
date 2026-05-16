#!/usr/bin/env node
/**
 * Valida que todas as chaves presentes em `locales/en/<namespace>.json`
 * existem em todas as outras locales. Falha o processo (exit 1) se houver
 * chaves faltando — usado no CI.
 *
 * Uso: `pnpm i18n:check`
 */

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..', 'src', 'i18n', 'locales');
const SOURCE = 'en';

function loadNamespaces(locale) {
  const dir = join(ROOT, locale);
  if (!statSync(dir, { throwIfNoEntry: false })?.isDirectory()) {
    throw new Error(`Locale dir not found: ${dir}`);
  }
  const files = readdirSync(dir).filter((f) => f.endsWith('.json'));
  const out = {};
  for (const f of files) {
    const name = f.replace(/\.json$/, '');
    out[name] = JSON.parse(readFileSync(join(dir, f), 'utf8'));
  }
  return out;
}

function collectKeys(obj, prefix = '') {
  const keys = [];
  for (const [k, v] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === 'object' && !Array.isArray(v)) {
      keys.push(...collectKeys(v, path));
    } else {
      keys.push(path);
    }
  }
  return keys;
}

function main() {
  const source = loadNamespaces(SOURCE);
  const locales = readdirSync(ROOT).filter((d) => d !== SOURCE);

  let hasError = false;
  for (const locale of locales) {
    const target = loadNamespaces(locale);
    for (const ns of Object.keys(source)) {
      const sourceKeys = new Set(collectKeys(source[ns]));
      const targetKeys = new Set(collectKeys(target[ns] ?? {}));

      const missing = [...sourceKeys].filter((k) => !targetKeys.has(k));
      const extra = [...targetKeys].filter((k) => !sourceKeys.has(k));

      if (missing.length) {
        hasError = true;
        console.error(`[${locale}/${ns}] missing keys (${missing.length}):`);
        missing.forEach((k) => console.error(`  - ${k}`));
      }
      if (extra.length) {
        console.warn(`[${locale}/${ns}] extra keys (${extra.length}):`);
        extra.forEach((k) => console.warn(`  - ${k}`));
      }
    }
  }

  if (hasError) {
    console.error('\ni18n check FAILED — fix missing keys above.');
    process.exit(1);
  }
  console.log('i18n check OK');
}

main();
