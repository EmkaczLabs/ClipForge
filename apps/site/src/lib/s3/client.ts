import { assertAllowedReleaseKey } from "./key-guard";
import { createPresignedGetUrl, signS3Request, type SigV4Credentials } from "./sigv4";

export interface RustFSConfig {
  endpoint: string;
  bucket: string;
  region: string;
  accessKeyId?: string;
  secretAccessKey?: string;
}

export function getRustFSConfig(): RustFSConfig {
  const endpoint = (process.env.RUSTFS_ENDPOINT || "https://s3.emkacz.dev").trim();
  const bucket = (process.env.RUSTFS_BUCKET || "clipforge-builds").trim();
  const region = (process.env.RUSTFS_REGION || "us-east-1").trim();
  const accessKeyId = process.env.RUSTFS_RELEASE_ACCESS_KEY_ID?.trim() || undefined;
  const secretAccessKey =
    process.env.RUSTFS_RELEASE_SECRET_ACCESS_KEY?.trim() || undefined;

  return {
    endpoint,
    bucket,
    region,
    accessKeyId,
    secretAccessKey,
  };
}

export function isRustFSConfigured(config: RustFSConfig = getRustFSConfig()): boolean {
  return Boolean(config.accessKeyId && config.secretAccessKey);
}

function requireCredentials(config: RustFSConfig): SigV4Credentials {
  if (!config.accessKeyId || !config.secretAccessKey) {
    throw new Error("RustFS release credentials are not configured");
  }
  return {
    accessKeyId: config.accessKeyId,
    secretAccessKey: config.secretAccessKey,
    region: config.region,
    service: "s3",
  };
}

export class RustFSClient {
  private readonly config: RustFSConfig;
  private readonly fetchFn: typeof fetch;

  constructor(config: RustFSConfig = getRustFSConfig(), fetchFn: typeof fetch = fetch) {
    this.config = config;
    this.fetchFn = fetchFn;
  }

  get configured(): boolean {
    return isRustFSConfigured(this.config);
  }

  async headObject(key: string): Promise<{ exists: boolean; contentLength?: number }> {
    assertAllowedReleaseKey(key);
    const credentials = requireCredentials(this.config);
    const { url, headers } = signS3Request({
      method: "HEAD",
      endpoint: this.config.endpoint,
      bucket: this.config.bucket,
      key,
      credentials,
    });

    const res = await this.fetchFn(url, {
      method: "HEAD",
      headers,
      cache: "no-store",
      signal: AbortSignal.timeout(8000),
    });

    if (res.status === 404) {
      return { exists: false };
    }
    if (!res.ok) {
      throw new Error(`RustFS HEAD failed with status ${res.status}`);
    }

    const lenHeader = res.headers.get("content-length");
    const contentLength = lenHeader ? Number.parseInt(lenHeader, 10) : undefined;
    return {
      exists: true,
      contentLength: Number.isFinite(contentLength) ? contentLength : undefined,
    };
  }

  async getObjectText(key: string): Promise<string | null> {
    assertAllowedReleaseKey(key);
    const credentials = requireCredentials(this.config);
    const { url, headers } = signS3Request({
      method: "GET",
      endpoint: this.config.endpoint,
      bucket: this.config.bucket,
      key,
      credentials,
    });

    const res = await this.fetchFn(url, {
      method: "GET",
      headers,
      cache: "no-store",
      signal: AbortSignal.timeout(8000),
    });

    if (res.status === 404) {
      return null;
    }
    if (!res.ok) {
      throw new Error(`RustFS GET failed with status ${res.status}`);
    }
    return await res.text();
  }

  async getObjectStream(key: string): Promise<Response | null> {
    assertAllowedReleaseKey(key);
    const credentials = requireCredentials(this.config);
    const { url, headers } = signS3Request({
      method: "GET",
      endpoint: this.config.endpoint,
      bucket: this.config.bucket,
      key,
      credentials,
    });

    const res = await this.fetchFn(url, {
      method: "GET",
      headers,
      cache: "no-store",
      signal: AbortSignal.timeout(30000),
    });

    if (res.status === 404) {
      return null;
    }
    if (!res.ok) {
      throw new Error(`RustFS stream GET failed with status ${res.status}`);
    }
    return res;
  }

  createPresignedDownloadUrl(key: string, expiresInSeconds = 120): string {
    assertAllowedReleaseKey(key);
    const credentials = requireCredentials(this.config);
    return createPresignedGetUrl({
      endpoint: this.config.endpoint,
      bucket: this.config.bucket,
      key,
      credentials,
      expiresInSeconds,
    });
  }
}
