# Scenarios: design

- Date: 2026-11-15
- Status: accepted (2026-11-15). Phase 1 implemented; phases 2–4 pending.
- Branch: `feat/scenarios`, cut from the fork's `homelab` branch. This feature
  is fork-only and is not proposed upstream.

## 1. Problem

Wealthfolio shows what happened. It cannot show what a plan says will happen,
and it cannot show how far reality has drifted from a plan made months ago.

The planning tools that exist answer narrower questions:

- Save-up goals (`crates/core/src/planning/save_up.rs`) project one balance with
  one rate and one monthly contribution.
- The retirement planner (`crates/core/src/planning/retirement/`) uses one rate
  for the whole portfolio.
- Neither one knows about individual accounts, money moving between them, or
  per-account tax. Neither one draws on the history charts.

A concrete case:

> Every month 44,000 AED arrive in account A (salary). Each time, 5,000 AED go
> to a brokerage account that is expected to grow 8 % a year (effective annual).
> A savings account keeps earning its current rate, minus its current interest
> withholding. Five years.

The user wants to save several of these as **scenarios**, turn one on from the
dashboard, and see it as a dashed line on the same charts that show history. A
scenario starts on a date that may lie in the past, so a plan made six months
ago can be compared with what actually happened.

## 2. Goals and non-goals

Goals:

1. Per-account projections, each in its own currency, combined in base currency.
2. Plans written as **dated hypothetical transactions**: deposits, withdrawals
   and transfers between accounts. They stay separate from real activities.
3. A growth model per account for what the user does not decide: interest,
   expected return, interest withholding, and tax on withdrawals.
4. An editable **anchor date** (today or earlier) where the projection starts
   from real balances.
5. An overlay on the dashboard and account history charts, plus a
   real-vs-projected figure for today.
6. Hypothetical transactions that an AI model can generate from plain
   instructions, behind the same draft-then-confirm pattern activities use.

Non-goals for the first version: Monte Carlo, volatility bands, inflation,
versioned snapshots of a scenario, device sync, and the net-worth page (it adds
alternative assets and liabilities; see phase 4).

## 3. Naming

The portfolio engine already uses "projection" for its read models
(`projection_state`, `crates/core/src/portfolio/projection.rs`). To avoid
confusion, the code says **scenario** everywhere: tables, modules, commands and
types. The UI may say "projection" for the line on a chart.

## 4. Concepts

**Scenario.** A name, an anchor date, an end date, per-account growth models,
per-currency FX assumptions, and a list of hypothetical transactions.

**Anchor.** The day the projection starts. Each account starts from its real
`total_value` on that day, read from the daily valuation history
(`DailyAccountValuation`). An account with no activity on or before the anchor
has no valuation and starts at zero, which is its real balance; only an account
that had activity but lacks a valuation raises a warning. The anchor can be
edited at any time, but never set after today. When it moves:

- the starting balances are read again for the new date;
- transactions dated on or before the anchor are ignored (the editor greys them
  out) and are not deleted;
- "Duplicate" keeps the old scenario when the user wants both lines.

Between the anchor and today, the projection and the real history overlap. That
overlap is the comparison.

**Hypothetical transaction.** One dated row: `DEPOSIT`, `WITHDRAWAL` or
`TRANSFER` (from one account to another). Its amount is in the source account's
currency. A transfer between currencies converts at the scenario's projected FX
rate for that day, unless the row gives `to_amount`. Interest and tax are not
rows: the growth model produces them, because they depend on the balance.

**Series.** A group of rows created together by a recurrence rule, for example
"44,000 AED to account A on the 25th of every month until 2031-11-25". The rows
are stored expanded, as literal transactions. They share a `series_id` and a
label, so the editor can show them as one line and replace or delete them
together. The rule itself is not stored: replacing a series deletes its rows and
expands a new rule.

**Growth model** (per account, in the scenario):

| Model  | Behaviour                                                            | Parameters                                        |
| ------ | -------------------------------------------------------------------- | ------------------------------------------------- |
| `flat` | Balance changes only through transactions. The default.              | none                                              |
| `rate` | Daily compounding of an effective annual rate on a positive balance. | `annualRate`, `interestTaxRate`, `outflowTaxRate` |

- `interestTaxRate` is the share of each day's interest withheld (for example
  source withholding on savings interest).
