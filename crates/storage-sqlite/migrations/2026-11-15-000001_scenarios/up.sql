-- Scenarios: user-defined projections. Hypothetical transactions live in their
-- own table and never enter activities or the portfolio engine.

CREATE TABLE scenarios (
    id TEXT NOT NULL PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    anchor_date TEXT NOT NULL,
    end_date TEXT NOT NULL,
    assumptions_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE scenario_transactions (
    id TEXT NOT NULL PRIMARY KEY,
    scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    date TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('DEPOSIT', 'WITHDRAWAL', 'TRANSFER')),
    account_id TEXT NOT NULL,
    to_account_id TEXT,
    amount DOUBLE NOT NULL,
    to_amount DOUBLE,
    note TEXT,
    series_id TEXT,
    series_label TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_scenario_transactions_scenario_date
    ON scenario_transactions (scenario_id, date);
