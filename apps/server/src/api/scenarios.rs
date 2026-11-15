use std::sync::Arc;

use axum::{
    extract::Path,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use wealthfolio_core::portfolios::AccountScope;
use wealthfolio_core::scenarios::{
    AccountGrowthSuggestion, SaveScenarioTransactionsRequest, Scenario, ScenarioInput,
    ScenarioProjection, ScenarioTransaction,
};

use crate::{error::ApiResult, main_lib::AppState};

fn parse_optional_date(value: Option<String>, field: &str) -> ApiResult<Option<chrono::NaiveDate>> {
    Ok(value
        .map(|date| {
            chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
                .map_err(|e| anyhow::anyhow!("Invalid {field}: {e}"))
        })
        .transpose()?)
}

async fn get_scenarios(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<Scenario>>> {
    Ok(Json(state.scenario_service.get_scenarios()?))
}

async fn get_scenario(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Scenario>> {
    Ok(Json(state.scenario_service.get_scenario(&id)?))
}

async fn create_scenario(
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(input): Json<ScenarioInput>,
) -> ApiResult<Json<Scenario>> {
    Ok(Json(state.scenario_service.create_scenario(input).await?))
}

async fn update_scenario(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(input): Json<ScenarioInput>,
) -> ApiResult<Json<Scenario>> {
    Ok(Json(
        state.scenario_service.update_scenario(&id, input).await?,
    ))
}

async fn delete_scenario(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<StatusCode> {
    state.scenario_service.delete_scenario(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct DuplicateScenarioBody {
    name: String,
}

async fn duplicate_scenario(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<DuplicateScenarioBody>,
) -> ApiResult<Json<Scenario>> {
    Ok(Json(
        state
            .scenario_service
            .duplicate_scenario(&id, body.name)
            .await?,
    ))
}

async fn get_scenario_transactions(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Vec<ScenarioTransaction>>> {
    Ok(Json(state.scenario_service.get_scenario_transactions(&id)?))
}

async fn save_scenario_transactions(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(request): Json<SaveScenarioTransactionsRequest>,
) -> ApiResult<Json<Vec<ScenarioTransaction>>> {
    Ok(Json(
        state
            .scenario_service
            .save_scenario_transactions(&id, request)
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionBody {
    filter: Option<AccountScope>,
    start_date: Option<String>,
    end_date: Option<String>,
}

async fn get_scenario_projection(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<ProjectionBody>,
) -> ApiResult<Json<ScenarioProjection>> {
    let start = parse_optional_date(body.start_date, "startDate")?;
    let end = parse_optional_date(body.end_date, "endDate")?;
    let scope = body.filter.unwrap_or(AccountScope::All);
    let base = state
        .base_currency
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    Ok(Json(
        state
            .scenario_service
            .get_scenario_projection(&id, &scope, &base, start, end)
            .await?,
    ))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NetWorthProjectionBody {
    start_date: Option<String>,
    end_date: Option<String>,
}

async fn get_scenario_net_worth_projection(
    Path(id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
    Json(body): Json<NetWorthProjectionBody>,
) -> ApiResult<Json<ScenarioProjection>> {
    let start = parse_optional_date(body.start_date, "startDate")?;
    let end = parse_optional_date(body.end_date, "endDate")?;
    let base = state
        .base_currency
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    Ok(Json(
        state
            .scenario_service
            .get_net_worth_projection(&id, &base, start, end)
            .await?,
    ))
}

async fn suggest_scenario_account_growth(
    Path(account_id): Path<String>,
    axum::Extension(state): axum::Extension<Arc<AppState>>,
) -> ApiResult<Json<Option<AccountGrowthSuggestion>>> {
    Ok(Json(
        state.scenario_service.suggest_account_growth(&account_id)?,
    ))
}

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/scenarios", get(get_scenarios).post(create_scenario))
        .route(
            "/scenarios/{id}",
            get(get_scenario)
                .put(update_scenario)
                .delete(delete_scenario),
        )
        .route("/scenarios/{id}/duplicate", post(duplicate_scenario))
        .route(
            "/scenarios/{id}/transactions",
            get(get_scenario_transactions).post(save_scenario_transactions),
        )
        .route("/scenarios/{id}/projection", post(get_scenario_projection))
        .route(
            "/scenarios/growth-suggestions/{account_id}",
            get(suggest_scenario_account_growth),
        )
        .route(
            "/scenarios/{id}/net-worth-projection",
            post(get_scenario_net_worth_projection),
        )
}
