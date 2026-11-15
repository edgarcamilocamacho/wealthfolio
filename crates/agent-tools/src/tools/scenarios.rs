//! Scenario tools: list saved projections, draft hypothetical transactions for
//! one, and (MCP only) commit a reviewed draft. Scenario transactions never
//! become real activities.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use wealthfolio_core::scenarios::{
    NewScenarioTransaction, SaveScenarioTransactionsRequest, ScenarioAssumptions,
    ScenarioSeriesRule, ScenarioServiceTrait, ScenarioTransactionInput, ScenarioTransactionKind,
};

use crate::env::AgentEnvironment;
use crate::scope::AgentScope;
use crate::tool::{AgentTool, AgentToolAccess, AgentToolError, AgentToolResult};

/// Rows one draft or commit may create, series included.
const MAX_ROWS_PER_CALL: usize = 2000;

fn scenario_service(
    env: &Arc<dyn AgentEnvironment>,
) -> Result<Arc<dyn ScenarioServiceTrait>, AgentToolError> {
    env.scenario_service()
        .ok_or_else(|| AgentToolError::ExecutionFailed("Scenarios are not available".to_string()))
}

fn transaction_schema(with_date: bool) -> serde_json::Value {
    let mut properties = serde_json::json!({
        "kind": {
            "type": "string",
            "enum": ["DEPOSIT", "WITHDRAWAL", "TRANSFER"],
            "description": "DEPOSIT adds money to accountId, WITHDRAWAL takes it out, TRANSFER moves it from accountId to toAccountId."
        },
        "accountId": { "type": "string", "description": "Account id from get_accounts. For a transfer, the source." },
        "toAccountId": { "type": "string", "description": "Destination account id. TRANSFER only." },
        "amount": { "type": "number", "exclusiveMinimum": 0, "description": "Positive amount in accountId's currency." },
        "toAmount": { "type": "number", "exclusiveMinimum": 0, "description": "Optional amount received, in the destination's currency, for a transfer between currencies. Omit to convert at the scenario's projected rate." },
        "note": { "type": "string" }
    });
    let mut required = vec!["kind", "accountId", "amount"];
    if with_date {
        properties["date"] = serde_json::json!({ "type": "string", "description": "YYYY-MM-DD" });
        required.push("date");
    }
    serde_json::json!({ "type": "object", "properties": properties, "required": required })
}

fn draft_input_schema() -> serde_json::Value {
    let mut series = transaction_schema(false);
    series["properties"]["label"] = serde_json::json!({
        "type": "string",
        "description": "Short name for the series, e.g. \"Salary\"."
    });
    series["properties"]["dayOfMonth"] = serde_json::json!({
        "type": "integer", "minimum": 1, "maximum": 31,
        "description": "Day of each month. Past a short month's end it falls on the last day."
    });
    series["properties"]["startDate"] = serde_json::json!({ "type": "string", "description": "First possible date, YYYY-MM-DD, inclusive." });
    series["properties"]["endDate"] = serde_json::json!({ "type": "string", "description": "Last possible date, YYYY-MM-DD, inclusive." });
    series["required"] = serde_json::json!([
        "label",
        "kind",
        "accountId",
        "amount",
        "dayOfMonth",
        "startDate",
        "endDate"
    ]);
    serde_json::json!({
        "type": "object",
        "properties": {
            "scenarioId": { "type": "string", "description": "Scenario id from get_scenarios." },
            "series": {
                "type": "array",
                "items": series,
                "description": "Monthly recurring transactions. Prefer one series over many single rows."
            },
            "transactions": {
                "type": "array",
                "items": transaction_schema(true),
                "description": "One-off transactions."
            }
        },
        "required": ["scenarioId"]
    })
}

/// A monthly series as the model writes it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesArgs {
    pub label: String,
    pub kind: ScenarioTransactionKind,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub amount: f64,
    pub to_amount: Option<f64>,
    pub note: Option<String>,
    pub day_of_month: u32,
    pub start_date: String,
    pub end_date: String,
}

/// Arguments shared by the draft and commit tools.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioTransactionsArgs {
    pub scenario_id: String,
    #[serde(default)]
    pub series: Vec<SeriesArgs>,
    #[serde(default)]
    pub transactions: Vec<ScenarioTransactionInput>,
}

