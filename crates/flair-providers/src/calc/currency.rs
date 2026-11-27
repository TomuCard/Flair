//! Exchange rates for currency conversions (`100 € en $`), from the European
//! Central Bank through Frankfurter (free, no key). Fetched in the background
//! and cached on disk; typing never waits for them.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

/// Rates of one day: units of each currency for one euro.
#[derive(Debug, Clone, PartialEq)]
pub struct Rates {
    /// Date of the rates (`2026-11-26`).
    pub date: String,
    pub per_euro: HashMap<String, f64>,
}

/// Rates shared between the fetcher and the calculator.
pub type SharedRates = Arc<RwLock<Option<Rates>>>;

/// Source of the rates.
pub const URL: &str = "https://api.frankfurter.app/latest";

impl Rates {
    /// Reads Frankfurter's answer (also the cache format):
    /// `{"base": "EUR", "date": "2026-11-26", "rates": {"USD": 1.05, …}}`.
    pub fn parse(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        if value.get("base").and_then(serde_json::Value::as_str) != Some("EUR") {
            return None;
        }
        let date = value.get("date")?.as_str()?.to_owned();
        let mut per_euro: HashMap<String, f64> = value
            .get("rates")?
            .as_object()?
            .iter()
            .filter_map(|(code, rate)| {
                Some((code.to_uppercase(), rate.as_f64().filter(|r| *r > 0.0)?))
            })
            .collect();
        per_euro.insert("EUR".into(), 1.0);
        (per_euro.len() > 1).then_some(Self { date, per_euro })
    }

    pub fn to_json(&self) -> String {
        let rates: serde_json::Map<String, serde_json::Value> = self
            .per_euro
            .iter()
            .filter(|(code, _)| *code != "EUR")
            .map(|(code, rate)| (code.clone(), serde_json::json!(rate)))
            .collect();
        serde_json::json!({ "base": "EUR", "date": self.date, "rates": rates }).to_string()
    }

    /// Units of `currency` for one euro (fend's base currency).
    pub fn per_euro(&self, currency: &str) -> Option<f64> {
        self.per_euro.get(&currency.to_uppercase()).copied()
    }

    /// Reads the cache file.
    pub fn load(file: &Path) -> Option<Self> {
        Self::parse(&std::fs::read_to_string(file).ok()?)
    }
}

/// Handler given to fend: rates from the shared value, never a network call.
pub struct Handler(pub SharedRates);

impl fend_core::ExchangeRateFnV2 for Handler {
    fn relative_to_base_currency(
        &self,
        currency: &str,
        _options: &fend_core::ExchangeRateFnV2Options,
    ) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.0
            .read()
            .ok()
            .and_then(|r| r.as_ref().and_then(|r| r.per_euro(currency)))
            .ok_or_else(|| format!("no exchange rate for {currency}").into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{"amount":1.0,"base":"EUR","date":"2026-11-26","rates":{"USD":1.1,"GBP":0.85,"JPY":160.0}}"#;

    #[test]
    fn parse_and_round_trip() {
        let rates = Rates::parse(SAMPLE).expect("rates");
        assert_eq!(rates.date, "2026-11-26");
        assert_eq!(rates.per_euro("eur"), Some(1.0));
        assert_eq!(rates.per_euro("usd"), Some(1.1));
        assert_eq!(Rates::parse(&rates.to_json()), Some(rates));
        assert!(Rates::parse(r#"{"base":"USD","date":"x","rates":{"EUR":1}}"#).is_none());
        assert!(Rates::parse("oops").is_none());
    }

    #[test]
    fn fend_converts_with_the_handler() {
        let shared: SharedRates = Arc::new(RwLock::new(Rates::parse(SAMPLE)));
        let mut context = fend_core::Context::new();
        context.set_exchange_rate_handler_v2(Handler(shared));
        let result = fend_core::evaluate("100 EUR to USD", &mut context).expect("conversion");
        assert_eq!(result.get_main_result(), "110 USD");
    }
}
