//! Scenarios: user-defined projections built from per-account growth models,
//! FX assumptions and dated hypothetical transactions. They never touch real
//! activities or the portfolio engine. Design: `docs/features/scenarios/design.md`.

pub mod engine;
mod scenarios_model;
mod scenarios_service;
mod scenarios_traits;
pub mod series;

pub use scenarios_model::{
    AccountGrowthModel, NewScenarioTransaction, SaveScenarioTransactionsRequest, Scenario,
    ScenarioAssumptions, ScenarioInput, ScenarioProjection, ScenarioProjectionPoint,
    ScenarioSeriesRule, ScenarioTransaction, ScenarioTransactionInput, ScenarioTransactionKind,
    ScenarioTransactionUpdate, ScenarioWarning, ScenarioWarningCode,
};
pub use scenarios_service::ScenarioService;
pub use scenarios_traits::{ScenarioRepositoryTrait, ScenarioServiceTrait};