impl ScenarioTransactionsArgs {
    fn into_request(self) -> Result<(String, SaveScenarioTransactionsRequest), AgentToolError> {
        if self.series.is_empty() && self.transactions.is_empty() {
            return Err(AgentToolError::InvalidInput(
                "Provide at least one series or transaction".to_string(),
            ));
        }
        let series = self
            .series
            .into_iter()
            .map(|series| ScenarioSeriesRule {
                label: series.label,
                kind: series.kind,
                account_id: series.account_id,
                to_account_id: series.to_account_id,
                amount: series.amount,
                to_amount: series.to_amount,
                note: series.note,
                day_of_month: series.day_of_month,
                start_date: series.start_date,
                end_date: series.end_date,
                replace_series_id: None,
            })
            .collect();
        Ok((
            self.scenario_id,
            SaveScenarioTransactionsRequest {
                creates: self.transactions,
                updates: Vec::new(),
                delete_ids: Vec::new(),
                series,
            },
        ))
    }
}

/// One series in a draft, summarized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftSeriesSummary {
    pub label: String,
    pub kind: ScenarioTransactionKind,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub amount: f64,
    pub to_amount: Option<f64>,
    pub count: usize,
    pub first_date: String,
    pub last_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftAccount {
    pub id: String,
    pub name: String,
    pub currency: String,
}

/// What the draft tool returns. `request` is exactly what the confirmation
/// card (or `commit_scenario_transactions`) saves.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftScenarioTransactionsOutput {
    pub scenario_id: String,
    pub scenario_name: String,
    pub anchor_date: String,
    pub end_date: String,
    pub series: Vec<DraftSeriesSummary>,
    pub transactions: Vec<ScenarioTransactionInput>,
    pub total_rows: usize,
    pub accounts: Vec<DraftAccount>,
    pub request: SaveScenarioTransactionsRequest,
}

fn summarize(
    rows: &[NewScenarioTransaction],
) -> (Vec<DraftSeriesSummary>, Vec<ScenarioTransactionInput>) {
    let mut series: BTreeMap<String, DraftSeriesSummary> = BTreeMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut singles = Vec::new();
    for row in rows {
        match &row.series_id {
            Some(series_id) => {
                let entry = series.entry(series_id.clone()).or_insert_with(|| {
                    order.push(series_id.clone());
                    DraftSeriesSummary {
                        label: row.series_label.clone().unwrap_or_default(),
                        kind: row.input.kind,
                        account_id: row.input.account_id.clone(),
                        to_account_id: row.input.to_account_id.clone(),
                        amount: row.input.amount,
                        to_amount: row.input.to_amount,
                        count: 0,
                        first_date: row.input.date.clone(),
                        last_date: row.input.date.clone(),
                    }
                });
                entry.count += 1;
                if row.input.date < entry.first_date {
                    entry.first_date = row.input.date.clone();
                }
                if row.input.date > entry.last_date {
                    entry.last_date = row.input.date.clone();
                }
            }
            None => singles.push(row.input.clone()),
        }
    }
    let summaries = order
        .into_iter()
        .filter_map(|series_id| series.remove(&series_id))
        .collect();
    (summaries, singles)
}

fn check_row_limit(rows: usize) -> Result<(), AgentToolError> {
    if rows > MAX_ROWS_PER_CALL {
        return Err(AgentToolError::InvalidInput(format!(
            "One call can create at most {MAX_ROWS_PER_CALL} transactions"
        )));
    }
    Ok(())
}

fn tool_error(error: wealthfolio_core::Error) -> AgentToolError {
    match error {
        wealthfolio_core::Error::Validation(validation) => {
            AgentToolError::InvalidInput(validation.to_string())
        }
        other => AgentToolError::ExecutionFailed(other.to_string()),
    }
}

// ---------------------------------------------------------------------------
// get_scenarios
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub anchor_date: String,
    pub end_date: String,
    pub assumptions: ScenarioAssumptions,
    pub transaction_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetScenariosOutput {
    pub scenarios: Vec<ScenarioDto>,
    pub count: usize,
}

