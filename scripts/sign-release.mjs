import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const tag = String(process.env.ATRIA_RELEASE_TAG || process.env.GITHUB_REF_NAME || '').trim();
const privatePem = process.env.ATRIA_UPDATE_PRIVATE_KEY;
const repository = 'Manixpcshot/atria-ai';
if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error('ATRIA_RELEASE_TAG must be a stable vMAJOR.MINOR.PATCH tag');
if (!privatePem) throw new Error('ATRIA_UPDATE_PRIVATE_KEY is not configured in GitHub Actions secrets');

const version = tag.slice(1);
const targets = [
  ['atria.exe', path.resolve('target/release/atria.exe')],
  ['Atria-Dawn-win64.zip', path.resolve('Atria-Dawn-win64.zip')],
];
const assets = targets.map(([name, file]) => {
  const bytes = fs.readFileSync(file);
  const sha256 = crypto.createHash('sha256').update(bytes).digest('hex');
  return {
    name,
    url: `https://github.com/${repository}/releases/download/${tag}/${name}`,
    sha256,
    size: bytes.byteLength,
  };
});

const manifest = {
  version,
  tag,
  release_url: `https://github.com/${repository}/releases/tag/${tag}`,
  assets,
};
const bytes = Buffer.from(JSON.stringify(manifest), 'utf8');
const key = crypto.createPrivateKey(privatePem);
const signature = crypto.sign(null, bytes, key).toString('hex');
fs.writeFileSync('update-manifest.json', bytes);
fs.writeFileSync('update-manifest.sig', `${signature}\n`, { mode: 0o644 });
const sums = assets.map((asset) => `${asset.sha256}  ${asset.name}`).join('\n') + '\n';
fs.writeFileSync('SHA256SUMS.txt', sums);
console.log(`Signed ${tag} update manifest; assets: ${assets.map((a) => a.name).join(', ')}`);
