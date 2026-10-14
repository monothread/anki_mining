//! Chooses the profile (deck, note type, field mapping) a card is made with, from where the word
//! was found.

use crate::types::profile::Profile;

/// Where the word was found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Origin {
    /// The text source (`screen`, `mpv`, `clipboard`, ...).
    pub source_id: Option<String>,
    /// The application's name (the X11 window class, e.g. `mpv` or `brave-browser`).
    pub app_name: Option<String>,
    /// The title of the window.
    pub window_title: Option<String>,
}

/// The profile for `origin`: among the profiles whose every rule fits, the one with the most
/// rules (the first one on a tie); the default profile when none does. `None` only when the
/// default profile does not exist.
pub fn resolve<'a>(
    profiles: &'a [Profile],
    default_profile_id: &str,
    origin: &Origin,
) -> Option<&'a Profile> {
    let mut best: Option<(usize, &Profile)> = None;
    for profile in profiles {
        let Some(rules) = matching_rules(profile, origin) else {
            continue;
        };
        if rules > 0 && best.is_none_or(|(most, _)| rules > most) {
            best = Some((rules, profile));
        }
    }
    best.map(|(_, profile)| profile).or_else(|| {
        profiles
            .iter()
            .find(|profile| profile.id == default_profile_id)
    })
}

/// How many rules of the profile fit; `None` when one of them does not.
fn matching_rules(profile: &Profile, origin: &Origin) -> Option<usize> {
    let rules = &profile.r#match;
    let mut matched = 0;
    if let Some(wanted) = non_empty(rules.source_id.as_deref()) {
        if !equals_ignoring_case(origin.source_id.as_deref(), wanted) {
            return None;
        }
        matched += 1;
    }
    if let Some(wanted) = non_empty(rules.app_name.as_deref()) {
        if !equals_ignoring_case(origin.app_name.as_deref(), wanted) {
            return None;
        }
        matched += 1;
    }
    if let Some(wanted) = non_empty(rules.window_title.as_deref()) {
        let title = origin.window_title.as_deref()?;
        if !title.to_lowercase().contains(&wanted.to_lowercase()) {
            return None;
        }
        matched += 1;
    }
    Some(matched)
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

fn equals_ignoring_case(actual: Option<&str>, wanted: &str) -> bool {
    actual.is_some_and(|actual| actual.eq_ignore_ascii_case(wanted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::default_profile;
    use crate::types::profile::ProfileMatch;

    fn profile(id: &str, deck: &str, rules: ProfileMatch) -> Profile {
        Profile {
            id: id.to_string(),
            name: id.to_string(),
            deck_name: deck.to_string(),
            r#match: rules,
            ..default_profile()
        }
    }

    fn rules(source: Option<&str>, app: Option<&str>, title: Option<&str>) -> ProfileMatch {
        ProfileMatch {
            source_id: source.map(str::to_string),
            app_name: app.map(str::to_string),
            window_title: title.map(str::to_string),
        }
    }

    fn origin(source: &str, app: Option<&str>, title: Option<&str>) -> Origin {
        Origin {
            source_id: Some(source.to_string()),
            app_name: app.map(str::to_string),
            window_title: title.map(str::to_string),
        }
    }

    fn profiles() -> Vec<Profile> {
        vec![
            profile("default", "Mining", rules(None, None, None)),
            profile("player", "Anime", rules(None, Some("mpv"), None)),
            profile(
                "youtube",
                "Youtube",
                rules(None, Some("brave-browser"), Some("YouTube")),
            ),
            profile("browser", "Web", rules(None, Some("Brave-Browser"), None)),
            profile("clip", "Clipboard", rules(Some("clipboard"), None, None)),
        ]
    }

    fn deck(origin: &Origin) -> String {
        resolve(&profiles(), "default", origin)
            .expect("profile")
            .deck_name
            .clone()
    }

    #[test]
    fn the_default_profile_is_used_when_nothing_matches() {
        assert_eq!(deck(&origin("screen", Some("firefox"), None)), "Mining");
        assert_eq!(deck(&Origin::default()), "Mining");
    }

    #[test]
    fn the_application_picks_the_profile_ignoring_case() {
        assert_eq!(
            deck(&origin("screen", Some("MPV"), Some("video.mkv"))),
            "Anime"
        );
    }

    #[test]
    fn the_source_picks_the_profile() {
        assert_eq!(deck(&origin("clipboard", None, None)), "Clipboard");
    }

    #[test]
    fn the_profile_with_more_matching_rules_wins() {
        let youtube = origin(
            "screen",
            Some("brave-browser"),
            Some("猫 - youtube - Brave"),
        );

        assert_eq!(deck(&youtube), "Youtube");
    }

    #[test]
    fn a_rule_that_does_not_fit_rules_the_profile_out() {
        let other_site = origin("screen", Some("brave-browser"), Some("Wikipedia"));

        assert_eq!(deck(&other_site), "Web");
        assert_eq!(deck(&origin("screen", Some("brave-browser"), None)), "Web");
    }

    #[test]
    fn the_first_profile_wins_a_tie() {
        let list = vec![
            profile("default", "Mining", rules(None, None, None)),
            profile("first", "First", rules(None, Some("mpv"), None)),
            profile("second", "Second", rules(None, Some("mpv"), None)),
        ];

        let chosen = resolve(&list, "default", &origin("screen", Some("mpv"), None));

        assert_eq!(
            chosen.map(|profile| profile.deck_name.as_str()),
            Some("First")
        );
    }

    #[test]
    fn blank_rules_are_ignored() {
        let list = vec![
            profile("default", "Mining", rules(None, None, None)),
            profile("blank", "Blank", rules(Some(" "), Some(""), Some(""))),
        ];

        let chosen = resolve(&list, "default", &origin("screen", Some("mpv"), None));

        assert_eq!(
            chosen.map(|profile| profile.deck_name.as_str()),
            Some("Mining")
        );
    }

    #[test]
    fn there_is_no_profile_when_the_default_does_not_exist() {
        assert!(resolve(&profiles(), "missing", &Origin::default()).is_none());
        assert!(resolve(&[], "default", &Origin::default()).is_none());
    }
}
