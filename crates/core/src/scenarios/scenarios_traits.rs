use async_trait::async_trait;
use chrono::NaiveDate;

use super::scenarios_model::{
    NewScenarioTransaction, SaveScenarioTransactionsRequest, Scenario, ScenarioInput,
    ScenarioProjection, ScenarioTransaction, ScenarioTransactionUpdate,
};
use crate::errors::Result;
use crate::portfolios::AccountScope;

/// Storage contract for scenarios and their hypothetical transactions.
#[async_trait]
pub trait ScenarioRepositoryTrait: Send + Sync {
    fn list_scenarios(&self) -> Result<Vec<Scenario>>;
    fn get_scenario(&self, id: &str) -> Result<Scenario>;
    async fn create_scenario(&self, input: ScenarioInput) -> Result<Scenario>;
    async fn update_scenario(&self, id: &str, input: ScenarioInput) -> Result<Scenario>;
    /// Deletes the scenario and its transactions.
    async fn delete_scenario(&self, id: &str) -> Result<()>;
    /// Copies a scenario and all of its transactions under a new name.
    async fn duplicate_scenario(&self, id: &str, name: String) -> Result<Scenario>;

    fn list_transactions(&self, scenario_id: &str) -> Result<Vec<ScenarioTransaction>>;
    /// Applies one batch in a single transaction: deletes rows by id and by
    /// series, updates rows, then inserts new ones. Ids that do not belong to
    /// the scenario fail the whole batch.
    async fn apply_transaction_changes(
        &self,
        scenario_id: &str,
        creates: Vec<NewScenarioTransaction>,
        updates: Vec<ScenarioTransactionUpdate>,
        delete_ids: Vec<String>,
        delete_series_ids: Vec<String>,
    ) -> Result<()>;
}

/// Scenario operations exposed to the Tauri and web runtimes.
#[async_trait]
pub trait ScenarioServiceTrait: Send + Sync {
    fn get_scenarios(&self) -> Result<Vec<Scenario>>;
    fn get_scenario(&self, id: &str) -> Result<Scenario>;
    async fn create_scenario(&self, input: ScenarioInput) -> Result<Scenario>;
    async fn update_scenario(&self, id: &str, input: ScenarioInput) -> Result<Scenario>;
    async fn delete_scenario(&self, id: &str) -> Result<()>;
    async fn duplicate_scenario(&self, id: &str, name: String) -> Result<Scenario>;

    fn get_scenario_transactions(&self, scenario_id: &str) -> Result<Vec<ScenarioTransaction>>;
    /// Validates a batch and expands its recurrence rules without saving it.
    /// Returns the rows the batch would create.
    fn preview_scenario_transactions(
        &self,
        scenario_id: &str,
        request: &SaveScenarioTransactionsRequest,
    ) -> Result<Vec<NewScenarioTransaction>>;

    /// Validates and applies a batch, expanding recurrence rules into rows.
    /// Returns the scenario's transactions after the change.
    async fn save_scenario_transactions(
        &self,
        scenario_id: &str,
        request: SaveScenarioTransactionsRequest,
    ) -> Result<Vec<ScenarioTransaction>>;

    /// Projects the accounts of `scope` from the scenario's anchor, optionally
    /// trimmed to `[start_date, end_date]`.
    async fn get_scenario_projection(
        &self,
        scenario_id: &str,
        scope: &AccountScope,
        base_currency: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<ScenarioProjection>;
}
