import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open, save } from "@tauri-apps/plugin-dialog";
import * as v from "valibot";

import {
  type Appearance,
  type AssetDrop,
  type AssetReceipt,
  assetDropSchema,
  assetFailureSchema,
  assetReceiptSchema,
  type Bootstrap,
  bootstrapSchema,
  type DesktopBridge,
  documentSchema,
  type EditRequest,
  type LinkCandidate,
  type LinkEdit,
  type LinkRange,
  type LinkResult,
  type LinkTarget,
  linkCandidatesSchema,
  linkEditSchema,
  linkResultSchema,
  type NoteDocument,
  type OpenResult,
  openResultSchema,
  type Preferences,
  preferencesSchema,
  type Unsubscribe,
} from "./contracts";

const filters = [{ name: "Markdown and text", extensions: ["md", "markdown", "txt"] }];
const ASSET_EXTENSION_HEADER = "celestite-extension";

export class TauriBridge implements DesktopBridge {
  public async bootstrap(): Promise<Bootstrap> {
    return v.parse(bootstrapSchema, await invoke<unknown>("bootstrap"));
  }

  public async checkpoint(edit: EditRequest): Promise<NoteDocument> {
    return v.parse(documentSchema, await invoke<unknown>("checkpoint_document", { edit }));
  }

  public async saveAs(edit: EditRequest, path: string): Promise<NoteDocument> {
    return v.parse(documentSchema, await invoke<unknown>("save_document_as", { edit, path }));
  }

  public async open(path: string, token: string, reuseCurrent: boolean): Promise<OpenResult> {
    return v.parse(
      openResultSchema,
      await invoke<unknown>("open_document", { path, token, reuseCurrent }),
    );
  }

  public async newWindow(): Promise<void> {
    v.parse(v.null(), await invoke<unknown>("new_document"));
  }

  public async followLink(edit: EditRequest, position: number): Promise<LinkResult> {
    return v.parse(linkResultSchema, await invoke<unknown>("follow_link", { edit, position }));
  }

  public async listLibraryNotes(query: string): Promise<LinkCandidate[]> {
    return v.parse(linkCandidatesSchema, await invoke<unknown>("list_library_notes", { query }));
  }

  public async insertLink(
    edit: EditRequest,
    range: LinkRange,
    words: string,
    target: LinkTarget,
    image: boolean,
  ): Promise<LinkEdit> {
    return v.parse(
      linkEditSchema,
      await invoke<unknown>("insert_link", {
        edit,
        link: { from: range.from, to: range.to, words, image },
        target,
      }),
    );
  }

  public async storeAsset(bytes: Uint8Array, extension: string): Promise<AssetReceipt> {
    return v.parse(
      assetReceiptSchema,
      await invoke<unknown>("store_asset", bytes, {
        headers: { [ASSET_EXTENSION_HEADER]: extension },
      }),
    );
  }

  public async inspect(token: string): Promise<boolean> {
    return v.parse(v.boolean(), await invoke<unknown>("inspect_document", { token }));
  }

  public async reload(token: string): Promise<NoteDocument> {
    return v.parse(documentSchema, await invoke<unknown>("reload_document", { token }));
  }

  public async setAppearance(appearance: Appearance): Promise<Preferences> {
    return v.parse(preferencesSchema, await invoke<unknown>("set_appearance", { appearance }));
  }

  public async confirmClose(token: string, revision: number, discard: boolean): Promise<void> {
    v.parse(v.null(), await invoke<unknown>("confirm_close", { token, revision, discard }));
  }

  public async chooseOpen(): Promise<string | null> {
    return v.parse(
      v.nullable(v.string()),
      await open({ multiple: false, directory: false, filters }),
    );
  }

  public async chooseSave(defaultPath: string): Promise<string | null> {
    return save({ defaultPath, filters });
  }

  public async requestClose(): Promise<void> {
    await getCurrentWindow().close();
  }

  public async minimize(): Promise<void> {
    await getCurrentWindow().minimize();
  }

  public async maximize(): Promise<void> {
    await getCurrentWindow().toggleMaximize();
  }

  public async onClose(handler: () => void): Promise<Unsubscribe> {
    return listen<unknown>("celestite:close-requested", () => handler(), {
      target: getCurrentWindow().label,
    });
  }

  public async onPreferences(handler: (preferences: Preferences) => void): Promise<Unsubscribe> {
    return listen<unknown>("celestite:preferences", (event) => {
      const result = v.safeParse(preferencesSchema, event.payload);
      if (result.success) handler(result.output);
    });
  }

  public async onAssetDropped(handler: (drop: AssetDrop) => void): Promise<Unsubscribe> {
    return listen<unknown>(
      "celestite:asset-dropped",
      (event) => {
        const result = v.safeParse(assetDropSchema, event.payload);
        if (result.success) handler(result.output);
      },
      { target: getCurrentWindow().label },
    );
  }

  public async onAssetFailed(handler: (message: string) => void): Promise<Unsubscribe> {
    return listen<unknown>(
      "celestite:asset-failed",
      (event) => {
        const result = v.safeParse(assetFailureSchema, event.payload);
        if (result.success) handler(result.output.message);
      },
      { target: getCurrentWindow().label },
    );
  }
}
