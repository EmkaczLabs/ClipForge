import { z } from "zod";
import { isAllowedReleaseKey } from "../s3/key-guard";

export const ChannelSchema = z.enum(["stable", "beta", "nightly"]);
export type ReleaseChannel = z.infer<typeof ChannelSchema>;

export const PlatformSchema = z.enum(["windows-x64", "linux-x64"]);
export type ReleasePlatform = z.infer<typeof PlatformSchema>;

export const SafeReleaseIdSchema = z
  .string()
  .min(1)
  .max(128)
  .regex(/^[a-zA-Z0-9._-]+$/, "Invalid releaseId format");

export const SafeVersionSchema = z
  .string()
  .min(1)
  .max(64)
  .regex(/^[a-zA-Z0-9._-]+$/, "Invalid version format");

export const SafeFilenameSchema = z
  .string()
  .min(1)
  .max(180)
  .regex(/^[a-zA-Z0-9._-]+$/, "Invalid filename format")
  .refine((val) => !val.includes(".."), "Filename cannot contain '..'");

export const ReleaseArtifactSchema = z.object({
  id: SafeReleaseIdSchema,
  platform: PlatformSchema,
  filename: SafeFilenameSchema,
  objectKey: z
    .string()
    .refine(isAllowedReleaseKey, "Artifact objectKey must be under releases/"),
  sizeBytes: z.number().int().nonnegative(),
  sha256: z
    .string()
    .regex(/^[a-fA-F0-9]{64}$/, "sha256 must be a 64-char hex digest")
    .transform((s) => s.toLowerCase()),
  contentType: z.string().min(1).max(128).default("application/octet-stream"),
});
export type ReleaseArtifact = z.infer<typeof ReleaseArtifactSchema>;

export const ReleaseEntrySchema = z.object({
  version: SafeVersionSchema,
  channel: ChannelSchema,
  publishedAt: z.string().datetime({ offset: true }).or(z.string().datetime()),
  changelog: z.string().max(50_000),
  highlights: z.array(z.string().max(300)).optional(),
  artifacts: z.array(ReleaseArtifactSchema),
});
export type ReleaseEntry = z.infer<typeof ReleaseEntrySchema>;

export const ReleaseIndexSchema = z.object({
  generatedAt: z.string().optional(),
  releases: z.array(ReleaseEntrySchema),
});
export type ReleaseIndex = z.infer<typeof ReleaseIndexSchema>;

export const ReleasesQuerySchema = z.object({
  channel: ChannelSchema.optional(),
  platform: PlatformSchema.optional(),
  version: SafeVersionSchema.optional(),
});

export const LatestReleaseQuerySchema = z.object({
  channel: ChannelSchema.optional(),
  platform: PlatformSchema.optional(),
});

export const UpdateCheckQuerySchema = z.object({
  version: SafeVersionSchema,
  platform: PlatformSchema,
  channel: ChannelSchema.default("stable"),
});
