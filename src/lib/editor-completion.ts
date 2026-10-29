import {
  autocompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { syntaxTree } from "@codemirror/language";
import type { EditorState, Extension } from "@codemirror/state";
import type { LinkCandidate, LinkRange, LinkTarget } from "./contracts";

const excludedNodes = new Set([
  "InlineCode",
  "FencedCode",
  "CodeBlock",
  "HTMLBlock",
  "CommentBlock",
  "HTMLTag",
  "Comment",
  "FrontMatter",
  "URL",
]);

export interface LinkContext {
  readonly range: LinkRange;
  readonly words: string;
  readonly image: boolean;
}

/**
 * Finds an unclosed `[` or `![` on the caret's line, outside code, HTML, and front matter.
 *
 * The returned range starts at the bracket (or the `!`) and ends at the caret.
 */
export function linkContext(state: EditorState, position: number): LinkContext | null {
  const line = state.doc.lineAt(position);
  const before = state.sliceDoc(line.from, position);
  const open = before.lastIndexOf("[");
  if (open < 0 || before.slice(open + 1).includes("]")) return null;
  if (before.slice(0, open).endsWith("\\")) return null;
  const image = open > 0 && before[open - 1] === "!";
  const from = line.from + open - (image ? 1 : 0);
  const tree = syntaxTree(state);
  for (const side of [-1, 1] as const) {
    let node = tree.resolveInner(from + (image ? 1 : 0) + 1, side);
    while (true) {
      if (excludedNodes.has(node.name)) return null;
      if (!node.parent) break;
      node = node.parent;
    }
  }
  if (frontMatterEnd(state.doc.toString()) > from) return null;
  return { range: { from, to: position }, words: before.slice(open + 1), image };
}

/** Returns the UTF-16 offset where Markdown begins after an optional BOM and front matter. */
export function frontMatterEnd(text: string): number {
  const match = /^\u{FEFF}?---\r?\n[\s\S]*?(?:\r?\n)(?:---|\.\.\.)[^\n]*(?:\n|$)/u.exec(text);
  return match ? match[0].length : text.startsWith("\u{FEFF}") ? 1 : 0;
}

export interface CompletionHooks {
  readonly candidates: (query: string) => Promise<LinkCandidate[]>;
  readonly chosen: (context: LinkContext, target: LinkTarget, candidate: LinkCandidate) => void;
}

const SHORT_ID_LENGTH = 8;
const LINK_OPENER_LENGTH = "[".length;
const IMAGE_OPENER_LENGTH = "![".length;

function shortId(id: string): string {
  return id.slice(0, SHORT_ID_LENGTH);
}

export function linkCompletion(hooks: CompletionHooks): Extension {
  const source = async (context: CompletionContext): Promise<CompletionResult | null> => {
    const link = linkContext(context.state, context.pos);
    if (!link) return null;
    if (!context.explicit && link.words.length === 0 && !link.image) return null;
    const candidates = await hooks.candidates(link.words);
    if (context.aborted) return null;
    const options: Completion[] = candidates.map((candidate) => ({
      label: candidate.title ?? candidate.id,
      detail: shortId(candidate.id),
      type: "note",
      apply: (view) => {
        const current = linkContext(view.state, view.state.selection.main.head);
        if (!current) return;
        hooks.chosen(current, { kind: "note", id: candidate.id }, candidate);
      },
    }));
    if (options.length === 0) return null;
    return {
      from: link.range.from + (link.image ? IMAGE_OPENER_LENGTH : LINK_OPENER_LENGTH),
      options,
      filter: false,
      validFor: /^[^\]\n]*$/u,
    };
  };
  return autocompletion({
    override: [source],
    activateOnTyping: true,
    closeOnBlur: true,
    icons: false,
    defaultKeymap: true,
    aboveCursor: false,
    tooltipClass: () => "note-completion",
  });
}
