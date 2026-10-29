import { completionStatus } from "@codemirror/autocomplete";
import { defaultKeymap, history, historyKeymap } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { findNext, findPrevious, SearchQuery, search, setSearchQuery } from "@codemirror/search";
import { Compartment, EditorState } from "@codemirror/state";
import { drawSelection, EditorView, keymap } from "@codemirror/view";
import { tags } from "@lezer/highlight";
import type { LinkPatch } from "./contracts";
import { type CompletionHooks, linkCompletion } from "./editor-completion";
import { editorLinks, isLinkAt } from "./editor-links";

const sourceStyle = HighlightStyle.define([
  { tag: [tags.heading, tags.strong], fontWeight: "700" },
  { tag: [tags.processingInstruction, tags.monospace], fontWeight: "400" },
  { tag: [tags.link, tags.url], textDecoration: "underline", textUnderlineOffset: "3px" },
]);

export interface SearchSummary {
  readonly current: number;
  readonly total: number;
}

export class MarkdownEditor {
  public readonly view: EditorView;
  private readonly readOnly = new Compartment();
  private readonly changed: (text: string) => void;
  private readonly searched: (summary: SearchSummary) => void;
  private readonly followed: (position: number) => void;
  private readonly completion: CompletionHooks;
  private query = new SearchQuery({ search: "" });
  private replacing = false;
  private centering = false;
  private destroyed = false;
  private readonly observer: ResizeObserver;

  public constructor(
    parent: HTMLElement,
    changed: (text: string) => void,
    searched: (summary: SearchSummary) => void,
    followed: (position: number) => void = () => {},
    completion: CompletionHooks = { candidates: async () => [], chosen: () => {} },
  ) {
    this.changed = changed;
    this.searched = searched;
    this.followed = followed;
    this.completion = completion;
    this.view = new EditorView({
      parent,
      state: this.createState(""),
      dispatchTransactions: (transactions, view) => {
        view.update(transactions);
        if (
          !view.composing &&
          transactions.some(
            (transaction) => transaction.docChanged || transaction.selection !== undefined,
          )
        ) {
          view.update([
            view.state.update({
              effects: EditorView.scrollIntoView(view.state.selection.main, { y: "center" }),
            }),
          ]);
        }
      },
    });
    this.observer = new ResizeObserver(() => this.center());
    this.observer.observe(parent);
  }

  private createState(text: string): EditorState {
    return EditorState.create({
      doc: text,
      extensions: [
        markdown(),
        syntaxHighlighting(sourceStyle),
        editorLinks,
        linkCompletion(this.completion),
        history(),
        drawSelection(),
        EditorView.lineWrapping,
        keymap.of([
          {
            key: "Mod-Enter",
            run: (view) => {
              const selection = view.state.selection.main;
              if (
                view.composing ||
                view.state.readOnly ||
                !selection.empty ||
                completionStatus(view.state) === "active" ||
                !isLinkAt(view, selection.head)
              )
                return false;
              this.followed(selection.head);
              return true;
            },
          },
          ...defaultKeymap,
          ...historyKeymap,
        ]),
        search({ top: true }),
        this.readOnly.of(EditorState.readOnly.of(true)),
        EditorView.contentAttributes.of({ "aria-label": "Markdown note", spellcheck: "false" }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !this.replacing) this.changed(update.state.doc.toString());
          if (update.docChanged || update.selectionSet) {
            this.searched(this.summary());
          }
        }),
        EditorView.domEventHandlers({
          mousedown: (event, view) => {
            if (
              view.composing ||
              view.state.readOnly ||
              event.button !== 0 ||
              !(event.metaKey || event.ctrlKey) ||
              event.altKey ||
              event.shiftKey ||
              !(event.target instanceof Element) ||
              !event.target.closest(".cm-note-link")
            )
              return false;
            const position = view.posAtCoords({ x: event.clientX, y: event.clientY });
            if (position === null || !isLinkAt(view, position)) return false;
            event.preventDefault();
            this.followed(position);
            return true;
          },
          compositionend: () => {
            this.center();
          },
        }),
      ],
    });
  }

  public center(): void {
    if (this.centering || this.destroyed) return;
    this.centering = true;
    requestAnimationFrame(() => {
      this.centering = false;
      if (!this.destroyed) {
        this.view.dispatch({
          effects: EditorView.scrollIntoView(this.view.state.selection.main, { y: "center" }),
        });
      }
    });
  }

  public replace(text: string): void {
    this.replacing = true;
    this.view.setState(this.createState(text));
    this.replacing = false;
    this.query = new SearchQuery({ search: "" });
    this.searched(this.summary());
    this.center();
  }

  public focus(): void {
    this.view.focus();
  }

  /**
   * Applies native link patches as one undoable transaction.
   *
   * The patches are UTF-16 ranges of the current text in ascending order, so a single undo removes both the label and its definition line.
   */
  public applyPatches(patches: readonly LinkPatch[], caret: number): void {
    if (this.destroyed || patches.length === 0) return;
    const length = this.view.state.doc.length;
    if (patches.some((patch) => patch.end > length)) return;
    this.view.dispatch({
      changes: patches.map((patch) => ({ from: patch.start, to: patch.end, insert: patch.insert })),
      selection: { anchor: caret },
      userEvent: "input.complete",
      scrollIntoView: true,
    });
    this.view.focus();
  }

  public jumpTo(position: number): void {
    if (
      this.destroyed ||
      !Number.isSafeInteger(position) ||
      position < 0 ||
      position > this.view.state.doc.length
    )
      return;
    this.view.dispatch({
      selection: { anchor: position },
      effects: EditorView.scrollIntoView(position, { y: "center" }),
    });
    this.view.focus();
  }

  public setReadOnly(value: boolean): void {
    if (this.view.state.readOnly !== value) {
      this.view.dispatch({ effects: this.readOnly.reconfigure(EditorState.readOnly.of(value)) });
    }
  }

  public search(text: string): void {
    this.query = new SearchQuery({ search: text, literal: true });
    this.view.dispatch({ effects: setSearchQuery.of(this.query) });
    if (text.length) findNext(this.view);
    this.searched(this.summary());
  }

  public next(backwards = false): void {
    if (backwards) findPrevious(this.view);
    else findNext(this.view);
    this.searched(this.summary());
  }

  private summary(): SearchSummary {
    if (!this.query.search.length) return { current: 0, total: 0 };
    const cursor = this.query.getCursor(this.view.state);
    const selection = this.view.state.selection.main;
    let total = 0;
    let current = 0;
    for (let match = cursor.next(); !match.done; match = cursor.next()) {
      total += 1;
      if (match.value.from === selection.from && match.value.to === selection.to) current = total;
    }
    return { current, total };
  }

  public destroy(): void {
    this.destroyed = true;
    this.observer.disconnect();
    this.view.destroy();
  }
}
