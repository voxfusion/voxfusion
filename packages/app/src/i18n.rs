//! Interface strings in the seven supported languages.

use gpui_kit::SharedString;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Locale {
    #[default]
    En,
    Ru,
    Es,
    Zh,
    De,
    Fr,
    It,
}

impl Locale {
    pub const ALL: [Locale; 7] = [
        Locale::En,
        Locale::Ru,
        Locale::Es,
        Locale::Zh,
        Locale::De,
        Locale::Fr,
        Locale::It,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Ru => "ru",
            Locale::Es => "es",
            Locale::Zh => "zh",
            Locale::De => "de",
            Locale::Fr => "fr",
            Locale::It => "it",
        }
    }

    pub fn from_code(code: &str) -> Option<Locale> {
        Locale::ALL.into_iter().find(|locale| locale.code() == code)
    }

    fn source(self) -> &'static str {
        match self {
            Locale::En => include_str!("../assets/i18n/en.json"),
            Locale::Ru => include_str!("../assets/i18n/ru.json"),
            Locale::Es => include_str!("../assets/i18n/es.json"),
            Locale::Zh => include_str!("../assets/i18n/zh.json"),
            Locale::De => include_str!("../assets/i18n/de.json"),
            Locale::Fr => include_str!("../assets/i18n/fr.json"),
            Locale::It => include_str!("../assets/i18n/it.json"),
        }
    }
}

type Dictionary = HashMap<String, SharedString>;

static DICTIONARIES: LazyLock<HashMap<Locale, Dictionary>> = LazyLock::new(|| {
    Locale::ALL
        .into_iter()
        .map(|locale| {
            let dictionary: HashMap<String, String> =
                serde_json::from_str(locale.source()).expect("translation file is valid JSON");

            let dictionary = dictionary
                .into_iter()
                .map(|(key, value)| (key, SharedString::from(value)))
                .collect();

            (locale, dictionary)
        })
        .collect()
});

/// The string for `key` in `locale`, falling back to English and then to the
/// key itself.
pub fn translate(locale: Locale, key: &str) -> SharedString {
    DICTIONARIES
        .get(&locale)
        .and_then(|dictionary| dictionary.get(key))
        .or_else(|| DICTIONARIES[&Locale::En].get(key))
        .cloned()
        .unwrap_or_else(|| SharedString::from(key.to_string()))
}

/// [`translate`], with each `{{name}}` or `{name}` placeholder replaced.
pub fn translate_with(locale: Locale, key: &str, params: &[(&str, &str)]) -> SharedString {
    let mut text = translate(locale, key).to_string();

    for (name, value) in params {
        text = text
            .replace(&format!("{{{{{name}}}}}"), value)
            .replace(&format!("{{{name}}}"), value);
    }

    text.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_locale_has_every_english_key() {
        let english = &DICTIONARIES[&Locale::En];

        for locale in Locale::ALL {
            for key in english.keys() {
                assert!(
                    DICTIONARIES[&locale].contains_key(key),
                    "{} is missing {key}",
                    locale.code()
                );
            }
        }
    }

    #[test]
    fn placeholders_are_replaced_in_both_spellings() {
        assert_eq!(
            translate_with(Locale::En, "home.pressToRecord", &[("hotkey", "⌘K")]).as_ref(),
            "Press ⌘K to start a new recording"
        );
        assert_eq!(
            translate_with(Locale::En, "dictionary.wordCount", &[("count", "3")]).as_ref(),
            "3 words"
        );
    }
}
