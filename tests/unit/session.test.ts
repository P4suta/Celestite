import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  type Bootstrap,
  type DesktopBridge,
  DesktopError,
  type EditRequest,
  type LinkPatch,
  type NoteDocument,
  type Preferences,
} from "../../src/lib/contracts";
import { NoteSession } from "../../src/lib/session";

const token = "00000000-0000-4000-8000-000000000000";
const generatedName = "017f22e2-79b0-7cc3-98c4-dc0c0c07398f.md";
function note(path: string | null = null): NoteDocument {
  return {
    token,
    text: "",
    path,
    name: path ? "Note.md" : generatedName,
    title: null,
    folder: path ? "/Notes" : null,
    revision: 0,
    savedRevision: 0,
  };
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve: (value: T) => void = () => {
    throw new Error("Promise not initialized");
  };
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

type ScheduleEvent = "edit" | "flush" | "acknowledge";

function schedules(edits = 0, flushes = 0, acknowledgements = 0): ScheduleEvent[][] {
  const depth = 3;
  if (acknowledgements === depth) return [[]];
  const result: ScheduleEvent[][] = [];
  if (edits < depth)
    for (const rest of schedules(edits + 1, flushes, acknowledgements))
      result.push(["edit", ...rest]);
  if (flushes < edits)
    for (const rest of schedules(edits, flushes + 1, acknowledgements))
      result.push(["flush", ...rest]);
  if (acknowledgements < flushes)
    for (const rest of schedules(edits, flushes, acknowledgements + 1))
      result.push(["acknowledge", ...rest]);
  return result;
}

function fakeBridge(document: NoteDocument): DesktopBridge {
  const preferences: Preferences = { appearance: "light", recentFiles: [], library: null };
  return {
    bootstrap: vi.fn(async (): Promise<Bootstrap> => ({ document, preferences, warnings: [] })),
    listLibraryNotes: vi.fn(async (): Promise<[]> => []),
    insertLink: vi.fn(async (edit: EditRequest, range: { from: number; to: number }) => ({
      token: edit.token,
      revision: edit.revision,
      patches: [{ start: range.from, end: range.to, insert: "[Obj]" }],
      caret: range.from + 5,
    })),
    checkpoint: vi.fn(
      async (edit: EditRequest): Promise<NoteDocument> => ({
        ...document,
        ...edit,
        savedRevision: document.path ? edit.revision : 0,
      }),
    ),
    saveAs: vi.fn(
      async (edit: EditRequest, path: string): Promise<NoteDocument> => ({
        ...document,
        ...edit,
        path,
        savedRevision: edit.revision,
      }),
    ),
    open: vi.fn(async (): Promise<{ document: null }> => ({ document: null })),
    followLink: vi.fn(async (edit: EditRequest) => ({
      token: edit.token,
      revision: edit.revision,
      selection: null,
    })),
    newWindow: vi.fn(async (): Promise<void> => {}),
    inspect: vi.fn(async (): Promise<boolean> => false),
    reload: vi.fn(async (): Promise<NoteDocument> => document),
    setAppearance: vi.fn(async (): Promise<Preferences> => preferences),
    confirmClose: vi.fn(async (): Promise<void> => {}),
    chooseOpen: vi.fn(async (): Promise<string | null> => null),
    chooseSave: vi.fn(async (): Promise<string | null> => null),
    requestClose: vi.fn(async (): Promise<void> => {}),
    minimize: vi.fn(async (): Promise<void> => {}),
    maximize: vi.fn(async (): Promise<void> => {}),
    onClose: vi.fn(async (): Promise<() => void> => () => {}),
    onPreferences: vi.fn(async (): Promise<() => void> => () => {}),
    onAssetDropped: vi.fn(async (): Promise<() => void> => () => {}),
    onAssetFailed: vi.fn(async (): Promise<() => void> => () => {}),
    storeAsset: vi.fn(async (_bytes: Uint8Array, extension: string) => ({
      digest: "a".repeat(64),
      extension,
    })),
  };
}

describe("note session", () => {
  const sessions: NoteSession[] = [];
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    for (const session of sessions) session.dispose();
    sessions.length = 0;
    vi.useRealTimers();
  });

  async function start(bridge: DesktopBridge): Promise<NoteSession> {
    const session = new NoteSession(bridge, () => {});
    sessions.push(session);
    await session.initialize();
    return session;
  }

  it("checkpoints before following a link and applies only a matching selection", async () => {
    const bridge = fakeBridge(note(null));
    const moved = vi.fn();
    const session = new NoteSession(bridge, () => {}, moved);
    sessions.push(session);
    await session.initialize();
    session.edit("[jump](#heading)\n# Heading");
    vi.mocked(bridge.followLink).mockImplementation(async (edit) => {
      expect(bridge.checkpoint).toHaveBeenCalledWith(edit);
      return {
        token: edit.token,
        revision: edit.revision,
        selection: edit.text.indexOf("# Heading"),
      };
    });
    await session.followLink(2);
    expect(bridge.followLink).toHaveBeenCalledWith(expect.objectContaining({ revision: 1 }), 2);
    expect(moved).toHaveBeenCalledWith(session.state.text.indexOf("# Heading"));
    expect(session.state.text).toBe("[jump](#heading)\n# Heading");
    expect(session.state.busy).toBe(false);
  });

  it("keeps a failed checkpoint visible and does not follow its link", async () => {
    const bridge = fakeBridge(note(null));
    const session = await start(bridge);
    session.edit("[jump](other.md)");
    vi.mocked(bridge.checkpoint).mockRejectedValue(new DesktopError("io", "Disk full"));
    await session.followLink(2);
    expect(bridge.followLink).not.toHaveBeenCalled();
    expect(session.state.notice).toEqual({ kind: "saveFailed", message: "Disk full" });
    expect(session.state.text).toBe("[jump](other.md)");
  });

  it.each([-1, 0.5, Number.NaN, Number.POSITIVE_INFINITY, Number.MAX_SAFE_INTEGER])(
    "refuses invalid link position %s before invoking the desktop",
    async (position) => {
      const bridge = fakeBridge(note(null));
      const session = await start(bridge);
      await session.followLink(position);
      expect(bridge.followLink).not.toHaveBeenCalled();
      expect(bridge.checkpoint).not.toHaveBeenCalled();
    },
  );

  it("freezes editing and refuses duplicate navigation while a link is opening", async () => {
    const bridge = fakeBridge(note(null));
    const pending = deferred<{ token: string; revision: number; selection: null }>();
    vi.mocked(bridge.followLink).mockReturnValue(pending.promise);
    const session = await start(bridge);
    const first = session.followLink(0);
    await vi.advanceTimersByTimeAsync(0);
    expect(session.state.busy).toBe(true);
    session.edit("too late");
    await session.followLink(0);
    expect(bridge.followLink).toHaveBeenCalledTimes(1);
    pending.resolve({ token: session.state.document?.token ?? "", revision: 0, selection: null });
    await first;
    expect(session.state.text).toBe("");
    expect(session.state.busy).toBe(false);
  });

  it("does not move the cursor when a pending link completes after disposal", async () => {
    const bridge = fakeBridge(note(null));
    const moved = vi.fn();
    const pending = deferred<{ token: string; revision: number; selection: number }>();
    vi.mocked(bridge.followLink).mockReturnValue(pending.promise);
    const session = new NoteSession(bridge, () => {}, moved);
    sessions.push(session);
    await session.initialize();
    const opening = session.followLink(0);
    await vi.advanceTimersByTimeAsync(0);
    session.dispose();
    pending.resolve({ token: session.state.document?.token ?? "", revision: 0, selection: 0 });
    await opening;
    expect(moved).not.toHaveBeenCalled();
  });

  it.each(["token", "revision", "selection"] as const)(
    "rejects an invalid link response %s without moving the cursor",
    async (field) => {
      const bridge = fakeBridge(note(null));
      const moved = vi.fn();
      const session = new NoteSession(bridge, () => {}, moved);
      sessions.push(session);
      await session.initialize();
      vi.mocked(bridge.followLink).mockImplementation(async (edit) => ({
        token: field === "token" ? "other" : edit.token,
        revision: field === "revision" ? edit.revision + 1 : edit.revision,
        selection: field === "selection" ? edit.text.length + 1 : 0,
      }));
      await session.followLink(0);
      expect(moved).not.toHaveBeenCalled();
      expect(session.state.notice?.kind).toBe("linkFailed");
    },
  );

  it.each(schedules().map((events) => [events.join(" / "), events] as const))(
    "preserves the latest edit throughout this complete three-edit schedule: %s",
    async (_name, events) => {
      const document = note("/Notes/Note.md");
      const bridge = fakeBridge(document);
      const pending: { edit: EditRequest; resolve: (value: NoteDocument) => void }[] = [];
      vi.mocked(bridge.checkpoint).mockImplementation((edit) => {
        const checkpoint = deferred<NoteDocument>();
        pending.push({ edit, resolve: checkpoint.resolve });
        expect(pending).toHaveLength(1);
        return checkpoint.promise;
      });
      const session = await start(bridge);
      const writes: { promise: Promise<boolean>; completed: boolean }[] = [];
      let edits = 0;
      let acknowledged = 0;
      let saved = 0;
      for (const event of events) {
        if (event === "edit") {
          edits += 1;
          session.edit(`revision ${edits} 日本語 🦀`);
        } else if (event === "flush") {
          const write = { promise: session.flush(), completed: false };
          void write.promise.then(() => {
            write.completed = true;
          });
          writes.push(write);
        } else {
          const write = writes.at(acknowledged);
          if (!write) throw new Error("The schedule acknowledges an absent write.");
          if (!write.completed) {
            const checkpoint = pending.shift();
            if (!checkpoint) throw new Error("The queued write did not enter the bridge.");
            saved = checkpoint.edit.revision;
            checkpoint.resolve({ ...document, ...checkpoint.edit, savedRevision: saved });
          }
          expect(await write.promise).toBe(true);
          acknowledged += 1;
        }
        await vi.advanceTimersByTimeAsync(0);
        expect(session.state.text).toBe(edits ? `revision ${edits} 日本語 🦀` : "");
        expect(session.state.revision).toBe(edits);
        expect(session.state.savedRevision).toBe(saved);
        expect(session.dirty).toBe(saved !== edits);
      }
      expect(pending).toHaveLength(0);
      expect(session.dirty).toBe(false);
    },
  );

  it("enumerates every linear extension of the three ordered edit, flush, and acknowledgement streams", () => {
    expect(schedules()).toHaveLength(42);
    expect(new Set(schedules().map((events) => events.join(","))).size).toBe(42);
  });

  it("checkpoints an untitled note after a pause without marking it saved", async () => {
    const bridge = fakeBridge(note());
    const session = await start(bridge);
    session.edit("日本語のメモ");
    await vi.advanceTimersByTimeAsync(799);
    expect(bridge.checkpoint).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(bridge.checkpoint).toHaveBeenCalledWith({ token, revision: 1, text: "日本語のメモ" });
    expect(session.dirty).toBe(true);
    await session.flush();
    expect(bridge.checkpoint).toHaveBeenCalledTimes(1);
  });

  it("keeps newer typing when an older save acknowledgement arrives", async () => {
    const document = note("/Notes/Note.md");
    const bridge = fakeBridge(document);
    const pending = deferred<NoteDocument>();
    vi.mocked(bridge.checkpoint).mockReturnValueOnce(pending.promise);
    const session = await start(bridge);
    session.edit("first");
    const saving = session.flush();
    await Promise.resolve();
    session.edit("first, then second");
    pending.resolve({ ...document, text: "first", revision: 1, savedRevision: 1 });
    await saving;
    expect(session.state.text).toBe("first, then second");
    expect(session.state.revision).toBe(2);
    expect(session.dirty).toBe(true);
    await session.flush();
    expect(session.dirty).toBe(false);
  });

  it.each([
    { token: "00000000-0000-4000-8000-000000000001" },
    { revision: 2, savedRevision: 2 },
    { revision: 0, savedRevision: 0 },
    { text: "different bytes" },
    { savedRevision: 2 },
  ])("refuses an acknowledgement that does not describe the sent edit: %j", async (change) => {
    const document = note("/Notes/Note.md");
    const bridge = fakeBridge(document);
    vi.mocked(bridge.checkpoint).mockResolvedValueOnce({
      ...document,
      text: "keep this",
      revision: 1,
      savedRevision: 1,
      ...change,
    });
    const session = await start(bridge);
    session.edit("keep this");
    expect(await session.flush()).toBe(false);
    expect(session.state.text).toBe("keep this");
    expect(session.state.revision).toBe(1);
    expect(session.state.savedRevision).toBe(0);
    expect(session.state.notice?.kind).toBe("saveFailed");
    expect(await session.flush()).toBe(true);
    expect(session.dirty).toBe(false);
  });

  it("serializes writes while preserving the latest revision", async () => {
    const document = note("/Notes/Note.md");
    const bridge = fakeBridge(document);
    const pending = deferred<NoteDocument>();
    vi.mocked(bridge.checkpoint).mockReturnValueOnce(pending.promise);
    const session = await start(bridge);
    session.edit("one");
    const first = session.flush();
    await Promise.resolve();
    session.edit("two");
    const second = session.flush();
    expect(bridge.checkpoint).toHaveBeenCalledTimes(1);
    pending.resolve({ ...document, text: "one", revision: 1, savedRevision: 1 });
    await Promise.all([first, second]);
    expect(vi.mocked(bridge.checkpoint).mock.calls.map(([edit]) => edit.revision)).toEqual([1, 2]);
    expect(session.state.text).toBe("two");
    expect(session.state.savedRevision).toBe(2);
  });

  it("keeps a failed save dirty and allows retry", async () => {
    const bridge = fakeBridge(note("/Notes/Note.md"));
    vi.mocked(bridge.checkpoint).mockRejectedValueOnce(new DesktopError("io", "Disk full"));
    const session = await start(bridge);
    session.edit("keep me");
    expect(await session.flush()).toBe(false);
    expect(session.state.notice?.kind).toBe("saveFailed");
    expect(session.state.text).toBe("keep me");
    expect(session.dirty).toBe(true);
    expect(await session.flush()).toBe(true);
    expect(session.dirty).toBe(false);
  });

  it("keeps an edit at the revision limit dirty and refuses closing without a choice", async () => {
    const document = {
      ...note("/Notes/Note.md"),
      revision: Number.MAX_SAFE_INTEGER,
      savedRevision: Number.MAX_SAFE_INTEGER,
    };
    const bridge = fakeBridge(document);
    const session = await start(bridge);
    session.edit("never lose this edit");
    expect(session.dirty).toBe(true);
    expect(await session.flush()).toBe(false);
    await session.requestClose();
    expect(session.state.closePrompt).toBe(true);
    expect(session.state.notice?.kind).toBe("saveFailed");
    expect(bridge.confirmClose).not.toHaveBeenCalled();
    expect(session.state.text).toBe("never lose this edit");
  });

  it("releases an acquired listener if another listener fails during startup", async () => {
    const bridge = fakeBridge(note());
    const cleanup = vi.fn();
    vi.mocked(bridge.onClose).mockResolvedValue(cleanup);
    vi.mocked(bridge.onPreferences).mockRejectedValue(new Error("subscription failed"));
    const session = await start(bridge);
    expect(session.state.notice?.kind).toBe("startup");
    expect(cleanup).toHaveBeenCalledTimes(1);
    session.dispose();
    expect(cleanup).toHaveBeenCalledTimes(1);
  });

  it("does not acquire listeners when disposal precedes the bootstrap response", async () => {
    const bridge = fakeBridge(note());
    const pending = deferred<Bootstrap>();
    vi.mocked(bridge.bootstrap).mockReturnValue(pending.promise);
    const session = new NoteSession(bridge, () => {});
    sessions.push(session);
    const initializing = session.initialize();
    session.dispose();
    pending.resolve({
      document: note(),
      preferences: { appearance: "light", recentFiles: [], library: null },
      warnings: [],
    });
    await initializing;
    expect(bridge.onClose).not.toHaveBeenCalled();
    expect(bridge.onPreferences).not.toHaveBeenCalled();
  });

  it("checkpoints before inserting a link and applies patches as one editor change", async () => {
    const bridge = fakeBridge(note(null));
    const patched = vi.fn((_patches: readonly LinkPatch[], _caret: number) => {
      session.edit("See [Obj]\n\n[Obj]: a.md\n");
    });
    const session = new NoteSession(
      bridge,
      () => {},
      () => {},
      patched,
    );
    sessions.push(session);
    await session.initialize();
    session.edit("See [Obj");
    vi.mocked(bridge.insertLink).mockImplementation(async (edit, range) => {
      expect(bridge.checkpoint).toHaveBeenCalledWith(edit);
      return {
        token: edit.token,
        revision: edit.revision,
        patches: [
          { start: range.from, end: range.to, insert: "[Obj]" },
          { start: edit.text.length, end: edit.text.length, insert: "\n\n[Obj]: a.md\n" },
        ],
        caret: range.from + 5,
      };
    });
    const target = { kind: "note", id: "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90" } as const;
    expect(await session.insertLink({ from: 4, to: 8 }, "Obj", target, false)).toBe(true);
    expect(bridge.insertLink).toHaveBeenCalledWith(
      expect.objectContaining({ revision: 1, text: "See [Obj" }),
      { from: 4, to: 8 },
      "Obj",
      target,
      false,
    );
    expect(patched).toHaveBeenCalledTimes(1);
    expect(patched.mock.calls[0]?.[1]).toBe(9);
    expect(session.state.text).toBe("See [Obj]\n\n[Obj]: a.md\n");
    expect(session.state.revision).toBe(2);
    expect(session.state.busy).toBe(false);
    expect(session.state.notice).toBeNull();
  });

  it("stores pasted files in order and chains their image references", async () => {
    const bridge = fakeBridge(note(null));
    const patched = vi.fn((patches: readonly LinkPatch[]) => {
      let text = session.state.text;
      for (const patch of [...patches].reverse())
        text = `${text.slice(0, patch.start)}${patch.insert}${text.slice(patch.end)}`;
      session.edit(text);
    });
    const session = new NoteSession(
      bridge,
      () => {},
      () => {},
      patched,
    );
    sessions.push(session);
    await session.initialize();
    session.edit("# Shots\n\n");
    vi.mocked(bridge.storeAsset).mockImplementation(async (bytes, extension) => ({
      digest: String(bytes.length).padStart(64, "0"),
      extension,
    }));
    vi.mocked(bridge.insertLink).mockImplementation(async (edit, range, words) => ({
      token: edit.token,
      revision: edit.revision,
      patches: [{ start: range.from, end: range.to, insert: `![${words}]` }],
      caret: range.from + words.length + 3,
    }));
    const files = [
      new File([new Uint8Array(3)], "Screen Shot.png", { type: "image/png" }),
      new File([new Uint8Array(5)], "", { type: "image/jpeg" }),
    ];
    await session.addMedia(files, 9);
    expect(session.state.notice).toBeNull();
    expect(bridge.storeAsset).toHaveBeenNthCalledWith(1, expect.any(Uint8Array), "png");
    expect(bridge.storeAsset).toHaveBeenNthCalledWith(2, expect.any(Uint8Array), "jpg");
    expect(bridge.insertLink).toHaveBeenNthCalledWith(
      1,
      expect.objectContaining({ text: "# Shots\n\n" }),
      { from: 9, to: 9 },
      "Screen Shot",
      { kind: "asset", digest: "3".padStart(64, "0"), extension: "png" },
      true,
    );
    expect(bridge.insertLink).toHaveBeenNthCalledWith(
      2,
      expect.anything(),
      { from: 9 + "![Screen Shot]".length, to: 9 + "![Screen Shot]".length },
      "image",
      { kind: "asset", digest: "5".padStart(64, "0"), extension: "jpg" },
      true,
    );
    expect(patched).toHaveBeenCalledTimes(2);
    expect(session.state.busy).toBe(false);
  });

  it("stops a media sequence at an oversized file or a refused store", async () => {
    const bridge = fakeBridge(note(null));
    const session = await start(bridge);
    session.edit("text");
    const big = { name: "huge.png", type: "image/png", size: 64 * 1024 * 1024 + 1 } as File;
    await session.addMedia([big], 4);
    expect(bridge.storeAsset).not.toHaveBeenCalled();
    expect(session.state.notice?.kind).toBe("linkFailed");
    vi.mocked(bridge.storeAsset).mockRejectedValue(new DesktopError("link", "Not supported"));
    await session.addMedia([new File([new Uint8Array(1)], "a.exe")], 4);
    expect(bridge.insertLink).not.toHaveBeenCalled();
    expect(session.state.notice).toEqual({ kind: "linkFailed", message: "Not supported" });
  });

  it("places a natively dropped asset at the editor caret", async () => {
    const bridge = fakeBridge(note(null));
    let dropped: ((drop: { digest: string; extension: string; label: string }) => void) | undefined;
    vi.mocked(bridge.onAssetDropped).mockImplementation(async (handler) => {
      dropped = handler;
      return () => {};
    });
    const patched = vi.fn();
    const session = new NoteSession(
      bridge,
      () => {},
      () => {},
      patched,
      () => 2,
    );
    sessions.push(session);
    await session.initialize();
    session.edit("ab cd");
    dropped?.({ digest: "b".repeat(64), extension: "pdf", label: "paper" });
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(0);
    expect(bridge.insertLink).toHaveBeenCalledWith(
      expect.objectContaining({ text: "ab cd" }),
      { from: 2, to: 2 },
      "paper",
      { kind: "asset", digest: "b".repeat(64), extension: "pdf" },
      true,
    );
    expect(patched).toHaveBeenCalledTimes(1);
  });

  it("refuses link insertion for invalid ranges, stale text, and bad responses", async () => {
    const target = { kind: "note", id: "019a3f25-9b0e-7d12-a3c7-5e1f8a2b6d90" } as const;
    const bridge = fakeBridge(note(null));
    const patched = vi.fn();
    const session = new NoteSession(
      bridge,
      () => {},
      () => {},
      patched,
    );
    sessions.push(session);
    await session.initialize();
    session.edit("See [Obj");
    for (const range of [
      { from: -1, to: 2 },
      { from: 5, to: 4 },
      { from: 0, to: 9 },
      { from: 0.5, to: 2 },
    ]) {
      expect(await session.insertLink(range, "Obj", target, false)).toBe(false);
    }
    expect(bridge.insertLink).not.toHaveBeenCalled();
    vi.mocked(bridge.insertLink).mockImplementation(async (edit, range) => {
      session.edit("See [Obj changed");
      return {
        token: edit.token,
        revision: edit.revision,
        patches: [{ start: range.from, end: range.to, insert: "[Obj]" }],
        caret: range.from + 5,
      };
    });
    expect(await session.insertLink({ from: 4, to: 8 }, "Obj", target, false)).toBe(true);
    expect(patched).toHaveBeenCalledTimes(1);
    expect(session.state.text).toBe("See [Obj");
    patched.mockClear();
    for (const field of ["token", "revision", "patches", "caret"] as const) {
      vi.mocked(bridge.insertLink).mockImplementation(async (edit, range) => ({
        token: field === "token" ? "other" : edit.token,
        revision: field === "revision" ? edit.revision + 1 : edit.revision,
        patches:
          field === "patches"
            ? [{ start: range.from, end: edit.text.length + 1, insert: "[Obj]" }]
            : [{ start: range.from, end: range.to, insert: "[Obj]" }],
        caret: field === "caret" ? edit.text.length + 10 : range.from + 5,
      }));
      expect(await session.insertLink({ from: 4, to: 8 }, "Obj", target, false)).toBe(false);
      expect(session.state.notice?.kind).toBe("linkFailed");
    }
    expect(patched).not.toHaveBeenCalled();
    vi.mocked(bridge.listLibraryNotes).mockRejectedValue(new DesktopError("io", "No library"));
    expect(await session.listLibraryNotes("obj")).toEqual([]);
    expect(session.state.notice).toEqual({ kind: "linkFailed", message: "No library" });
  });

  it("ignores close callbacks while disposed subscriptions are still settling", async () => {
    const bridge = fakeBridge(note());
    const close = deferred<() => void>();
    const preferences = deferred<() => void>();
    const callbacks: { close: (() => void) | null } = { close: null };
    vi.mocked(bridge.onClose).mockImplementation((handler) => {
      callbacks.close = handler;
      return close.promise;
    });
    vi.mocked(bridge.onPreferences).mockReturnValue(preferences.promise);
    const session = new NoteSession(bridge, () => {});
    sessions.push(session);
    const initializing = session.initialize();
    await vi.advanceTimersByTimeAsync(0);
    session.dispose();
    if (!callbacks.close) throw new Error("The close subscription was not acquired.");
    callbacks.close();
    await vi.advanceTimersByTimeAsync(0);
    expect(bridge.confirmClose).not.toHaveBeenCalled();
    const releaseClose = vi.fn();
    const releasePreferences = vi.fn();
    close.resolve(releaseClose);
    preferences.resolve(releasePreferences);
    await initializing;
    expect(releaseClose).toHaveBeenCalledTimes(1);
    expect(releasePreferences).toHaveBeenCalledTimes(1);
  });

  it("does not start a queued write after the session is disposed", async () => {
    const document = note("/Notes/Note.md");
    const bridge = fakeBridge(document);
    const pending = deferred<NoteDocument>();
    vi.mocked(bridge.checkpoint).mockReturnValueOnce(pending.promise);
    const session = await start(bridge);
    session.edit("first");
    const first = session.flush();
    await vi.advanceTimersByTimeAsync(0);
    session.edit("second");
    const second = session.flush();
    session.dispose();
    pending.resolve({ ...document, text: "first", revision: 1, savedRevision: 1 });
    const results = await Promise.all([first, second]);
    expect(bridge.checkpoint).toHaveBeenCalledTimes(1);
    expect(results).toEqual([true, false]);
  });

  it("does not close an untitled note when Save As is cancelled", async () => {
    const bridge = fakeBridge(note());
    const session = await start(bridge);
    session.edit("draft");
    await session.requestClose();
    expect(session.state.closePrompt).toBe(true);
    await session.saveAndClose();
    expect(bridge.confirmClose).not.toHaveBeenCalled();
    expect(session.state.text).toBe("draft");
    session.cancelClose();
    session.edit("draft continued");
    expect(session.state.text).toBe("draft continued");
  });

  it("reuses the generated filename after cancelling Save As", async () => {
    const bridge = fakeBridge(note());
    const session = await start(bridge);
    session.edit("draft");
    expect(await session.save()).toBe(false);
    expect(await session.save()).toBe(false);
    expect(bridge.chooseSave).toHaveBeenNthCalledWith(1, generatedName);
    expect(bridge.chooseSave).toHaveBeenNthCalledWith(2, generatedName);
    expect(bridge.saveAs).not.toHaveBeenCalled();
    expect(session.state.text).toBe("draft");
  });

  it("keeps the existing path as the Save As suggestion", async () => {
    const bridge = fakeBridge(note("/Notes/Note.md"));
    const session = await start(bridge);
    expect(await session.save(true)).toBe(false);
    expect(bridge.chooseSave).toHaveBeenCalledWith("/Notes/Note.md");
  });

  it("saves the current text before confirming close", async () => {
    const bridge = fakeBridge(note());
    vi.mocked(bridge.chooseSave).mockResolvedValue("/Notes/Draft.md");
    const session = await start(bridge);
    session.edit("draft");
    await session.requestClose();
    await session.saveAndClose();
    expect(bridge.saveAs).toHaveBeenCalledWith(
      { token, revision: 1, text: "draft" },
      "/Notes/Draft.md",
    );
    expect(bridge.confirmClose).toHaveBeenCalledWith(token, 1, false);
  });

  it("preserves edits while an external change requires an explicit choice", async () => {
    const bridge = fakeBridge(note("/Notes/Note.md"));
    vi.mocked(bridge.checkpoint).mockRejectedValue(new DesktopError("conflict", "Changed outside"));
    const session = await start(bridge);
    session.edit("local edits");
    await session.requestClose();
    expect(session.state.notice?.kind).toBe("conflict");
    expect(session.state.closePrompt).toBe(true);
    expect(bridge.confirmClose).not.toHaveBeenCalled();
    expect(session.state.text).toBe("local edits");
  });

  it("requests another window when opening from an edited untitled note", async () => {
    const bridge = fakeBridge(note());
    const session = await start(bridge);
    session.edit("unfinished");
    await session.open("/Notes/Other.md");
    expect(bridge.open).toHaveBeenCalledWith("/Notes/Other.md", token, false);
    expect(session.state.text).toBe("unfinished");
  });
});
