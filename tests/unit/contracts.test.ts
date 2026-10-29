import * as v from "valibot";
import { describe, expect, it } from "vitest";
import {
  bodyIsEmpty,
  bootstrapSchema,
  documentSchema,
  linkEditSchema,
  linkResultSchema,
  linkTargetSchema,
} from "../../src/lib/contracts";
import { frontMatterEnd } from "../../src/lib/editor-completion";
import { validPatches } from "../../src/lib/session";

describe("library contracts", () => {
  const id = "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90";
  it.each([
    ["", true],
    ["  \n", true],
    ["---\nid: x\n---\n", true],
    ["---\nid: x\n---\n\n  \n", true],
    ["\u{FEFF}---\r\nid: x\r\n---\r\n", true],
    ["---\nid: x\n...\n", true],
    ["---\nid: x\n---\ntext", false],
    ["text", false],
    ["---\nunterminated", false],
  ])("treats %j as body-empty: %s", (text, empty) => {
    expect(bodyIsEmpty(text)).toBe(empty);
    expect(frontMatterEnd(text) <= text.length).toBe(true);
  });

  it("measures the front matter prefix in UTF-16 units", () => {
    expect(frontMatterEnd("---\nid: x\n---\nbody")).toBe("---\nid: x\n---\n".length);
    expect(frontMatterEnd("\u{FEFF}body")).toBe(1);
    expect(frontMatterEnd("body")).toBe(0);
  });

  it("accepts note and asset targets only in their exact shapes", () => {
    expect(v.safeParse(linkTargetSchema, { kind: "note", id }).success).toBe(true);
    expect(
      v.safeParse(linkTargetSchema, { kind: "asset", digest: "a".repeat(64), extension: "png" })
        .success,
    ).toBe(true);
    for (const invalid of [
      { kind: "note", id: "not-a-uuid" },
      { kind: "asset", digest: "A".repeat(64), extension: "png" },
      { kind: "asset", digest: "a".repeat(63), extension: "png" },
      { kind: "asset", digest: "a".repeat(64), extension: "PNG" },
      { kind: "asset", digest: "a".repeat(64), extension: "" },
      { kind: "other" },
    ]) {
      expect(v.safeParse(linkTargetSchema, invalid).success).toBe(false);
    }
  });

  it("validates link patches as ordered, disjoint, in-range edits", () => {
    const body = { start: 4, end: 8, insert: "[Obj]" };
    const definition = { start: 8, end: 8, insert: "\n\n[Obj]: x.md\n" };
    const edit = {
      token: "00000000-0000-4000-8000-000000000000",
      revision: 1,
      patches: [body, definition],
      caret: 9,
    };
    expect(v.safeParse(linkEditSchema, edit).success).toBe(true);
    expect(validPatches(edit.patches, edit.caret, 8)).toBe(true);
    expect(validPatches(edit.patches, edit.caret, 7)).toBe(false);
    expect(validPatches([definition, body], edit.caret, 8)).toBe(false);
    expect(validPatches(edit.patches, 100, 8)).toBe(false);
    expect(validPatches([{ start: 2, end: 1, insert: "" }], 0, 8)).toBe(false);
    expect(
      v.safeParse(linkEditSchema, { ...edit, patches: [{ start: 2, end: 1, insert: "" }] }).success,
    ).toBe(false);
  });
});

describe("IPC documents", () => {
  const valid = {
    token: "00000000-0000-4000-8000-000000000000",
    text: "note",
    name: "Untitled",
    folder: null,
    path: null,
    revision: 1,
    savedRevision: 0,
  };
  it.each([
    { revision: -1 },
    { revision: 1.5 },
    { revision: Number.MAX_SAFE_INTEGER + 1 },
    { savedRevision: 2 },
    { token: "not-a-token" },
    { text: null },
  ])("rejects an invalid native response: %j", (invalid) => {
    expect(v.safeParse(documentSchema, { ...valid, ...invalid }).success).toBe(false);
  });
  it("accepts a valid native response", () => {
    expect(v.parse(documentSchema, valid)).toEqual({ ...valid, title: null });
    expect(v.parse(documentSchema, { ...valid, title: "Object" }).title).toBe("Object");
  });

  it.each([-1, 0.5, Number.NaN, Number.POSITIVE_INFINITY, Number.MAX_SAFE_INTEGER + 1])(
    "rejects an unsafe link selection: %s",
    (selection) => {
      expect(
        v.safeParse(linkResultSchema, { token: valid.token, revision: valid.revision, selection })
          .success,
      ).toBe(false);
    },
  );

  it("bounds an initial heading selection against UTF-16 document length", () => {
    const bootstrap = {
      document: { ...valid, text: "🦀\n# Heading" },
      preferences: { appearance: "light", recentFiles: [], library: null },
      warnings: [],
    };
    expect(
      v.safeParse(bootstrapSchema, { ...bootstrap, selection: bootstrap.document.text.length })
        .success,
    ).toBe(true);
    expect(
      v.safeParse(bootstrapSchema, { ...bootstrap, selection: bootstrap.document.text.length + 1 })
        .success,
    ).toBe(false);
    expect(v.safeParse(bootstrapSchema, { ...bootstrap, selection: null }).success).toBe(true);
  });
});
