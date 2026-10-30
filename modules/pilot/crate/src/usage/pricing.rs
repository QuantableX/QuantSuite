//! What the tokens would have cost through the API (card 18) — an estimate,
//! not the subscription bill. Rates are LiteLLM's public price list (the one
//! ccusage uses), filtered to Anthropic's and OpenAI's own listings and the
//! standard tier: uncached input, cache reads, 5-minute and 1-hour cache
//! writes, output, and the long-context rates above 200k / 272k prompt
//! tokens. Batch, flex and priority rates do not apply to agent CLIs.

use super::scan::Tokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// USD per token.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_token_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_token_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_token_cost_above_1hr: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost_per_token_above_200k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost_per_token_above_200k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_token_cost_above_200k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_token_cost_above_200k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_cost_per_token_above_272k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_cost_per_token_above_272k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_read_input_token_cost_above_272k_tokens: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_token_cost_above_272k_tokens: Option<f64>,
}

/// The rate for a prompt size: the 272k tier above 272k, the 200k tier above 200k.
fn tier(base: f64, above_200k: Option<f64>, above_272k: Option<f64>, class: u8) -> f64 {
    match class {
        2 => above_272k.or(above_200k).unwrap_or(base),
        1 => above_200k.unwrap_or(base),
        _ => base,
    }
}

/// An estimate in USD, split as the page shows it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cost {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
    /// What the cache reads saved against uncached input.
    pub savings: f64,
}

impl Cost {
    pub fn add(&mut self, o: &Cost) {
        self.input += o.input;
        self.cache_read += o.cache_read;
        self.cache_write += o.cache_write;
        self.output += o.output;
        self.savings += o.savings;
    }
}

impl Rates {
    pub fn cost(&self, class: u8, t: &Tokens) -> Cost {
        let input = tier(self.input_cost_per_token, self.input_cost_per_token_above_200k_tokens, self.input_cost_per_token_above_272k_tokens, class);
        let output = tier(self.output_cost_per_token, self.output_cost_per_token_above_200k_tokens, self.output_cost_per_token_above_272k_tokens, class);
        // Without a listed cache rate, cached tokens cost what input costs.
        let read = self
            .cache_read_input_token_cost
            .map(|base| tier(base, self.cache_read_input_token_cost_above_200k_tokens, self.cache_read_input_token_cost_above_272k_tokens, class))
            .unwrap_or(input);
        let write_5m = self
            .cache_creation_input_token_cost
            .map(|base| tier(base, self.cache_creation_input_token_cost_above_200k_tokens, self.cache_creation_input_token_cost_above_272k_tokens, class))
            .unwrap_or(input);
        let write_1h = self.cache_creation_input_token_cost_above_1hr.unwrap_or(write_5m);
        Cost {
            input: t.input as f64 * input,
            cache_read: t.cache_read as f64 * read,
            cache_write: t.cache_write_5m as f64 * write_5m + t.cache_write_1h as f64 * write_1h,
            output: t.output as f64 * output,
            savings: t.cache_read as f64 * (input - read).max(0.0),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PriceList {
    models: HashMap<String, Rates>,
}

const BUNDLED: &str = include_str!("prices.json");

fn listed(name: &str) -> bool {
    name.starts_with("claude-") || name.starts_with("gpt-") || name.starts_with("codex") || (name.starts_with('o') && name[1..].starts_with(|c: char| c.is_ascii_digit()))
}

impl PriceList {
    /// The snapshot built into the app, for when nothing newer is at hand.
    pub fn bundled() -> Self {
        Self::from_json(BUNDLED).unwrap_or_default()
    }

    /// A saved list (`{"model": Rates}`).
    pub fn from_json(text: &str) -> Option<Self> {
        let models: HashMap<String, Rates> = serde_json::from_str(text).ok()?;
        (!models.is_empty()).then_some(Self { models })
    }

    /// LiteLLM's whole file → the vendors' own chat models and standard rates.
    pub fn from_litellm(all: &Value) -> Option<Self> {
        let mut models = HashMap::new();
        for (name, v) in all.as_object()? {
            let provider = v["litellm_provider"].as_str().unwrap_or_default();
            if !matches!(provider, "anthropic" | "openai") || !listed(name) {
                continue;
            }
            if !matches!(v["mode"].as_str(), None | Some("chat") | Some("responses")) {
                continue;
            }
            // Unknown fields (batch, flex, priority, audio …) fall away in the typed copy.
            if let Ok(rates) = serde_json::from_value::<Rates>(v.clone()) {
                if rates.input_cost_per_token > 0.0 || rates.output_cost_per_token > 0.0 {
                    models.insert(name.clone(), rates);
                }
            }
        }
        (models.len() > 10).then_some(Self { models })
    }

    pub fn to_json(&self) -> String {
        let sorted: std::collections::BTreeMap<_, _> = self.models.iter().collect();
        serde_json::to_string(&sorted).unwrap_or_else(|_| "{}".into())
    }

    /// The rates for a model as the logs name it: dated releases, provider
    /// prefixes and context suffixes find their listing.
    pub fn lookup(&self, model: &str) -> Option<&Rates> {
        let m = model.trim().to_ascii_lowercase();
        let m = m.trim_start_matches("anthropic/").trim_start_matches("openai/");
        let m = m.split('[').next().unwrap_or(m);
        if let Some(r) = self.models.get(m) {
            return Some(r);
        }
        // claude-haiku-4-5-20251001, gpt-4o-2024-08-06
        let undated = strip_date(m);
        self.models.get(undated)
    }
}

fn strip_date(m: &str) -> &str {
    let b = m.as_bytes();
    let digits = |s: &[u8]| s.iter().all(u8::is_ascii_digit);
    if b.len() > 9 && b[b.len() - 9] == b'-' && digits(&b[b.len() - 8..]) {
        return &m[..m.len() - 9];
    }
    if b.len() > 11 && b[b.len() - 11] == b'-' && digits(&b[b.len() - 10..b.len() - 6]) && b[b.len() - 6] == b'-' && b[b.len() - 3] == b'-' {
        return &m[..m.len() - 11];
    }
    m
}
