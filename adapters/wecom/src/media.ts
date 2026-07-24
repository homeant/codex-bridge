import { randomUUID } from "node:crypto";
import { realpath, stat, unlink, writeFile } from "node:fs/promises";
import path from "node:path";

export type InboundImageSource = {
  url: string;
  aesKey?: string;
};

export interface ImageDownloadClient {
  downloadFile(url: string, aesKey?: string): Promise<{ buffer: Buffer; filename?: string }>;
}

const MAX_IMAGES = 5;
const MAX_IMAGE_BYTES = 10 * 1024 * 1024;
const RETENTION_MS = 2 * 60 * 60 * 1000;

export class InboundImageStore {
  private readonly root: Promise<string>;

  constructor(
    private readonly client: ImageDownloadClient,
    mediaRoot: string,
  ) {
    this.root = verifyMediaRoot(mediaRoot);
  }

  async saveImages(messageId: string, sources: InboundImageSource[]): Promise<string[]> {
    if (sources.length === 0) return [];
    if (sources.length > MAX_IMAGES) {
      throw new Error("too many inbound images");
    }

    const root = await this.root;
    const safeMessageId =
      [...messageId]
        .filter((character) => /[a-zA-Z0-9_-]/.test(character))
        .join("")
        .slice(0, 64) || "message";
    const saved: string[] = [];
    try {
      for (const [index, source] of sources.entries()) {
        const { buffer } = await this.client.downloadFile(source.url, source.aesKey);
        if (buffer.length === 0 || buffer.length > MAX_IMAGE_BYTES) {
          throw new Error("inbound image size is invalid");
        }
        const extension = imageExtension(buffer);
        if (!extension) throw new Error("unsupported inbound image format");
        const filename = `${safeMessageId}-${index + 1}-${randomUUID()}.${extension}`;
        const target = path.join(root, filename);
        await writeFile(target, buffer, { flag: "wx", mode: 0o600 });
        saved.push(target);
      }
    } catch (error) {
      await Promise.allSettled(saved.map((target) => unlink(target)));
      throw error;
    }

    const timer = setTimeout(() => {
      void Promise.allSettled(saved.map((target) => unlink(target)));
    }, RETENTION_MS);
    timer.unref();
    return saved;
  }
}

async function verifyMediaRoot(mediaRoot: string): Promise<string> {
  const root = await realpath(mediaRoot);
  const metadata = await stat(root);
  if (!metadata.isDirectory()) throw new Error("inbound media root is not a directory");
  return root;
}

function imageExtension(buffer: Buffer): "png" | "jpg" | "gif" | "webp" | undefined {
  if (
    buffer.length >= 8 &&
    buffer.subarray(0, 8).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]))
  ) {
    return "png";
  }
  if (buffer.length >= 3 && buffer[0] === 0xff && buffer[1] === 0xd8 && buffer[2] === 0xff) {
    return "jpg";
  }
  if (
    buffer.length >= 6 &&
    (buffer.subarray(0, 6).toString("ascii") === "GIF87a" ||
      buffer.subarray(0, 6).toString("ascii") === "GIF89a")
  ) {
    return "gif";
  }
  if (
    buffer.length >= 12 &&
    buffer.subarray(0, 4).toString("ascii") === "RIFF" &&
    buffer.subarray(8, 12).toString("ascii") === "WEBP"
  ) {
    return "webp";
  }
  return undefined;
}
