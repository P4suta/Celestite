import { syntaxTree } from "@codemirror/language";
import type { Range } from "@codemirror/state";
import {
  Decoration,
  type DecorationSet,
  type EditorView,
  ViewPlugin,
  type ViewUpdate,
} from "@codemirror/view";

const linkNodes = new Set(["Link", "Image", "Autolink"]);
const linkMark = Decoration.mark({
  class: "cm-note-link",
  attributes: { role: "link", title: "⌘/Ctrl-click or ⌘/Ctrl-Enter to open link" },
});

function decorations(view: EditorView): DecorationSet {
  const ranges: Range<Decoration>[] = [];
  syntaxTree(view.state).iterate({
    enter(node) {
      if (linkNodes.has(node.name)) ranges.push(linkMark.range(node.from, node.to));
    },
  });
  return Decoration.set(ranges, true);
}

class LinkDecorations {
  public decorations: DecorationSet;

  public constructor(view: EditorView) {
    this.decorations = decorations(view);
  }

  public update(update: ViewUpdate): void {
    if (update.docChanged || syntaxTree(update.startState) !== syntaxTree(update.state)) {
      this.decorations = decorations(update.view);
    }
  }
}

export const editorLinks = ViewPlugin.fromClass(LinkDecorations, {
  decorations: (plugin) => plugin.decorations,
});

export function isLinkAt(view: EditorView, position: number): boolean {
  const tree = syntaxTree(view.state);
  return [tree.resolveInner(position, -1), tree.resolveInner(position, 1)].some((node) => {
    while (true) {
      if (linkNodes.has(node.name)) return true;
      if (!node.parent) return false;
      node = node.parent;
    }
  });
}
