import { saveScenarioTransactions, updateToolResult } from "@/adapters";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { QueryKeys } from "@/lib/query-keys";
import type { SaveScenarioTransactionsRequest, ScenarioTransactionKind } from "@/lib/types";
import type { ToolCallMessagePartProps } from "@assistant-ui/react";
import { makeAssistantToolUI } from "@assistant-ui/react";
import { useQueryClient } from "@tanstack/react-query";
import {
  AmountDisplay,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Skeleton,
  Spinner,
  useDateFormatting,
} from "@wealthfolio/ui";
import { Alert, AlertTitle } from "@wealthfolio/ui/components/ui/alert";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { memo, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import { useRuntimeContext } from "../../hooks/use-runtime-context";

interface DraftSeries {
  label: string;
  kind: ScenarioTransactionKind;
  accountId: string;
  toAccountId?: string | null;
  amount: number;
  count: number;
  firstDate: string;
  lastDate: string;
}

interface DraftTransaction {
  date: string;
  kind: ScenarioTransactionKind;
  accountId: string;
  toAccountId?: string | null;
  amount: number;
  note?: string | null;
}

export interface DraftScenarioTransactionsOutput {
  scenarioId: string;
  scenarioName: string;
  anchorDate: string;
  endDate: string;
  series: DraftSeries[];
  transactions: DraftTransaction[];
  totalRows: number;
  accounts: { id: string; name: string; currency: string }[];
  request: SaveScenarioTransactionsRequest;
  /** Set once the user has added the draft to the scenario. */
  submitted?: boolean;
  createdCount?: number;
}

/** Reads the tool result, which may arrive as a JSON string or an object. */
export function parseDraftScenarioResult(result: unknown): DraftScenarioTransactionsOutput | null {
  if (!result) return null;
  if (typeof result === "string") {
    try {
      return parseDraftScenarioResult(JSON.parse(result));
    } catch {
      return null;
    }
  }
  if (typeof result !== "object") return null;
  const candidate = result as Partial<DraftScenarioTransactionsOutput>;
  if (typeof candidate.scenarioId !== "string" || !candidate.request) return null;
  return {
    ...candidate,
    series: candidate.series ?? [],
    transactions: candidate.transactions ?? [],
    accounts: candidate.accounts ?? [],
    totalRows: candidate.totalRows ?? 0,
  } as DraftScenarioTransactionsOutput;
}

type Props = ToolCallMessagePartProps<Record<string, unknown>, DraftScenarioTransactionsOutput>;

function DraftScenarioTransactionsContent({ result, status, toolCallId }: Props) {
  const { t } = useTranslation();
  const { formatDate } = useDateFormatting();
  const { isBalanceHidden } = useBalancePrivacy();
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const runtime = useRuntimeContext();
  const draft = useMemo(() => parseDraftScenarioResult(result), [result]);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [createdCount, setCreatedCount] = useState<number | null>(null);

  if (status?.type === "running") return <Skeleton className="h-40 w-full" />;
  if (status?.type === "incomplete" || !draft) {
    return (
      <Alert variant="destructive">
        <Icons.AlertCircle />
        <AlertTitle>{t("scenarios:assistant.failed")}</AlertTitle>
      </Alert>
    );
  }

  const accounts = new Map(draft.accounts.map((account) => [account.id, account]));
  const accountName = (id?: string | null) => (id ? (accounts.get(id)?.name ?? id) : "");
  const currencyOf = (id: string) => accounts.get(id)?.currency ?? "";
  const route = (row: {
    kind: ScenarioTransactionKind;
    accountId: string;
    toAccountId?: string | null;
  }) =>
    row.kind === "TRANSFER"
      ? t("scenarios:transactions.route", {
          from: accountName(row.accountId),
          to: accountName(row.toAccountId),
        })
      : accountName(row.accountId);

  const done = draft.submitted || createdCount !== null;
  const shownCount = createdCount ?? draft.createdCount ?? draft.totalRows;

  const handleSubmit = async () => {
    setIsSubmitting(true);
    setSubmitError(null);
    try {
      await saveScenarioTransactions(draft.scenarioId, draft.request);
      setCreatedCount(draft.totalRows);
      queryClient.invalidateQueries({ queryKey: QueryKeys.scenarioTransactions(draft.scenarioId) });
      queryClient.invalidateQueries({ queryKey: [QueryKeys.SCENARIO_PROJECTION] });
      if (runtime.currentThreadId && toolCallId) {
        try {
          await updateToolResult({
            threadId: runtime.currentThreadId,
            toolCallId,
            resultPatch: {
              submitted: true,
              createdCount: draft.totalRows,
              submittedAt: new Date().toISOString(),
            },
          });
        } catch (error) {
          console.error("Failed to persist draft_scenario_transactions tool state:", error);
        }
      }
    } catch (error) {
      setSubmitError(error instanceof Error ? error.message : t("scenarios:save_error"));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Card>
      <CardHeader className="pb-3">
        <CardTitle className="flex flex-wrap items-center gap-2 text-sm">
          <Icons.TrendingUp className="h-4 w-4" />
          {t("scenarios:assistant.title", { name: draft.scenarioName })}
        </CardTitle>
        <p className="text-muted-foreground text-xs">{t("scenarios:assistant.description")}</p>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="divide-y rounded-lg border text-sm">
          {draft.series.map((series, index) => (
            <div key={`series-${index}`} className="flex items-center gap-3 p-3">
              <Icons.RefreshCw className="text-muted-foreground h-4 w-4 shrink-0" />
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{series.label}</span>
                  <Badge variant="secondary">{t(`scenarios:kind.${series.kind}`)}</Badge>
                </div>
                <p className="text-muted-foreground truncate text-xs">
                  {route(series)} ·{" "}
                  {t("scenarios:transactions.series_summary", {
                    count: series.count,
                    first: formatDate(series.firstDate),
                    last: formatDate(series.lastDate),
                  })}
                </p>
              </div>
              <AmountDisplay
                value={series.amount}
                currency={currencyOf(series.accountId)}
                isHidden={isBalanceHidden}
              />
            </div>
          ))}
          {draft.transactions.map((row, index) => (
            <div key={`single-${index}`} className="flex items-center gap-3 p-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{formatDate(row.date)}</span>
                  <Badge variant="secondary">{t(`scenarios:kind.${row.kind}`)}</Badge>
                </div>
                <p className="text-muted-foreground truncate text-xs">
                  {route(row)}
                  {row.note ? ` · ${row.note}` : ""}
                </p>
              </div>
              <AmountDisplay
                value={row.amount}
                currency={currencyOf(row.accountId)}
                isHidden={isBalanceHidden}
              />
            </div>
          ))}
        </div>

        {submitError && <p className="text-destructive text-xs">{submitError}</p>}

        <div className="flex flex-wrap items-center justify-end gap-2">
          {done ? (
            <>
              <span className="text-muted-foreground text-xs">
                {t("scenarios:assistant.added", { count: shownCount })}
              </span>
              <Button
                size="sm"
                variant="outline"
                onClick={() => navigate(`/scenarios/${draft.scenarioId}`)}
              >
                {t("scenarios:assistant.open")}
              </Button>
            </>
          ) : (
            <Button size="sm" onClick={handleSubmit} disabled={isSubmitting}>
              {isSubmitting ? (
                <Spinner className="me-2 h-4 w-4" />
              ) : (
                <Icons.Check className="me-2 h-4 w-4" />
              )}
              {t("scenarios:assistant.add", { count: draft.totalRows })}
            </Button>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

const DraftScenarioTransactionsToolUIContent = memo(DraftScenarioTransactionsContent);

export const DraftScenarioTransactionsToolUI = makeAssistantToolUI<
  Record<string, unknown>,
  DraftScenarioTransactionsOutput
>({
  toolName: "draft_scenario_transactions",
  render: (props) => <DraftScenarioTransactionsToolUIContent {...props} />,
});
