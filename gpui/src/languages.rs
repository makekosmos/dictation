//! Recognition-language dropdown data: the Whisper-supported language table
//! (ISO 639-1 code + native label, "auto" first) and the `SearchableListItem`
//! row type for the settings `Select`.
use ::gpui::{Entity, SharedString};
use gpui_component::{
    searchable_list::{SearchableListItem, SearchableVec},
    select::SelectState,
};

/// One dropdown row: `code` is the wire value written to
/// `dictation.update_config` (`"auto"` = Engine auto-detect), `label` is the
/// native name shown in the list. Search matches both, so typing "ru" or
/// "рус" finds Русский.
#[derive(Debug, Clone)]
pub(crate) struct LangItem {
    pub code: &'static str,
    pub label: &'static str,
}

impl SearchableListItem for LangItem {
    type Value = &'static str;

    fn title(&self) -> SharedString {
        SharedString::from(self.label)
    }

    fn value(&self) -> &Self::Value {
        &self.code
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.label.to_lowercase().contains(&query) || self.code.contains(&query)
    }
}

/// The lazily-created select entity stored on `DictationApp`.
pub(crate) type LangSelect = Entity<SelectState<SearchableVec<LangItem>>>;

/// Whisper-supported languages (whisper.cpp tokenizer list) as
/// `(ISO 639-1 code, native label)`. `"auto"` is the first entry — Engine
/// auto-detect — so `position()` returns row 0 for it.
pub(crate) const LANGUAGES: &[(&str, &str)] = &[
    ("auto", "Авто"),
    ("en", "English"),
    ("zh", "Chinese"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("ru", "Русский"),
    ("ko", "한국어"),
    ("fr", "Français"),
    ("ja", "日本語"),
    ("pt", "Português"),
    ("tr", "Türkçe"),
    ("pl", "Polski"),
    ("ca", "Català"),
    ("nl", "Nederlands"),
    ("ar", "العربية"),
    ("sv", "Svenska"),
    ("it", "Italiano"),
    ("id", "Indonesia"),
    ("hi", "हिन्दी"),
    ("fi", "Suomi"),
    ("vi", "Tiếng Việt"),
    ("he", "עברית"),
    ("uk", "Українська"),
    ("el", "Ελληνικά"),
    ("ms", "Bahasa Melayu"),
    ("cs", "Čeština"),
    ("ro", "Română"),
    ("da", "Dansk"),
    ("hu", "Magyar"),
    ("ta", "தமிழ்"),
    ("no", "Norsk"),
    ("th", "ไทย"),
    ("ur", "اردو"),
    ("hr", "Hrvatski"),
    ("bg", "Български"),
    ("lt", "Lietuvių"),
    ("la", "Latina"),
    ("mi", "Māori"),
    ("ml", "മലയാളം"),
    ("cy", "Cymraeg"),
    ("sk", "Slovenčina"),
    ("te", "తెలుగు"),
    ("fa", "فارسی"),
    ("lv", "Latviešu"),
    ("bn", "বাংলা"),
    ("sr", "Српски"),
    ("az", "Azərbaycan"),
    ("sl", "Slovenščina"),
    ("kn", "ಕನ್ನಡ"),
    ("et", "Eesti"),
    ("mk", "Македонски"),
    ("br", "Brezhoneg"),
    ("eu", "Euskara"),
    ("is", "Íslenska"),
    ("hy", "Հայերեն"),
    ("ne", "नेपाली"),
    ("mn", "Монгол"),
    ("bs", "Bosanski"),
    ("kk", "Қазақша"),
    ("sq", "Shqip"),
    ("sw", "Kiswahili"),
    ("gl", "Galego"),
    ("mr", "मराठी"),
    ("pa", "ਪੰਜਾਬੀ"),
    ("si", "සිංහල"),
    ("km", "ខ្មែរ"),
    ("sn", "ChiShona"),
    ("yo", "Yorùbá"),
    ("so", "Soomaali"),
    ("af", "Afrikaans"),
    ("oc", "Occitan"),
    ("ka", "ქართული"),
    ("be", "Беларуская"),
    ("tg", "Тоҷикӣ"),
    ("sd", "سنڌي"),
    ("gu", "ગુજરાતી"),
    ("am", "አማርኛ"),
    ("yi", "ייִדיש"),
    ("lo", "ລາວ"),
    ("uz", "O'zbek"),
    ("fo", "Føroyskt"),
    ("ht", "Kreyòl ayisyen"),
    ("ps", "پښتو"),
    ("tk", "Türkmen"),
    ("nn", "Nynorsk"),
    ("mt", "Malti"),
    ("sa", "संस्कृतम्"),
    ("lb", "Lëtzebuergesch"),
    ("my", "မြန်မာ"),
    ("bo", "བོད་ཡིག"),
    ("tl", "Tagalog"),
    ("mg", "Malagasy"),
    ("as", "অসমীয়া"),
    ("tt", "Татар"),
    ("haw", "ʻŌlelo Hawaiʻi"),
    ("ln", "Lingála"),
    ("ha", "Hausa"),
    ("ba", "Башҡорт"),
    ("jw", "Basa Jawa"),
    ("su", "Basa Sunda"),
    ("yue", "粵語"),
];

/// Fresh item vec for a `SearchableVec<LangItem>` delegate.
pub(crate) fn language_items() -> Vec<LangItem> {
    LANGUAGES
        .iter()
        .map(|&(code, label)| LangItem { code, label })
        .collect()
}

/// Wire code for `language` when it is a known table entry (`Some`), so
/// `set_selected_value` gets a `&'static str` matching `Value`.
pub(crate) fn language_code(language: &str) -> Option<&'static str> {
    LANGUAGES
        .iter()
        .map(|(code, _)| *code)
        .find(|code| *code == language)
}

#[cfg(test)]
mod tests {
    use super::{language_items, LANGUAGES};

    /// The table stays non-empty, `auto` leads, codes are unique — a
    /// duplicate wire code would make `position()` pick the wrong row.
    #[test]
    fn language_table_is_consistent() {
        assert!(language_items().len() > 90);
        assert_eq!(LANGUAGES[0], ("auto", "Авто"));
        let mut codes: Vec<_> = LANGUAGES.iter().map(|(code, _)| *code).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), LANGUAGES.len());
        assert!(LANGUAGES.iter().any(|(code, _)| *code == "ru"));
        assert!(LANGUAGES.iter().any(|(code, _)| *code == "en"));
    }
}
