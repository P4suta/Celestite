import {
  type Appearance,
  bodyIsEmpty,
  type DesktopBridge,
  DesktopError,
  desktopError,
  type EditRequest,
  type LinkCandidate,
  type LinkPatch,
  type LinkRange,
  type LinkTarget,
  type NoteDocument,
  type Preferences,
  type Unsubscribe,
} from "./contracts";

const AUTOSAVE_DELAY_MS = 800;
const EXTERNAL_CHANGE_POLL_INTERVAL_MS = 3000;

export interface Notice {
  readonly kind: "conflict" | "saveFailed" | "openFailed" | "linkFailed" | "startup";
  readonly message: string;
}

export interface SessionState {
  readonly document: NoteDocument | null;
  readonly text: string;
  readonly revision: number;
  readonly savedRevision: number;
  readonly revisionExhausted: boolean;
  readonly preferences: Preferences;
  readonly loading: boolean;
  readonly busy: boolean;
  readonly closing: boolean;
  readonly closePrompt: boolean;
  readonly notice: Notice | null;
}

export const initialSession: SessionState = {
  document: null,
  text: "",
  revision: 0,
  savedRevision: 0,
  revisionExhausted: false,
  preferences: { appearance: "light", recentFiles: [], library: null },
  loading: true,
  busy: false,
  closing: false,
  closePrompt: false,
  notice: null,
};

/** Checks that patches are ordered, disjoint, inside the text, and leave the caret in range. */
export function validPatches(
  patches: readonly LinkPatch[],
  caret: number,
  length: number,
): boolean {
  let previous = 0;
  let delta = 0;
  for (const patch of patches) {
    if (patch.start < previous || patch.start > patch.end || patch.end > length) return false;
    previous = patch.end;
    delta += patch.insert.length - (patch.end - patch.start);
  }
  return caret >= 0 && caret <= length + delta;
}

export class NoteSession {
  private current: SessionState = initialSession;
  private readonly bridge: DesktopBridge;
  private readonly changed: (state: SessionState, replaceText: boolean) => void;
  private readonly moved: (position: number) => void;
  private readonly patched: (patches: readonly LinkPatch[], caret: number) => void;
  private queue: Promise<void> = Promise.resolve();
  private debounce: ReturnType<typeof setTimeout> | undefined;
  private poll: ReturnType<typeof setInterval> | undefined;
  private checkpointedRevision = 0;
  private disposed = false;
  private cleanups: Unsubscribe[] = [];

  public constructor(
    bridge: DesktopBridge,
    changed: (state: SessionState, replaceText: boolean) => void,
    moved: (position: number) => void = () => {},
    patched: (patches: readonly LinkPatch[], caret: number) => void = () => {},
  ) {
    this.bridge = bridge;
    this.changed = changed;
    this.moved = moved;
    this.patched = patched;
  }

  public get state(): SessionState {
    return this.current;
  }

  public get dirty(): boolean {
    return this.current.revisionExhausted || this.current.revision !== this.current.savedRevision;
  }

  private publish(update: Partial<SessionState>, replaceText = false): void {
    if (this.disposed) return;
    this.current = { ...this.current, ...update };
    this.changed(this.current, replaceText);
  }

  private serialize<T>(operation: () => Promise<T>): Promise<T> {
    const result = this.queue.then(() => {
      if (this.disposed) throw new DesktopError("disposed", "This editing session has closed.");
      return operation();
    });
    this.queue = result.then(
      () => undefined,
      () => undefined,
    );
    return result;
  }

  private request(): EditRequest | null {
    const document = this.current.document;
    if (!document) return null;
    return { token: document.token, revision: this.current.revision, text: this.current.text };
  }

  private replace(document: NoteDocument): void {
    this.checkpointedRevision = document.revision;
    this.publish(
      {
        document,
        text: document.text,
        revision: document.revision,
        savedRevision: document.savedRevision,
        revisionExhausted: false,
        notice: null,
      },
      true,
    );
  }

  private acknowledge(edit: EditRequest, document: NoteDocument): boolean {
    if (
      document.token !== edit.token ||
      document.revision !== edit.revision ||
      document.text !== edit.text ||
      document.savedRevision > edit.revision
    ) {
      throw new DesktopError("invalid-response", "The save response does not match this edit.");
    }
    if (document.token !== this.current.document?.token) return false;
    this.checkpointedRevision = Math.max(this.checkpointedRevision, document.revision);
    this.publish({
      document,
      savedRevision: Math.max(this.current.savedRevision, document.savedRevision),
      notice: null,
    });
    return true;
  }

