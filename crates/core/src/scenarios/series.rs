//! Expansion of monthly recurrence rules into literal transactions.

use chrono::{Datelike, Months, NaiveDate};

use super::scenarios_model::{
    NewScenarioTransaction, ScenarioSeriesRule, ScenarioTransactionInput,
};
use crate::errors::{Error, Result, ValidationError};

/// Most rows one rule may create (100 years of monthly rows).
pub const MAX_SERIES_OCCURRENCES: usize = 1200;
const MAX_SERIES_LABEL_CHARS: usize = 100;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.into()))
}

pub(crate) fn parse_date(value: &str, field: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| invalid(format!("{field} must be a date in YYYY-MM-DD format")))
}

fn last_day_of_month(year: i32, month: u32) -> Option<u32> {
    let first = NaiveDate::from_ymd_opt(year, month, 1)?;
    let next = first.checked_add_months(Months::new(1))?;
    next.pred_opt().map(|day| day.day())
}

/// The dates a rule falls on: `day_of_month` of every month from `start_date`
/// to `end_date` inclusive, clamped to each month's last day.
pub fn series_dates(rule: &ScenarioSeriesRule) -> Result<Vec<NaiveDate>> {
    if !(1..=31).contains(&rule.day_of_month) {
        return Err(invalid("The day of the month must be between 1 and 31"));
    }
    let start = parse_date(&rule.start_date, "The series start date")?;
    let end = parse_date(&rule.end_date, "The series end date")?;
    if end < start {
        return Err(invalid("The series must end on or after its start date"));
    }

    let mut dates = Vec::new();
    let mut month = NaiveDate::from_ymd_opt(start.year(), start.month(), 1)
        .ok_or_else(|| invalid("The series start date is out of range"))?;
    while month <= end {
        let last_day = last_day_of_month(month.year(), month.month())
            .ok_or_else(|| invalid("The series runs out of the supported date range"))?;
        let day = rule.day_of_month.min(last_day);
        let date = NaiveDate::from_ymd_opt(month.year(), month.month(), day)
            .ok_or_else(|| invalid("The series runs out of the supported date range"))?;
        if date >= start && date <= end {
            dates.push(date);
            if dates.len() > MAX_SERIES_OCCURRENCES {
                return Err(invalid(format!(
                    "A series can create at most {MAX_SERIES_OCCURRENCES} transactions"
                )));
            }
        }
        month = month
            .checked_add_months(Months::new(1))
            .ok_or_else(|| invalid("The series runs out of the supported date range"))?;
    }
    Ok(dates)
}

/// Expands a rule into rows that share `series_id`. The rows still go through
/// the same per-transaction validation as single rows.
pub fn expand_series(
    rule: &ScenarioSeriesRule,
    series_id: &str,
) -> Result<Vec<NewScenarioTransaction>> {
    let label = rule.label.trim();
    if label.is_empty() {
        return Err(invalid("A series needs a label"));
    }
    if label.chars().count() > MAX_SERIES_LABEL_CHARS {
        return Err(invalid(format!(
            "A series label can have at most {MAX_SERIES_LABEL_CHARS} characters"
        )));
    }
    Ok(series_dates(rule)?
        .into_iter()
        .map(|date| NewScenarioTransaction {
            input: ScenarioTransactionInput {
                date: date.format("%Y-%m-%d").to_string(),
                kind: rule.kind,
                account_id: rule.account_id.clone(),
                to_account_id: rule.to_account_id.clone(),
                amount: rule.amount,
                to_amount: rule.to_amount,
                note: rule.note.clone(),
            },
            series_id: Some(series_id.to_string()),
            series_label: Some(label.to_string()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenarios::ScenarioTransactionKind;

    fn rule(day_of_month: u32, start: &str, end: &str) -> ScenarioSeriesRule {
        ScenarioSeriesRule {
            label: "Salary".to_string(),
            kind: ScenarioTransactionKind::Deposit,
            account_id: "a".to_string(),
            to_account_id: None,
            amount: 44_000.0,
            to_amount: None,
            note: None,
            day_of_month,
            start_date: start.to_string(),
            end_date: end.to_string(),
            replace_series_id: None,
        }
    }

    fn formatted(dates: Vec<NaiveDate>) -> Vec<String> {
        dates
            .into_iter()
            .map(|date| date.format("%Y-%m-%d").to_string())
            .collect()
    }

    #[test]
    fn monthly_dates_are_inclusive_at_both_ends() {
        let dates = series_dates(&rule(25, "2026-01-25", "2026-04-25")).expect("valid rule");
        assert_eq!(
            formatted(dates),
            ["2026-01-25", "2026-02-25", "2026-03-25", "2026-04-25"]
        );
    }

    #[test]
    fn days_past_the_month_end_fall_on_its_last_day() {
        let dates = series_dates(&rule(31, "2026-01-01", "2028-03-01")).expect("valid rule");
        let formatted = formatted(dates);
        assert_eq!(formatted[0], "2026-01-31");
        assert_eq!(formatted[1], "2026-02-28");
        assert_eq!(formatted[3], "2026-04-30");
        assert!(formatted.contains(&"2028-02-29".to_string()));
    }

    #[test]
    fn a_start_after_the_day_skips_that_month() {
        let dates = series_dates(&rule(5, "2026-01-10", "2026-03-31")).expect("valid rule");
        assert_eq!(formatted(dates), ["2026-02-05", "2026-03-05"]);
    }

    #[test]
    fn invalid_rules_are_rejected() {
        assert!(series_dates(&rule(0, "2026-01-01", "2026-02-01")).is_err());
        assert!(series_dates(&rule(32, "2026-01-01", "2026-02-01")).is_err());
        assert!(series_dates(&rule(1, "2026-03-01", "2026-02-01")).is_err());
        assert!(series_dates(&rule(1, "2026-01-01x", "2026-02-01")).is_err());
        assert!(series_dates(&rule(1, "2026-01-01", "2127-01-01")).is_err());
    }

    #[test]
    fn expanded_rows_share_the_series() {
        let rows = expand_series(&rule(1, "2026-01-01", "2026-03-01"), "s1").expect("valid rule");
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.series_id.as_deref() == Some("s1")
            && row.series_label.as_deref() == Some("Salary")
            && row.input.amount == 44_000.0));
    }

    #[test]
    fn a_series_needs_a_label() {
        let mut unnamed = rule(1, "2026-01-01", "2026-03-01");
        unnamed.label = "  ".to_string();
        assert!(expand_series(&unnamed, "s1").is_err());
    }
}
