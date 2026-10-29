import { describe, expect, it } from "vitest";
import { MAX_PASTED_BYTES, mediaExtension, mediaLabel } from "../../src/lib/media";

describe("media naming", () => {
  it.each([
    [{ name: "Screen Shot.PNG", type: "image/png" }, "png"],
    [{ name: "image.png", type: "" }, "png"],
    [{ name: "", type: "image/jpeg" }, "jpg"],
    [{ name: "blob", type: "image/webp" }, "webp"],
    [{ name: "archive.tar.gz", type: "application/gzip" }, "gz"],
    [{ name: ".hidden", type: "application/pdf" }, "pdf"],
    [{ name: "trailing.", type: "audio/mpeg" }, "mp3"],
    [{ name: "", type: "text/plain" }, ""],
  ])("derives the extension of %j", (file, expected) => {
    expect(mediaExtension(file)).toBe(expected);
  });

  it.each([
    ["Screen Shot.png", "Screen Shot"],
    ["diagram", "diagram"],
    ["  spaced .jpg", "spaced"],
    ["", "image"],
    [".png", "image"],
  ])("labels %j as %j", (name, expected) => {
    expect(mediaLabel(name)).toBe(expected);
  });

  it("limits pasted media to 64 MiB", () => {
    expect(MAX_PASTED_BYTES).toBe(67_108_864);
  });
});
