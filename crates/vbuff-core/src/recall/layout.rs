//! Search-only physical key mapping: English QWERTY ↔ Russian ЙЦУКЕН.
//! Stored content and structured query operators are never rewritten.
pub fn layout_variants(query: &str) -> Vec<String> {
    const EN: &str = "`qwertyuiop[]asdfghjkl;'zxcvbnm,.";
    const RU: &str = "ёйцукенгшщзхъфывапролджэячсмитьбю";
    let original = query.to_lowercase();
    let mut variants = vec![original.clone()];
    if !original.chars().any(char::is_alphabetic) {
        return variants;
    }
    for (from, to) in [(EN, RU), (RU, EN)] {
        let mapped: String = original
            .chars()
            .map(|ch| {
                from.chars()
                    .position(|key| key == ch)
                    .and_then(|i| to.chars().nth(i))
                    .unwrap_or(ch)
            })
            .collect();
        if !variants.contains(&mapped) {
            variants.push(mapped);
        }
    }
    variants
}

pub fn layout_contains(text: &str, query: &str) -> bool {
    let text = text.to_lowercase();
    layout_variants(query)
        .iter()
        .any(|query| text.contains(query))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_keys_work_both_ways_without_transliteration() {
        assert!(layout_contains("Привет мир", "GHBDTN"));
        assert!(layout_contains("hello world", "руддщ"));
        assert!(layout_contains("работа", "hf,jnf"));
        assert!(!layout_contains("привет", "privet"));
        assert_eq!(layout_variants("123."), vec!["123."]);
    }
}
