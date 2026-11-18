import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { afterEach, describe, it } from 'node:test'
import { expect } from './expect-shim.mjs';
// @ts-expect-error Repository scripts are executable JavaScript, not TS modules.
import { buildLatestManifest, collectAssets, trustedCommentOf } from './shell-latest-json.mjs';
// A signature as the Tauri CLI writes it: the base64 of a minisign signature text. The lines after the trusted comment
// are placeholders, because the builder reads the trusted comment and leaves the cryptography to minisign.
function signatureFor(file, version) {
    const text = [
        'untrusted comment: signature from tauri secret key',
        'RUQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==',
        `trusted comment: timestamp:1700000000\tfile:${file}\tversion:${version}`,
        'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==',
        '',
    ].join('\n');
    return Buffer.from(text).toString('base64');
}
const REPOSITORY = 'gustavomfg/NOCT';
const VERSION = '1.0.5';
const TAG = 'rust-v1.0.5';
const PUB_DATE = '2026-11-18T12:00:00Z';
function threeTargets(version = VERSION) {
    return [
        {
            target: 'linux-x86_64',
            file: 'unsigned/shell-unsigned-linux-x86_64/NOCT_1.0.5_amd64.AppImage',
            signature: signatureFor('NOCT_1.0.5_amd64.AppImage', version),
        },
        {
            target: 'windows-x86_64',
            file: 'unsigned/shell-unsigned-windows-x86_64/NOCT_1.0.5_x64-setup.exe',
            signature: signatureFor('NOCT_1.0.5_x64-setup.exe', version),
        },
        {
            target: 'darwin-aarch64',
            file: 'unsigned/shell-unsigned-darwin-aarch64/NOCT.app.tar.gz',
            signature: signatureFor('NOCT.app.tar.gz', version),
        },
    ];
}
function build(assets, overrides = {}) {
    return buildLatestManifest({
        version: VERSION,
        tag: TAG,
        repository: REPOSITORY,
        pubDate: PUB_DATE,
        assets,
        ...overrides,
    });
}
describe('latest.json of the shell', () => {
    it('lists each signed artifact under its platform with the tag URL and the signature as written', () => {
        const assets = threeTargets();
        const manifest = build(assets);
        expect(manifest.version).toBe(VERSION);
        expect(manifest.pub_date).toBe(PUB_DATE);
        expect(Object.keys(manifest.platforms)).toEqual(['darwin-aarch64', 'linux-x86_64', 'windows-x86_64']);
        expect(manifest.platforms['linux-x86_64']).toEqual({
            signature: assets[0].signature,
            url: 'https://github.com/gustavomfg/NOCT/releases/download/rust-v1.0.5/NOCT_1.0.5_amd64.AppImage',
        });
        expect(manifest.platforms['darwin-aarch64'].url).toBe('https://github.com/gustavomfg/NOCT/releases/download/rust-v1.0.5/NOCT.app.tar.gz');
    });
    it('refuses a signature made for another version, so an older signed build cannot be served as this one', () => {
        const assets = threeTargets();
        assets[0].signature = signatureFor('NOCT_1.0.5_amd64.AppImage', '1.0.4');
        expect(() => build(assets)).toThrow('names version 1.0.4, not 1.0.5');
    });
    it('refuses a signature made for another file', () => {
        const assets = threeTargets();
        assets[1].signature = signatureFor('NOCT.app.tar.gz', VERSION);
        expect(() => build(assets)).toThrow('names the file NOCT.app.tar.gz');
    });
    it('refuses a tag that does not name the version', () => {
        expect(() => build(threeTargets(), { tag: 'rust-v1.0.6' })).toThrow('does not name version 1.0.5');
    });
    it('refuses the Electron tag of the same version, so a v tag cannot publish a shell manifest', () => {
        expect(() => build(threeTargets(), { tag: 'v1.0.5' })).toThrow('does not name version 1.0.5');
    });
    it('refuses a platform key outside OS-ARCH, and two artifacts for one platform', () => {
        const unknown = threeTargets();
        unknown[0].target = 'linux-riscv64';
        expect(() => build(unknown)).toThrow('is not OS-ARCH');
        const duplicate = threeTargets();
        duplicate[1].target = 'linux-x86_64';
        expect(() => build(duplicate)).toThrow('two artifacts target linux-x86_64');
    });
    it('refuses a signature that is not base64 or has no trusted comment', () => {
        const notBase64 = threeTargets();
        notBase64[0].signature = 'not a signature!';
        expect(() => build(notBase64)).toThrow('is not base64');
        const noComment = threeTargets();
        noComment[0].signature = Buffer.from('untrusted comment: nothing else\n').toString('base64');
        expect(() => build(noComment)).toThrow('has no trusted comment');
    });
});
describe('trustedCommentOf', () => {
    it('reads the timestamp, file and version of the trusted comment', () => {
        const fields = trustedCommentOf(signatureFor('NOCT.app.tar.gz', '1.0.5'));
        expect(fields.get('file')).toBe('NOCT.app.tar.gz');
        expect(fields.get('version')).toBe('1.0.5');
        expect(fields.get('timestamp')).toBe('1700000000');
    });
});
describe('collectAssets', () => {
    const directories = [];
    afterEach(() => {
        for (const directory of directories.splice(0))
            fs.rmSync(directory, { recursive: true, force: true });
    });
    function unsignedTree(layout) {
        const root = fs.mkdtempSync(path.join(os.tmpdir(), 'noct-shell-latest-'));
        directories.push(root);
        for (const [directory, files] of Object.entries(layout)) {
            fs.mkdirSync(path.join(root, directory), { recursive: true });
            for (const file of files) {
                fs.writeFileSync(path.join(root, directory, file), 'artifact');
                fs.writeFileSync(path.join(root, directory, `${file}.sig`), signatureFor(file, VERSION));
            }
        }
        return root;
    }
    it('reads one artifact and its signature for each platform directory', () => {
        const root = unsignedTree({
            'shell-unsigned-linux-x86_64': ['NOCT_1.0.5_amd64.AppImage'],
            'shell-unsigned-darwin-aarch64': ['NOCT.app.tar.gz'],
        });
        const assets = collectAssets(root);
        expect(assets.map((asset) => asset.target).sort()).toEqual(['darwin-aarch64', 'linux-x86_64']);
        expect(build(assets)).toMatchObject({ version: VERSION });
    });
    it('refuses a platform directory that holds two artifacts', () => {
        const root = unsignedTree({ 'shell-unsigned-linux-x86_64': ['one.AppImage', 'two.AppImage'] });
        expect(() => collectAssets(root)).toThrow('must hold exactly one artifact, found 2');
    });
});
