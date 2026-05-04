pub fn layout_code(layout_name: &str) -> String {
    let first_word = layout_name.split_whitespace().next().unwrap_or(layout_name);
    let code = match first_word.to_lowercase().as_str() {
        "english" => "EN",
        "russian" => "RU",
        "german" => "DE",
        "french" => "FR",
        "spanish" => "ES",
        "italian" => "IT",
        "portuguese" => "PT",
        "dutch" => "NL",
        "polish" => "PL",
        "czech" => "CZ",
        "slovak" => "SK",
        "hungarian" => "HU",
        "romanian" => "RO",
        "bulgarian" => "BG",
        "ukrainian" => "UA",
        "belarusian" => "BY",
        "serbian" => "RS",
        "croatian" => "HR",
        "slovenian" => "SI",
        "turkish" => "TR",
        "greek" => "GR",
        "arabic" => "AR",
        "hebrew" => "HE",
        "japanese" => "JP",
        "korean" => "KR",
        "chinese" => "CN",
        "thai" => "TH",
        "vietnamese" => "VN",
        "swedish" => "SE",
        "norwegian" => "NO",
        "danish" => "DK",
        "finnish" => "FI",
        "estonian" => "EE",
        "latvian" => "LV",
        "lithuanian" => "LT",
        "georgian" => "GE",
        _ => {
            if !layout_name.contains(' ') {
                return layout_name.to_uppercase();
            }

            return first_word
                .chars()
                .take(2)
                .collect::<String>()
                .to_uppercase();
        }
    };

    code.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_code_maps_known_language_names() {
        assert_eq!(layout_code("English (US)"), "EN");
        assert_eq!(layout_code("Russian"), "RU");
        assert_eq!(layout_code("German"), "DE");
        assert_eq!(layout_code("Polish"), "PL");
        assert_eq!(layout_code("Georgian"), "GE");
    }

    #[test]
    fn layout_code_handles_raw_xkb_codes() {
        assert_eq!(layout_code("us"), "US");
        assert_eq!(layout_code("de_ch"), "DE_CH");
        assert_eq!(layout_code("ru"), "RU");
    }
}
