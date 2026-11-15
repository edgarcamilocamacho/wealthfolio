//! Scenario service: validation, series expansion and projection assembly.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use chrono::{Days, NaiveDate};
use rust_decimal::prelude::ToPrimitive;
use uuid::Uuid;

use super::engine::{project, EngineAccount, EngineInput, EngineTransaction};
use super::scenarios_model::{
    AccountGrowthModel, AccountGrowthSuggestion, NewScenarioTransaction,
    SaveScenarioTransactionsRequest, Scenario, ScenarioAssumptions, ScenarioInput,
    ScenarioProjection, ScenarioProjectionPoint, ScenarioTransaction, ScenarioTransactionInput,
    ScenarioTransactionKind, ScenarioTransactionUpdate, ScenarioWarning, ScenarioWarningCode,
};
use super::scenarios_traits::{ScenarioRepositoryTrait, ScenarioServiceTrait};
use super::series::{expand_series, parse_date};
use super::suggest::{infer_rate, InterestEvent, SuggestionInputs};
use crate::accounts::{AccountPurpose, AccountServiceTrait};
use crate::activities::{
    ActivityRepositoryTrait, ActivityStatus, ACTIVITY_TYPE_INTEREST, ACTIVITY_TYPE_TAX,
};
use crate::assets::loan::{
    calculate_loan, LoanBalance, LoanCalculationRequest, LOAN_PROJECTION_KEY,
};
use crate::assets::{Asset, AssetKind, AssetServiceTrait};
use crate::errors::{Error, Result, ValidationError};
use crate::fx::currency::normalize_amount;
use crate::fx::FxServiceTrait;
use crate::portfolio::valuation::{ValuationServiceTrait, ValuationStatus};
use crate::portfolios::{AccountScope, PortfolioServiceTrait};
use crate::quotes::QuoteServiceTrait;
use crate::utils::time_utils::{parse_user_timezone_or_default, user_today};

const MAX_NAME_CHARS: usize = 100;
const MAX_DESCRIPTION_CHARS: usize = 1000;
const MAX_NOTE_CHARS: usize = 500;
const MAX_HORIZON_YEARS: u32 = 50;
const MAX_AMOUNT: f64 = 1_000_000_000_000.0;
const MIN_RATE: f64 = -0.99;
const MAX_RATE: f64 = 1.0;
/// Most rows one save may create, series included.
const MAX_CREATES_PER_SAVE: usize = 5000;
/// How far before the anchor to look for an account's last valuation.
const ANCHOR_LOOKBACK_DAYS: u64 = 366;

fn invalid(message: impl Into<String>) -> Error {
    Error::Validation(ValidationError::InvalidInput(message.into()))
}

fn format_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn check_rate(value: f64, field: &str) -> Result<()> {
    if !value.is_finite() || !(MIN_RATE..=MAX_RATE).contains(&value) {
        return Err(invalid(format!(
            "{field} must be between {MIN_RATE} and {MAX_RATE}"
        )));
    }
    Ok(())
}

fn check_share(value: f64, field: &str) -> Result<()> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(invalid(format!("{field} must be between 0 and 1")));
    }
    Ok(())
}

/// Normalizes and validates a scenario before it is stored.
pub(crate) fn validate_scenario_input(
    mut input: ScenarioInput,
    today: NaiveDate,
) -> Result<ScenarioInput> {
    input.name = input.name.trim().to_string();
    if input.name.is_empty() {
        return Err(invalid("A scenario needs a name"));
    }
    if input.name.chars().count() > MAX_NAME_CHARS {
        return Err(invalid(format!(
            "A scenario name can have at most {MAX_NAME_CHARS} characters"
        )));
    }
    input.description = input
        .description
        .map(|description| description.trim().to_string())
        .filter(|description| !description.is_empty());
    if input
        .description
        .as_ref()
        .is_some_and(|description| description.chars().count() > MAX_DESCRIPTION_CHARS)
    {
        return Err(invalid(format!(
            "A description can have at most {MAX_DESCRIPTION_CHARS} characters"
        )));
    }

    let anchor = parse_date(&input.anchor_date, "The anchor date")?;
    let end = parse_date(&input.end_date, "The end date")?;
    if anchor > today {
        return Err(invalid("The anchor date cannot be after today"));
    }
    if end <= anchor {
        return Err(invalid("The end date must be after the anchor date"));
    }
    let latest_end = anchor
        .checked_add_months(chrono::Months::new(MAX_HORIZON_YEARS * 12))
        .ok_or_else(|| invalid("The end date is out of range"))?;
    if end > latest_end {
        return Err(invalid(format!(
            "A scenario can run at most {MAX_HORIZON_YEARS} years"
        )));
    }
    input.anchor_date = format_date(anchor);
    input.end_date = format_date(end);

    validate_assumptions(&input.assumptions)?;
    Ok(input)
}