  public async initialize(): Promise<void> {
    if (this.disposed) return;
    try {
      const data = await this.bridge.bootstrap();
      if (this.disposed) return;
      this.publish({ preferences: data.preferences, loading: false });
      this.replace(data.document);
      if (data.selection != null) this.moved(data.selection);
      if (data.warnings.length)
        this.publish({ notice: { kind: "startup", message: data.warnings.join("\n") } });
      const subscriptions = await Promise.allSettled([
        this.bridge.onClose(() => {
          void this.requestClose();
        }),
        this.bridge.onPreferences((preferences) => this.publish({ preferences })),
      ]);
      const cleanups = subscriptions.flatMap((result) =>
        result.status === "fulfilled" ? [result.value] : [],
      );
      const failure = subscriptions.find((result) => result.status === "rejected");
      if (this.disposed || failure) {
        for (const cleanup of cleanups) cleanup();
        if (failure) throw desktopError(failure.reason);
        return;
      }
      this.cleanups = cleanups;
      this.poll = setInterval(() => {
        void this.inspect();
      }, EXTERNAL_CHANGE_POLL_INTERVAL_MS);
    } catch (error) {
      this.publish({
        loading: false,
        notice: { kind: "startup", message: desktopError(error).message },
      });
    }
  }

  public edit(text: string): void {
    if (this.disposed || this.current.loading || this.current.busy || this.current.closing) return;
    const revision = this.current.revision + 1;
    if (!Number.isSafeInteger(revision)) {
      this.publish({
        text,
        revisionExhausted: true,
        notice: {
          kind: "saveFailed",
          message: "The editing revision limit was reached; copy this note before restarting.",
        },
      });
      return;
    }
    this.publish({ text, revision });
    clearTimeout(this.debounce);
    this.debounce = setTimeout(() => {
      void this.flush();
    }, AUTOSAVE_DELAY_MS);
  }

  public async flush(): Promise<boolean> {
    clearTimeout(this.debounce);
    if (this.current.revisionExhausted || this.disposed) return false;
    const edit = this.request();
    if (!edit) return false;
    if (this.current.document?.path ? !this.dirty : edit.revision === this.checkpointedRevision)
      return true;
    try {
      const result = await this.serialize(() => this.bridge.checkpoint(edit));
      return this.acknowledge(edit, result);
    } catch (error) {
      if (edit.token !== this.current.document?.token) return false;
      const failure = desktopError(error);
      this.publish({
        notice: {
          kind: failure.kind === "conflict" ? "conflict" : "saveFailed",
          message: failure.message,
        },
      });
      return false;
    }
  }

  public async save(forceChoose = false): Promise<boolean> {
    const document = this.current.document;
    if (!document || this.current.busy || this.current.revisionExhausted || this.disposed)
      return false;
    if (document.path && !forceChoose) return this.flush();
    this.publish({ busy: true });
    try {
      const path = await this.bridge.chooseSave(document.path ?? document.name);
      if (path === null) return false;
      const edit = this.request();
      if (!edit || edit.token !== document.token) return false;
      const result = await this.serialize(() => this.bridge.saveAs(edit, path));
      return this.acknowledge(edit, result);
    } catch (error) {
      const failure = desktopError(error);
      this.publish({
        notice: {
          kind: failure.kind === "conflict" ? "conflict" : "saveFailed",
          message: failure.message,
        },
      });
      return false;
    } finally {
      this.publish({ busy: false });
    }
  }

  public async open(path?: string): Promise<void> {
    if (!this.current.document || this.current.busy || this.current.closing) return;
    this.publish({ busy: true });
    try {
      const selected = path ?? (await this.bridge.chooseOpen());
      if (selected === null) return;
      await this.flush();
      const document = this.current.document;
      if (!document) return;
      const reuse = document.path === null && bodyIsEmpty(this.current.text);
      const result = await this.serialize(() => this.bridge.open(selected, document.token, reuse));
      if (result.document) this.replace(result.document);
    } catch (error) {
      this.publish({ notice: { kind: "openFailed", message: desktopError(error).message } });
    } finally {
      this.publish({ busy: false });
    }
  }

  public async newWindow(): Promise<void> {
    if (this.disposed || this.current.busy || this.current.closing) return;
    try {
      await this.flush();
      await this.serialize(() => this.bridge.newWindow());
    } catch (error) {
      this.publish({ notice: { kind: "openFailed", message: desktopError(error).message } });
    }
  }

  public async followLink(position: number): Promise<void> {
    if (
      this.disposed ||
      this.current.loading ||
      this.current.busy ||
      this.current.closing ||
      !Number.isSafeInteger(position) ||
      position < 0 ||
      position > this.current.text.length
    )
      return;
    this.publish({ busy: true });
    try {
      if (!(await this.flush())) return;
      const edit = this.request();
      if (!edit) return;
      const result = await this.serialize(() => this.bridge.followLink(edit, position));
      if (this.disposed || edit.token !== this.current.document?.token) return;
      if (
        result.token !== edit.token ||
        result.revision !== edit.revision ||
        (result.selection !== null &&
          (!Number.isSafeInteger(result.selection) ||
            result.selection < 0 ||
            result.selection > edit.text.length))
      )
        throw new DesktopError("invalid-response", "The link response does not match this note.");
      this.publish({ notice: null });
      if (result.selection !== null) this.moved(result.selection);
    } catch (error) {
      this.publish({ notice: { kind: "linkFailed", message: desktopError(error).message } });
    } finally {
      this.publish({ busy: false });
    }
  }

