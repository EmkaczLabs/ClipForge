const ALLOWED_PREFIX = "releases/";
const FORBIDDEN_PREFIXES = ["ci/", "nightly/", ".git/", "tmp/"];

export class UnsafeObjectKeyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "UnsafeObjectKeyError";
  }
}

/**
 * Validates that an S3 object key strictly belongs to the public `releases/`
 * prefix and contains no path traversal, encoded dots/slashes, or control chars.
 */
export function isAllowedReleaseKey(rawKey: string): boolean {
  if (!rawKey || typeof rawKey !== "string") {
    return false;
  }

  // Reject control characters, whitespace, backslashes, query/fragment delimiters
  if (/[\x00-\x1f\x7f\\?#\s]/.test(rawKey)) {
    return false;
  }

  // Reject URL-encoded traversal or slashes
  const lower = rawKey.toLowerCase();
  if (lower.includes("%2e") || lower.includes("%2f") || lower.includes("%5c")) {
    return false;
  }

  // Reject leading/trailing slashes or empty path segments
  if (rawKey.startsWith("/") || rawKey.endsWith("/") || rawKey.includes("//")) {
    return false;
  }

  // Reject any forbidden prefix
  for (const prefix of FORBIDDEN_PREFIXES) {
    if (lower.startsWith(prefix)) {
      return false;
    }
  }

  if (!rawKey.startsWith(ALLOWED_PREFIX)) {
    return false;
  }

  // Validate each segment
  const segments = rawKey.split("/");
  if (segments.length < 2) {
    return false;
  }

  for (const seg of segments) {
    if (!seg || seg === "." || seg === "..") {
      return false;
    }
    if (!/^[a-zA-Z0-9._-]+$/.test(seg)) {
      return false;
    }
  }

  return true;
}

export function assertAllowedReleaseKey(rawKey: string): string {
  if (!isAllowedReleaseKey(rawKey)) {
    throw new UnsafeObjectKeyError(
      "Object key rejected: only safe keys under the releases/ prefix are permitted"
    );
  }
  return rawKey;
}
