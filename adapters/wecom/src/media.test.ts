import assert from "node:assert/strict";
import { mkdtemp, readFile, realpath, rm, stat } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { InboundImageStore, type ImageDownloadClient } from "./media.js";

class FakeDownloadClient implements ImageDownloadClient {
  calls: Array<{ url: string; aesKey?: string }> = [];

  constructor(private readonly buffer: Buffer) {}

  async downloadFile(url: string, aesKey?: string): Promise<{ buffer: Buffer }> {
    this.calls.push({ url, aesKey });
    return { buffer: this.buffer };
  }
}

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  Buffer.from("test image bytes"),
]);

test("downloads decrypted images into the private media directory", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "wecom-media-test-"));
  try {
    const client = new FakeDownloadClient(png);
    const store = new InboundImageStore(client, root);
    const saved = await store.saveImages("msg/unsafe", [
      { url: "https://example.invalid/encrypted", aesKey: "aes-key" },
    ]);

    assert.equal(saved.length, 1);
    assert.equal(path.dirname(saved[0] ?? ""), await realpath(root));
    assert.deepEqual(await readFile(saved[0] ?? ""), png);
    assert.deepEqual(client.calls, [
      { url: "https://example.invalid/encrypted", aesKey: "aes-key" },
    ]);
    assert.equal((await stat(saved[0] ?? "")).mode & 0o777, 0o600);
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});

test("rejects unsupported image bytes and excessive image counts", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "wecom-media-test-"));
  try {
    const invalid = new InboundImageStore(new FakeDownloadClient(Buffer.from("not an image")), root);
    await assert.rejects(
      invalid.saveImages("msg-1", [{ url: "https://example.invalid/file" }]),
      /unsupported inbound image format/,
    );

    const valid = new InboundImageStore(new FakeDownloadClient(png), root);
    await assert.rejects(
      valid.saveImages(
        "msg-2",
        Array.from({ length: 6 }, (_, index) => ({ url: `https://example.invalid/${index}` })),
      ),
      /too many inbound images/,
    );
  } finally {
    await rm(root, { recursive: true, force: true });
  }
});