  public async listLibraryNotes(query: string): Promise<LinkCandidate[]> {
    if (this.disposed || this.current.loading || !this.current.document) return [];
    try {
      return await this.bridge.listLibraryNotes(query);
    } catch (error) {
      this.publish({ notice: { kind: "linkFailed", message: desktopError(error).message } });
      return [];
    }
  }

  /**
   * Turns the words at `range` into a reference link to `target`.
   *
   * Pending edits are checkpointed first; the native patches are applied only when the text is still the one they were planned for, as one editor transaction.
   */
  public async insertLink(
    range: LinkRange,
    words: string,
    target: LinkTarget,
    image: boolean,
  ): Promise<boolean> {
    const length = this.current.text.length;
    if (
      this.disposed ||
      this.current.loading ||
      this.current.busy ||
      this.current.closing ||
      !Number.isSafeInteger(range.from) ||
      !Number.isSafeInteger(range.to) ||
      range.from < 0 ||
      range.from > range.to ||
      range.to > length
    )
      return false;
    this.publish({ busy: true });
    try {
      if (!(await this.flush())) return false;
      const edit = this.request();
      if (!edit) return false;
      const result = await this.serialize(() =>
        this.bridge.insertLink(edit, range, words, target, image),
      );
      if (this.disposed || edit.token !== this.current.document?.token) return false;
      if (
        result.token !== edit.token ||
        result.revision !== edit.revision ||
        !validPatches(result.patches, result.caret, edit.text.length)
      )
        throw new DesktopError("invalid-response", "The link response does not match this note.");
      if (edit.text !== this.current.text) return false;
      this.publish({ notice: null });
      this.patched(result.patches, result.caret);
      return true;
    } catch (error) {
      this.publish({ notice: { kind: "linkFailed", message: desktopError(error).message } });
      return false;
    } finally {
      this.publish({ busy: false });
    }
  }

  public async reload(): Promise<void> {
    const document = this.current.document;
    if (!document || this.current.busy) return;
    this.publish({ busy: true });
    try {
      const result = await this.serialize(() => this.bridge.reload(document.token));
      this.replace(result);
    } catch (error) {
      this.publish({ notice: { kind: "openFailed", message: desktopError(error).message } });
    } finally {
      this.publish({ busy: false });
    }
  }

  public async setAppearance(appearance: Appearance): Promise<void> {
    try {
      const preferences = await this.serialize(() => this.bridge.setAppearance(appearance));
      this.publish({ preferences });
    } catch (error) {
      this.publish({ notice: { kind: "startup", message: desktopError(error).message } });
    }
  }

  public async inspect(): Promise<void> {
    const document = this.current.document;
    if (
      !document?.path ||
      this.current.busy ||
      this.current.closing ||
      this.current.notice?.kind === "conflict"
    )
      return;
    try {
      const changed = await this.serialize(() => this.bridge.inspect(document.token));
      if (changed && document.token === this.current.document?.token) {
        this.publish({
          notice: {
            kind: "conflict",
            message:
              "The file on disk has changed; reload it or save your edits as a separate copy.",
          },
        });
      }
    } catch (error) {
      const failure = desktopError(error);
      if (failure.kind !== "stale")
        this.publish({ notice: { kind: "openFailed", message: failure.message } });
    }
  }

  public dismissNotice(): void {
    this.publish({ notice: null });
  }

  public async requestClose(): Promise<void> {
    if (this.disposed || this.current.closing || this.current.busy || !this.current.document)
      return;
    this.publish({ closing: true });
    await this.flush();
    if (this.dirty) {
      this.publish({ closePrompt: true });
      return;
    }
    await this.finishClose(false);
  }

  private async finishClose(discard: boolean): Promise<void> {
    const edit = this.request();
    if (!edit) return;
    try {
      await this.serialize(() => this.bridge.confirmClose(edit.token, edit.revision, discard));
    } catch (error) {
      this.publish({
        closing: false,
        closePrompt: false,
        notice: { kind: "saveFailed", message: desktopError(error).message },
      });
    }
  }

  public cancelClose(): void {
    this.publish({ closing: false, closePrompt: false });
  }

  public async discardAndClose(): Promise<void> {
    await this.finishClose(true);
  }

  public async saveAndClose(): Promise<void> {
    const saved = await this.save(this.current.notice?.kind === "conflict");
    if (saved && !this.dirty) await this.finishClose(false);
  }

  public report(error: unknown): void {
    this.publish({ notice: { kind: "startup", message: desktopError(error).message } });
  }

  public dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    clearTimeout(this.debounce);
    clearInterval(this.poll);
    for (const cleanup of this.cleanups) cleanup();
    this.cleanups = [];
  }
}
