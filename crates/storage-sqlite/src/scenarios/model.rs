//! Database models for scenarios.

use diesel::prelude::*;

use wealthfolio_core::scenarios::{
    Scenario, ScenarioAssumptions, ScenarioTransaction, ScenarioTransactionKind,
};
use wealthfolio_core::Result;

use crate::errors::StorageError;

#[derive(Queryable, Insertable, Identifiable, Selectable, Debug, Clone)]
#[diesel(table_name = crate::schema::scenarios)]
pub struct ScenarioDB {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub anchor_date: String,
    pub end_date: String,
    pub assumptions_json: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Queryable, Insertable, Identifiable, Selectable, Debug, Clone)]
#[diesel(table_name = crate::schema::scenario_transactions)]
pub struct ScenarioTransactionDB {
    pub id: String,
    pub scenario_id: String,
    pub date: String,
    pub kind: String,
    pub account_id: String,
    pub to_account_id: Option<String>,
    pub amount: f64,
    pub to_amount: Option<f64>,
    pub note: Option<String>,
    pub series_id: Option<String>,
    pub series_label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub(super) fn serialize_assumptions(assumptions: &ScenarioAssumptions) -> Result<String> {
    serde_json::to_string(assumptions).map_err(|_| {
        StorageError::SerializationError("Could not serialize scenario assumptions".to_string())
            .into()
    })
}

impl TryFrom<ScenarioDB> for Scenario {
    type Error = wealthfolio_core::Error;

    fn try_from(db: ScenarioDB) -> Result<Self> {
        let assumptions = serde_json::from_str(&db.assumptions_json).map_err(|_| {
            StorageError::SerializationError("Invalid scenario assumptions".to_string())
        })?;
        Ok(Self {
            id: db.id,
            name: db.name,
            description: db.description,
            anchor_date: db.anchor_date,
            end_date: db.end_date,
            assumptions,
            created_at: db.created_at,
            updated_at: db.updated_at,
        })
    }
}

impl TryFrom<ScenarioTransactionDB> for ScenarioTransaction {
    type Error = wealthfolio_core::Error;

    fn try_from(db: ScenarioTransactionDB) -> Result<Self> {
        let kind = ScenarioTransactionKind::parse(&db.kind).ok_or_else(|| {
            StorageError::SerializationError("Invalid scenario transaction kind".to_string())
        })?;
        Ok(Self {
            id: db.id,
            scenario_id: db.scenario_id,
            date: db.date,
            kind,
            account_id: db.account_id,
            to_account_id: db.to_account_id,
            amount: db.amount,
            to_amount: db.to_amount,
            note: db.note,
            series_id: db.series_id,
            series_label: db.series_label,
            created_at: db.created_at,
            updated_at: db.updated_at,
        })
    }
}
