//! Scenario domain models.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A saved projection: an anchor date, an end date, per-account growth models,
/// per-currency FX drift and a list of hypothetical transactions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    /// YYYY-MM-DD. The projection starts from real balances on this day.
    pub anchor_date: String,
    /// YYYY-MM-DD. Last projected day.
    pub end_date: String,
    pub assumptions: ScenarioAssumptions,
    pub created_at: String,
    pub updated_at: String,
}

/// Input for creating or updating a scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioInput {
    pub name: String,
    pub description: Option<String>,
    pub anchor_date: String,
    pub end_date: String,
    #[serde(default)]
    pub assumptions: ScenarioAssumptions,
}

/// What the user does not decide: how each account grows and how each currency
/// moves against the base currency.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioAssumptions {
    /// Growth model per account id. An account without an entry is `flat`.
    #[serde(default)]
    pub accounts: BTreeMap<String, AccountGrowthModel>,
    /// Annual drift of each currency against the base currency
    /// (0.03 = the currency gains 3 % a year). Missing currencies stay flat.
    #[serde(default)]
    pub fx: BTreeMap<String, f64>,
}

/// How an account's balance grows between transactions.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(tag = "model", rename_all = "camelCase")]
pub enum AccountGrowthModel {
    /// The balance changes only through transactions.
    #[default]
    Flat,
    /// Daily compounding of an effective annual rate on a positive balance.
    #[serde(rename_all = "camelCase")]
    Rate {
        /// Effective annual rate (0.08 = 8 % a year).
        annual_rate: f64,
        /// Share of each day's interest withheld as tax (0.07 = 7 %).
        #[serde(default)]
        interest_tax_rate: f64,
        /// Tax charged on each withdrawal or outgoing transfer (0.004 = 4×1000).
        #[serde(default)]
        outflow_tax_rate: f64,
    },
}

/// Kind of a hypothetical transaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScenarioTransactionKind {
    Deposit,
    Withdrawal,
    Transfer,
}

impl ScenarioTransactionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Deposit => "DEPOSIT",
            Self::Withdrawal => "WITHDRAWAL",
            Self::Transfer => "TRANSFER",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DEPOSIT" => Some(Self::Deposit),
            "WITHDRAWAL" => Some(Self::Withdrawal),
            "TRANSFER" => Some(Self::Transfer),
            _ => None,
        }
    }
}

/// One dated hypothetical transaction. It never enters the activities table.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTransaction {
    pub id: String,
    pub scenario_id: String,
    /// YYYY-MM-DD
    pub date: String,
    pub kind: ScenarioTransactionKind,
    /// The account money comes from (or goes to, for a deposit).
    pub account_id: String,
    /// Destination of a transfer.
    pub to_account_id: Option<String>,
    /// Positive amount in `account_id`'s currency.
    pub amount: f64,
    /// Amount received, in the destination's currency. Without it a transfer
    /// converts at the scenario's projected FX rate for the day.
    pub to_amount: Option<f64>,
    pub note: Option<String>,
    /// Rows created together by one recurrence rule share a series id.
    pub series_id: Option<String>,
    pub series_label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Fields the user writes for a single transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTransactionInput {
    pub date: String,
    pub kind: ScenarioTransactionKind,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub amount: f64,
    pub to_amount: Option<f64>,
    pub note: Option<String>,
}

/// An edit of an existing transaction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTransactionUpdate {
    pub id: String,
    #[serde(flatten)]
    pub input: ScenarioTransactionInput,
}

/// A row ready to insert: the user's fields plus the series it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct NewScenarioTransaction {
    pub input: ScenarioTransactionInput,
    pub series_id: Option<String>,
    pub series_label: Option<String>,
}

/// A monthly recurrence rule. The backend expands it into literal rows that
/// share a series id; the rule itself is not stored.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioSeriesRule {
    pub label: String,
    pub kind: ScenarioTransactionKind,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub amount: f64,
    pub to_amount: Option<f64>,
    pub note: Option<String>,
    /// 1..=31. A day past the end of a short month falls on its last day.
    pub day_of_month: u32,
    /// YYYY-MM-DD, inclusive.
    pub start_date: String,
    /// YYYY-MM-DD, inclusive.
    pub end_date: String,
    /// Deletes this series' rows before inserting the new ones.
    pub replace_series_id: Option<String>,
}

/// One batch of changes to a scenario's transactions, applied atomically.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SaveScenarioTransactionsRequest {
    #[serde(default)]
    pub creates: Vec<ScenarioTransactionInput>,
    #[serde(default)]
    pub updates: Vec<ScenarioTransactionUpdate>,
    #[serde(default)]
    pub delete_ids: Vec<String>,
    #[serde(default)]
    pub series: Vec<ScenarioSeriesRule>,
}

/// Projected values for an account scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioProjection {
    pub scenario_id: String,
    pub base_currency: String,
    /// Currency of `value`: the account's for a single-account scope, the base
    /// currency otherwise.
    pub currency: String,
    pub points: Vec<ScenarioProjectionPoint>,
    pub warnings: Vec<ScenarioWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioProjectionPoint {
    /// YYYY-MM-DD
    pub date: String,
    /// In `ScenarioProjection::currency`.
    pub value: f64,
    pub value_base: f64,
}

/// Something the user should know about a projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioWarning {
    pub code: ScenarioWarningCode,
    pub account_id: Option<String>,
    pub currency: Option<String>,
    /// YYYY-MM-DD, when the warning concerns a day.
    pub date: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScenarioWarningCode {
    /// A transaction or growth model names an account that does not exist.
    UnknownAccount,
    /// The account has no valuation on or before the anchor; it starts at 0.
    NoAnchorValuation,
    /// No FX rate to the base currency; the account is left out of base totals.
    MissingFxRate,
    /// The projected balance goes below zero on `date`.
    NegativeBalance,
}
