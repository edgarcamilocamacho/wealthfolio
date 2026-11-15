//! Scenario projection engine.
//!
//! A pure function: it reads neither the clock nor storage.
//!
//! - Each account is projected in its own currency, one point per day from the
//!   anchor (the real starting balance) to the end date inclusive.
//! - Each day after the anchor, each `rate` account first grows by
//!   `(1 + annual_rate)^(1/365) - 1` on a positive balance, less the interest
//!   tax. Then that day's transactions apply, in input order. A withdrawal or
//!   outgoing transfer from a `rate` account also pays the outflow tax.
//! - A currency's rate to the base currency on day `i` is
//!   `rate(anchor) × (1 + drift)^(i / 365)`.
//! - Transactions on or before the anchor, or after the end, have no effect.
//!   A side naming an unknown account is skipped; the other side still applies.

use std::collections::{BTreeMap, HashMap};

use chrono::NaiveDate;

use super::scenarios_model::{AccountGrowthModel, ScenarioTransactionKind};

const DAYS_PER_YEAR: f64 = 365.0;

#[derive(Debug, Clone, PartialEq)]
pub struct EngineAccount {
    pub id: String,
    pub currency: String,
    /// Balance on the anchor date, in `currency`.
    pub start_value: f64,
    pub model: AccountGrowthModel,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineTransaction {
    pub date: NaiveDate,
    pub kind: ScenarioTransactionKind,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub amount: f64,
    pub to_amount: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct EngineInput<'a> {
    pub anchor: NaiveDate,
    pub end: NaiveDate,
    pub accounts: &'a [EngineAccount],
    /// Units of base currency per unit of each currency on the anchor date.
    /// A currency without an entry has no known rate.
    pub fx_at_anchor: &'a HashMap<String, f64>,
    /// Annual drift of each currency against the base currency.
    pub fx_drift: &'a BTreeMap<String, f64>,
    pub transactions: &'a [EngineTransaction],
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineOutput {
    /// Anchor to end, one entry per day.
    pub dates: Vec<NaiveDate>,
    /// Balance per account per day, in the account's currency.
    pub balances: HashMap<String, Vec<f64>>,
    /// First day each account's balance went below zero.
    pub first_negative: BTreeMap<String, NaiveDate>,
    fx_at_anchor: HashMap<String, f64>,
    fx_drift: BTreeMap<String, f64>,
}

impl EngineOutput {
    /// Units of base currency per unit of `currency` on day `index`.
    pub fn fx_rate(&self, currency: &str, index: usize) -> Option<f64> {
        projected_rate(&self.fx_at_anchor, &self.fx_drift, currency, index)
    }
}

fn projected_rate(
    fx_at_anchor: &HashMap<String, f64>,
    fx_drift: &BTreeMap<String, f64>,
    currency: &str,
    index: usize,
) -> Option<f64> {
    let start = *fx_at_anchor.get(currency)?;
    let drift = fx_drift.get(currency).copied().unwrap_or(0.0);
    if drift == 0.0 {
        return Some(start);
    }
    Some(start * (1.0 + drift).powf(index as f64 / DAYS_PER_YEAR))
}

struct Growth {
    daily_rate: f64,
    interest_tax_rate: f64,
    outflow_tax_rate: f64,
}

fn growth_of(model: &AccountGrowthModel) -> Growth {
    match model {
        AccountGrowthModel::Flat => Growth {
            daily_rate: 0.0,
            interest_tax_rate: 0.0,
            outflow_tax_rate: 0.0,
        },
        AccountGrowthModel::Rate {
            annual_rate,
            interest_tax_rate,
            outflow_tax_rate,
        } => Growth {
            daily_rate: (1.0 + annual_rate).powf(1.0 / DAYS_PER_YEAR) - 1.0,
            interest_tax_rate: *interest_tax_rate,
            outflow_tax_rate: *outflow_tax_rate,
        },
    }
}

/// Runs the projection. Returns only the anchor day when `end <= anchor`.
pub fn project(input: &EngineInput<'_>) -> EngineOutput {
    let days = (input.end - input.anchor).num_days().max(0) as usize;
    let dates: Vec<NaiveDate> = (0..=days)
        .filter_map(|offset| {
            input
                .anchor
                .checked_add_days(chrono::Days::new(offset as u64))
        })
        .collect();

    let index_of: HashMap<&str, usize> = input
        .accounts
        .iter()
        .enumerate()
        .map(|(index, account)| (account.id.as_str(), index))
        .collect();
    let growth: Vec<Growth> = input
        .accounts
        .iter()
        .map(|account| growth_of(&account.model))
        .collect();
    let mut current: Vec<f64> = input
        .accounts
        .iter()
        .map(|account| account.start_value)
        .collect();
    let mut series: Vec<Vec<f64>> = current
        .iter()
        .map(|value| {
            let mut values = Vec::with_capacity(dates.len());
            values.push(*value);
            values
        })
        .collect();
    let mut first_negative: BTreeMap<String, NaiveDate> = BTreeMap::new();

    let mut transactions: Vec<&EngineTransaction> = input
        .transactions
        .iter()
        .filter(|tx| tx.date > input.anchor && tx.date <= input.end)
        .collect();
    transactions.sort_by_key(|tx| tx.date);
    let mut next_tx = 0;

    for (day, date) in dates.iter().enumerate().skip(1) {
        for (balance, account_growth) in current.iter_mut().zip(&growth) {
            if account_growth.daily_rate != 0.0 && *balance > 0.0 {
                let interest = *balance * account_growth.daily_rate;
                *balance += interest * (1.0 - account_growth.interest_tax_rate);
            }
        }

        while next_tx < transactions.len() && transactions[next_tx].date == *date {
            let tx = transactions[next_tx];
            next_tx += 1;
            let source = index_of.get(tx.account_id.as_str()).copied();
            match tx.kind {
                ScenarioTransactionKind::Deposit => {
                    if let Some(index) = source {
                        current[index] += tx.amount;
                    }
                }
                ScenarioTransactionKind::Withdrawal => {
                    if let Some(index) = source {
                        current[index] -= tx.amount * (1.0 + growth[index].outflow_tax_rate);
                    }
                }
                ScenarioTransactionKind::Transfer => {
                    if let Some(index) = source {
                        current[index] -= tx.amount * (1.0 + growth[index].outflow_tax_rate);
                    }
                    let destination = tx
                        .to_account_id
                        .as_deref()
                        .and_then(|id| index_of.get(id).copied());
                    if let Some(to_index) = destination {
                        let received = tx
                            .to_amount
                            .or_else(|| converted_amount(input, source, to_index, tx, day));
                        if let Some(received) = received {
                            current[to_index] += received;
                        }
                    }
                }
            }
        }

        for (index, balance) in current.iter().enumerate() {
            series[index].push(*balance);
            if *balance < 0.0 {
                first_negative
                    .entry(input.accounts[index].id.clone())
                    .or_insert(*date);
            }
        }
    }

    EngineOutput {
        dates,
        balances: input
            .accounts
            .iter()
            .zip(series)
            .map(|(account, values)| (account.id.clone(), values))
            .collect(),
        first_negative,
        fx_at_anchor: input.fx_at_anchor.clone(),
        fx_drift: input.fx_drift.clone(),
    }
}

/// Amount a transfer delivers in the destination's currency, at the day's
/// projected rates. A source outside the projection is assumed to share the
/// destination's currency only when there is nothing to convert from.
fn converted_amount(
    input: &EngineInput<'_>,
    source: Option<usize>,
    to_index: usize,
    tx: &EngineTransaction,
    day: usize,
) -> Option<f64> {
    let to_currency = &input.accounts[to_index].currency;
    let from_currency = match source {
        Some(index) => &input.accounts[index].currency,
        None => return Some(tx.amount),
    };
    if from_currency == to_currency {
        return Some(tx.amount);
    }
    let from_rate = projected_rate(input.fx_at_anchor, input.fx_drift, from_currency, day)?;
    let to_rate = projected_rate(input.fx_at_anchor, input.fx_drift, to_currency, day)?;
    if to_rate == 0.0 {
        return None;
    }
    Some(tx.amount * from_rate / to_rate)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("valid test date")
    }

    fn account(
        id: &str,
        currency: &str,
        start_value: f64,
        model: AccountGrowthModel,
    ) -> EngineAccount {
        EngineAccount {
            id: id.to_string(),
            currency: currency.to_string(),
            start_value,
            model,
        }
    }

    fn rate(annual_rate: f64, interest_tax_rate: f64, outflow_tax_rate: f64) -> AccountGrowthModel {
        AccountGrowthModel::Rate {
            annual_rate,
            interest_tax_rate,
            outflow_tax_rate,
        }
    }

    fn tx(
        on: &str,
        kind: ScenarioTransactionKind,
        from: &str,
        to: Option<&str>,
        amount: f64,
    ) -> EngineTransaction {
        EngineTransaction {
            date: date(on),
            kind,
            account_id: from.to_string(),
            to_account_id: to.map(str::to_string),
            amount,
            to_amount: None,
        }
    }

    fn fx(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
        pairs
            .iter()
            .map(|(currency, value)| (currency.to_string(), *value))
            .collect()
    }

    fn run(
        anchor: &str,
        end: &str,
        accounts: &[EngineAccount],
        rates: &HashMap<String, f64>,
        drift: &BTreeMap<String, f64>,
        transactions: &[EngineTransaction],
    ) -> EngineOutput {
        project(&EngineInput {
            anchor: date(anchor),
            end: date(end),
            accounts,
            fx_at_anchor: rates,
            fx_drift: drift,
            transactions,
        })
    }

    fn last(output: &EngineOutput, id: &str) -> f64 {
        *output.balances[id]
            .last()
            .expect("at least the anchor point")
    }

    #[test]
    fn first_point_is_the_anchor_value_and_dates_are_daily() {
        let accounts = [account("a", "USD", 100.0, rate(0.08, 0.0, 0.0))];
        let output = run(
            "2026-01-01",
            "2026-01-31",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &[],
        );
        assert_eq!(output.dates.len(), 31);
        assert_eq!(output.dates[0], date("2026-01-01"));
        assert_eq!(output.dates[30], date("2026-01-31"));
        assert_eq!(output.balances["a"][0], 100.0);
    }

    #[test]
    fn flat_accounts_without_transactions_stay_put() {
        let accounts = [account("a", "USD", 250.0, AccountGrowthModel::Flat)];
        let output = run(
            "2026-01-01",
            "2027-01-01",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &[],
        );
        assert!(output.balances["a"].iter().all(|value| *value == 250.0));
    }

    #[test]
    fn rate_grows_by_the_annual_rate_over_365_days() {
        let accounts = [account("a", "USD", 1000.0, rate(0.08, 0.0, 0.0))];
        let output = run(
            "2026-01-01",
            "2027-01-01",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &[],
        );
        assert!((last(&output, "a") - 1080.0).abs() < 1e-6);
    }

    #[test]
    fn interest_tax_withholds_a_share_of_interest() {
        let accounts = [account("a", "USD", 1000.0, rate(0.10, 0.5, 0.0))];
        let output = run(
            "2026-01-01",
            "2026-01-02",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &[],
        );
        let daily = 1.1_f64.powf(1.0 / 365.0) - 1.0;
        assert!((last(&output, "a") - (1000.0 + 1000.0 * daily * 0.5)).abs() < 1e-9);
    }

    #[test]
    fn negative_balances_earn_nothing_and_are_reported() {
        let accounts = [account("a", "USD", 10.0, rate(0.20, 0.0, 0.0))];
        let transactions = [tx(
            "2026-01-02",
            ScenarioTransactionKind::Withdrawal,
            "a",
            None,
            100.0,
        )];
        let output = run(
            "2026-01-01",
            "2026-02-01",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &transactions,
        );
        let after_withdrawal = output.balances["a"][1];
        assert!(after_withdrawal < 0.0);
        assert_eq!(last(&output, "a"), after_withdrawal);
        assert_eq!(output.first_negative["a"], date("2026-01-02"));
    }

    #[test]
    fn a_transfer_between_flat_accounts_keeps_the_total() {
        let accounts = [
            account("a", "AED", 50_000.0, AccountGrowthModel::Flat),
            account("b", "AED", 0.0, AccountGrowthModel::Flat),
        ];
        let transactions = [tx(
            "2026-01-25",
            ScenarioTransactionKind::Transfer,
            "a",
            Some("b"),
            5_000.0,
        )];
        let output = run(
            "2026-01-01",
            "2026-02-01",
            &accounts,
            &fx(&[("AED", 0.2723)]),
            &BTreeMap::new(),
            &transactions,
        );
        assert_eq!(last(&output, "a"), 45_000.0);
        assert_eq!(last(&output, "b"), 5_000.0);
    }

    #[test]
    fn a_transfer_converts_at_the_projected_rate_unless_to_amount_is_given() {
        let accounts = [
            account("aed", "AED", 10_000.0, AccountGrowthModel::Flat),
            account("usd", "USD", 0.0, AccountGrowthModel::Flat),
        ];
        let rates = fx(&[("AED", 0.25), ("USD", 1.0)]);
        let converted = run(
            "2026-01-01",
            "2026-01-03",
            &accounts,
            &rates,
            &BTreeMap::new(),
            &[tx(
                "2026-01-02",
                ScenarioTransactionKind::Transfer,
                "aed",
                Some("usd"),
                4_000.0,
            )],
        );
        assert!((last(&converted, "usd") - 1_000.0).abs() < 1e-9);

        let mut fixed = tx(
            "2026-01-02",
            ScenarioTransactionKind::Transfer,
            "aed",
            Some("usd"),
            4_000.0,
        );
        fixed.to_amount = Some(1_088.0);
        let explicit = run(
            "2026-01-01",
            "2026-01-03",
            &accounts,
            &rates,
            &BTreeMap::new(),
            &[fixed],
        );
        assert_eq!(last(&explicit, "usd"), 1_088.0);
    }

    #[test]
    fn outflow_tax_applies_to_withdrawals_and_outgoing_transfers() {
        let accounts = [
            account("bank", "COP", 1_000_000.0, rate(0.0, 0.0, 0.004)),
            account("other", "COP", 0.0, AccountGrowthModel::Flat),
        ];
        let transactions = [
            tx(
                "2026-01-02",
                ScenarioTransactionKind::Withdrawal,
                "bank",
                None,
                100_000.0,
            ),
            tx(
                "2026-01-03",
                ScenarioTransactionKind::Transfer,
                "bank",
                Some("other"),
                100_000.0,
            ),
        ];
        let output = run(
            "2026-01-01",
            "2026-01-04",
            &accounts,
            &fx(&[("COP", 0.00025)]),
            &BTreeMap::new(),
            &transactions,
        );
        assert!((last(&output, "bank") - (1_000_000.0 - 2.0 * 100_400.0)).abs() < 1e-6);
        assert_eq!(last(&output, "other"), 100_000.0);
    }

    #[test]
    fn transactions_outside_the_window_are_ignored() {
        let accounts = [account("a", "USD", 100.0, AccountGrowthModel::Flat)];
        let transactions = [
            tx(
                "2026-01-01",
                ScenarioTransactionKind::Deposit,
                "a",
                None,
                1.0,
            ),
            tx(
                "2025-12-01",
                ScenarioTransactionKind::Deposit,
                "a",
                None,
                1.0,
            ),
            tx(
                "2026-03-01",
                ScenarioTransactionKind::Deposit,
                "a",
                None,
                1.0,
            ),
        ];
        let output = run(
            "2026-01-01",
            "2026-02-01",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &transactions,
        );
        assert_eq!(last(&output, "a"), 100.0);
    }

    #[test]
    fn unknown_accounts_skip_only_their_side() {
        let accounts = [account("a", "USD", 100.0, AccountGrowthModel::Flat)];
        let transactions = [
            tx(
                "2026-01-02",
                ScenarioTransactionKind::Transfer,
                "a",
                Some("missing"),
                40.0,
            ),
            tx(
                "2026-01-03",
                ScenarioTransactionKind::Transfer,
                "missing",
                Some("a"),
                10.0,
            ),
        ];
        let output = run(
            "2026-01-01",
            "2026-01-04",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &transactions,
        );
        assert_eq!(last(&output, "a"), 70.0);
    }

    #[test]
    fn fx_drift_compounds_from_the_anchor_rate() {
        let accounts = [account("cop", "COP", 1.0, AccountGrowthModel::Flat)];
        let drift = BTreeMap::from([("COP".to_string(), -0.05)]);
        let output = run(
            "2026-01-01",
            "2027-01-01",
            &accounts,
            &fx(&[("COP", 0.00025)]),
            &drift,
            &[],
        );
        let rate = output
            .fx_rate("COP", output.dates.len() - 1)
            .expect("rate for a known currency");
        assert!((rate - 0.00025 * 0.95).abs() < 1e-12);
        assert_eq!(output.fx_rate("EUR", 0), None);
    }

    #[test]
    fn end_before_anchor_returns_only_the_anchor() {
        let accounts = [account("a", "USD", 5.0, AccountGrowthModel::Flat)];
        let output = run(
            "2026-01-10",
            "2026-01-01",
            &accounts,
            &fx(&[("USD", 1.0)]),
            &BTreeMap::new(),
            &[],
        );
        assert_eq!(output.dates, vec![date("2026-01-10")]);
        assert_eq!(output.balances["a"], vec![5.0]);
    }
}