/// Lists saved scenarios with their assumptions.
pub struct GetScenarios;

#[async_trait::async_trait]
impl AgentTool for GetScenarios {
    fn name(&self) -> &'static str {
        "get_scenarios"
    }

    fn description(&self) -> &'static str {
        "List saved scenarios (projections). Each has an anchor date where it starts from real balances, an end date, growth assumptions per account (flat, or an effective annual rate with interest and withdrawal taxes), yearly currency drift, and a count of planned transactions."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({ "type": "object", "properties": {}, "required": [] })
    }

    fn required_scopes(&self) -> &'static [AgentScope] {
        &[AgentScope::FinancialPlanningRead]
    }

    fn access_level(&self) -> AgentToolAccess {
        AgentToolAccess::Read
    }

    async fn call(
        &self,
        env: Arc<dyn AgentEnvironment>,
        _args: serde_json::Value,
    ) -> Result<AgentToolResult, AgentToolError> {
        let service = scenario_service(&env)?;
        let mut scenarios = Vec::new();
        for scenario in service.get_scenarios().map_err(tool_error)? {
            let transaction_count = service
                .get_scenario_transactions(&scenario.id)
                .map_err(tool_error)?
                .len();
            scenarios.push(ScenarioDto {
                id: scenario.id,
                name: scenario.name,
                description: scenario.description,
                anchor_date: scenario.anchor_date,
                end_date: scenario.end_date,
                assumptions: scenario.assumptions,
                transaction_count,
            });
        }
        let output = GetScenariosOutput {
            count: scenarios.len(),
            scenarios,
        };
        Ok(AgentToolResult {
            content: serde_json::to_value(output)?,
        })
    }
}

// ---------------------------------------------------------------------------
// draft_scenario_transactions
// ---------------------------------------------------------------------------

/// Validates and expands planned transactions for a scenario without saving.
pub struct DraftScenarioTransactions;

#[async_trait::async_trait]
impl AgentTool for DraftScenarioTransactions {
    fn name(&self) -> &'static str {
        "draft_scenario_transactions"
    }

    fn description(&self) -> &'static str {
        "Draft planned (hypothetical) transactions for a saved scenario from the user's instructions, e.g. a monthly salary, a monthly transfer to a brokerage, or a one-off purchase. Use monthly series for anything recurring. Nothing is saved: the user reviews the draft and adds it to the scenario. These never become real activities. Interest and taxes are not transactions; they come from each account's growth model in the scenario."
    }

    fn input_schema(&self) -> serde_json::Value {
        draft_input_schema()
    }

    fn required_scopes(&self) -> &'static [AgentScope] {
        &[AgentScope::FinancialPlanningRead, AgentScope::AccountsRead]
    }

    fn access_level(&self) -> AgentToolAccess {
        AgentToolAccess::Draft
    }

    async fn call(
        &self,
        env: Arc<dyn AgentEnvironment>,
        args: serde_json::Value,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: ScenarioTransactionsArgs = serde_json::from_value(args)?;
        let (scenario_id, request) = args.into_request()?;
        let service = scenario_service(&env)?;
        let scenario = service.get_scenario(&scenario_id).map_err(tool_error)?;
        let rows = service
            .preview_scenario_transactions(&scenario_id, &request)
            .map_err(tool_error)?;
        check_row_limit(rows.len())?;
        let (series, transactions) = summarize(&rows);

        let account_ids: Vec<String> = {
            let mut ids: Vec<String> = rows
                .iter()
                .flat_map(|row| {
                    std::iter::once(row.input.account_id.clone())
                        .chain(row.input.to_account_id.clone())
                })
                .collect();
            ids.sort();
            ids.dedup();
            ids
        };
        let accounts = env
            .account_service()
            .get_accounts_by_ids(&account_ids)
            .map_err(tool_error)?
            .into_iter()
            .map(|account| DraftAccount {
                id: account.id,
                name: account.name,
                currency: account.currency,
            })
            .collect();

        let output = DraftScenarioTransactionsOutput {
            scenario_id: scenario.id,
            scenario_name: scenario.name,
            anchor_date: scenario.anchor_date,
            end_date: scenario.end_date,
            series,
            transactions,
            total_rows: rows.len(),
            accounts,
            request,
        };
        Ok(AgentToolResult {
            content: serde_json::to_value(output)?,
        })
    }
}

