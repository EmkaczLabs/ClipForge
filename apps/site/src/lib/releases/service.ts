import { RustFSClient } from "../s3/client";
import { isAllowedReleaseKey } from "../s3/key-guard";
import {
  ReleaseIndexSchema,
  type ReleaseArtifact,
  type ReleaseChannel,
  type ReleaseEntry,
  type ReleasePlatform,
} from "./schemas";

const RELEASE_INDEX_KEY = "releases/index.json";
const CACHE_TTL_MS = 60_000;

interface CachedIndex {
  fetchedAt: number;
  releases: ReleaseEntry[];
  reachable: boolean;
}

let cachedIndex: CachedIndex | null = null;

export function clearReleaseCache(): void {
  cachedIndex = null;
}

export interface ReleaseFetchResult {
  configured: boolean;
  reachable: boolean;
  releases: ReleaseEntry[];
}

export async function fetchReleaseIndex(
  client: RustFSClient = new RustFSClient(),
  options?: { bypassCache?: boolean }
): Promise<ReleaseFetchResult> {
  if (!client.configured) {
    return {
      configured: false,
      reachable: false,
      releases: [],
    };
  }

  const now = Date.now();
  if (!options?.bypassCache && cachedIndex && now - cachedIndex.fetchedAt < CACHE_TTL_MS) {
    return {
      configured: true,
      reachable: cachedIndex.reachable,
      releases: cachedIndex.releases,
    };
  }

  try {
    const raw = await client.getObjectText(RELEASE_INDEX_KEY);
    if (raw === null) {
      cachedIndex = { fetchedAt: now, releases: [], reachable: true };
      return { configured: true, reachable: true, releases: [] };
    }

    const parsedJson: unknown = JSON.parse(raw);
    const validated = ReleaseIndexSchema.safeParse(parsedJson);
    if (!validated.success) {
      cachedIndex = { fetchedAt: now, releases: [], reachable: true };
      return { configured: true, reachable: true, releases: [] };
    }

    // Sort newest first by publishedAt
    const sorted = [...validated.data.releases].sort(
      (a, b) => Date.parse(b.publishedAt) - Date.parse(a.publishedAt)
    );

    cachedIndex = { fetchedAt: now, releases: sorted, reachable: true };
    return { configured: true, reachable: true, releases: sorted };
  } catch {
    return {
      configured: true,
      reachable: false,
      releases: cachedIndex?.releases ?? [],
    };
  }
}

export function filterReleases(
  releases: ReleaseEntry[],
  filters: {
    channel?: ReleaseChannel;
    platform?: ReleasePlatform;
    version?: string;
  }
): ReleaseEntry[] {
  return releases
    .filter((rel) => {
      if (filters.channel && rel.channel !== filters.channel) return false;
      if (filters.version && normalizeVersion(rel.version) !== normalizeVersion(filters.version)) {
        return false;
      }
      if (filters.platform) {
        return rel.artifacts.some((a) => a.platform === filters.platform);
      }
      return true;
    })
    .map((rel) => {
      if (!filters.platform) return rel;
      return {
        ...rel,
        artifacts: rel.artifacts.filter((a) => a.platform === filters.platform),
      };
    });
}

export async function getFilteredReleases(
  filters: {
    channel?: ReleaseChannel;
    platform?: ReleasePlatform;
    version?: string;
  },
  client?: RustFSClient
): Promise<ReleaseFetchResult> {
  const result = await fetchReleaseIndex(client);
  return {
    ...result,
    releases: filterReleases(result.releases, filters),
  };
}

export async function getLatestRelease(
  filters: {
    channel?: ReleaseChannel;
    platform?: ReleasePlatform;
  },
  client?: RustFSClient
): Promise<{ configured: boolean; reachable: boolean; release: ReleaseEntry | null }> {
  const result = await getFilteredReleases(filters, client);
  return {
    configured: result.configured,
    reachable: result.reachable,
    release: result.releases[0] ?? null,
  };
}

export async function getReleaseByVersion(
  version: string,
  client?: RustFSClient
): Promise<{ configured: boolean; reachable: boolean; release: ReleaseEntry | null }> {
  const result = await getFilteredReleases({ version }, client);
  return {
    configured: result.configured,
    reachable: result.reachable,
    release: result.releases[0] ?? null,
  };
}

export async function findApprovedArtifactById(
  releaseId: string,
  client?: RustFSClient
): Promise<{
  configured: boolean;
  artifact: ReleaseArtifact | null;
  release: ReleaseEntry | null;
}> {
  const result = await fetchReleaseIndex(client);
  if (!result.configured) {
    return { configured: false, artifact: null, release: null };
  }

  for (const rel of result.releases) {
    for (const artifact of rel.artifacts) {
      if (artifact.id === releaseId && isAllowedReleaseKey(artifact.objectKey)) {
        return { configured: true, artifact, release: rel };
      }
    }
  }

  return { configured: true, artifact: null, release: null };
}

export function normalizeVersion(v: string): string {
  return v.trim().replace(/^v/i, "");
}

/**
 * Compares two semver-like versions (e.g. "0.2.0" vs "0.1.0").
 * Returns > 0 if `a > b`, < 0 if `a < b`, 0 if equal.
 */
export function compareVersions(a: string, b: string): number {
  const cleanA = normalizeVersion(a).split("-")[0];
  const cleanB = normalizeVersion(b).split("-")[0];
  const partsA = cleanA.split(".").map((n) => Number.parseInt(n, 10) || 0);
  const partsB = cleanB.split(".").map((n) => Number.parseInt(n, 10) || 0);
  const len = Math.max(partsA.length, partsB.length);

  for (let i = 0; i < len; i++) {
    const numA = partsA[i] ?? 0;
    const numB = partsB[i] ?? 0;
    if (numA !== numB) return numA - numB;
  }

  // If numeric parts are equal, a stable version (no prerelease suffix) is newer than a prerelease
  const hasPreA = normalizeVersion(a).includes("-");
  const hasPreB = normalizeVersion(b).includes("-");
  if (hasPreA !== hasPreB) {
    return hasPreA ? -1 : 1;
  }

  return 0;
}

export async function checkStorageHealth(
  client: RustFSClient = new RustFSClient()
): Promise<{ configured: boolean; reachable: boolean | null }> {
  if (!client.configured) {
    return { configured: false, reachable: null };
  }

  try {
    await client.headObject(RELEASE_INDEX_KEY);
    return { configured: true, reachable: true };
  } catch {
    return { configured: true, reachable: false };
  }
}
