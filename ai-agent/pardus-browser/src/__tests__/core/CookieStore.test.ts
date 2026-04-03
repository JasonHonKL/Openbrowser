import { describe, it, beforeEach, afterEach } from 'node:test';
import assert from 'node:assert';
import { CookieStore } from '../../core/CookieStore.js';
import { mkdirSync, writeFileSync, rmSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

describe('CookieStore', () => {
  let store: CookieStore;
  let testDir: string;

  beforeEach(() => {
    testDir = join(tmpdir(), `pardus-test-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`);
    mkdirSync(testDir, { recursive: true });
    store = new CookieStore(testDir);
  });

  afterEach(() => {
    if (existsSync(testDir)) {
      rmSync(testDir, { recursive: true, force: true });
    }
  });

  describe('saveCookies / loadCookies', () => {
    it('should save and load cookies round-trip', () => {
      const cookies = [
        { name: 'session', value: 'abc123', domain: '.example.com', path: '/', sameSite: 'Lax' as const },
        { name: 'token', value: 'xyz789', domain: '.example.com', path: '/api', secure: true, httpOnly: true },
      ];

      store.saveCookies('test-profile', cookies);

      const loaded = store.loadCookies('test-profile');
      assert.strictEqual(loaded.length, 2);
      assert.strictEqual(loaded[0].name, 'session');
      assert.strictEqual(loaded[0].value, 'abc123');
      assert.strictEqual(loaded[0].domain, '.example.com');
      assert.strictEqual(loaded[1].name, 'token');
      assert.strictEqual(loaded[1].secure, true);
      assert.strictEqual(loaded[1].httpOnly, true);
    });

    it('should return empty array for non-existent profile', () => {
      const loaded = store.loadCookies('non-existent');
      assert.deepStrictEqual(loaded, []);
    });

    it('should return empty array for empty profile name', () => {
      const loaded = store.loadCookies('');
      assert.deepStrictEqual(loaded, []);
    });

    it('should overwrite existing cookies on save', () => {
      store.saveCookies('test', [{ name: 'a', value: '1' }]);
      store.saveCookies('test', [{ name: 'b', value: '2' }, { name: 'c', value: '3' }]);

      const loaded = store.loadCookies('test');
      assert.strictEqual(loaded.length, 2);
      assert.strictEqual(loaded[0].name, 'b');
    });

    it('should not save empty cookie array', () => {
      store.saveCookies('test', []);

      const loaded = store.loadCookies('test');
      assert.deepStrictEqual(loaded, []);
    });

    it('should not save when profile is empty', () => {
      store.saveCookies('', [{ name: 'a', value: '1' }]);

      const loaded = store.loadCookies('');
      assert.deepStrictEqual(loaded, []);
    });

    it('should create the profile directory if it does not exist', () => {
      store.saveCookies('new-profile', [{ name: 'test', value: 'val' }]);

      const loaded = store.loadCookies('new-profile');
      assert.strictEqual(loaded.length, 1);
    });
  });

  describe('corrupt data handling', () => {
    it('should return empty array for corrupt JSON', () => {
      const profileDir = join(testDir, 'corrupt');
      mkdirSync(profileDir, { recursive: true });
      writeFileSync(join(profileDir, 'cookies.json'), 'not valid json{{{');

      const loaded = store.loadCookies('corrupt');
      assert.deepStrictEqual(loaded, []);
    });

    it('should return empty array when cookies field is missing', () => {
      const profileDir = join(testDir, 'no-cookies');
      mkdirSync(profileDir, { recursive: true });
      writeFileSync(join(profileDir, 'cookies.json'), JSON.stringify({ savedAt: Date.now() }));

      const loaded = store.loadCookies('no-cookies');
      assert.deepStrictEqual(loaded, []);
    });
  });

  describe('deleteProfile', () => {
    it('should delete an existing profile', () => {
      store.saveCookies('to-delete', [{ name: 'a', value: '1' }]);

      assert.ok(store.loadCookies('to-delete').length > 0);
      store.deleteProfile('to-delete');

      const loaded = store.loadCookies('to-delete');
      assert.deepStrictEqual(loaded, []);
    });

    it('should not throw for non-existent profile', () => {
      assert.doesNotThrow(() => store.deleteProfile('non-existent'));
    });
  });

  describe('listProfiles', () => {
    it('should list profiles that have cookies', () => {
      store.saveCookies('profile-a', [{ name: 'a', value: '1' }]);
      store.saveCookies('profile-b', [{ name: 'b', value: '2' }]);

      const profiles = store.listProfiles();
      assert.ok(profiles.includes('profile-a'));
      assert.ok(profiles.includes('profile-b'));
      assert.strictEqual(profiles.length, 2);
    });

    it('should not list profiles with empty cookies (not saved)', () => {
      store.saveCookies('empty', []);

      const profiles = store.listProfiles();
      assert.ok(!profiles.includes('empty'));
    });

    it('should return empty array when no profiles exist', () => {
      const profiles = store.listProfiles();
      assert.deepStrictEqual(profiles, []);
    });
  });
});
