/** Pasted media above this size is refused before any bytes leave the webview; dropped files have no such limit. */
export const MAX_PASTED_BYTES = 64 * 1024 * 1024;

const extensionsByMimeType: Readonly<Record<string, string>> = {
  "application/pdf": "pdf",
  "image/png": "png",
  "image/jpeg": "jpg",
  "image/gif": "gif",
  "image/webp": "webp",
  "image/avif": "avif",
  "image/heic": "heic",
  "audio/mpeg": "mp3",
  "audio/mp4": "m4a",
  "audio/x-m4a": "m4a",
  "audio/wav": "wav",
  "audio/x-wav": "wav",
  "video/mp4": "mp4",
  "video/quicktime": "mov",
};

/**
 * Picks the extension to send with pasted media: the file name's extension, else the one its MIME type implies.
 *
 * The native store decides whether the extension is supported.
 */
export function mediaExtension(file: { readonly name: string; readonly type: string }): string {
  const dot = file.name.lastIndexOf(".");
  if (dot > 0 && dot < file.name.length - 1) return file.name.slice(dot + 1).toLowerCase();
  return extensionsByMimeType[file.type.toLowerCase()] ?? "";
}

/** The reference label for media: the file name without its extension, or `image` for unnamed pastes. */
export function mediaLabel(name: string): string {
  const dot = name.lastIndexOf(".");
  const stem = (dot < 0 ? name : name.slice(0, dot)).trim();
  return stem.length ? stem : "image";
}
