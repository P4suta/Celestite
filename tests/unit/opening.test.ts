import { expect, it, vi } from "vitest";
import type { Bootstrap, DesktopBridge } from "../../src/lib/contracts";
import { NoteSession } from "../../src/lib/session";

it("freezes input while replacing an empty document with an opened file", async () => {
  const token = "00000000-0000-4000-8000-000000000000";
  let release: () => void = () => {};
  const waiting = new Promise<void>((resolve) => {
    release = resolve;
  });
  const bootstrap: Bootstrap = {
    document: {
      token,
      text: "",
      path: null,
      name: "Untitled",
      title: null,
      folder: null,
      revision: 0,
      savedRevision: 0,
    },
    preferences: { appearance: "light", recentFiles: [], library: null },
    warnings: [],
  };
  const bridge = {
    bootstrap: async (): Promise<Bootstrap> => bootstrap,
    onClose: async (): Promise<() => void> => () => {},
    onPreferences: async (): Promise<() => void> => () => {},
    open: vi.fn(async (): Promise<{ document: null }> => {
      await waiting;
      return { document: null };
    }),
  } satisfies Partial<DesktopBridge>;
  const unavailable = async (): Promise<never> => {
    throw new Error("Unexpected bridge call");
  };
  const session = new NoteSession(
    {
      ...bridge,
      checkpoint: unavailable,
      saveAs: unavailable,
      followLink: unavailable,
      listLibraryNotes: unavailable,
      insertLink: unavailable,
      newWindow: unavailable,
      inspect: unavailable,
      reload: unavailable,
      setAppearance: unavailable,
      confirmClose: unavailable,
      chooseOpen: unavailable,
      chooseSave: unavailable,
      requestClose: unavailable,
      minimize: unavailable,
      maximize: unavailable,
    },
    () => {},
  );
  try {
    await session.initialize();
    const opening = session.open("/Notes/Other.md");
    session.edit("typed during replacement");
    expect(session.state.text).toBe("");
    release();
    await opening;
  } finally {
    release();
    session.dispose();
  }
});
