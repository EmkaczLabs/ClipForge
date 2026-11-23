import { createHash, createHmac } from "node:crypto";
import { assertAllowedReleaseKey } from "./key-guard";

export interface SigV4Credentials {
  accessKeyId: string;
  secretAccessKey: string;
  region: string;
  service?: string;
}

export interface SignedRequestHeaders extends Record<string, string> {
  host: string;
  "x-amz-date": string;
  "x-amz-content-sha256": string;
  authorization: string;
}

const EMPTY_PAYLOAD_SHA256 =
  "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

export function sha256Hex(data: string | Buffer): string {
  return createHash("sha256").update(data).digest("hex");
}

function hmacSha256(key: Buffer | string, data: string): Buffer {
  return createHmac("sha256", key).update(data, "utf8").digest();
}

export function formatAmzDate(date: Date): { amzDate: string; dateStamp: string } {
  const iso = date.toISOString().replace(/[:-]|\.\d{3}/g, "");
  return {
    amzDate: iso,
    dateStamp: iso.slice(0, 8),
  };
}

/**
 * RFC 3986 URI encoding required by AWS SigV4.
 */
export function encodeRfc3986(value: string): string {
  return encodeURIComponent(value).replace(
    /[!'()*]/g,
    (ch) => `%${ch.charCodeAt(0).toString(16).toUpperCase()}`
  );
}

export function encodeS3Path(path: string): string {
  return path
    .split("/")
    .map((segment) => encodeRfc3986(segment))
    .join("/");
}

function deriveSigningKey(
  secretAccessKey: string,
  dateStamp: string,
  region: string,
  service: string
): Buffer {
  const kDate = hmacSha256(`AWS4${secretAccessKey}`, dateStamp);
  const kRegion = hmacSha256(kDate, region);
  const kService = hmacSha256(kRegion, service);
  return hmacSha256(kService, "aws4_request");
}

export function signS3Request(params: {
  method: "GET" | "HEAD";
  endpoint: string;
  bucket: string;
  key: string;
  credentials: SigV4Credentials;
  now?: Date;
}): { url: string; headers: SignedRequestHeaders } {
  assertAllowedReleaseKey(params.key);

  const endpointUrl = new URL(params.endpoint);
  if (endpointUrl.protocol !== "https:") {
    throw new Error("S3 endpoint must use HTTPS");
  }

  const service = params.credentials.service ?? "s3";
  const now = params.now ?? new Date();
  const { amzDate, dateStamp } = formatAmzDate(now);

  const canonicalUri = `/${encodeRfc3986(params.bucket)}/${encodeS3Path(params.key)}`;
  const host = endpointUrl.host;
  const payloadHash = EMPTY_PAYLOAD_SHA256;

  const canonicalHeaders =
    `host:${host}\n` +
    `x-amz-content-sha256:${payloadHash}\n` +
    `x-amz-date:${amzDate}\n`;
  const signedHeaders = "host;x-amz-content-sha256;x-amz-date";

  const canonicalRequest = [
    params.method,
    canonicalUri,
    "",
    canonicalHeaders,
    signedHeaders,
    payloadHash,
  ].join("\n");

  const credentialScope = `${dateStamp}/${params.credentials.region}/${service}/aws4_request`;
  const stringToSign = [
    "AWS4-HMAC-SHA256",
    amzDate,
    credentialScope,
    sha256Hex(canonicalRequest),
  ].join("\n");

  const signingKey = deriveSigningKey(
    params.credentials.secretAccessKey,
    dateStamp,
    params.credentials.region,
    service
  );
  const signature = createHmac("sha256", signingKey)
    .update(stringToSign, "utf8")
    .digest("hex");

  const authorization =
    `AWS4-HMAC-SHA256 Credential=${params.credentials.accessKeyId}/${credentialScope}, ` +
    `SignedHeaders=${signedHeaders}, Signature=${signature}`;

  const url = `${endpointUrl.origin}${canonicalUri}`;

  return {
    url,
    headers: {
      host,
      "x-amz-date": amzDate,
      "x-amz-content-sha256": payloadHash,
      authorization,
    },
  };
}

export function createPresignedGetUrl(params: {
  endpoint: string;
  bucket: string;
  key: string;
  credentials: SigV4Credentials;
  expiresInSeconds?: number;
  now?: Date;
}): string {
  assertAllowedReleaseKey(params.key);

  const endpointUrl = new URL(params.endpoint);
  if (endpointUrl.protocol !== "https:") {
    throw new Error("S3 endpoint must use HTTPS");
  }

  const expires = Math.min(Math.max(params.expiresInSeconds ?? 120, 10), 300);
  const service = params.credentials.service ?? "s3";
  const now = params.now ?? new Date();
  const { amzDate, dateStamp } = formatAmzDate(now);
  const host = endpointUrl.host;
  const canonicalUri = `/${encodeRfc3986(params.bucket)}/${encodeS3Path(params.key)}`;
  const credentialScope = `${dateStamp}/${params.credentials.region}/${service}/aws4_request`;

  const queryParams: Array<[string, string]> = [
    ["X-Amz-Algorithm", "AWS4-HMAC-SHA256"],
    ["X-Amz-Credential", `${params.credentials.accessKeyId}/${credentialScope}`],
    ["X-Amz-Date", amzDate],
    ["X-Amz-Expires", String(expires)],
    ["X-Amz-SignedHeaders", "host"],
  ];

  queryParams.sort(([a], [b]) => a.localeCompare(b));

  const canonicalQuery = queryParams
    .map(([k, v]) => `${encodeRfc3986(k)}=${encodeRfc3986(v)}`)
    .join("&");

  const canonicalHeaders = `host:${host}\n`;
  const signedHeaders = "host";

  const canonicalRequest = [
    "GET",
    canonicalUri,
    canonicalQuery,
    canonicalHeaders,
    signedHeaders,
    "UNSIGNED-PAYLOAD",
  ].join("\n");

  const stringToSign = [
    "AWS4-HMAC-SHA256",
    amzDate,
    credentialScope,
    sha256Hex(canonicalRequest),
  ].join("\n");

  const signingKey = deriveSigningKey(
    params.credentials.secretAccessKey,
    dateStamp,
    params.credentials.region,
    service
  );
  const signature = createHmac("sha256", signingKey)
    .update(stringToSign, "utf8")
    .digest("hex");

  return `${endpointUrl.origin}${canonicalUri}?${canonicalQuery}&X-Amz-Signature=${signature}`;
}
