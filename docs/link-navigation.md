# Link navigation

Command/Ctrl-click a Markdown link, or place the caret on it and press Command/Ctrl-Enter.
Ordinary clicks continue editing the Markdown source.
The gesture works on link labels, destinations, and their delimiters.

## Linking notes in the library

Type `[` followed by a word or two to see library notes whose titles match, newest first, with the short identifier beside each title.
Choose one to turn the words into a shortcut reference link such as `[Object]` and append its definition `[Object]: 019a….md` to the end of the note.
Type `![` for an image reference; its definition points at `assets/<digest>.<extension>`.
The words are the link label and can differ from the target's title; they only need to be distinct within this note.
Choosing a different note for words that already have a definition is refused with the existing target, because a label can point to one place.
Labels are matched case-insensitively with ASCII whitespace runs collapsed, must stay on one line, cannot contain unescaped brackets, cannot start with `^`, and are limited to 999 characters.
Brackets and backslashes in the words are escaped automatically.
The definition joins the trailing definition block, after a blank line when the note ends with a paragraph.
The completion is the only Markdown Celestite writes on your behalf, it is applied as one undoable edit, and it is refused inside front matter, code, and HTML.
A definition you write by hand anywhere in the note is honored; the first definition for a label wins.
A missing identifier-named note reports that it is not in the library.

## Adding media

Paste an image or drop files onto the window to store them in the library's `assets/` folder under their SHA-256 digest and insert `![name]` with its definition at the caret.
The label is the file name without its extension, or `image` for an unnamed paste, and it is written as one undoable edit.
Supported types are the PDF, PNG, JPEG, GIF, WebP, AVIF, HEIC, MP3, M4A, WAV, MP4, and MOV extensions, written in lowercase with `jpeg` as `jpg`.
The same bytes are stored once, and a different file already named by that digest is refused rather than replaced.
Pasted media stop at 64 MiB; drop larger files instead.
Media need a configured library and a note that is untitled or already inside it.

- Relative `.md`, `.markdown`, and `.txt` destinations open in another note window.
- A `#heading` link moves within the current buffer.
- Another note's `#heading` fragment positions its initial caret at that heading.
- HTTP and HTTPS links open in the default browser.
- Scheme-less domains such as `[google.com](google.com)` open using HTTPS, including paths, queries, ports, and fragments.
- Local PDF, PNG, JPEG, GIF, WebP, AVIF, HEIC, MP3, M4A, WAV, MP4, and MOV files open in their default application.

Inline links, reference links, image links, and angle autolinks use CommonMark parsing.
Code, HTML links, and YAML front matter are excluded.
Relative destinations start at the saved source file's directory.
Parent paths, Unicode names, and percent-encoded spaces are supported.
Save an untitled note before following its relative file links.
Scheme-less Web links also work in untitled notes and leave their Markdown unchanged.
Well-formed dotted domain names without credentials can use HTTPS inference.
Supported note and media filename extensions retain their local meaning.
An existing file, directory, or symbolic link in the source directory also takes precedence over domain inference.
Use `./` or `../` to make a relative file destination explicit, or `https://` to make a Web destination explicit.
Local file links cannot contain a query, and remote file hosts and other URL schemes are unsupported.

Heading fragments are percent-decoded as UTF-8.
Identifiers lowercase the heading's rendered text, replace whitespace with hyphens, retain Unicode letters and numbers, hyphens, and underscores, and remove other characters.
Inline code contributes its text.
Repeated identifiers receive the first unused `-1`, `-2`, and subsequent suffix.
An exact rendered heading title is also accepted.
An empty fragment moves to the beginning of the note.
These identifiers are a Celestite convention; CommonMark does not specify heading identifiers.
HTTPS inference is also an application convention rather than a change to CommonMark's relative-link semantics.

Navigation checkpoints current input first.
Checkpoint errors stop the operation, and link errors leave the source intact.
Another note opens in another window so the current editing context remains available.
There is no visited-link history or automatic lookup of moved files through archive identities.
