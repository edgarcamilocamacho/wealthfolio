use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;
use uuid::Uuid;

use wealthfolio_core::errors::{Error, ValidationError};
use wealthfolio_core::scenarios::{
    NewScenarioTransaction, Scenario, ScenarioInput, ScenarioRepositoryTrait, ScenarioTransaction,
    ScenarioTransactionUpdate,
};
use wealthfolio_core::Result;

use super::model::{serialize_assumptions, ScenarioDB, ScenarioTransactionDB};
use crate::db::{get_connection, WriteHandle};
use crate::errors::StorageError;
use crate::schema::{scenario_transactions, scenarios};

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn not_in_scenario(id: &str) -> Error {
    Error::Validation(ValidationError::InvalidInput(format!(
        "Transaction {id} does not belong to this scenario"
    )))
}

fn new_row(
    scenario_id: &str,
    row: NewScenarioTransaction,
    timestamp: &str,
) -> ScenarioTransactionDB {
    ScenarioTransactionDB {
        id: Uuid::new_v4().to_string(),
        scenario_id: scenario_id.to_string(),
        date: row.input.date,
        kind: row.input.kind.as_str().to_string(),
        account_id: row.input.account_id,
        to_account_id: row.input.to_account_id,
        amount: row.input.amount,
        to_amount: row.input.to_amount,
        note: row.input.note,
        series_id: row.series_id,
        series_label: row.series_label,
        created_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    }
}

pub struct ScenarioRepository {
    pool: Arc<Pool<ConnectionManager<SqliteConnection>>>,
    writer: WriteHandle,
}

impl ScenarioRepository {
    pub fn new(pool: Arc<Pool<ConnectionManager<SqliteConnection>>>, writer: WriteHandle) -> Self {
        Self { pool, writer }
    }
}

#[async_trait]
impl ScenarioRepositoryTrait for ScenarioRepository {
    fn list_scenarios(&self) -> Result<Vec<Scenario>> {
        let mut conn = get_connection(&self.pool)?;
        scenarios::table
            .order((scenarios::created_at.asc(), scenarios::id.asc()))
            .select(ScenarioDB::as_select())
            .load::<ScenarioDB>(&mut conn)
            .map_err(StorageError::from)?
            .into_iter()
            .map(Scenario::try_from)
            .collect()
    }

    fn get_scenario(&self, id: &str) -> Result<Scenario> {
        let mut conn = get_connection(&self.pool)?;
        let row = scenarios::table
            .find(id)
            .select(ScenarioDB::as_select())
            .first::<ScenarioDB>(&mut conn)
            .map_err(StorageError::from)?;
        Scenario::try_from(row)
    }

    async fn create_scenario(&self, input: ScenarioInput) -> Result<Scenario> {
        let assumptions_json = serialize_assumptions(&input.assumptions)?;
        self.writer
            .exec_tx(move |tx| -> Result<Scenario> {
                let timestamp = now();
                let row = ScenarioDB {
                    id: Uuid::new_v4().to_string(),
                    name: input.name,
                    description: input.description,
                    anchor_date: input.anchor_date,
                    end_date: input.end_date,
                    assumptions_json,
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                };
                diesel::insert_into(scenarios::table)
                    .values(&row)
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Scenario::try_from(row)
            })
            .await
    }

    async fn update_scenario(&self, id: &str, input: ScenarioInput) -> Result<Scenario> {
        let id = id.to_string();
        let assumptions_json = serialize_assumptions(&input.assumptions)?;
        self.writer
            .exec_tx(move |tx| -> Result<Scenario> {
                let row = diesel::update(scenarios::table.find(&id))
                    .set((
                        scenarios::name.eq(input.name),
                        scenarios::description.eq(input.description),
                        scenarios::anchor_date.eq(input.anchor_date),
                        scenarios::end_date.eq(input.end_date),
                        scenarios::assumptions_json.eq(assumptions_json),
                        scenarios::updated_at.eq(now()),
                    ))
                    .returning(ScenarioDB::as_returning())
                    .get_result::<ScenarioDB>(tx.conn())
                    .map_err(StorageError::from)?;
                Scenario::try_from(row)
            })
            .await
    }

