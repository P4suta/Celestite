import { emit } from "@tauri-apps/api/event";
import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import * as v from "valibot";
import { appearanceSchema, type NoteDocument, type Preferences } from "../../src/lib/contracts";

const fixture = new URLSearchParams(location.search).get("fixture");
const fault = new URLSearchParams(location.search).get("fault");
const FAILED_SAVE_ATTEMPTS = 2;
let failuresRemaining = FAILED_SAVE_ATTEMPTS;
const generatedName = "017f22e2-79b0-7cc3-98c4-dc0c0c07398f.md";
const sample =
  "# A place to think\n\nWrite slowly.\nLeave room for a thought to become clear.\n\nMarkdown stays **visible**.\nA [link](https://example.com) is still a link.\n\n`const idea = simple;`\n\n日本語でも、ただ書くことに集中する。\n";
const linkSample =
  "# Links\n\n🦀 [here](#target)\n\n[other](other.md#target)\n\n<https://example.com>\n\n`[code](#target)`\n\n# Target\n\nKeep writing.\n";
const linkFixture = fixture === "links" || fixture === "linked-heading";
const libraryFixture = fixture === "library";
const libraryId = "019a3f1e-7c2d-7b8a-9e21-0f4b6c8d1a2e";
const libraryNotes = [
  { id: "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90", title: "Object", path: "/Library/a.md" },
  {
    id: "019a3f21-13aa-7c01-8c55-2b9e7d0f3c44",
    title: "Object (philosophy)",
    path: "/Library/b.md",
  },
  { id: "019a3f20-0000-7000-8000-000000000003", title: "Subject", path: "/Library/c.md" },
];
let document: NoteDocument = {
  token: crypto.randomUUID(),
  text: linkFixture
    ? linkSample
    : fixture === "markdown"
      ? sample
      : libraryFixture
        ? `---\nid: ${libraryId}\n---\n\n`
        : "",
  path: linkFixture
    ? "/Notes/Links.md"
    : fixture === "markdown"
      ? "/Notes/A place to think.md"
      : null,
  name: linkFixture
    ? "Links.md"
    : fixture === "markdown"
      ? "A place to think.md"
      : libraryFixture
        ? `${libraryId}.md`
        : generatedName,
  title: null,
  folder: linkFixture || fixture === "markdown" ? "/Notes" : null,
  revision: fault === "revision-limit" ? Number.MAX_SAFE_INTEGER : 0,
  savedRevision: fault === "revision-limit" ? Number.MAX_SAFE_INTEGER : 0,
};
let preferences: Preferences = {
  appearance: "light",
  recentFiles: ["/Notes/A place to think.md"],
  library: libraryFixture ? "/Library" : null,
};
const editSchema = v.object({ token: v.string(), revision: v.number(), text: v.string() });
const payloadSchema = v.object({ edit: editSchema });
const targetSchema = v.variant("kind", [
  v.object({ kind: v.literal("note"), id: v.string() }),
  v.object({ kind: v.literal("asset"), digest: v.string(), extension: v.string() }),
]);

function titleOf(text: string): string | null {
  return /^# (.+)$/mu.exec(text)?.[1]?.trim() ?? null;
}

