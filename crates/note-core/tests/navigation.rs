use celestite_core::{MAX_SAFE_REVISION, NavigationDecision, plan_navigation, prefer_web_link};

#[test]
fn https_inference_never_overrides_local_intent_or_an_absent_domain() {
    for (domain, suffix, entry, expected) in [
        (false, false, false, false),
        (false, false, true, false),
        (false, true, false, false),
        (false, true, true, false),
        (true, false, false, true),
        (true, false, true, false),
        (true, true, false, false),
        (true, true, true, false),
    ] {
        assert_eq!(prefer_web_link(domain, suffix, entry), expected);
    }
}

#[test]
fn only_the_exact_current_source_can_navigate_at_an_admitted_position() {
    for current in [0, 1, MAX_SAFE_REVISION] {
        for (token_matches, source_matches) in
            [(true, true), (false, true), (true, false), (false, false)]
        {
            for (position, length) in [(0, 0), (0, 1), (1, 1), (usize::MAX, usize::MAX)] {
                let expected = if token_matches && source_matches {
                    NavigationDecision::Follow
                } else {
                    NavigationDecision::Stale
                };
                assert_eq!(
                    plan_navigation(
                        current,
                        current,
                        token_matches,
                        source_matches,
                        position,
                        length
                    ),
                    expected
                );
            }
        }
        assert_eq!(
            plan_navigation(current, current, true, true, 1, 0),
            NavigationDecision::InvalidPosition
        );
        assert_eq!(
            plan_navigation(current, current, true, true, usize::MAX, usize::MAX - 1),
            NavigationDecision::InvalidPosition
        );
        assert_eq!(
            plan_navigation(current, current + 1, true, true, 0, 0),
            NavigationDecision::Stale
        );
        if current > 0 {
            assert_eq!(
                plan_navigation(current, current - 1, true, true, 0, 0),
                NavigationDecision::Stale
            );
        }
    }
    for invalid in [MAX_SAFE_REVISION + 1, u64::MAX] {
        assert_eq!(
            plan_navigation(invalid, invalid, true, true, 0, 0),
            NavigationDecision::Stale
        );
    }
}