- `outflowTaxRate` is a tax charged on each withdrawal or outgoing transfer (for
  example Colombia's 4×1000).
- An investment account with an expected return uses `rate` with no taxes. It is
  a deterministic expected path, not a market model.

**FX assumption** (per currency other than base): an annual drift against the
base currency, `0` by default. The starting rate is the stored rate on the
anchor date. On day `d`, `rate(d) = rate(anchor) × (1 + drift)^(days / 365)`. No
market-data request is made, for any date.

## 5. Engine

A pure function in `crates/core/src/scenarios/engine.rs`. It reads neither the
clock nor storage, the same contract `crates/core/src/assets/loan.rs` follows.

```text
input:  anchor, end, base currency,
        start value + currency per account,
        FX at anchor + drift per currency,
        growth model per account,
        transactions (already filtered to anchor < date <= end)
output: one point per day per account: value (account currency), value (base)
        warnings (account with no valuation on the anchor, negative balance, ...)
```

For each day after the anchor, for each account:

1. **Growth.** With `rate`, if the balance is positive:
   `interest = balance × ((1 + annualRate)^(1/365) − 1)`, then
   `balance += interest × (1 − interestTaxRate)`.
2. **Transactions** dated that day, in row order. A withdrawal or outgoing
   transfer also subtracts `amount × outflowTaxRate`.
3. **Record** the balance and its base value at that day's projected FX rate.

The output is daily on purpose: the history charts use a category X axis with
one row per day (`history-chart.tsx`, `XAxis type="category"`), so daily
projected points join the history without changing the axis. Five years is about
1,830 points per account, which is cheap to compute.

Invariants, each with a test:

- A transfer between two `flat` accounts in the same currency leaves the total
  unchanged.
- With every model `flat` and no transactions, the line stays at the anchor
  values (FX drift aside).
- The first point equals the real valuation on the anchor date.
- Rows on or before the anchor, or after the end, have no effect.
- `rate` with `annualRate = r` and no flows grows by `(1 + r)` over 365 days.

## 6. Persistence

Two new tables. Why storage is needed: scenarios are user-entered data that must
survive restarts and be editable row by row, including by an AI tool. A
generated series is easily hundreds of rows. The add-on prototype kept one JSON
blob in a key-value store, which could not be queried or edited row by row.
`app_settings` would have the same problem.

```sql
CREATE TABLE scenarios (
    id               TEXT PRIMARY KEY NOT NULL,
    name             TEXT NOT NULL,
    description      TEXT,
    anchor_date      TEXT NOT NULL,              -- YYYY-MM-DD, <= today when saved
    end_date         TEXT NOT NULL,              -- YYYY-MM-DD, > anchor_date
    assumptions_json TEXT NOT NULL DEFAULT '{}', -- growth models per account, FX drift
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE TABLE scenario_transactions (
    id            TEXT PRIMARY KEY NOT NULL,
    scenario_id   TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    date          TEXT NOT NULL,                 -- YYYY-MM-DD
    kind          TEXT NOT NULL,                 -- DEPOSIT | WITHDRAWAL | TRANSFER
    account_id    TEXT NOT NULL,                 -- source account
    to_account_id TEXT,                          -- TRANSFER only
    amount        DOUBLE NOT NULL,               -- > 0, source account currency
    to_amount     DOUBLE,                        -- optional, destination currency
    note          TEXT,
    series_id     TEXT,
    series_label  TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);
CREATE INDEX idx_scenario_transactions_scenario_date
    ON scenario_transactions (scenario_id, date);
```

- `assumptions_json` is stored as text, like `goal_plans.settings_json`, and
  read into a typed struct (`ScenarioAssumptions`). Shape:
  `{"accounts": {"<accountId>": {"model": "rate", "annualRate": 0.08, ...}}, "fx": {"COP": -0.04}}`.
- Account ids are not foreign keys: a scenario survives an account's deletion,
  and the engine reports the unknown id as a warning.
- Amounts are `DOUBLE`, like `goals.target_amount`. These are estimates, not
  ledger entries.
- Writes go through the write actor
  (`crates/storage-sqlite/src/db/write_actor.rs`). Ids are `Uuid::new_v4()`, as
  in the other repositories.
- **Device sync is out of v1.** The tables skip the outbox calls, as
  `projection_state` does. Joining sync touches six places (outbox model,
  `SyncEntity`, `APP_SYNC_TABLES`, adapter descriptor, storage mapping, wire
  names) and can be done once the shape is stable.

## 7. Backend surface

The service follows the small contribution-limits service
(`crates/core/src/limits/`), not the much larger goals service. Commands are
thin in both runtimes (`apps/tauri/src/commands/scenario.rs`,
`apps/server/src/api/scenarios.rs`).

| Command                      | Purpose                                                                                       |
| ---------------------------- | --------------------------------------------------------------------------------------------- |
| `get_scenarios`              | List                                                                                          |
| `get_scenario`               | One scenario with its assumptions                                                             |
| `create_scenario`            | Validates the anchor (≤ today, app time zone) and the end date                                |
| `update_scenario`            | Name, dates, assumptions                                                                      |
| `delete_scenario`            | Cascades to its transactions                                                                  |
| `duplicate_scenario`         | Copies the scenario and its rows                                                              |
| `get_scenario_transactions`  | Rows of a scenario                                                                            |
| `save_scenario_transactions` | `{creates, updates, deleteIds, series}`. `series` holds recurrence rules the backend expands. |
| `get_scenario_projection`    | `(scenarioId, filter: AccountScope, startDate?, endDate?)`                                    |

**Recurrence rules** are expanded in Rust, so the editor and the AI tool produce
the same rows:
`{label, kind, accountId, toAccountId?, amount, toAmount?, frequency: "monthly", dayOfMonth, startDate, endDate, replaceSeriesId?}`.
A day past the end of a short month falls on that month's last day.

**`get_scenario_projection`** resolves the scope with `resolve_account_scope`
and keeps the accounts the valuation command keeps (`holdings_account_ids` in
`apps/tauri/src/commands/portfolio.rs`), so the projected line and the real line
always cover the same accounts. For an `account` scope it returns values in the
account currency and in base, as valuation rows do. For a wider scope it returns
the summed base values, like `get_historical_valuation_totals_for_accounts`.
Every account in scope is projected: an account without a model is `flat`. A
transfer to an account outside the scope reads as an outflow, which is also how
the real line shows it.

It loads the starting balances (one valuation per account on the anchor date)
and the anchor FX rates through the existing services, then runs the engine.
Whole-history loads run through `blocking` in the portfolio coordinator, as
`AGENTS.md` requires; this command reads one day of valuations, so it does not
need to.

Cache invalidation needs no new event. The frontend invalidates scenario keys
after its own mutations, and `portfolio-update-complete` already invalidates
every query, so a projection whose anchor values changed refreshes by itself.

## 8. Frontend

**Scenarios page** (`apps/frontend/src/features/scenarios/`, route `/scenarios`,
next to Goals in the navigation):

- List: name, anchor, end, and today's real-vs-projected figure.
- Detail:
  - Header: name, anchor date and end date pickers (`DatePickerInput`).
  - **Accounts**: one row per account, with its model and parameters.
  - **Currencies**: drift per currency.
  - **Transactions**: grouped by series ("Salary · 60 × 44,000 AED · monthly"),
    with single rows below. Each row can be added, edited and deleted. The form
    has a "Repeat monthly until" option, which sends a recurrence rule.
  - Preview: the same history chart, `all` scope, with the overlay.

**Overlay on the charts** (dashboard and account page):

- A scenario selector next to the dashboard's portfolio selector, and on the
  account page. The choice is a UI preference stored the way the shared
  dashboard period is stored. It is not financial data.
- `HistoryChart` gets an optional `projection` prop with `{date, value}` points.
  These are merged by date into the same data array as an optional
  `projectedValue` field.
  - Past rows keep `totalValue`. Future rows have only `projectedValue`.
  - The projected series uses the chart's colour, a fine dash
    (`strokeDasharray="2 4"`), lower opacity and no fill. The existing
    net-contribution line keeps its `5 5` dash, so the two stay distinct.
  - The Y-domain calculation (`history-chart-scale.ts`) also reads the projected
    values, so the line is not clipped.
  - The tooltip handles rows without `totalValue`. On days with both values it
    shows "Projected" and "Difference".
- **Future window.** While a scenario is active, a chip next to the period
  selector picks how far right the chart runs: 1 year, 3 years, or to the end.
  The period selector keeps controlling the left edge.
- Above the chart, "Real vs projected today: −3.2 %" is shown when the anchor is
  in the past.

Every new string goes into every locale. RTL rules apply: logical classes, and
the chart's direction follows the existing history chart.

## 9. AI generation

Reuse the draft-then-confirm pattern of `record_activities`
(`crates/agent-tools/src/tools/record_activities.rs` and
`record-activities-tool-ui.tsx`):

- **In-app tool `draft_scenario_transactions`** (access level `Draft`). Input:
  the scenario id, recurrence rules and single rows. The backend validates them
  and expands the rules. The tool returns the rows without saving anything. The
  card shows a summary per series, and **Add N transactions** calls
  `save_scenario_transactions`, then records the result with `updateToolResult`,
  as the activities card does. The model sends rules, not 60 rows, so the
  100-row batch limit and copying errors stop mattering.
- **MCP tool `commit_scenario_transactions`**, behind a new `scenarios:write`
  scope, so an external agent can write a scenario after the user reviews it in
  chat. Rows are hypothetical and never touch real activities.
- `get_scenarios` as a read tool, so the assistant can see what exists.

## 10. Phases

All four phases are implemented on `feat/scenarios`.

1. **Core and editor.** Migration, engine with the invariant tests, service,
   commands in both runtimes, adapters (with the parity test), the scenarios
   page and editor, and the preview chart.
2. **Overlay.** `HistoryChart` projection prop, selector, future window,
   tooltip, and the real-vs-projected figure on the dashboard and account page.
3. **AI.** The draft tool and its card, the MCP commit tool and scope, and the
   read tool.
4. **Net worth and history.**
   - **Alternative assets are positions.** A scenario projects every alternative
     asset from its latest quote on or before the anchor, in its quote currency.
     A non-liability asset can take the `rate` model as an annual appreciation,
     and a transfer can pay into it (an instalment on a property). The `rate`
     model gains an optional `growthFrom` date: an off-plan property stays at
     what was paid until handover, then appreciates.
   - **Liabilities** count negative. A loan with terms follows its amortization
     from `calculate_loan` as a schedule of balances (recorded balances
     included; tagged payments are not read). Other liabilities stay flat.
     Transfers cannot target a liability.
   - **Net worth projection** (`get_scenario_net_worth_projection`): every
     net-worth account plus every alternative asset, in base currency. It draws
     on the net-worth chart through the same picker, and the scenario page's
     preview switches between accounts and net worth.
   - **Suggest from history** (`suggest_scenario_account_growth`): gross
     interest (final amount plus the tax recorded on it) over the average daily
     balance of the last 12 months, compounded daily into an effective annual
     rate; withholding is that recorded tax plus standalone `TAX` activities,
     over gross interest. Activities in another currency than the account's are
     left out.

Still out: inclusion in "export every dataset", device sync, volatility bands,
inflation, and parity tests against the add-on prototype's CSVs.

## 11. Verification

- Engine: unit tests for the invariants in §5. Parity tests against the add-on
  prototype's month-end CSVs for the `flat` and `rate` cases it shares.
- Service and repository: tests for the anchor ≤ today rule, series expansion
  (month-end days, replacement), the cascade on delete, and unknown accounts.
- `cargo fmt --check`, Clippy on `wealthfolio-core` and
  `wealthfolio-storage-sqlite`, `cargo check` on both runtimes (the Tauri app
  does not build on the author's machine, so CI covers it).
- Frontend: vitest for the merge of history and projection points, the scale
  with projected values, and the tooltip. `pnpm type-check`, `pnpm check`,
  `i18n:check` and the adapter parity test.
- End to end on a throwaway server with seed data: create the example scenario
  from §1 with an anchor three months back, and check the dashboard line, the
  account line and the real-vs-projected figure, also with `?dir=rtl`.

## 12. Architecture impact

- **Added:** two tables, a core module (`crates/core/src/scenarios`), a service,
  eleven commands in both runtimes, a frontend feature, optional projection
  props on `HistoryChart` and the net-worth chart, three agent tools and the
  `scenarios:write` agent scope.
- **Unchanged:**
  - the portfolio engine, valuations and their recalculation lifecycle;
  - real activities (scenario rows never enter the activities table or the
    engine);
  - market-data and FX fetching (projection reads stored rates and quotes only,
    so no new network calls);
  - domain events (no new event);
  - device sync (the new tables are local-only).
- **Read-only dependencies:** valuation history, stored FX rates and quotes,
  alternative assets and their loan terms, an account's INTEREST and TAX
  activities (for suggestions), and account scope resolution.

## 13. Decisions

1. **Starting balances are read live** from the valuation history on every
   request. A later correction to history (a backfilled deposit, a repriced
   asset) moves the starting point with it. No snapshot is stored.
2. **Accounts the scenario does not mention are projected flat**, so the real
   and projected lines always cover the same accounts.
3. **Future window:** a chip next to the period selector picks how far right the
   chart runs while a scenario is active.
4. **Fork-only.** The feature lives on the fork and is not proposed upstream.
   Its tables do not exist in the official images, so this feature ends the
   option of going back to them with the same database.