fn validate_assumptions(assumptions: &ScenarioAssumptions) -> Result<()> {
    for (account_id, model) in &assumptions.accounts {
        if account_id.trim().is_empty() {
            return Err(invalid("A growth model needs an account"));
        }
        if let AccountGrowthModel::Rate {
            annual_rate,
            interest_tax_rate,
            outflow_tax_rate,
            growth_from,
        } = model
        {
            check_rate(*annual_rate, "The annual rate")?;
            check_share(*interest_tax_rate, "The interest tax")?;
            check_share(*outflow_tax_rate, "The withdrawal tax")?;
            if let Some(date) = growth_from {
                parse_date(date, "The growth start date")?;
            }
        }
    }
    for (currency, drift) in &assumptions.fx {
        if currency.trim().is_empty() || currency.len() > 10 {
            return Err(invalid("A currency drift needs a currency code"));
        }
        check_rate(*drift, "The currency drift")?;
    }
    Ok(())
}

/// Normalizes and validates one hypothetical transaction.
pub(crate) fn validate_transaction_input(
    mut input: ScenarioTransactionInput,
) -> Result<ScenarioTransactionInput> {
    input.date = format_date(parse_date(&input.date, "The transaction date")?);
    input.account_id = input.account_id.trim().to_string();
    if input.account_id.is_empty() {
        return Err(invalid("A transaction needs an account"));
    }
    if !input.amount.is_finite() || input.amount <= 0.0 || input.amount > MAX_AMOUNT {
        return Err(invalid("The amount must be a positive number"));
    }
    input.to_account_id = input
        .to_account_id
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty());
    match input.kind {
        ScenarioTransactionKind::Transfer => {
            match input.to_account_id.as_deref() {
                None => return Err(invalid("A transfer needs a destination account")),
                Some(to) if to == input.account_id => {
                    return Err(invalid("A transfer needs two different accounts"))
                }
                Some(_) => {}
            }
            if let Some(to_amount) = input.to_amount {
                if !to_amount.is_finite() || to_amount <= 0.0 || to_amount > MAX_AMOUNT {
                    return Err(invalid("The amount received must be a positive number"));
                }
            }
        }
        ScenarioTransactionKind::Deposit | ScenarioTransactionKind::Withdrawal => {
            if input.to_account_id.is_some() || input.to_amount.is_some() {
                return Err(invalid(
                    "Only a transfer can have a destination account or amount received",
                ));
            }
        }
    }
    input.note = input
        .note
        .map(|note| note.trim().to_string())
        .filter(|note| !note.is_empty());
    if input
        .note
        .as_ref()
        .is_some_and(|note| note.chars().count() > MAX_NOTE_CHARS)
    {
        return Err(invalid(format!(
            "A note can have at most {MAX_NOTE_CHARS} characters"
        )));
    }
    Ok(input)
}

/// One account's real balance on the anchor date.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AnchorValue {
    pub value: f64,
    pub fx_rate_to_base: Option<f64>,
}

/// Everything the projection needs, already loaded.
pub(crate) struct ProjectionInputs<'a> {
    pub scenario: &'a Scenario,
    pub base_currency: &'a str,
    /// Accounts in the requested scope, in order.
    pub scope_account_ids: &'a [String],
    /// True for a single-account scope: values stay in the account's currency.
    pub single_account: bool,
    /// Every known account the projection touches, with its currency.
    pub account_currencies: &'a BTreeMap<String, String>,
    /// Account ids that transactions name but that do not exist.
    pub unknown_account_ids: &'a BTreeSet<String>,
    pub anchor_values: &'a HashMap<String, AnchorValue>,
    /// Scheduled balances per position (loan amortization), in its currency.
    pub schedules: &'a HashMap<String, Vec<(NaiveDate, f64)>>,
    /// Units of base currency per unit of each currency on the anchor date.
    pub fx_at_anchor: &'a HashMap<String, f64>,
    pub transactions: &'a [ScenarioTransaction],
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
}