    async fn delete_scenario(&self, id: &str) -> Result<()> {
        let id = id.to_string();
        self.writer
            .exec_tx(move |tx| -> Result<()> {
                diesel::delete(
                    scenario_transactions::table.filter(scenario_transactions::scenario_id.eq(&id)),
                )
                .execute(tx.conn())
                .map_err(StorageError::from)?;
                diesel::delete(scenarios::table.find(&id))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }

    async fn duplicate_scenario(&self, id: &str, name: String) -> Result<Scenario> {
        let id = id.to_string();
        self.writer
            .exec_tx(move |tx| -> Result<Scenario> {
                let source = scenarios::table
                    .find(&id)
                    .select(ScenarioDB::as_select())
                    .first::<ScenarioDB>(tx.conn())
                    .map_err(StorageError::from)?;
                let rows = scenario_transactions::table
                    .filter(scenario_transactions::scenario_id.eq(&id))
                    .select(ScenarioTransactionDB::as_select())
                    .load::<ScenarioTransactionDB>(tx.conn())
                    .map_err(StorageError::from)?;

                let timestamp = now();
                let copy = ScenarioDB {
                    id: Uuid::new_v4().to_string(),
                    name,
                    created_at: timestamp.clone(),
                    updated_at: timestamp.clone(),
                    ..source
                };
                diesel::insert_into(scenarios::table)
                    .values(&copy)
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;

                // Each copied series gets its own id, so editing one scenario's
                // series never touches the other's.
                let mut series_ids: HashMap<String, String> = HashMap::new();
                let copies: Vec<ScenarioTransactionDB> = rows
                    .into_iter()
                    .map(|row| ScenarioTransactionDB {
                        id: Uuid::new_v4().to_string(),
                        scenario_id: copy.id.clone(),
                        series_id: row.series_id.as_ref().map(|series_id| {
                            series_ids
                                .entry(series_id.clone())
                                .or_insert_with(|| Uuid::new_v4().to_string())
                                .clone()
                        }),
                        created_at: timestamp.clone(),
                        updated_at: timestamp.clone(),
                        ..row
                    })
                    .collect();
                for chunk in copies.chunks(500) {
                    diesel::insert_into(scenario_transactions::table)
                        .values(chunk)
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                Scenario::try_from(copy)
            })
            .await
    }

    fn list_transactions(&self, scenario_id: &str) -> Result<Vec<ScenarioTransaction>> {
        let mut conn = get_connection(&self.pool)?;
        scenario_transactions::table
            .filter(scenario_transactions::scenario_id.eq(scenario_id))
            .order((
                scenario_transactions::date.asc(),
                scenario_transactions::created_at.asc(),
                scenario_transactions::id.asc(),
            ))
            .select(ScenarioTransactionDB::as_select())
            .load::<ScenarioTransactionDB>(&mut conn)
            .map_err(StorageError::from)?
            .into_iter()
            .map(ScenarioTransaction::try_from)
            .collect()
    }

    async fn apply_transaction_changes(
        &self,
        scenario_id: &str,
        creates: Vec<NewScenarioTransaction>,
        updates: Vec<ScenarioTransactionUpdate>,
        delete_ids: Vec<String>,
        delete_series_ids: Vec<String>,
    ) -> Result<()> {
        let scenario_id = scenario_id.to_string();
        self.writer
            .exec_tx(move |tx| -> Result<()> {
                let timestamp = now();
                let in_scenario = scenario_transactions::scenario_id.eq(scenario_id.clone());

                for id in &delete_ids {
                    let deleted = diesel::delete(
                        scenario_transactions::table
                            .filter(in_scenario.clone())
                            .filter(scenario_transactions::id.eq(id)),
                    )
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                    if deleted == 0 {
                        return Err(not_in_scenario(id));
                    }
                }
                for series_id in &delete_series_ids {
                    diesel::delete(
                        scenario_transactions::table
                            .filter(in_scenario.clone())
                            .filter(scenario_transactions::series_id.eq(series_id)),
                    )
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                }
                for update in updates {
                    let input = update.input;
                    let updated = diesel::update(
                        scenario_transactions::table
                            .filter(in_scenario.clone())
                            .filter(scenario_transactions::id.eq(&update.id)),
                    )
                    .set((
                        scenario_transactions::date.eq(input.date),
                        scenario_transactions::kind.eq(input.kind.as_str()),
                        scenario_transactions::account_id.eq(input.account_id),
                        scenario_transactions::to_account_id.eq(input.to_account_id),
                        scenario_transactions::amount.eq(input.amount),
                        scenario_transactions::to_amount.eq(input.to_amount),
                        scenario_transactions::note.eq(input.note),
                        scenario_transactions::updated_at.eq(&timestamp),
                    ))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                    if updated == 0 {
                        return Err(not_in_scenario(&update.id));
                    }
                }
                let rows: Vec<ScenarioTransactionDB> = creates
                    .into_iter()
                    .map(|row| new_row(&scenario_id, row, &timestamp))
                    .collect();
                for chunk in rows.chunks(500) {
                    diesel::insert_into(scenario_transactions::table)
                        .values(chunk)
                        .execute(tx.conn())
                        .map_err(StorageError::from)?;
                }
                diesel::update(scenarios::table.find(&scenario_id))
                    .set(scenarios::updated_at.eq(&timestamp))
                    .execute(tx.conn())
                    .map_err(StorageError::from)?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_pool, run_migrations, write_actor::spawn_writer, TestDir};
    use wealthfolio_core::scenarios::{
        AccountGrowthModel, ScenarioAssumptions, ScenarioTransactionInput, ScenarioTransactionKind,
    };

    fn repository() -> (ScenarioRepository, TestDir) {
        std::env::set_var("CONNECT_API_URL", "http://test.local");
        let dir = TestDir::new();
        let db_path = dir.path().join("test.db").to_string_lossy().to_string();
        run_migrations(&db_path).expect("migrations");
        let pool = create_pool(&db_path).expect("pool");
        let writer = spawn_writer((*pool).clone()).expect("writer");
        (ScenarioRepository::new(pool, writer), dir)
    }

    fn input(name: &str) -> ScenarioInput {
        let mut assumptions = ScenarioAssumptions::default();
        assumptions.accounts.insert(
            "ibkr".to_string(),
            AccountGrowthModel::Rate {
                annual_rate: 0.08,
                interest_tax_rate: 0.0,
                outflow_tax_rate: 0.0,
                growth_from: None,
            },
        );
        assumptions.fx.insert("COP".to_string(), -0.04);
        ScenarioInput {
            name: name.to_string(),
            description: None,
            anchor_date: "2026-06-01".to_string(),
            end_date: "2031-06-01".to_string(),
            assumptions,
        }
    }

    fn row(date: &str, amount: f64, series: Option<&str>) -> NewScenarioTransaction {
        NewScenarioTransaction {
            input: ScenarioTransactionInput {
                date: date.to_string(),
                kind: ScenarioTransactionKind::Transfer,
                account_id: "adcb".to_string(),
                to_account_id: Some("ibkr".to_string()),
                amount,
                to_amount: None,
                note: None,
            },
            series_id: series.map(str::to_string),
            series_label: series.map(|_| "To IBKR".to_string()),
        }
    }

    #[tokio::test]
    async fn scenarios_round_trip_with_their_assumptions() {
        let (repo, _dir) = repository();
        let created = repo.create_scenario(input("Plan A")).await.expect("create");
        let loaded = repo.get_scenario(&created.id).expect("load");
        assert_eq!(loaded, created);
        assert_eq!(loaded.assumptions.fx["COP"], -0.04);

        let mut changed = input("Plan B");
        changed.anchor_date = "2026-07-01".to_string();
        let updated = repo
            .update_scenario(&created.id, changed)
            .await
            .expect("update");
        assert_eq!(updated.name, "Plan B");
        assert_eq!(updated.anchor_date, "2026-07-01");
        assert_eq!(repo.list_scenarios().expect("list").len(), 1);
    }

    #[tokio::test]
    async fn transaction_batches_apply_atomically_and_stay_in_their_scenario() {
        let (repo, _dir) = repository();
        let first = repo.create_scenario(input("A")).await.expect("create");
        let second = repo.create_scenario(input("B")).await.expect("create");
        repo.apply_transaction_changes(
            &first.id,
            vec![
                row("2026-07-25", 5_000.0, Some("s1")),
                row("2026-08-25", 5_000.0, Some("s1")),
                row("2026-07-01", 100.0, None),
            ],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("insert");
        let rows = repo.list_transactions(&first.id).expect("list");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].date, "2026-07-01");

        // An id from another scenario fails the whole batch.
        let single = rows
            .iter()
            .find(|row| row.series_id.is_none())
            .expect("single row")
            .clone();
        let error = repo
            .apply_transaction_changes(
                &second.id,
                vec![row("2026-09-01", 1.0, None)],
                vec![],
                vec![single.id.clone()],
                vec![],
            )
            .await;
        assert!(error.is_err());
        assert!(repo.list_transactions(&second.id).expect("list").is_empty());
        assert_eq!(repo.list_transactions(&first.id).expect("list").len(), 3);

        // Replacing a series removes its rows; updates keep the row id.
        repo.apply_transaction_changes(
            &first.id,
            vec![row("2026-07-25", 6_000.0, Some("s1"))],
            vec![ScenarioTransactionUpdate {
                id: single.id.clone(),
                input: ScenarioTransactionInput {
                    amount: 250.0,
                    ..row("2026-07-02", 0.0, None).input
                },
            }],
            vec![],
            vec!["s1".to_string()],
        )
        .await
        .expect("replace");
        let rows = repo.list_transactions(&first.id).expect("list");
        assert_eq!(rows.len(), 2);
        let edited = rows
            .iter()
            .find(|row| row.id == single.id)
            .expect("edited row");
        assert_eq!(edited.amount, 250.0);
        assert_eq!(edited.date, "2026-07-02");
        assert!(rows
            .iter()
            .any(|row| row.series_id.as_deref() == Some("s1") && row.amount == 6_000.0));
    }

    #[tokio::test]
    async fn duplicating_copies_rows_with_new_series_ids_and_delete_cascades() {
        let (repo, _dir) = repository();
        let source = repo.create_scenario(input("A")).await.expect("create");
        repo.apply_transaction_changes(
            &source.id,
            vec![
                row("2026-07-25", 5_000.0, Some("s1")),
                row("2026-08-25", 5_000.0, Some("s1")),
            ],
            vec![],
            vec![],
            vec![],
        )
        .await
        .expect("insert");

        let copy = repo
            .duplicate_scenario(&source.id, "A (copy)".to_string())
            .await
            .expect("duplicate");
        assert_eq!(copy.name, "A (copy)");
        assert_eq!(copy.assumptions, source.assumptions);
        let copied = repo.list_transactions(&copy.id).expect("list");
        assert_eq!(copied.len(), 2);
        let series = copied[0].series_id.clone().expect("series id");
        assert_ne!(series, "s1");
        assert!(copied
            .iter()
            .all(|row| row.series_id.as_deref() == Some(series.as_str())));

        repo.delete_scenario(&source.id).await.expect("delete");
        assert!(repo.get_scenario(&source.id).is_err());
        assert!(repo.list_transactions(&source.id).expect("list").is_empty());
        assert_eq!(repo.list_transactions(&copy.id).expect("list").len(), 2);
    }
}
