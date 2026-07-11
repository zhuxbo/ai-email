/* global console, process */

import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { access, copyFile, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function readAndroidOutputMetadata(rawJson) {
  const parsed = JSON.parse(rawJson);
  const first = parsed.elements?.[0];
  if (!first || typeof first.outputFile !== 'string' || first.outputFile.length === 0) {
    throw new Error('Android output-metadata.json 缺少 elements[0].outputFile');
  }
  if (typeof first.versionName !== 'string' || first.versionName.length === 0) {
    throw new Error('Android output-metadata.json 缺少 elements[0].versionName');
  }
  const versionCode = Number(first.versionCode);
  if (!Number.isSafeInteger(versionCode) || versionCode <= 0) {
    throw new Error('Android output-metadata.json 缺少有效的 elements[0].versionCode');
  }
  return {
    versionName: first.versionName,
    versionCode,
    outputFile: first.outputFile,
  };
}

export function buildAndroidMetadata({
  version,
  versionCode,
  notes,
  pubDate,
  apkUrl,
  apkSize,
  sha256,
}) {
  return { version, versionCode, notes, pubDate, apkUrl, apkSize, sha256 };
}

export function releaseAssetNames(version) {
  return [
    `ai-email_${version}_aarch64.dmg`,
    `ai-email_${version}_arm64-v8a.apk`,
    'android-latest.json',
  ];
}

export function assertSignedApkName(fileName) {
  if (fileName.includes('unsigned')) {
    throw new Error('Android Release 不能使用 unsigned APK');
  }
}

function compareVersionName(left, right) {
  return left.localeCompare(right, undefined, { numeric: true, sensitivity: 'base' });
}

async function pathExists(filePath, accessImpl) {
  try {
    await accessImpl(filePath);
    return true;
  } catch {
    return false;
  }
}

export async function findApkSigner({
  env = process.env,
  readdirImpl = readdir,
  accessImpl = access,
} = {}) {
  const explicitPath = env.APKSIGNER || env.ANDROID_APKSIGNER;
  if (explicitPath) {
    return explicitPath;
  }

  const androidHomeCandidates = [
    env.ANDROID_HOME,
    env.ANDROID_SDK_ROOT,
    '/opt/homebrew/share/android-commandlinetools',
  ].filter(Boolean);

  for (const androidHome of [...new Set(androidHomeCandidates)]) {
    const buildToolsDir = path.join(androidHome, 'build-tools');
    let entries = [];
    try {
      entries = await readdirImpl(buildToolsDir, { withFileTypes: true });
    } catch {
      entries = [];
    }
    const versions = entries
      .filter((entry) => entry.isDirectory())
      .map((entry) => entry.name)
      .sort(compareVersionName)
      .reverse();

    for (const version of versions) {
      const candidate = path.join(buildToolsDir, version, 'apksigner');
      if (await pathExists(candidate, accessImpl)) {
        return candidate;
      }
    }
  }

  return 'apksigner';
}

export function normalizeSha256Fingerprint(value) {
  const normalized = String(value)
    .toLowerCase()
    .replace(/[^a-f0-9]/g, '');
  if (!/^[a-f0-9]{64}$/.test(normalized)) {
    throw new Error('Android release 证书 SHA-256 指纹格式非法');
  }
  return normalized;
}

export function extractApkCertificateSha256(apksignerOutput) {
  const match = String(apksignerOutput).match(/certificate SHA-256 digest:\s*([a-f0-9: ]+)/i);
  if (!match) {
    throw new Error('apksigner 输出缺少 Android APK 证书 SHA-256 指纹');
  }
  return normalizeSha256Fingerprint(match[1]);
}

export function assertExpectedApkCertificate(apksignerOutput, expectedFingerprint) {
  const actual = extractApkCertificateSha256(apksignerOutput);
  const expected = normalizeSha256Fingerprint(expectedFingerprint);
  if (actual !== expected) {
    throw new Error(`Android APK 签名证书 SHA-256 不匹配: expected ${expected}, actual ${actual}`);
  }
}

export async function verifySignedApk(
  _apkPath,
  { execFileImpl = execFile, apksignerPath = 'apksigner' } = {},
) {
  return await new Promise((resolve, reject) => {
    execFileImpl(apksignerPath, ['verify', '--print-certs', _apkPath], (error, stdout, stderr) => {
      if (error) {
        if (error.code === 'ENOENT') {
          reject(new Error('缺少 apksigner，无法校验 Android APK 签名'));
          return;
        }
        const detail = [String(stderr || '').trim(), String(stdout || '').trim()]
          .filter(Boolean)
          .join('\n');
        reject(new Error(detail ? `apksigner verify 失败:\n${detail}` : 'apksigner verify 失败'));
        return;
      }
      resolve(String(stdout));
    });
  });
}

async function runSelfTest() {
  assert.deepEqual(parseArgs(['--', '--version', '0.1.0', '--notes-file', 'notes.md']), {
    version: '0.1.0',
    notesFile: 'notes.md',
    stagingDir: 'src-tauri/target/release/release-assets',
  });

  assert.throws(() => assertSignedApkName('app-universal-release-unsigned.apk'), /unsigned APK/);

  await assert.rejects(
    () =>
      verifySignedApk('/tmp/app.apk', {
        execFileImpl: (_cmd, _args, callback) =>
          callback(Object.assign(new Error('spawn apksigner ENOENT'), { code: 'ENOENT' })),
      }),
    /apksigner/i,
  );
  await assert.rejects(
    () =>
      verifySignedApk('/tmp/app.apk', {
        execFileImpl: (_cmd, _args, callback) =>
          callback(Object.assign(new Error('bad signature'), { code: 1 }), '', 'DOES NOT VERIFY'),
      }),
    /DOES NOT VERIFY/,
  );

  const androidMetadata = readAndroidOutputMetadata(
    JSON.stringify({
      elements: [{ outputFile: 'app-release.apk', versionName: '0.1.0', versionCode: 1000 }],
    }),
  );
  assert.equal(androidMetadata.versionCode, 1000);
  assert.throws(
    () =>
      readAndroidOutputMetadata(
        JSON.stringify({
          elements: [{ outputFile: 'app-release.apk', versionName: '0.1.0' }],
        }),
      ),
    /versionCode/,
  );

  const expectedCertSha256 = '00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff';
  const apksignerOutput = `Signer #1 certificate SHA-256 digest: ${expectedCertSha256.match(/.{2}/g).join(':')}`;
  assert.doesNotThrow(() => assertExpectedApkCertificate(apksignerOutput, expectedCertSha256));
  assert.throws(
    () =>
      assertExpectedApkCertificate(
        apksignerOutput,
        'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff',
      ),
    /不匹配/,
  );

  const foundApkSigner = await findApkSigner({
    env: { ANDROID_HOME: '/android-sdk' },
    readdirImpl: async () => [
      { name: '34.0.0', isDirectory: () => true },
      { name: '35.0.0', isDirectory: () => true },
    ],
    accessImpl: async (candidate) => {
      if (candidate === '/android-sdk/build-tools/35.0.0/apksigner') {
        return;
      }
      throw Object.assign(new Error('missing'), { code: 'ENOENT' });
    },
  });
  assert.equal(foundApkSigner, '/android-sdk/build-tools/35.0.0/apksigner');

  const foundDefaultApkSigner = await findApkSigner({
    env: {},
    readdirImpl: async () => [{ name: '35.0.0', isDirectory: () => true }],
    accessImpl: async (candidate) => {
      if (
        candidate === '/opt/homebrew/share/android-commandlinetools/build-tools/35.0.0/apksigner'
      ) {
        return;
      }
      throw Object.assign(new Error('missing'), { code: 'ENOENT' });
    },
  });
  assert.equal(
    foundDefaultApkSigner,
    '/opt/homebrew/share/android-commandlinetools/build-tools/35.0.0/apksigner',
  );

  assert.deepEqual(releaseAssetNames('0.1.0'), [
    'ai-email_0.1.0_aarch64.dmg',
    'ai-email_0.1.0_arm64-v8a.apk',
    'android-latest.json',
  ]);

  console.log('self-test passed');
}

function parseArgs(argv) {
  const args = argv[0] === '--' ? argv.slice(1) : argv;
  const options = {
    stagingDir: 'src-tauri/target/release/release-assets',
  };

  for (let index = 0; index < args.length; index += 1) {
    const token = args[index];
    if (token === '--version') {
      options.version = args[index + 1];
      index += 1;
      continue;
    }
    if (token === '--notes-file') {
      options.notesFile = args[index + 1];
      index += 1;
      continue;
    }
    if (token === '--staging-dir') {
      options.stagingDir = args[index + 1];
      index += 1;
      continue;
    }
    throw new Error(`未知参数: ${token}`);
  }

  if (!options.version) {
    throw new Error('缺少必填参数 --version');
  }
  if (!options.notesFile) {
    throw new Error('缺少必填参数 --notes-file');
  }
  return options;
}

async function readJsonFile(filePath) {
  return JSON.parse(await readFile(filePath, 'utf8'));
}

function assertExactVersion(filePath, actualVersion, expectedVersion, label) {
  if (actualVersion !== expectedVersion) {
    throw new Error(
      `${label} 版本 ${actualVersion} 与 --version ${expectedVersion} 不一致 (${filePath})`,
    );
  }
}

async function readCargoPackageVersion(filePath) {
  const raw = await readFile(filePath, 'utf8');
  const lines = raw.split(/\r?\n/);
  let inPackageSection = false;

  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed === '[package]') {
      inPackageSection = true;
      continue;
    }
    if (inPackageSection && trimmed.startsWith('[')) {
      break;
    }
    if (inPackageSection) {
      const version = trimmed.match(/^version\s*=\s*"([^"]+)"$/)?.[1];
      if (version) {
        return version;
      }
    }
  }
  throw new Error(`无法从 ${filePath} 读取 Cargo package version`);
}

