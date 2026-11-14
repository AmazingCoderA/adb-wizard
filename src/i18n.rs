//! Простая локализация интерфейса (RU/EN).

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Ru, Lang::En];

    pub fn label(self) -> &'static str {
        match self {
            Lang::Ru => "RU",
            Lang::En => "EN",
        }
    }
}

pub fn tr(lang: Lang, ru: &'static str, en: &'static str) -> &'static str {
    match lang {
        Lang::Ru => ru,
        Lang::En => en,
    }
}
