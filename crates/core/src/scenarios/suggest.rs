//! Infers a `rate` growth model from an account's recent interest history.
//!
//! - Gross interest is each posted INTEREST activity's final amount plus the
//!   tax recorded on it. Withheld tax is that recorded tax plus the account's
//!   standalone TAX activities in the window.
//! - The yield is gross interest over the average daily balance, scaled to
//!   the window's length and compounded daily into an effective annual rate.

use chrono::NaiveDate;

/// Least history the window needs before a suggestion means anything.
pub const MIN_SUGGESTION_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct InterestEvent {
    pub date: NaiveDate,
    /// Final (net) amount received.
    pub net_amount: f64,
    /// Tax recorded on the interest payment itself.
    pub tax: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestionInputs<'a> {
    pub interest: &'a [InterestEvent],
    /// Standalone TAX activities in the window.
    pub standalone_tax: f64,
    /// Daily balances in the window, in the account's currency.
    pub balances: &'a [(NaiveDate, f64)],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InferredRate {
    pub annual_rate: f64,
    pub interest_tax_rate: f64,
    pub days: i64,
}

/// None without interest, without a positive average balance, or with less
/// than [`MIN_SUGGESTION_DAYS`] of balances.
pub fn infer_rate(inputs: &SuggestionInputs<'_>) -> Option<InferredRate> {
    let first = inputs.balances.first()?.0;
    let last = inputs.balances.last()?.0;
    let days = (last - first).num_days() + 1;
    if days < MIN_SUGGESTION_DAYS {
        return None;
    }
    let gross: f64 = inputs
        .interest
        .iter()
        .map(|event| event.net_amount + event.tax)
        .sum();
    if gross <= 0.0 {
        return None;
    }
    let withheld: f64 =
        inputs.interest.iter().map(|event| event.tax).sum::<f64>() + inputs.standalone_tax;
    let average =
        inputs.balances.iter().map(|(_, value)| value).sum::<f64>() / inputs.balances.len() as f64;
    if average <= 0.0 {
        return None;
    }
    let daily = gross / average / days as f64;
    let annual_rate = (1.0 + daily).powf(365.0) - 1.0;
    if !annual_rate.is_finite() {
        return None;
    }
    Some(InferredRate {
        annual_rate,
        interest_tax_rate: (withheld / gross).clamp(0.0, 1.0),
        days,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("valid test date")
    }

    fn year_of_balances(value: f64) -> Vec<(NaiveDate, f64)> {
        let start = date("2026-01-01");
        (0..365)
            .map(|offset| (start + chrono::Days::new(offset), value))
            .collect()
    }

    #[test]
    fn a_year_of_interest_gives_its_effective_rate_and_withholding() {
        let balances = year_of_balances(10_000.0);
        // 12 monthly payments: 1,000 gross a year, 7 % withheld on the payment.
        let interest: Vec<InterestEvent> = (1..=12)
            .map(|month| InterestEvent {
                date: NaiveDate::from_ymd_opt(2026, month, 28).expect("valid date"),
                net_amount: 1000.0 / 12.0 * 0.93,
                tax: 1000.0 / 12.0 * 0.07,
            })
            .collect();
        let inferred = infer_rate(&SuggestionInputs {
            interest: &interest,
            standalone_tax: 0.0,
            balances: &balances,
        })
        .expect("suggestion");
        let expected = (1.0 + 0.1 / 365.0_f64).powf(365.0) - 1.0;
        assert!((inferred.annual_rate - expected).abs() < 1e-9);
        assert!((inferred.interest_tax_rate - 0.07).abs() < 1e-9);
        assert_eq!(inferred.days, 365);
    }

    #[test]
    fn standalone_taxes_count_as_withholding() {
        let balances = year_of_balances(1_000.0);
        let interest = [InterestEvent {
            date: date("2026-06-30"),
            net_amount: 50.0,
            tax: 0.0,
        }];
        let inferred = infer_rate(&SuggestionInputs {
            interest: &interest,
            standalone_tax: 5.0,
            balances: &balances,
        })
        .expect("suggestion");
        assert!((inferred.interest_tax_rate - 0.1).abs() < 1e-9);
    }

    #[test]
    fn no_interest_or_short_history_gives_nothing() {
        let balances = year_of_balances(1_000.0);
        assert!(infer_rate(&SuggestionInputs {
            interest: &[],
            standalone_tax: 0.0,
            balances: &balances,
        })
        .is_none());
        let interest = [InterestEvent {
            date: date("2026-01-05"),
            net_amount: 1.0,
            tax: 0.0,
        }];
        assert!(infer_rate(&SuggestionInputs {
            interest: &interest,
            standalone_tax: 0.0,
            balances: &balances[..10],
        })
        .is_none());
    }
}