/// Runs the engine and sums the scope. Pure, so it is tested without storage.
pub(crate) fn assemble_projection(inputs: &ProjectionInputs<'_>) -> Result<ScenarioProjection> {
    let scenario = inputs.scenario;
    let anchor = parse_date(&scenario.anchor_date, "The anchor date")?;
    let end = parse_date(&scenario.end_date, "The end date")?;
    let in_scope: BTreeSet<&str> = inputs
        .scope_account_ids
        .iter()
        .map(String::as_str)
        .collect();
    let mut warnings = Vec::new();

    for account_id in inputs.unknown_account_ids {
        warnings.push(ScenarioWarning {
            code: ScenarioWarningCode::UnknownAccount,
            account_id: Some(account_id.clone()),
            currency: None,
            date: None,
        });
    }

    let accounts: Vec<EngineAccount> = inputs
        .account_currencies
        .iter()
        .map(|(id, currency)| {
            let start_value = match inputs.anchor_values.get(id) {
                Some(anchor_value) => anchor_value.value,
                None => {
                    if in_scope.contains(id.as_str()) {
                        warnings.push(ScenarioWarning {
                            code: ScenarioWarningCode::NoAnchorValuation,
                            account_id: Some(id.clone()),
                            currency: None,
                            date: None,
                        });
                    }
                    0.0
                }
            };
            EngineAccount {
                id: id.clone(),
                currency: currency.clone(),
                start_value,
                model: scenario
                    .assumptions
                    .accounts
                    .get(id)
                    .cloned()
                    .unwrap_or_default(),
                schedule: inputs.schedules.get(id).cloned().unwrap_or_default(),
            }
        })
        .collect();

    let mut missing_currencies: BTreeSet<&str> = BTreeSet::new();
    for account in &accounts {
        if in_scope.contains(account.id.as_str())
            && !inputs.fx_at_anchor.contains_key(&account.currency)
        {
            missing_currencies.insert(account.currency.as_str());
        }
    }
    for currency in &missing_currencies {
        warnings.push(ScenarioWarning {
            code: ScenarioWarningCode::MissingFxRate,
            account_id: None,
            currency: Some((*currency).to_string()),
            date: None,
        });
    }

    let transactions: Vec<EngineTransaction> = inputs
        .transactions
        .iter()
        .filter_map(|tx| {
            Some(EngineTransaction {
                date: NaiveDate::parse_from_str(&tx.date, "%Y-%m-%d").ok()?,
                kind: tx.kind,
                account_id: tx.account_id.clone(),
                to_account_id: tx.to_account_id.clone(),
                amount: tx.amount,
                to_amount: tx.to_amount,
            })
        })
        .collect();

    let output = project(&EngineInput {
        anchor,
        end,
        accounts: &accounts,
        fx_at_anchor: inputs.fx_at_anchor,
        fx_drift: &scenario.assumptions.fx,
        transactions: &transactions,
    });

    // A position that already starts below zero (a liability, an overdrawn
    // card) is not news when it stays there.
    let starts_below_zero: BTreeSet<&str> = accounts
        .iter()
        .filter(|account| account.start_value < 0.0)
        .map(|account| account.id.as_str())
        .collect();
    for (account_id, date) in &output.first_negative {
        if in_scope.contains(account_id.as_str())
            && !starts_below_zero.contains(account_id.as_str())
        {
            warnings.push(ScenarioWarning {
                code: ScenarioWarningCode::NegativeBalance,
                account_id: Some(account_id.clone()),
                currency: None,
                date: Some(format_date(*date)),
            });
        }
    }

    let scoped: Vec<&EngineAccount> = accounts
        .iter()
        .filter(|account| in_scope.contains(account.id.as_str()))
        .collect();
    let currency = match (inputs.single_account, scoped.first()) {
        (true, Some(account)) => account.currency.clone(),
        _ => inputs.base_currency.to_string(),
    };

    let mut points = Vec::new();
    for (index, date) in output.dates.iter().enumerate() {
        if inputs.start_date.is_some_and(|start| *date < start)
            || inputs.end_date.is_some_and(|last| *date > last)
        {
            continue;
        }
        let mut value_base = 0.0;
        let mut native = 0.0;
        for account in &scoped {
            let balance = output.balances[&account.id][index];
            native += balance;
            if let Some(rate) = output.fx_rate(&account.currency, index) {
                value_base += balance * rate;
            }
        }
        points.push(ScenarioProjectionPoint {
            date: format_date(*date),
            value: if inputs.single_account {
                native
            } else {
                value_base
            },
            value_base,
        });
    }

    Ok(ScenarioProjection {
        scenario_id: scenario.id.clone(),
        base_currency: inputs.base_currency.to_string(),
        currency,
        points,
        warnings,
    })
}

/// A validated batch, ready for the repository.
struct PreparedChanges {
    creates: Vec<NewScenarioTransaction>,
    updates: Vec<ScenarioTransactionUpdate>,
    delete_ids: Vec<String>,
    delete_series_ids: Vec<String>,
}

pub struct ScenarioService {
    repository: Arc<dyn ScenarioRepositoryTrait>,
    account_service: Arc<dyn AccountServiceTrait>,
    portfolio_service: Arc<dyn PortfolioServiceTrait>,
    valuation_service: Arc<dyn ValuationServiceTrait>,
    fx_service: Arc<dyn FxServiceTrait>,
    asset_service: Arc<dyn AssetServiceTrait>,
    quote_service: Arc<dyn QuoteServiceTrait>,
    activity_repository: Arc<dyn ActivityRepositoryTrait>,
    timezone: Arc<RwLock<String>>,
}

/// The services a scenario projection reads from.
pub struct ScenarioServiceSources {
    pub account_service: Arc<dyn AccountServiceTrait>,
    pub portfolio_service: Arc<dyn PortfolioServiceTrait>,
    pub valuation_service: Arc<dyn ValuationServiceTrait>,
    pub fx_service: Arc<dyn FxServiceTrait>,
    pub asset_service: Arc<dyn AssetServiceTrait>,
    pub quote_service: Arc<dyn QuoteServiceTrait>,
    pub activity_repository: Arc<dyn ActivityRepositoryTrait>,
}

impl ScenarioService {
    pub fn new(
        repository: Arc<dyn ScenarioRepositoryTrait>,
        sources: ScenarioServiceSources,
        timezone: Arc<RwLock<String>>,
    ) -> Self {
        Self {
            repository,
            account_service: sources.account_service,
            portfolio_service: sources.portfolio_service,
            valuation_service: sources.valuation_service,
            fx_service: sources.fx_service,
            asset_service: sources.asset_service,
            quote_service: sources.quote_service,
            activity_repository: sources.activity_repository,
            timezone,
        }
    }

