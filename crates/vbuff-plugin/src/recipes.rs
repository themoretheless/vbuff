//! Reviewed, deterministic starter recipes with no ambient capabilities.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use vbuff_core::workflow::clean_link;

use crate::{PluginError, Result};

const MAX_RECIPE_INPUT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StarterRecipeId {
    CleanUrl,
    NormalizeSmartQuotes,
    PrettyJson,
    MaskCardPreview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StarterRecipe {
    pub id: StarterRecipeId,
    pub title: &'static str,
    pub changes_canonical_content: bool,
}

pub const STARTER_RECIPES: [StarterRecipe; 4] = [
    StarterRecipe {
        id: StarterRecipeId::CleanUrl,
        title: "Copy clean link",
        changes_canonical_content: true,
    },
    StarterRecipe {
        id: StarterRecipeId::NormalizeSmartQuotes,
        title: "Normalize smart quotes",
        changes_canonical_content: true,
    },
    StarterRecipe {
        id: StarterRecipeId::PrettyJson,
        title: "Pretty-print JSON",
        changes_canonical_content: true,
    },
    StarterRecipe {
        id: StarterRecipeId::MaskCardPreview,
        title: "Mask card number in preview",
        changes_canonical_content: false,
    },
];

pub fn apply_starter_recipe(id: StarterRecipeId, input: &str) -> Result<String> {
    if input.len() > MAX_RECIPE_INPUT_BYTES {
        return Err(PluginError::InvalidInput(
            "recipe input exceeds the byte limit".into(),
        ));
    }
    match id {
        StarterRecipeId::CleanUrl => {
            clean_link(input).map_err(|error| PluginError::InvalidInput(error.to_string()))
        }
        StarterRecipeId::NormalizeSmartQuotes => Ok(input
            .replace(['\u{2018}', '\u{2019}'], "'")
            .replace(['\u{201c}', '\u{201d}'], "\"")
            .replace(['\u{2013}', '\u{2014}'], "-")),
        StarterRecipeId::PrettyJson => {
            let value: serde_json::Value = serde_json::from_str(input)
                .map_err(|_| PluginError::InvalidInput("invalid JSON".into()))?;
            serde_json::to_string_pretty(&value)
                .map_err(|error| PluginError::Serialization(error.to_string()))
        }
        StarterRecipeId::MaskCardPreview => Ok(mask_card_numbers(input)),
    }
}

fn mask_card_numbers(input: &str) -> String {
    static CANDIDATES: OnceLock<Regex> = OnceLock::new();
    let candidates =
        CANDIDATES.get_or_init(|| Regex::new(r"\b(?:[0-9][ -]?){12,18}[0-9]\b").unwrap());
    candidates
        .replace_all(input, |captures: &regex::Captures<'_>| {
            let original = captures.get(0).map_or("", |value| value.as_str());
            let digits = original
                .chars()
                .filter(char::is_ascii_digit)
                .collect::<String>();
            if (13..=19).contains(&digits.len()) && luhn_valid(&digits) {
                format!("**** **** **** {}", &digits[digits.len() - 4..])
            } else {
                original.to_string()
            }
        })
        .into_owned()
}

fn luhn_valid(digits: &str) -> bool {
    let sum = digits
        .bytes()
        .rev()
        .enumerate()
        .map(|(index, byte)| {
            let mut value = u32::from(byte - b'0');
            if index % 2 == 1 {
                value *= 2;
                if value > 9 {
                    value -= 9;
                }
            }
            value
        })
        .sum::<u32>();
    sum % 10 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_url_unwraps_only_known_redirects_and_drops_trackers() {
        let clean = apply_starter_recipe(
            StarterRecipeId::CleanUrl,
            "https://www.google.com/url?q=https%3A%2F%2Fexample.test%2Fdoc%3Fa%3D1%26utm_source%3Dx&fbclid=y",
        )
        .unwrap();
        assert_eq!(clean, "https://example.test/doc?a=1");
        let ordinary = apply_starter_recipe(
            StarterRecipeId::CleanUrl,
            "https://example.test/path?url=https%3A%2F%2Fevil.test&utm_campaign=x",
        )
        .unwrap();
        assert_eq!(
            ordinary,
            "https://example.test/path?url=https%3A%2F%2Fevil.test"
        );
    }

    #[test]
    fn preview_mask_uses_luhn_and_never_changes_non_cards() {
        assert_eq!(
            apply_starter_recipe(StarterRecipeId::MaskCardPreview, "Card 4111 1111 1111 1111")
                .unwrap(),
            "Card **** **** **** 1111"
        );
        assert_eq!(
            apply_starter_recipe(StarterRecipeId::MaskCardPreview, "Build 1234-5678").unwrap(),
            "Build 1234-5678"
        );
    }

    #[test]
    fn gallery_is_small_reviewable_and_unique() {
        let ids = STARTER_RECIPES
            .iter()
            .map(|recipe| recipe.id)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(ids.len(), STARTER_RECIPES.len());
    }
}
