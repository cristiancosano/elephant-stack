//! Idioma de lo que leen la persona y el agente (docs/i18n.md). Todo sale de
//! `elephant-i18n`; aquí solo se añade [`resolve`], que conoce el ajuste.

pub use elephant_i18n::*;

use crate::settings::Language;

/// Convierte el ajuste en un idioma concreto. «Sistema» lee macOS.
pub fn resolve(language: Language) -> Locale {
    match language {
        Language::System => system_locale(),
        Language::Es => Locale::Es,
        Language::En => Locale::En,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tr;

    #[test]
    fn explicit_settings_do_not_read_the_system() {
        assert_eq!(resolve(Language::Es), Locale::Es);
        assert_eq!(resolve(Language::En), Locale::En);
    }

    #[test]
    fn core_tests_write_in_spanish() {
        assert_eq!(locale(), Locale::Es);
        assert_eq!(with_locale(Locale::En, || tr!("Arrancar", "Start")), "Start");
    }
}
