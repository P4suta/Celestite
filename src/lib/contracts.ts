import * as v from "valibot";

const revision = v.pipe(
  v.number(),
  v.integer(),
  v.minValue(0),
  v.maxValue(Number.MAX_SAFE_INTEGER),
);

export const documentSchema = v.pipe(
  v.object({
    token: v.pipe(v.string(), v.uuid()),
    text: v.string(),
    path: v.nullable(v.string()),
    name: v.string(),
    title: v.optional(v.nullable(v.string()), null),
    folder: v.nullable(v.string()),
    revision,
    savedRevision: revision,
  }),
  v.check((document) => document.savedRevision <= document.revision, "Invalid saved revision"),
);

export const appearanceSchema = v.picklist(["light", "dark"]);
export const preferencesSchema = v.object({
  appearance: appearanceSchema,
  recentFiles: v.array(v.string()),
  library: v.optional(v.nullable(v.string()), null),
});
export const linkCandidateSchema = v.object({
  id: v.pipe(v.string(), v.uuid()),
  title: v.nullable(v.string()),
  path: v.string(),
});
export const linkCandidatesSchema = v.array(linkCandidateSchema);
export const linkPatchSchema = v.pipe(
  v.object({ start: revision, end: revision, insert: v.string() }),
  v.check((patch) => patch.start <= patch.end, "Invalid patch range"),
);
export const linkEditSchema = v.object({
  token: v.pipe(v.string(), v.uuid()),
  revision,
  patches: v.array(linkPatchSchema),
  caret: revision,
});
const digest = v.pipe(v.string(), v.regex(/^[0-9a-f]{64}$/u));
const extension = v.pipe(v.string(), v.regex(/^[a-z0-9]{1,16}$/u));
export const linkTargetSchema = v.variant("kind", [
  v.object({ kind: v.literal("note"), id: v.pipe(v.string(), v.uuid()) }),
  v.object({ kind: v.literal("asset"), digest, extension }),
]);
export const assetReceiptSchema = v.object({ digest, extension });
export const assetDropSchema = v.object({ digest, extension, label: v.string() });
export const assetFailureSchema = v.object({ message: v.string() });
export const bootstrapSchema = v.pipe(
  v.object({
    document: documentSchema,
    preferences: preferencesSchema,
    warnings: v.array(v.string()),
    selection: v.optional(v.nullable(revision)),
  }),
  v.check(
    (data) => data.selection == null || data.selection <= data.document.text.length,
    "Invalid initial selection",
  ),
);
export const openResultSchema = v.object({ document: v.nullable(documentSchema) });
export const linkResultSchema = v.object({
  token: v.pipe(v.string(), v.uuid()),
  revision,
  selection: v.nullable(revision),
});
export const commandErrorSchema = v.object({ kind: v.string(), message: v.string() });

export type NoteDocument = v.InferOutput<typeof documentSchema>;
export type Preferences = v.InferOutput<typeof preferencesSchema>;
export type Appearance = v.InferOutput<typeof appearanceSchema>;
export type Bootstrap = v.InferOutput<typeof bootstrapSchema>;
export type OpenResult = v.InferOutput<typeof openResultSchema>;
export type LinkResult = v.InferOutput<typeof linkResultSchema>;
export type LinkCandidate = v.InferOutput<typeof linkCandidateSchema>;
export type LinkPatch = v.InferOutput<typeof linkPatchSchema>;
export type LinkEdit = v.InferOutput<typeof linkEditSchema>;
export type LinkTarget = v.InferOutput<typeof linkTargetSchema>;
export type AssetReceipt = v.InferOutput<typeof assetReceiptSchema>;
export type AssetDrop = v.InferOutput<typeof assetDropSchema>;
export type Unsubscribe = () => void;
export interface EditRequest {
  readonly token: string;
  readonly revision: number;
  readonly text: string;
}
export interface LinkRange {
  readonly from: number;
  readonly to: number;
}

/**
 * Reports whether the Markdown after an optional BOM and front matter block is blank.
 *
 * This mirrors the native reuse rule, so a fresh library note that only carries its identity front matter is still replaced by an opened file.
 */
export function bodyIsEmpty(text: string): boolean {
  const body = text.replace(/^\u{FEFF}?---\r?\n[\s\S]*?(?:\r?\n)(?:---|\.\.\.)[^\n]*(?:\n|$)/u, "");
  return body.trim().length === 0;
}

export interface DesktopBridge {
  bootstrap(): Promise<Bootstrap>;
  checkpoint(edit: EditRequest): Promise<NoteDocument>;
  saveAs(edit: EditRequest, path: string): Promise<NoteDocument>;
  open(path: string, token: string, reuseCurrent: boolean): Promise<OpenResult>;
  followLink(edit: EditRequest, position: number): Promise<LinkResult>;
  listLibraryNotes(query: string): Promise<LinkCandidate[]>;
  insertLink(
    edit: EditRequest,
    range: LinkRange,
    words: string,
    target: LinkTarget,
    image: boolean,
  ): Promise<LinkEdit>;
  storeAsset(bytes: Uint8Array, extension: string): Promise<AssetReceipt>;
  newWindow(): Promise<void>;
  inspect(token: string): Promise<boolean>;
  reload(token: string): Promise<NoteDocument>;
  setAppearance(appearance: Appearance): Promise<Preferences>;
  confirmClose(token: string, revision: number, discard: boolean): Promise<void>;
  chooseOpen(): Promise<string | null>;
  chooseSave(defaultPath: string): Promise<string | null>;
  requestClose(): Promise<void>;
  minimize(): Promise<void>;
  maximize(): Promise<void>;
  onClose(handler: () => void): Promise<Unsubscribe>;
  onPreferences(handler: (preferences: Preferences) => void): Promise<Unsubscribe>;
  onAssetDropped(handler: (drop: AssetDrop) => void): Promise<Unsubscribe>;
  onAssetFailed(handler: (message: string) => void): Promise<Unsubscribe>;
}

export class DesktopError extends Error {
  public readonly kind: string;

  public constructor(kind: string, message: string) {
    super(message);
    this.name = "DesktopError";
    this.kind = kind;
  }
}

export function desktopError(error: unknown): DesktopError {
  if (error instanceof DesktopError) return error;
  const parsed = v.safeParse(commandErrorSchema, error);
  if (parsed.success) return new DesktopError(parsed.output.kind, parsed.output.message);
  if (error instanceof Error) return new DesktopError("internal", error.message);
  if (typeof error === "string") return new DesktopError("internal", error);
  return new DesktopError("internal", "The operation could not be completed.");
}
