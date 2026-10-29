use celestite_core::{AssetStore, media_link_allowed, plan_asset_store};

#[test]
fn media_are_written_once_kept_when_identical_and_refused_on_collision() {
    assert_eq!(plan_asset_store(false, false), AssetStore::Write);
    assert_eq!(plan_asset_store(false, true), AssetStore::Write);
    assert_eq!(plan_asset_store(true, true), AssetStore::Keep);
    assert_eq!(plan_asset_store(true, false), AssetStore::Refuse);
}

#[test]
fn media_links_need_a_library_and_a_note_without_a_path_or_inside_it() {
    for has_library in [false, true] {
        for has_path in [false, true] {
            for in_library in [false, true] {
                assert_eq!(
                    media_link_allowed(has_library, has_path, in_library),
                    has_library && (!has_path || in_library)
                );
            }
        }
    }
    assert!(media_link_allowed(true, false, false));
    assert!(media_link_allowed(true, true, true));
    assert!(!media_link_allowed(true, true, false));
    assert!(!media_link_allowed(false, false, false));
}