    fn today(&self) -> NaiveDate {
        user_today(parse_user_timezone_or_default(
            &self
                .timezone
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        ))
    }

    fn ensure_accounts_exist(&self, account_ids: BTreeSet<String>) -> Result<()> {
        if account_ids.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = account_ids.iter().cloned().collect();
        let mut found: BTreeSet<String> = self
            .account_service
            .get_accounts_by_ids(&ids)?
            .into_iter()
            .map(|account| account.id)
            .collect();
        if found.len() < account_ids.len() {
            // Money can also go into an alternative asset (an instalment on a
            // property), never into a liability, whose balance its loan drives.
            found.extend(
                self.alternative_assets()?
                    .into_iter()
                    .filter(|asset| asset.kind != AssetKind::Liability)
                    .map(|asset| asset.id),
            );
        }
        if let Some(missing) = account_ids.difference(&found).next() {
            return Err(invalid(format!("Account {missing} does not exist")));
        }
        Ok(())
    }

    /// Validates a batch, expands its series and checks that every account
    /// it names exists.
    fn prepare_changes(
        &self,
        scenario_id: &str,
        request: SaveScenarioTransactionsRequest,
    ) -> Result<PreparedChanges> {
        self.repository.get_scenario(scenario_id)?;

        let mut creates: Vec<NewScenarioTransaction> = Vec::new();
        for input in request.creates {
            creates.push(NewScenarioTransaction {
                input: validate_transaction_input(input)?,
                series_id: None,
                series_label: None,
            });
        }
        let mut delete_series_ids = Vec::new();
        for rule in &request.series {
            let series_id = match &rule.replace_series_id {
                Some(id) => {
                    delete_series_ids.push(id.clone());
                    id.clone()
                }
                None => Uuid::new_v4().to_string(),
            };
            for mut row in expand_series(rule, &series_id)? {
                row.input = validate_transaction_input(row.input)?;
                creates.push(row);
            }
        }
        if creates.len() > MAX_CREATES_PER_SAVE {
            return Err(invalid(format!(
                "One save can create at most {MAX_CREATES_PER_SAVE} transactions"
            )));
        }
        let mut updates = Vec::new();
        for mut update in request.updates {
            update.input = validate_transaction_input(update.input)?;
            updates.push(update);
        }

        let referenced: BTreeSet<String> = creates
            .iter()
            .map(|row| &row.input)
            .chain(updates.iter().map(|update| &update.input))
            .flat_map(|input| {
                std::iter::once(input.account_id.clone()).chain(input.to_account_id.clone())
            })
            .collect();
        self.ensure_accounts_exist(referenced)?;

        Ok(PreparedChanges {
            creates,
            updates,
            delete_ids: request.delete_ids,
            delete_series_ids,
        })
    }

    /// The last usable valuation on or before the anchor.
    fn anchor_value(&self, account_id: &str, anchor: NaiveDate) -> Result<Option<AnchorValue>> {
        let from = anchor.checked_sub_days(Days::new(ANCHOR_LOOKBACK_DAYS));
        let rows =
            self.valuation_service
                .get_historical_valuations(account_id, from, Some(anchor))?;
        Ok(rows
            .into_iter()
            .filter(|row| row.valuation_date <= anchor)
            .filter(|row| row.value_status != ValuationStatus::Unavailable)
            .max_by_key(|row| row.valuation_date)
            .and_then(|row| {
                Some(AnchorValue {
                    value: row.total_value.to_f64()?,
                    fx_rate_to_base: row.fx_rate_to_base.to_f64().filter(|rate| *rate > 0.0),
                })
            }))
    }
}

#[async_trait]
impl ScenarioServiceTrait for ScenarioService {
    fn get_scenarios(&self) -> Result<Vec<Scenario>> {
        self.repository.list_scenarios()
    }

    fn get_scenario(&self, id: &str) -> Result<Scenario> {
        self.repository.get_scenario(id)
    }

    async fn create_scenario(&self, input: ScenarioInput) -> Result<Scenario> {
        let input = validate_scenario_input(input, self.today())?;
        self.repository.create_scenario(input).await
    }

    async fn update_scenario(&self, id: &str, input: ScenarioInput) -> Result<Scenario> {
        let input = validate_scenario_input(input, self.today())?;
        self.repository.update_scenario(id, input).await
    }

    async fn delete_scenario(&self, id: &str) -> Result<()> {
        self.repository.delete_scenario(id).await
    }

    async fn duplicate_scenario(&self, id: &str, name: String) -> Result<Scenario> {
        let name = name.trim().to_string();
        if name.is_empty() || name.chars().count() > MAX_NAME_CHARS {
            return Err(invalid(format!(
                "A scenario name needs 1 to {MAX_NAME_CHARS} characters"
            )));
        }
        self.repository.duplicate_scenario(id, name).await
    }

    fn get_scenario_transactions(&self, scenario_id: &str) -> Result<Vec<ScenarioTransaction>> {
        self.repository.list_transactions(scenario_id)
    }

    fn preview_scenario_transactions(
        &self,
        scenario_id: &str,
        request: &SaveScenarioTransactionsRequest,
    ) -> Result<Vec<NewScenarioTransaction>> {
        Ok(self.prepare_changes(scenario_id, request.clone())?.creates)
    }

