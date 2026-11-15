import type { Account, Scenario, ScenarioTransaction } from "@/lib/types";
import { cn } from "@/lib/utils";
import { AmountDisplay, Button, useDateFormatting } from "@wealthfolio/ui";
import { Badge } from "@wealthfolio/ui/components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@wealthfolio/ui/components/ui/card";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import {
  groupScenarioTransactions,
  isBeforeAnchor,
  type ScenarioSeriesGroup,
} from "../lib/scenario-transactions";
import type { ScenarioTransactionTarget } from "./scenario-transaction-dialog";

interface Props {
  scenario: Scenario;
  transactions: ScenarioTransaction[];
  accounts: Account[];
  isBalanceHidden: boolean;
  isSaving: boolean;
  onEdit: (target: ScenarioTransactionTarget) => void;
  onDelete: (ids: string[]) => void;
}

function RowActions({
  onEdit,
  onDelete,
  disabled,
}: {
  onEdit: () => void;
  onDelete: () => void;
  disabled: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex shrink-0 items-center gap-1">
      <Button variant="ghost" size="icon" onClick={onEdit} aria-label={t("common:edit")}>
        <Icons.Pencil className="h-4 w-4" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        onClick={onDelete}
        disabled={disabled}
        aria-label={t("common:delete")}
      >
        <Icons.Trash className="h-4 w-4" />
      </Button>
    </div>
  );
}

export function ScenarioTransactionsCard({
  scenario,
  transactions,
  accounts,
  isBalanceHidden,
  isSaving,
  onEdit,
  onDelete,
}: Props) {
  const { t } = useTranslation();
  const { formatDate } = useDateFormatting();
  const grouped = useMemo(() => groupScenarioTransactions(transactions), [transactions]);
  const accountsById = useMemo(
    () => new Map(accounts.map((account) => [account.id, account])),
    [accounts],
  );

  const accountName = (id: string | null | undefined) =>
    id ? (accountsById.get(id)?.name ?? t("scenarios:transactions.unknown_account")) : "";
  const currencyOf = (id: string) => accountsById.get(id)?.currency ?? "";

  const route = (row: ScenarioTransaction) =>
    row.kind === "TRANSFER"
      ? t("scenarios:transactions.route", {
          from: accountName(row.accountId),
          to: accountName(row.toAccountId),
        })
      : accountName(row.accountId);

  const seriesSummary = (group: ScenarioSeriesGroup) =>
    t("scenarios:transactions.series_summary", {
      count: group.rows.length,
      first: formatDate(group.firstDate),
      last: formatDate(group.lastDate),
    });

  const isEmpty = transactions.length === 0;

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-4">
        <div className="space-y-1">
          <CardTitle className="text-base">{t("scenarios:transactions.title")}</CardTitle>
          <CardDescription>{t("scenarios:transactions.description")}</CardDescription>
        </div>
        <Button size="sm" onClick={() => onEdit({ type: "new" })} disabled={accounts.length === 0}>
          <Icons.Plus className="me-1 h-4 w-4" />
          {t("scenarios:transactions.add")}
        </Button>
      </CardHeader>
      <CardContent className="space-y-4">
        {isEmpty && (
          <p className="text-muted-foreground py-6 text-center text-sm">
            {t("scenarios:transactions.empty")}
          </p>
        )}

        {grouped.series.length > 0 && (
          <div className="divide-y rounded-lg border">
            {grouped.series.map((group) => {
              const first = group.rows[0];
              return (
                <div key={group.seriesId} className="flex items-center gap-3 p-3">
                  <Icons.RefreshCw className="text-muted-foreground h-4 w-4 shrink-0" />
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="truncate text-sm font-medium">{group.label}</p>
                      <Badge variant="secondary">{t(`scenarios:kind.${first.kind}`)}</Badge>
                    </div>
                    <p className="text-muted-foreground truncate text-xs">
                      {route(first)} · {seriesSummary(group)}
                    </p>
                  </div>
                  <div className="text-end text-sm">
                    {group.amount === null ? (
                      <span className="text-muted-foreground text-xs">
                        {t("scenarios:transactions.varied_amounts")}
                      </span>
                    ) : (
                      <AmountDisplay
                        value={group.amount}
                        currency={currencyOf(first.accountId)}
                        isHidden={isBalanceHidden}
                      />
                    )}
                  </div>
                  <RowActions
                    onEdit={() => onEdit({ type: "series", group })}
                    onDelete={() => onDelete(group.rows.map((row) => row.id))}
                    disabled={isSaving}
                  />
                </div>
              );
            })}
          </div>
        )}

        {grouped.singles.length > 0 && (
          <div className="divide-y rounded-lg border">
            {grouped.singles.map((row) => {
              const ignored = isBeforeAnchor(row.date, scenario.anchorDate);
              return (
                <div
                  key={row.id}
                  className={cn("flex items-center gap-3 p-3", ignored && "opacity-50")}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-2">
                      <p className="text-sm font-medium">{formatDate(row.date)}</p>
                      <Badge variant="secondary">{t(`scenarios:kind.${row.kind}`)}</Badge>
                      {ignored && (
                        <span className="text-muted-foreground text-xs">
                          {t("scenarios:transactions.before_anchor")}
                        </span>
                      )}
                    </div>
                    <p className="text-muted-foreground truncate text-xs">
                      {route(row)}
                      {row.note ? ` · ${row.note}` : ""}
                    </p>
                  </div>
                  <div className="text-end text-sm">
                    <AmountDisplay
                      value={row.amount}
                      currency={currencyOf(row.accountId)}
                      isHidden={isBalanceHidden}
                    />
                  </div>
                  <RowActions
                    onEdit={() => onEdit({ type: "single", row })}
                    onDelete={() => onDelete([row.id])}
                    disabled={isSaving}
                  />
                </div>
              );
            })}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
