#!/usr/bin/env node
// Sets the app version everywhere it lives, so releases and updates agree:
//   npm run version:set -- 0.2.0
// Then commit, tag (git tag v0.2.0) and push the tag to build a release.
import { readFileSync, writeFileSync } from "node:fs";

const version = process.argv[2];
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version ?? "")) {
  console.error("Usage: npm run version:set -- <major.minor.patch>, e.g. 0.2.0");
  process.exit(1);
}

const update = (file, fn) => {
  const before = readFileSync(file, "utf8");
  const after = fn(before);
  if (after === before) throw new Error(`No version found to change in ${file}`);
  writeFileSync(file, after);
  console.log(`  ${file}`);
};

update("package.json", (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
update("src-tauri/tauri.conf.json", (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${version}$2`));
// Only the [package] version, not dependency versions.
update("src-tauri/Cargo.toml", (s) => s.replace(/(\[package\][^[]*?\nversion\s*=\s*")[^"]+(")/, `$1${version}$2`));
update("src-tauri/Cargo.lock", (s) => s.replace(/(name = "f95-game-manager"\r?\nversion = ")[^"]+(")/, `$1${version}$2`));

console.log(`Version set to ${version}. Next: commit, then \`git tag v${version}\` and \`git push --tags\`.`);
