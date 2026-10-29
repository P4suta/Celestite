use celestite_core::{
    DefinitionAction, LabelVerdict, MAX_LABEL_BYTES, MAX_LABEL_SCALARS, Separator,
    definition_separator, label_verdict, library_write, plan_definition,
};

#[test]
fn only_an_untitled_nonempty_note_with_a_library_is_written_there() {
    for has_path in [false, true] {
        for has_library in [false, true] {
            for body_is_empty in [false, true] {
                assert_eq!(
                    library_write(has_path, has_library, body_is_empty),
                    !has_path && has_library && !body_is_empty
                );
            }
        }
    }
}

#[test]
fn labels_need_content_and_balanced_escapes_within_the_commonmark_limit() {
    assert_eq!(label_verdict(b"Object"), LabelVerdict::Valid);
    assert_eq!(label_verdict("日本語 メモ".as_bytes()), LabelVerdict::Valid);
    assert_eq!(label_verdict(b"a\\]b"), LabelVerdict::Valid);
    assert_eq!(label_verdict(b"a\\[b\\\\"), LabelVerdict::Valid);
    assert_eq!(label_verdict(b"x^y"), LabelVerdict::Valid);
    assert_eq!(label_verdict(b""), LabelVerdict::Empty);
    assert_eq!(label_verdict(b" \t "), LabelVerdict::Empty);
    assert_eq!(label_verdict(b"a\nb"), LabelVerdict::LineEnding);
    assert_eq!(label_verdict(b"a\rb"), LabelVerdict::LineEnding);
    assert_eq!(label_verdict(b"a[b"), LabelVerdict::UnescapedBracket);
    assert_eq!(label_verdict(b"a]b"), LabelVerdict::UnescapedBracket);
    assert_eq!(label_verdict(b"\\"), LabelVerdict::Valid);
    assert_eq!(label_verdict(b"^note"), LabelVerdict::Reserved);
    assert_eq!(label_verdict(b" ^note"), LabelVerdict::Reserved);
    let longest = "字".repeat(MAX_LABEL_SCALARS);
    assert_eq!(label_verdict(longest.as_bytes()), LabelVerdict::Valid);
    let too_long = format!("{longest}字");
    assert_eq!(label_verdict(too_long.as_bytes()), LabelVerdict::TooLong);
    let widest = "𠮷".repeat(MAX_LABEL_SCALARS);
    assert_eq!(widest.len(), MAX_LABEL_BYTES);
    assert_eq!(label_verdict(widest.as_bytes()), LabelVerdict::Valid);
    assert_eq!(
        label_verdict(&vec![0x80; MAX_LABEL_BYTES]),
        LabelVerdict::Valid
    );
    assert_eq!(
        label_verdict(&vec![0x80; MAX_LABEL_BYTES + 1]),
        LabelVerdict::TooLong
    );
    let escaped = "\\]".repeat(MAX_LABEL_SCALARS / 2 + 1);
    assert_eq!(label_verdict(escaped.as_bytes()), LabelVerdict::TooLong);
    let escaped_fit = format!("{}x", "\\!".repeat(MAX_LABEL_SCALARS / 2));
    assert_eq!(label_verdict(escaped_fit.as_bytes()), LabelVerdict::Valid);
    let escaped_over = format!("{}xy", "\\!".repeat(MAX_LABEL_SCALARS / 2));
    assert_eq!(
        label_verdict(escaped_over.as_bytes()),
        LabelVerdict::TooLong
    );
    assert_eq!(label_verdict(b"\\\n"), LabelVerdict::LineEnding);
    assert_eq!(label_verdict(b"a\\\r"), LabelVerdict::LineEnding);
    assert_eq!(label_verdict(b"\\a]"), LabelVerdict::UnescapedBracket);
    let huge = vec![b'a'; MAX_LABEL_BYTES + 1];
    assert_eq!(label_verdict(&huge), LabelVerdict::TooLong);
}

#[test]
fn definitions_append_once_and_never_silently_retarget_a_label() {
    assert_eq!(plan_definition(false, false), DefinitionAction::Append);
    assert_eq!(plan_definition(false, true), DefinitionAction::Append);
    assert_eq!(plan_definition(true, true), DefinitionAction::Keep);
    assert_eq!(plan_definition(true, false), DefinitionAction::Refuse);
}

#[test]
fn a_definition_never_interrupts_a_paragraph() {
    assert_eq!(
        definition_separator(true, false, false, false),
        Separator::None
    );
    assert_eq!(
        definition_separator(false, false, false, false),
        Separator::BlankLine
    );
    assert_eq!(
        definition_separator(false, false, false, true),
        Separator::Newline
    );
    assert_eq!(
        definition_separator(false, false, true, false),
        Separator::Newline
    );
    assert_eq!(
        definition_separator(false, true, false, false),
        Separator::Newline
    );
    assert_eq!(
        definition_separator(false, true, true, false),
        Separator::None
    );
    assert_eq!(
        definition_separator(false, true, false, true),
        Separator::None
    );
}
