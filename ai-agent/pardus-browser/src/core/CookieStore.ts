import { mkdirSync, readFileSync, writeFileSync, rmSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { homedir } from 'node:os';
import type { Cookie } from './types.js';

interface PersistedCookieData {
  cookies: Cookie[];
  savedAt: number;
}

export class CookieStore {
  private profilesDir: string;

  constructor(baseDir?: string) {
    this.profilesDir = baseDir ?? join(homedir(), '.pardus-agent', 'profiles');
  }

  saveCookies(profile: string, cookies: Cookie[]): void {
    if (!profile || cookies.length === 0) return;

    const profileDir = join(this.profilesDir, profile);
    mkdirSync(profileDir, { recursive: true });

    const data: PersistedCookieData = {
      cookies,
      savedAt: Date.now(),
    };

    writeFileSync(join(profileDir, 'cookies.json'), JSON.stringify(data, null, 2), 'utf-8');
  }

  loadCookies(profile: string): Cookie[] {
    if (!profile) return [];

    const filePath = join(this.profilesDir, profile, 'cookies.json');
    if (!existsSync(filePath)) return [];

    try {
      const raw = readFileSync(filePath, 'utf-8');
      const data = JSON.parse(raw) as PersistedCookieData;
      return data.cookies ?? [];
    } catch {
      return [];
    }
  }

  deleteProfile(profile: string): void {
    const profileDir = join(this.profilesDir, profile);
    if (existsSync(profileDir)) {
      rmSync(profileDir, { recursive: true, force: true });
    }
  }

  listProfiles(): string[] {
    if (!existsSync(this.profilesDir)) return [];

    try {
      return readdirSync(this.profilesDir).filter(name => {
        const cookieFile = join(this.profilesDir, name, 'cookies.json');
        return existsSync(cookieFile);
      });
    } catch {
      return [];
    }
  }
}

export const cookieStore = new CookieStore();