mockWindows("note-main");
mockIPC(
  (command, payload): unknown => {
    switch (command) {
      case "bootstrap":
        if (fault === "invalid-bootstrap")
          return { document: { ...document, revision: -1 }, preferences, warnings: [] };
        return {
          document,
          preferences,
          warnings: [],
          selection: fixture === "linked-heading" ? linkSample.indexOf("# Target") : null,
        };
      case "follow_link": {
        const { edit, position } = v.parse(
          v.object({ edit: editSchema, position: v.number() }),
          payload,
        );
        if (
          edit.token !== document.token ||
          edit.revision !== document.revision ||
          edit.text !== document.text
        ) {
          throw { kind: "stale", message: "The link request does not match this note." };
        }
        const target = [
          { source: "[here](#target)", action: "heading" },
          { source: "[other](other.md#target)", action: "note" },
          { source: "<https://example.com>", action: "web" },
        ].find((link) => {
          const start = document.text.indexOf(link.source);
          return start >= 0 && position >= start && position <= start + link.source.length;
        });
        if (!target) throw { kind: "link", message: "No link at this position." };
        if (fault === "missing-link")
          throw { kind: "link", message: "The linked file could not be found." };
        const body = window.document.body;
        body.setAttribute("data-link-action", target.action);
        body.setAttribute("data-link-position", String(position));
        return {
          token: edit.token,
          revision: edit.revision,
          selection: target.action === "heading" ? document.text.indexOf("# Target") : null,
        };
      }
      case "checkpoint_document":
      case "save_document_as": {
        const { edit } = v.parse(payloadSchema, payload);
        if (
          command === "checkpoint_document" &&
          (fault === "conflict" || (fault === "disk-full" && failuresRemaining > 0))
        ) {
          if (fault === "disk-full") failuresRemaining -= 1;
          throw {
            kind: fault === "conflict" ? "conflict" : "io",
            message:
              fault === "conflict" ? "The file changed outside Celestite." : "The disk is full.",
          };
        }
        if (edit.token !== document.token || edit.revision < document.revision) {
          throw { kind: "stale", message: "A newer edit has already been accepted." };
        }
        if (command === "save_document_as") {
          const { path } = v.parse(v.object({ path: v.string() }), payload);
          document = { ...document, path, name: path.split("/").at(-1) ?? path, folder: "/Notes" };
        }
        if (
          libraryFixture &&
          document.path === null &&
          edit.text.replace(/^---\n[\s\S]*?\n---\n/u, "").trim().length > 0
        ) {
          document = { ...document, path: `/Library/${libraryId}.md`, folder: "/Library" };
        }
        document = {
          ...document,
          text: edit.text,
          title: titleOf(edit.text),
          revision: edit.revision,
          savedRevision: document.path ? edit.revision : 0,
        };
        if (fault === "future-response" && failuresRemaining > 0) {
          failuresRemaining -= 1;
          return { ...document, revision: edit.revision + 1, savedRevision: edit.revision + 1 };
        }
        return document;
      }
      case "list_library_notes": {
        const { query } = v.parse(v.object({ query: v.string() }), payload);
        const needle = query.trim().toLowerCase();
        return libraryNotes.filter((note) => note.title.toLowerCase().includes(needle));
      }
      case "insert_link": {
        const { edit, link, target } = v.parse(
          v.object({
            edit: editSchema,
            link: v.object({
              from: v.number(),
              to: v.number(),
              words: v.string(),
              image: v.boolean(),
            }),
            target: targetSchema,
          }),
          payload,
        );
        const request = { edit, target, ...link };
        if (
          request.edit.token !== document.token ||
          request.edit.revision !== document.revision ||
          request.edit.text !== document.text
        ) {
          throw { kind: "stale", message: "The link request does not match this note." };
        }
        if (fault === "duplicate-label") {
          throw { kind: "link", message: "This note already links “Obj” elsewhere." };
        }
        const label = request.words.trim();
        const destination =
          request.target.kind === "note"
            ? `${request.target.id}.md`
            : `assets/${request.target.digest}.${request.target.extension}`;
        const body = request.image ? `![${label}]` : `[${label}]`;
        const patches = [{ start: request.from, end: request.to, insert: body }];
        const text = request.edit.text;
        if (!text.includes(`[${label}]: `)) {
          const separator = text.endsWith("\n\n") ? "" : text.endsWith("\n") ? "\n" : "\n\n";
          patches.push({
            start: text.length,
            end: text.length,
            insert: `${separator}[${label}]: ${destination}\n`,
          });
        }
        return {
          token: request.edit.token,
          revision: request.edit.revision,
          patches,
          caret: request.from + body.length,
        };
      }
      case "set_appearance": {
        const { appearance } = v.parse(v.object({ appearance: appearanceSchema }), payload);
        preferences = { ...preferences, appearance };
        void emit("celestite:preferences", preferences);
        return preferences;
      }
      case "inspect_document":
        return false;
      case "reload_document":
        return document;
      case "plugin:dialog|save": {
        const { options } = v.parse(
          v.object({ options: v.object({ defaultPath: v.string() }) }),
          payload,
        );
        window.document.body.setAttribute("data-save-default", options.defaultPath);
        if (fault === "cancel-save") return null;
        if (fault === "conflict") return "/Notes/Recovered copy.md";
        return `/Notes/${options.defaultPath.split("/").at(-1) ?? options.defaultPath}`;
      }
      case "plugin:dialog|open":
        return null;
      case "plugin:window|close":
        void emit("celestite:close-requested");
        return null;
      case "confirm_close":
        window.document.body.setAttribute("data-closed", "true");
        return null;
      case "new_document":
        return null;
      case "plugin:window|minimize":
      case "plugin:window|toggle_maximize":
        return null;
      default:
        throw new Error(`Unexpected mock command: ${command}`);
    }
  },
  { shouldMockEvents: true },
);
