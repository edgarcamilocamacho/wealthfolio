use log::debug;
use wealthfolio_core::portfolios::AccountScope;
use wealthfolio_core::scenarios::{
    AccountGrowthSuggestion, SaveScenarioTransactionsRequest, Scenario, ScenarioInput,
    ScenarioProjection, ScenarioTransaction,
};

use super::portfolio::AccountScopeInput;
use crate::profiles::ProfileAccess;

fn parse_optional_date(
    value: Option<String>,
    field: &str,
) -> Result<Option<chrono::NaiveDate>, String> {
    value
        .map(|date| {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .map_err(|e| format!("Invalid {field}: {e}"))
        })
        .transpose()
}

#[tauri::command]
pub async fn get_scenarios(state: ProfileAccess) -> Result<Vec<Scenario>, String> {
    let context = state.context()?;
    debug!("Fetching scenarios...");
    context
        .scenario_service()
        .get_scenarios()
        .map_err(|e| format!("Failed to load scenarios: {e}"))
}

#[tauri::command]
pub async fn get_scenario(id: String, state: ProfileAccess) -> Result<Scenario, String> {
    let context = state.context()?;
    context
        .scenario_service()
        .get_scenario(&id)
        .map_err(|e| format!("Failed to load scenario: {e}"))
}

#[tauri::command]
pub async fn create_scenario(
    scenario: ScenarioInput,
    state: ProfileAccess,
) -> Result<Scenario, String> {
    let context = state.context()?;
    debug!("Creating scenario...");
    context
        .scenario_service()
        .create_scenario(scenario)
        .await
        .map_err(|e| format!("Failed to create scenario: {e}"))
}

#[tauri::command]
pub async fn update_scenario(
    id: String,
    scenario: ScenarioInput,
    state: ProfileAccess,
) -> Result<Scenario, String> {
    let context = state.context()?;
    debug!("Updating scenario...");
    context
        .scenario_service()
        .update_scenario(&id, scenario)
        .await
        .map_err(|e| format!("Failed to update scenario: {e}"))
}

#[tauri::command]
pub async fn delete_scenario(id: String, state: ProfileAccess) -> Result<(), String> {
    let context = state.context()?;
    debug!("Deleting scenario...");
    context
        .scenario_service()
        .delete_scenario(&id)
        .await
        .map_err(|e| format!("Failed to delete scenario: {e}"))
}

#[tauri::command]
pub async fn duplicate_scenario(
    id: String,
    name: String,
    state: ProfileAccess,
) -> Result<Scenario, String> {
    let context = state.context()?;
    context
        .scenario_service()
        .duplicate_scenario(&id, name)
        .await
        .map_err(|e| format!("Failed to duplicate scenario: {e}"))
}

#[tauri::command]
pub async fn get_scenario_transactions(
    scenario_id: String,
    state: ProfileAccess,
) -> Result<Vec<ScenarioTransaction>, String> {
    let context = state.context()?;
    context
        .scenario_service()
        .get_scenario_transactions(&scenario_id)
        .map_err(|e| format!("Failed to load scenario transactions: {e}"))
}

#[tauri::command]
pub async fn save_scenario_transactions(
    scenario_id: String,
    request: SaveScenarioTransactionsRequest,
    state: ProfileAccess,
) -> Result<Vec<ScenarioTransaction>, String> {
    let context = state.context()?;
    debug!("Saving scenario transactions...");
    context
        .scenario_service()
        .save_scenario_transactions(&scenario_id, request)
        .await
        .map_err(|e| format!("Failed to save scenario transactions: {e}"))
}

#[tauri::command]
pub async fn get_scenario_projection(
    scenario_id: String,
    filter: Option<AccountScopeInput>,
    start_date: Option<String>,
    end_date: Option<String>,
    state: ProfileAccess,
) -> Result<ScenarioProjection, String> {
    let context = state.context()?;
    let scope = match filter {
        Some(input) => input.into_account_filter()?,
        None => AccountScope::All,
    };
    let start = parse_optional_date(start_date, "start date")?;
    let end = parse_optional_date(end_date, "end date")?;
    let base_currency = context.get_base_currency();
    context
        .scenario_service()
        .get_scenario_projection(&scenario_id, &scope, &base_currency, start, end)
        .await
        .map_err(|e| format!("Failed to project scenario: {e}"))
}

#[tauri::command]
pub async fn get_scenario_net_worth_projection(
    scenario_id: String,
    start_date: Option<String>,
    end_date: Option<String>,
    state: ProfileAccess,
) -> Result<ScenarioProjection, String> {
    let context = state.context()?;
    let start = parse_optional_date(start_date, "start date")?;
    let end = parse_optional_date(end_date, "end date")?;
    let base_currency = context.get_base_currency();
    context
        .scenario_service()
        .get_net_worth_projection(&scenario_id, &base_currency, start, end)
        .await
        .map_err(|e| format!("Failed to project net worth: {e}"))
}

#[tauri::command]
pub async fn suggest_scenario_account_growth(
    account_id: String,
    state: ProfileAccess,
) -> Result<Option<AccountGrowthSuggestion>, String> {
    let context = state.context()?;
    context
        .scenario_service()
        .suggest_account_growth(&account_id)
        .map_err(|e| format!("Failed to suggest a growth rate: {e}"))
}