async function sha256File(filePath) {
  const buffer = await readFile(filePath);
  return createHash('sha256').update(buffer).digest('hex');
}

async function copyIntoStaging(sourcePath, stagingDir, targetName) {
  const targetPath = path.join(stagingDir, targetName);
  await copyFile(sourcePath, targetPath);
  return targetPath;
}

export async function main(argv) {
  if (argv.includes('--self-test')) {
    await runSelfTest();
    return;
  }

  const { version, notesFile, stagingDir } = parseArgs(argv);
  const tauriConfPath = 'src-tauri/tauri.conf.json';
  const packageJsonPath = 'package.json';
  const cargoTomlPath = 'src-tauri/Cargo.toml';
  const androidMetadataPath =
    'src-tauri/gen/android/app/build/outputs/apk/universal/release/output-metadata.json';

  const tauriConf = await readJsonFile(tauriConfPath);
  await assertExactVersion(tauriConfPath, String(tauriConf.version), version, 'tauri.conf.json');

  const packageJson = await readJsonFile(packageJsonPath);
  await assertExactVersion(packageJsonPath, String(packageJson.version), version, 'package.json');

  const cargoVersion = await readCargoPackageVersion(cargoTomlPath);
  await assertExactVersion(cargoTomlPath, cargoVersion, version, 'Cargo.toml');

  const androidOutput = readAndroidOutputMetadata(await readFile(androidMetadataPath, 'utf8'));
  if (androidOutput.versionName !== version) {
    throw new Error(
      `Android versionName ${androidOutput.versionName} 与 --version ${version} 不一致`,
    );
  }
  assertSignedApkName(androidOutput.outputFile);

  const apkSourcePath = path.resolve(path.dirname(androidMetadataPath), androidOutput.outputFile);
  const dmgSourcePath = `src-tauri/target/release/bundle/dmg/ai-email_${version}_aarch64.dmg`;

  const expectedAndroidCertSha256 = process.env.ANDROID_RELEASE_CERT_SHA256;
  if (!expectedAndroidCertSha256) {
    throw new Error('缺少 ANDROID_RELEASE_CERT_SHA256，无法确认 Android release 签名证书');
  }
  const apksignerPath = await findApkSigner();
  const apkSignatureOutput = await verifySignedApk(apkSourcePath, { apksignerPath });
  assertExpectedApkCertificate(apkSignatureOutput, expectedAndroidCertSha256);
  const notes = (await readFile(notesFile, 'utf8')).trimEnd();
  const pubDate = new Date().toISOString();
  const [dmgTargetName, apkTargetName, androidMetadataTargetName] = releaseAssetNames(version);
  const apkSha256 = await sha256File(apkSourcePath);
  const apkSize = (await readFile(apkSourcePath)).byteLength;
  const githubBaseUrl = `https://github.com/zhuxbo/ai-email/releases/download/v${version}`;
  const apkUrl = `${githubBaseUrl}/${apkTargetName}`;

  await rm(stagingDir, { recursive: true, force: true });
  await mkdir(stagingDir, { recursive: true });
  await copyIntoStaging(apkSourcePath, stagingDir, apkTargetName);
  await copyIntoStaging(dmgSourcePath, stagingDir, dmgTargetName);
  await writeFile(
    path.join(stagingDir, androidMetadataTargetName),
    `${JSON.stringify(
      buildAndroidMetadata({
        version,
        versionCode: androidOutput.versionCode,
        notes,
        pubDate,
        apkUrl,
        apkSize,
        sha256: apkSha256,
      }),
      null,
      2,
    )}\n`,
    'utf8',
  );
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main(process.argv.slice(2));
}
