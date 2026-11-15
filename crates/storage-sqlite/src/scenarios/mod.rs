//! SQLite storage for scenarios. The tables are local-only: they do not join
//! device sync (see `docs/features/scenarios/design.md`).

mod model;
mod repository;

pub use model::{ScenarioDB, ScenarioTransactionDB};
pub use repository::ScenarioRepository;
