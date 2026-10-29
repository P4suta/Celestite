<script lang="ts">
import { onMount, tick } from "svelte";
import chevron from "./assets/chevron.svg";
import closeFind from "./assets/close-find.svg";
import nextMatch from "./assets/next-match.svg";
import previousMatch from "./assets/previous-match.svg";
import CloseDialog from "./CloseDialog.svelte";
import { TauriBridge } from "./lib/desktop";
import { MarkdownEditor, type SearchSummary } from "./lib/editor";
import { initialSession, NoteSession, type SessionState } from "./lib/session";

let note = $state.raw<SessionState>(initialSession);
let chrome = $state(true);
let menu = $state(false);
let recent = $state(false);
let finding = $state(false);
let query = $state("");
let matches = $state.raw<SearchSummary>({ current: 0, total: 0 });
let host: HTMLDivElement;
let filename: HTMLButtonElement;
let menuElement = $state<HTMLDivElement>();
let searchInput = $state<HTMLInputElement>();
let editor: MarkdownEditor | undefined;
let session: NoteSession | undefined;
let bridge: TauriBridge | undefined;

onMount(() => {
  bridge = new TauriBridge();
  editor = new MarkdownEditor(
    host,
    (text) => {
      session?.edit(text);
      chrome = false;
      menu = false;
    },
    (summary) => {
      matches = summary;
    },
    (position) => {
      void session?.followLink(position);
    },
    {
      candidates: (words) => session?.listLibraryNotes(words) ?? Promise.resolve([]),
      chosen: (context, target, candidate) => {
        const words = context.words.trim().length
          ? context.words
          : (candidate.title ?? candidate.id);
        void session?.insertLink(context.range, words, target, context.image);
      },
      media: (files, caret) => {
        void session?.addMedia(files, caret);
      },
    },
  );
  session = new NoteSession(
    bridge,
    (next, replaceText) => {
      note = next;
      if (replaceText) {
        editor?.replace(next.text);
        query = "";
        void tick().then(() => editor?.focus());
      }
      editor?.setReadOnly(next.loading || !next.document || next.busy || next.closing);
    },
    (position) => editor?.jumpTo(position),
    (patches, caret) => editor?.applyPatches(patches, caret),
    () => editor?.view.state.selection.main.head ?? 0,
  );
  void session.initialize().then(() => editor?.focus());
  return (): void => {
    editor?.destroy();
    editor = undefined;
    session?.dispose();
    session = undefined;
    bridge = undefined;
  };
});

$effect(() => {
  document.documentElement.dataset.appearance = note.preferences.appearance;
});

async function toggleMenu(): Promise<void> {
  menu = !menu;
  recent = false;
  chrome = true;
  if (menu) {
    await tick();
    menuElement?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
  } else editor?.focus();
}

async function cancelClose(): Promise<void> {
  session?.cancelClose();
  await tick();
  editor?.focus();
}

async function showRecent(value: boolean): Promise<void> {
  recent = value;
  await tick();
  menuElement?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
}

function closeMenu(): void {
  menu = false;
  recent = false;
}

async function saveNote(forceChoose = false): Promise<void> {
  closeMenu();
  await session?.save(forceChoose);
  await tick();
  editor?.focus();
}

async function find(): Promise<void> {
  closeMenu();
  chrome = true;
  finding = true;
  await tick();
  searchInput?.focus();
  searchInput?.select();
}

function closeSearch(): void {
  finding = false;
  query = "";
  editor?.search("");
  editor?.focus();
}

function handleKeys(event: KeyboardEvent): void {
  if (event.isComposing || event.repeat || note.closePrompt) return;
  if (event.key === "Escape") {
    event.preventDefault();
    if (menu) {
      closeMenu();
      filename.focus();
    } else if (finding) closeSearch();
    else {
      chrome = true;
      filename.focus();
    }
    return;
  }
  if (!(event.metaKey || event.ctrlKey) || event.altKey) return;
  switch (event.key.toLowerCase()) {
    case "n":
      event.preventDefault();
      closeMenu();
      void session?.newWindow();
      break;
    case "o":
      event.preventDefault();
      closeMenu();
      void session?.open();
      break;
    case "s":
      event.preventDefault();
      void saveNote(event.shiftKey);
      break;
    case "f":
      event.preventDefault();
      void find();
      break;
    case "w":
      event.preventDefault();
      runWindowAction("close");
      break;
  }
}