// ---------------------------------------------------------------------------
// commit_scenario_transactions (MCP only)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitScenarioTransactionsOutput {
    pub scenario_id: String,
    pub created_count: usize,
    pub transaction_count: usize,
}

/// Saves planned transactions to a scenario. Reached only through MCP.
pub struct CommitScenarioTransactions;

#[async_trait::async_trait]
impl AgentTool for CommitScenarioTransactions {
    fn name(&self) -> &'static str {
        "commit_scenario_transactions"
    }

    fn description(&self) -> &'static str {
        "Add planned (hypothetical) transactions to a saved scenario. Takes the same arguments as draft_scenario_transactions; only call it after the user has reviewed that draft. Adds rows only and never touches real activities."
    }

    fn input_schema(&self) -> serde_json::Value {
        draft_input_schema()
    }

    fn required_scopes(&self) -> &'static [AgentScope] {
        &[
            AgentScope::FinancialPlanningRead,
            AgentScope::ScenariosWrite,
        ]
    }

    fn access_level(&self) -> AgentToolAccess {
        AgentToolAccess::Write
    }

    fn is_additive(&self) -> bool {
        true
    }

    async fn call(
        &self,
        env: Arc<dyn AgentEnvironment>,
        args: serde_json::Value,
    ) -> Result<AgentToolResult, AgentToolError> {
        let args: ScenarioTransactionsArgs = serde_json::from_value(args)?;
        let (scenario_id, request) = args.into_request()?;
        let service = scenario_service(&env)?;
        let created_count = service
            .preview_scenario_transactions(&scenario_id, &request)
            .map_err(tool_error)?
            .len();
        check_row_limit(created_count)?;
        let rows = service
            .save_scenario_transactions(&scenario_id, request)
            .await
            .map_err(tool_error)?;
        let output = CommitScenarioTransactionsOutput {
            scenario_id,
            created_count,
            transaction_count: rows.len(),
        };
        Ok(AgentToolResult {
            content: serde_json::to_value(output)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(date: &str, series: Option<&str>) -> NewScenarioTransaction {
        NewScenarioTransaction {
            input: ScenarioTransactionInput {
                date: date.to_string(),
                kind: ScenarioTransactionKind::Deposit,
                account_id: "a".to_string(),
                to_account_id: None,
                amount: 10.0,
                to_amount: None,
                note: None,
            },
            series_id: series.map(str::to_string),
            series_label: series.map(|_| "Salary".to_string()),
        }
    }

    #[test]
    fn drafts_summarize_series_in_order_and_keep_singles() {
        let rows = vec![
            row("2027-01-25", Some("s1")),
            row("2027-03-01", None),
            row("2027-02-25", Some("s1")),
        ];
        let (series, singles) = summarize(&rows);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].count, 2);
        assert_eq!(series[0].first_date, "2027-01-25");
        assert_eq!(series[0].last_date, "2027-02-25");
        assert_eq!(singles.len(), 1);
    }

    #[test]
    fn an_empty_request_is_rejected() {
        let args: ScenarioTransactionsArgs =
            serde_json::from_value(serde_json::json!({ "scenarioId": "s" })).expect("args");
        assert!(args.into_request().is_err());
    }

    #[test]
    fn series_args_become_rules_without_replacement() {
        let args: ScenarioTransactionsArgs = serde_json::from_value(serde_json::json!({
            "scenarioId": "s",
            "series": [{
                "label": "Salary", "kind": "DEPOSIT", "accountId": "a", "amount": 44000,
                "dayOfMonth": 25, "startDate": "2027-01-01", "endDate": "2031-12-31"
            }]
        }))
        .expect("args");
        let (scenario_id, request) = args.into_request().expect("request");
        assert_eq!(scenario_id, "s");
        assert_eq!(request.series.len(), 1);
        assert_eq!(request.series[0].replace_series_id, None);
        assert!(request.creates.is_empty());
    }
}