    async fn save_scenario_transactions(
        &self,
        scenario_id: &str,
        request: SaveScenarioTransactionsRequest,
    ) -> Result<Vec<ScenarioTransaction>> {
        let changes = self.prepare_changes(scenario_id, request)?;
        self.repository
            .apply_transaction_changes(
                scenario_id,
                changes.creates,
                changes.updates,
                changes.delete_ids,
                changes.delete_series_ids,
            )
            .await?;
        self.repository.list_transactions(scenario_id)
    }

    async fn get_scenario_projection(
        &self,
        scenario_id: &str,
        scope: &AccountScope,
        base_currency: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<ScenarioProjection> {
        let scenario = self.repository.get_scenario(scenario_id)?;
        let resolved = self.portfolio_service.resolve_account_scope_for_purpose(
            scope,
            base_currency,
            AccountPurpose::Holdings,
        )?;
        let single_account = matches!(scope, AccountScope::Account { .. });
        self.project_positions(
            &scenario,
            &resolved.account_ids,
            single_account,
            base_currency,
            start_date,
            end_date,
        )
    }

    fn suggest_account_growth(&self, account_id: &str) -> Result<Option<AccountGrowthSuggestion>> {
        let account = self.account_service.get_account(account_id)?;
        let end = self.today();
        let start = end
            .checked_sub_days(Days::new(364))
            .ok_or_else(|| invalid("The suggestion window is out of range"))?;
        let balances: Vec<(NaiveDate, f64)> = self
            .valuation_service
            .get_historical_valuations(account_id, Some(start), Some(end))?
            .into_iter()
            .filter(|row| row.value_status != ValuationStatus::Unavailable)
            .filter_map(|row| Some((row.valuation_date, row.total_value.to_f64()?)))
            .collect();
        let Some(first_day) = balances.first().map(|(date, _)| *date) else {
            return Ok(None);
        };
        let window_start = first_day
            .and_hms_opt(0, 0, 0)
            .map(|time| time.and_utc())
            .ok_or_else(|| invalid("The suggestion window is out of range"))?;
        let window_end = end
            .succ_opt()
            .and_then(|day| day.and_hms_opt(0, 0, 0))
            .map(|time| time.and_utc())
            .ok_or_else(|| invalid("The suggestion window is out of range"))?;
        let activities = self
            .activity_repository
            .get_activities_by_account_ids_in_date_range(
                std::slice::from_ref(&account.id),
                window_start,
                window_end,
            )?;
        let mut interest = Vec::new();
        let mut standalone_tax = 0.0;
        for activity in activities {
            // Amounts in another currency are left out rather than mixed in.
            if activity.status != ActivityStatus::Posted || activity.currency != account.currency {
                continue;
            }
            let amount = activity
                .amount
                .and_then(|value| value.to_f64())
                .unwrap_or(0.0)
                .abs();
            let tax = activity
                .tax
                .and_then(|value| value.to_f64())
                .unwrap_or(0.0)
                .abs();
            match activity.effective_type() {
                ACTIVITY_TYPE_INTEREST => interest.push(InterestEvent {
                    date: activity.activity_date.date_naive(),
                    net_amount: amount,
                    tax,
                }),
                ACTIVITY_TYPE_TAX => standalone_tax += amount,
                _ => {}
            }
        }
        let interest_payments = interest.len();
        Ok(infer_rate(&SuggestionInputs {
            interest: &interest,
            standalone_tax,
            balances: &balances,
        })
        .map(|inferred| AccountGrowthSuggestion {
            account_id: account.id,
            annual_rate: inferred.annual_rate,
            interest_tax_rate: inferred.interest_tax_rate,
            period_start: format_date(first_day),
            period_end: format_date(end),
            interest_payments,
        }))
    }

    async fn get_net_worth_projection(
        &self,
        scenario_id: &str,
        base_currency: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<ScenarioProjection> {
        let scenario = self.repository.get_scenario(scenario_id)?;
        let resolved = self.portfolio_service.resolve_account_scope_for_purpose(
            &AccountScope::All,
            base_currency,
            AccountPurpose::NetWorth,
        )?;
        let mut scope_ids = resolved.account_ids;
        scope_ids.extend(self.alternative_assets()?.into_iter().map(|asset| asset.id));
        self.project_positions(
            &scenario,
            &scope_ids,
            false,
            base_currency,
            start_date,
            end_date,
        )
    }
}

impl ScenarioService {
    fn alternative_assets(&self) -> Result<Vec<Asset>> {
        Ok(self
            .asset_service
            .get_assets()?
            .into_iter()
            .filter(|asset| asset.kind.is_alternative())
            .collect())
    }

    /// An alternative asset's value on or before the anchor, in its quote
    /// currency. Liabilities come out negative.
    fn asset_anchor_values(
        &self,
        assets: &[Asset],
        anchor: NaiveDate,
    ) -> Result<HashMap<String, (String, f64)>> {
        let ids: Vec<String> = assets.iter().map(|asset| asset.id.clone()).collect();
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let quotes = self.quote_service.get_latest_quotes_as_of(&ids, anchor)?;
        let mut values = HashMap::new();
        for asset in assets {
            let Some(quote) = quotes.get(&asset.id) else {
                continue;
            };
            let (amount, currency) = normalize_amount(quote.close, &quote.currency);
            let Some(value) = amount.to_f64() else {
                continue;
            };
            let signed = if asset.kind == AssetKind::Liability {
                -value.abs()
            } else {
                value
            };
            values.insert(asset.id.clone(), (currency.to_string(), signed));
        }
        Ok(values)
    }

    /// A loan's scheduled balances up to `end`, negative, from its terms and
    /// recorded balances. None for liabilities without loan terms.
    fn loan_schedule(&self, asset: &Asset, end: NaiveDate) -> Option<Vec<(NaiveDate, f64)>> {
        let metadata = asset.metadata.as_ref()?;
        metadata.get(LOAN_PROJECTION_KEY)?;
        let quotes = self.quote_service.get_historical_quotes(&asset.id).ok()?;
        let calculation = calculate_loan(&LoanCalculationRequest {
            metadata: metadata.clone(),
            balances: quotes.iter().map(LoanBalance::from).collect(),
            as_of: end,
            payments: Vec::new(),
        })?;
        Some(
            calculation
                .rows
                .iter()
                .map(|row| (row.date, -row.balance.abs()))
                .collect(),
        )
    }

    /// Projects the positions in `scope_ids`: accounts start from their
    /// valuation on the anchor, alternative assets from their latest quote
    /// on or before it. Positions that transactions name but that sit
    /// outside the scope are projected too, so transfers convert correctly,
    /// and left out of the totals.
    fn project_positions(
        &self,
        scenario: &Scenario,
        scope_ids: &[String],
        single_account: bool,
        base_currency: &str,
        start_date: Option<NaiveDate>,
        end_date: Option<NaiveDate>,
    ) -> Result<ScenarioProjection> {
        let anchor = parse_date(&scenario.anchor_date, "The anchor date")?;
        let end = parse_date(&scenario.end_date, "The end date")?;
        let transactions = self.repository.list_transactions(&scenario.id)?;

        let mut referenced: BTreeSet<String> = scope_ids.iter().cloned().collect();
        for tx in &transactions {
            referenced.insert(tx.account_id.clone());
            if let Some(to) = &tx.to_account_id {
                referenced.insert(to.clone());
            }
        }
        let ids: Vec<String> = referenced.iter().cloned().collect();
        let accounts = self.account_service.get_accounts_by_ids(&ids)?;
        let mut currencies: BTreeMap<String, String> = accounts
            .iter()
            .map(|account| (account.id.clone(), account.currency.clone()))
            .collect();

        let missing: BTreeSet<&String> = referenced
            .iter()
            .filter(|id| !currencies.contains_key(*id))
            .collect();
        let assets: Vec<Asset> = if missing.is_empty() {
            Vec::new()
        } else {
            self.alternative_assets()?
                .into_iter()
                .filter(|asset| missing.contains(&asset.id))
                .collect()
        };

        let mut anchor_values = HashMap::new();
        let mut fx_at_anchor: HashMap<String, f64> = HashMap::new();
        fx_at_anchor.insert(base_currency.to_string(), 1.0);
        for account in &accounts {
            if let Some(anchor_value) = self.anchor_value(&account.id, anchor)? {
                if let Some(rate) = anchor_value.fx_rate_to_base {
                    fx_at_anchor.entry(account.currency.clone()).or_insert(rate);
                }
                anchor_values.insert(account.id.clone(), anchor_value);
            }
        }
        let asset_values = self.asset_anchor_values(&assets, anchor)?;
        let mut schedules = HashMap::new();
        for asset in &assets {
            let currency = asset_values
                .get(&asset.id)
                .map(|(currency, _)| currency.clone())
                .unwrap_or_else(|| asset.quote_ccy.clone());
            currencies.insert(asset.id.clone(), currency);
            if let Some((_, value)) = asset_values.get(&asset.id) {
                anchor_values.insert(
                    asset.id.clone(),
                    AnchorValue {
                        value: *value,
                        fx_rate_to_base: None,
                    },
                );
            }
            if asset.kind == AssetKind::Liability {
                if let Some(schedule) = self.loan_schedule(asset, end) {
                    schedules.insert(asset.id.clone(), schedule);
                }
            }
        }
        let unknown_account_ids: BTreeSet<String> = referenced
            .into_iter()
            .filter(|id| !currencies.contains_key(id))
            .collect();

        let wanted: BTreeSet<&String> = currencies.values().collect();
        for currency in wanted {
            if fx_at_anchor.contains_key(currency) {
                continue;
            }
            match self
                .fx_service
                .get_exchange_rate_for_date(currency, base_currency, anchor)
            {
                Ok(rate) => {
                    if let Some(rate) = rate.to_f64().filter(|rate| *rate > 0.0) {
                        fx_at_anchor.insert(currency.clone(), rate);
                    }
                }
                Err(error) => {
                    log::warn!("Scenario projection has no FX rate for {currency}: {error}");
                }
            }
        }

        assemble_projection(&ProjectionInputs {
            scenario,
            base_currency,
            scope_account_ids: scope_ids,
            single_account,
            account_currencies: &currencies,
            unknown_account_ids: &unknown_account_ids,
            anchor_values: &anchor_values,
            schedules: &schedules,
            fx_at_anchor: &fx_at_anchor,
            transactions: &transactions,
            start_date,
            end_date,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(value: &str) -> NaiveDate {
        NaiveDate::parse_from_str(value, "%Y-%m-%d").expect("valid test date")
    }

    fn scenario_input(anchor: &str, end: &str) -> ScenarioInput {
        ScenarioInput {
            name: "  Plan A ".to_string(),
            description: Some("   ".to_string()),
            anchor_date: anchor.to_string(),
            end_date: end.to_string(),
            assumptions: ScenarioAssumptions::default(),
        }
    }

    fn transaction_input(kind: ScenarioTransactionKind) -> ScenarioTransactionInput {
        ScenarioTransactionInput {
            date: "2026-02-01".to_string(),
            kind,
            account_id: "a".to_string(),
            to_account_id: None,
            amount: 100.0,
            to_amount: None,
            note: None,
        }
    }

    #[test]
    fn scenario_input_is_normalized() {
        let input = validate_scenario_input(
            scenario_input("2026-01-01", "2031-01-01"),
            date("2026-11-15"),
        )
        .expect("valid input");
        assert_eq!(input.name, "Plan A");
        assert_eq!(input.description, None);
    }

    #[test]
    fn the_anchor_cannot_be_after_today_and_the_end_follows_it() {
        let today = date("2026-11-15");
        assert!(validate_scenario_input(scenario_input("2026-11-15", "2031-11-15"), today).is_ok());
        assert!(
            validate_scenario_input(scenario_input("2026-11-16", "2031-11-15"), today).is_err()
        );
        assert!(
            validate_scenario_input(scenario_input("2026-01-01", "2026-01-01"), today).is_err()
        );
        assert!(
            validate_scenario_input(scenario_input("2026-01-01", "2077-01-02"), today).is_err()
        );
    }

    #[test]
    fn rates_outside_their_bounds_are_rejected() {
        let mut input = scenario_input("2026-01-01", "2027-01-01");
        input.assumptions.accounts.insert(
            "a".to_string(),
            AccountGrowthModel::Rate {
                annual_rate: 0.08,
                interest_tax_rate: 1.5,
                outflow_tax_rate: 0.0,
                growth_from: None,
            },
        );
        assert!(validate_scenario_input(input, date("2026-11-15")).is_err());

        let mut input = scenario_input("2026-01-01", "2027-01-01");
        input.assumptions.fx.insert("COP".to_string(), f64::NAN);
        assert!(validate_scenario_input(input, date("2026-11-15")).is_err());
    }

    #[test]
    fn transfers_need_two_accounts_and_others_need_one() {
        let mut transfer = transaction_input(ScenarioTransactionKind::Transfer);
        assert!(validate_transaction_input(transfer.clone()).is_err());
        transfer.to_account_id = Some("a".to_string());
        assert!(validate_transaction_input(transfer.clone()).is_err());
        transfer.to_account_id = Some("b".to_string());
        assert!(validate_transaction_input(transfer).is_ok());

        let mut deposit = transaction_input(ScenarioTransactionKind::Deposit);
        deposit.to_amount = Some(5.0);
        assert!(validate_transaction_input(deposit).is_err());

        let mut negative = transaction_input(ScenarioTransactionKind::Withdrawal);
        negative.amount = -1.0;
        assert!(validate_transaction_input(negative).is_err());
    }

    fn scenario(assumptions: ScenarioAssumptions) -> Scenario {
        Scenario {
            id: "s".to_string(),
            name: "Plan".to_string(),
            description: None,
            anchor_date: "2026-01-01".to_string(),
            end_date: "2026-01-31".to_string(),
            assumptions,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn transaction(
        on: &str,
        kind: ScenarioTransactionKind,
        from: &str,
        to: Option<&str>,
        amount: f64,
    ) -> ScenarioTransaction {
        ScenarioTransaction {
            id: format!("{from}-{on}"),
            scenario_id: "s".to_string(),
            date: on.to_string(),
            kind,
            account_id: from.to_string(),
            to_account_id: to.map(str::to_string),
            amount,
            to_amount: None,
            note: None,
            series_id: None,
            series_label: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    struct Fixture {
        scenario: Scenario,
        currencies: BTreeMap<String, String>,
        unknown: BTreeSet<String>,
        anchors: HashMap<String, AnchorValue>,
        fx: HashMap<String, f64>,
        transactions: Vec<ScenarioTransaction>,
    }

    impl Fixture {
        fn new() -> Self {
            let currencies = BTreeMap::from([
                ("aed".to_string(), "AED".to_string()),
                ("usd".to_string(), "USD".to_string()),
            ]);
            let anchors = HashMap::from([
                (
                    "aed".to_string(),
                    AnchorValue {
                        value: 40_000.0,
                        fx_rate_to_base: Some(0.25),
                    },
                ),
                (
                    "usd".to_string(),
                    AnchorValue {
                        value: 1_000.0,
                        fx_rate_to_base: Some(1.0),
                    },
                ),
            ]);
            let fx = HashMap::from([("AED".to_string(), 0.25), ("USD".to_string(), 1.0)]);
            Self {
                scenario: scenario(ScenarioAssumptions::default()),
                currencies,
                unknown: BTreeSet::new(),
                anchors,
                fx,
                transactions: Vec::new(),
            }
        }

        fn project(
            &self,
            scope: &[&str],
            single_account: bool,
            window: (Option<&str>, Option<&str>),
        ) -> ScenarioProjection {
            let scope: Vec<String> = scope.iter().map(|id| id.to_string()).collect();
            assemble_projection(&ProjectionInputs {
                scenario: &self.scenario,
                base_currency: "USD",
                scope_account_ids: &scope,
                single_account,
                account_currencies: &self.currencies,
                unknown_account_ids: &self.unknown,
                anchor_values: &self.anchors,
                schedules: &HashMap::new(),
                fx_at_anchor: &self.fx,
                transactions: &self.transactions,
                start_date: window.0.map(date),
                end_date: window.1.map(date),
            })
            .expect("projection")
        }
    }

    #[test]
    fn a_wide_scope_sums_base_values() {
        let fixture = Fixture::new();
        let projection = fixture.project(&["aed", "usd"], false, (None, None));
        assert_eq!(projection.currency, "USD");
        assert_eq!(projection.points.len(), 31);
        assert_eq!(projection.points[0].value, 11_000.0);
        assert_eq!(projection.points[0].value_base, 11_000.0);
        assert!(projection.warnings.is_empty());
    }

    #[test]
    fn a_single_account_keeps_its_currency() {
        let fixture = Fixture::new();
        let projection = fixture.project(&["aed"], true, (None, None));
        assert_eq!(projection.currency, "AED");
        assert_eq!(projection.points[0].value, 40_000.0);
        assert_eq!(projection.points[0].value_base, 10_000.0);
    }

    #[test]
    fn a_transfer_out_of_scope_reads_as_an_outflow() {
        let mut fixture = Fixture::new();
        fixture.transactions.push(transaction(
            "2026-01-10",
            ScenarioTransactionKind::Transfer,
            "aed",
            Some("usd"),
            4_000.0,
        ));
        let usd_only = fixture.project(&["usd"], true, (None, None));
        assert_eq!(usd_only.points.last().map(|p| p.value), Some(2_000.0));
        let both = fixture.project(&["aed", "usd"], false, (None, None));
        assert_eq!(both.points.last().map(|p| p.value), Some(11_000.0));
    }

    #[test]
    fn the_window_trims_points() {
        let fixture = Fixture::new();
        let projection = fixture.project(&["usd"], false, (Some("2026-01-10"), Some("2026-01-12")));
        let dates: Vec<&str> = projection.points.iter().map(|p| p.date.as_str()).collect();
        assert_eq!(dates, ["2026-01-10", "2026-01-11", "2026-01-12"]);
    }

    #[test]
    fn missing_data_becomes_warnings() {
        let mut fixture = Fixture::new();
        fixture.anchors.remove("usd");
        fixture.fx.remove("AED");
        fixture.unknown.insert("gone".to_string());
        fixture.transactions.push(transaction(
            "2026-01-05",
            ScenarioTransactionKind::Withdrawal,
            "usd",
            None,
            10.0,
        ));
        let projection = fixture.project(&["aed", "usd"], false, (None, None));
        let codes: Vec<ScenarioWarningCode> = projection.warnings.iter().map(|w| w.code).collect();
        assert_eq!(
            codes,
            [
                ScenarioWarningCode::UnknownAccount,
                ScenarioWarningCode::NoAnchorValuation,
                ScenarioWarningCode::MissingFxRate,
                ScenarioWarningCode::NegativeBalance,
            ]
        );
        // The AED account has no rate, so it is left out of base totals.
        assert_eq!(projection.points[0].value_base, 0.0);
        assert_eq!(projection.points.last().map(|p| p.value_base), Some(-10.0));
    }

    #[test]
    fn positions_that_start_negative_raise_no_negative_balance_warning() {
        let mut fixture = Fixture::new();
        fixture
            .currencies
            .insert("loan".to_string(), "USD".to_string());
        fixture.anchors.insert(
            "loan".to_string(),
            AnchorValue {
                value: -8_000.0,
                fx_rate_to_base: None,
            },
        );
        let projection = fixture.project(&["usd", "loan"], false, (None, None));
        assert!(projection.warnings.is_empty());
        assert_eq!(projection.points[0].value, 1_000.0 - 8_000.0);
    }

    #[test]
    fn growth_models_come_from_the_assumptions() {
        let mut fixture = Fixture::new();
        fixture.scenario.end_date = "2027-01-01".to_string();
        fixture.scenario.assumptions.accounts.insert(
            "usd".to_string(),
            AccountGrowthModel::Rate {
                annual_rate: 0.08,
                interest_tax_rate: 0.0,
                outflow_tax_rate: 0.0,
                growth_from: None,
            },
        );
        let projection = fixture.project(&["usd"], true, (None, None));
        let last = projection
            .points
            .last()
            .map(|p| p.value)
            .unwrap_or_default();
        assert!((last - 1_080.0).abs() < 1e-6);
    }
}