function menuKeys(event: KeyboardEvent): void {
  const items = Array.from(
    menuElement?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [],
  );
  const active = document.activeElement;
  const index = active instanceof HTMLButtonElement ? items.indexOf(active) : -1;
  let target: number;
  switch (event.key) {
    case "ArrowDown":
      target = (index + 1) % items.length;
      break;
    case "ArrowUp":
      target = (index - 1 + items.length) % items.length;
      break;
    case "Home":
      target = 0;
      break;
    case "End":
      target = items.length - 1;
      break;
    default:
      return;
  }
  event.preventDefault();
  items.at(target)?.focus();
}

function outside(event: PointerEvent): void {
  if (!(event.target instanceof Node)) return;
  if (menu && !menuElement?.contains(event.target) && !filename?.contains(event.target))
    closeMenu();
}

function runWindowAction(action: "close" | "minimize" | "maximize"): void {
  if (!bridge) return;
  const result =
    action === "close"
      ? bridge.requestClose()
      : action === "minimize"
        ? bridge.minimize()
        : bridge.maximize();
  void result.catch((error: unknown) => session?.report(error));
}

function chooseRecent(path: string): void {
  closeMenu();
  void session?.open(path);
}

function basename(path: string): string {
  return path.split(/[\\/]/u).at(-1) ?? path;
}

const shortcuts = navigator.platform.includes("Mac")
  ? { mod: "⌘", shift: "⇧" }
  : { mod: "Ctrl+", shift: "Shift+" };
</script>

<svelte:window
  onkeydown={handleKeys}
  onpointermove={(event) => {
    if (event.clientY <= 52) chrome = true;
  }}
  onpointerdown={outside}
  onfocus={() => {
    void session?.inspect();
  }}
/>

<main aria-label="Celestite">
  <header
    class:revealed={chrome || menu || finding}
    inert={!chrome && !menu && !finding}
    data-tauri-drag-region
  >
    <div class="window-controls">
      <button type="button" aria-label="Close window" onclick={() => runWindowAction("close")}>
        <span></span>
      </button>
      <button
        type="button"
        aria-label="Minimize window"
        onclick={() => runWindowAction("minimize")}
      >
        <span></span>
      </button>
      <button
        type="button"
        aria-label="Maximize window"
        onclick={() => runWindowAction("maximize")}
      >
        <span></span>
      </button>
    </div>
    <button
      bind:this={filename}
      type="button"
      class="filename"
      aria-label="File menu"
      aria-haspopup="menu"
      aria-expanded={menu}
      disabled={note.loading}
      onclick={toggleMenu}
    >
      <span class:unsaved={note.revisionExhausted || note.revision !== note.savedRevision}
        >{note.document?.title ?? note.document?.name ?? "Untitled"}</span
      >
      <span class="asset chevron"><img src={chevron} alt=""></span>
    </button>
    <div></div>
  </header>

  <div class="editor-host" bind:this={host}></div>

  {#if menu}
    <div
      class="file-menu"
      bind:this={menuElement}
      role="menu"
      aria-label={recent ? "Recent files" : "File"}
      tabindex="-1"
      onkeydown={menuKeys}
    >
      {#if recent}
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            void showRecent(false);
          }}
        >
          <span>Back</span>
        </button>
        <hr>
        {#if note.preferences.recentFiles.length === 0}
          <p class="empty-recent">No recent files</p>
        {:else}
          <div class="recent-files">
            {#each note.preferences.recentFiles as path (path)}
              <button type="button" role="menuitem" title={path} onclick={() => chooseRecent(path)}>
                <span class="recent-name">{basename(path)}</span>
              </button>
            {/each}
          </div>
        {/if}
      {:else}
        <div class="file-metadata">
          <strong>{note.document?.title ?? note.document?.name ?? "Untitled"}</strong>
          {#if note.document?.title}
            <span title={note.document.name}>{note.document.name}</span>
          {/if}
          <span title={note.document?.folder ?? ""}
            >{note.document?.folder ?? "Not saved yet"}</span
          >
        </div>
        <hr>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            closeMenu();
            void session?.newWindow();
          }}
        >
          <span>New</span><kbd>{shortcuts.mod}N</kbd>
        </button>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            closeMenu();
            void session?.open();
          }}
        >
          <span>Open…</span><kbd>{shortcuts.mod}O</kbd>
        </button>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            void showRecent(true);
          }}
        >
          <span>Open Recent</span>
        </button>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            void saveNote(true);
          }}
        >
          <span>Save As…</span><kbd>{shortcuts.shift}{shortcuts.mod}S</kbd>
        </button>
        <hr>
        <button
          type="button"
          role="menuitem"
          onclick={() => {
            void find();
          }}
        >
          <span>Find…</span><kbd>{shortcuts.mod}F</kbd>
        </button>
        <hr>
        <span class="menu-label">Appearance</span>
        <button
          type="button"
          role="menuitemradio"
          aria-checked={note.preferences.appearance === "light"}
          onclick={() => {
            void session?.setAppearance("light");
          }}
        >
          <span>Light</span>
          {#if note.preferences.appearance === "light"}
            <span class="selection-label">Selected</span>
          {/if}
        </button>
        <button
          type="button"
          role="menuitemradio"
          aria-checked={note.preferences.appearance === "dark"}
          onclick={() => {
            void session?.setAppearance("dark");
          }}
        >
          <span>Dark</span>
          {#if note.preferences.appearance === "dark"}
            <span class="selection-label">Selected</span>
          {/if}
        </button>
      {/if}
    </div>
  {/if}

  {#if finding}
    <search class="find-bar" aria-label="Find in note">
      <input
        bind:this={searchInput}
        aria-label="Find"
        placeholder="Find"
        bind:value={query}
        oninput={() => editor?.search(query)}
        onkeydown={(event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            editor?.next(event.shiftKey);
          }
        }}
      >
      <output aria-live="polite">{matches.current} / {matches.total}</output>
      <button
        type="button"
        class="asset"
        aria-label="Previous match"
        disabled={!matches.total}
        onclick={() => editor?.next(true)}
      >
        <img src={previousMatch} alt="">
      </button>
      <button
        type="button"
        class="asset"
        aria-label="Next match"
        disabled={!matches.total}
        onclick={() => editor?.next()}
      >
        <img src={nextMatch} alt="">
      </button>
      <button type="button" class="asset" aria-label="Close find" onclick={closeSearch}>
        <img src={closeFind} alt="">
      </button>
    </search>
  {/if}

  {#if note.notice}
    <aside class="notice" role="alert">
      <strong
        >{note.notice.kind === "conflict"
          ? "File changed outside Celestite"
          : note.notice.kind === "saveFailed"
            ? "Couldn’t save"
            : note.notice.kind === "openFailed"
              ? "Couldn’t open the file"
              : note.notice.kind === "linkFailed"
                ? "Couldn’t open the link"
                : "Celestite needs your attention"}</strong
      >
      <p>{note.notice.message}</p>
      <div class="notice-actions">
        {#if note.notice.kind === "conflict"}
          <button
            type="button"
            disabled={note.busy}
            onclick={() => {
              void session?.reload();
            }}
          >
            Reload
          </button>
          <button
            type="button"
            disabled={note.busy}
            onclick={() => {
              void saveNote(true);
            }}
          >
            Save As…
          </button>
        {:else if note.notice.kind === "saveFailed"}
          <button
            type="button"
            disabled={note.busy}
            onclick={() => {
              void saveNote();
            }}
          >
            Try Again
          </button>
          <button
            type="button"
            disabled={note.busy}
            onclick={() => {
              void saveNote(true);
            }}
          >
            Save As…
          </button>
        {:else}
          <button type="button" onclick={() => session?.dismissNotice()}>Dismiss</button>
        {/if}
      </div>
    </aside>
  {/if}

  {#if note.closePrompt}
    <CloseDialog
      name={note.document?.title ?? note.document?.name ?? "Untitled"}
      busy={note.busy}
      cancel={() => {
        void cancelClose();
      }}
      discard={() => {
        void session?.discardAndClose();
      }}
      save={() => {
        void session?.saveAndClose();
      }}
    />
  {/if}
</main>
