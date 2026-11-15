import { formatZonedDateKey } from "@/features/spending/lib/timezone";
import { useAccounts } from "@/hooks/use-accounts";
import { useAlternativeHoldings, useNetWorthHistory } from "@/hooks/use-alternative-assets";
import { useBalancePrivacy } from "@/hooks/use-balance-privacy";
import { useValuationHistory } from "@/hooks/use-valuation-history";
import { AccountPurpose } from "@/lib/constants";
import { useSettingsContext } from "@/lib/settings-provider";
import type {
  SaveScenarioTransactionsRequest,
  ScenarioAssumptions,
  ScenarioInput,
  ScenarioWarning,
} from "@/lib/types";
import { parseLocalDate } from "@/lib/utils";
import { useBackNavigation } from "@/pages/layouts/mobile-stack/use-mobile-stack";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
  Button,
  Page,
  PageContent,
  PageHeader,
  Skeleton,
  useDateFormatting,
} from "@wealthfolio/ui";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate, useParams } from "react-router-dom";
import { ScenarioAssumptionsCard } from "../components/scenario-assumptions-card";
import { ScenarioFormDialog } from "../components/scenario-form-dialog";
import { ScenarioPreviewChart } from "../components/scenario-preview-chart";
import {
  ScenarioTransactionDialog,
  type ScenarioTransactionTarget,
} from "../components/scenario-transaction-dialog";
import { ScenarioTransactionsCard } from "../components/scenario-transactions-card";
import {
  useScenario,
  useScenarioMutations,
  useScenarioNetWorthProjection,
  useScenarioProjection,
  useScenarioTransactions,
} from "../hooks/use-scenarios";
import type { ScenarioPosition } from "../lib/scenario-positions";

const ALL_ACCOUNTS = { type: "all" } as const;

export default function ScenarioDetailPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const goBack = useBackNavigation("/scenarios");
  const { scenarioId } = useParams<{ scenarioId: string }>();
  const { formatDate } = useDateFormatting();
  const { settings } = useSettingsContext();
  const { isBalanceHidden } = useBalancePrivacy();
  const baseCurrency = settings?.baseCurrency ?? "USD";
  const todayISO = formatZonedDateKey(new Date(), settings?.timezone);

  const { data: scenario, isLoading, error } = useScenario(scenarioId);
  const { data: transactions = [] } = useScenarioTransactions(scenarioId);
  const { accounts } = useAccounts({ accountPurpose: AccountPurpose.HOLDINGS });
  const { data: alternativeHoldings } = useAlternativeHoldings();
  const assets = useMemo<ScenarioPosition[]>(
    () =>
      (alternativeHoldings ?? [])
        .filter((holding) => holding.kind.toLowerCase() !== "liability")
        .map((holding) => ({ id: holding.id, name: holding.name, currency: holding.currency })),
    [alternativeHoldings],
  );
  const [previewKind, setPreviewKind] = useState<"accounts" | "netWorth">("accounts");
  const isNetWorthPreview = previewKind === "netWorth";
  const accountsProjection = useScenarioProjection(scenarioId, ALL_ACCOUNTS);
  const netWorthProjection = useScenarioNetWorthProjection(
    isNetWorthPreview ? scenarioId : undefined,
  );
  const { data: projection, isLoading: isProjectionLoading } = isNetWorthPreview
    ? netWorthProjection
    : accountsProjection;
  const historyRange = useMemo(
    () =>
      scenario
        ? { from: parseLocalDate(scenario.anchorDate), to: parseLocalDate(todayISO) }
        : undefined,
    [scenario, todayISO],
  );
  const { valuationHistory, isLoading: isValuationLoading } = useValuationHistory(
    historyRange,
    ALL_ACCOUNTS,
    { enabled: Boolean(scenario) && !isNetWorthPreview },
  );
  const { data: netWorthHistory, isLoading: isNetWorthLoading } = useNetWorthHistory({
    startDate: scenario?.anchorDate ?? "",
    endDate: todayISO,
    enabled: Boolean(scenario) && isNetWorthPreview,
  });
  const isHistoryLoading = isNetWorthPreview ? isNetWorthLoading : isValuationLoading;

  const { updateMutation, deleteMutation, duplicateMutation, saveTransactionsMutation } =
    useScenarioMutations();
  const [editOpen, setEditOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [transactionTarget, setTransactionTarget] = useState<ScenarioTransactionTarget | null>(
    null,
  );

  const real = useMemo(
    () =>
      isNetWorthPreview
        ? (netWorthHistory ?? []).map((point) => ({
            date: point.date.slice(0, 10),
            value: point.netWorth,
          }))
        : (valuationHistory ?? [])
            .filter((row) => row.valueStatus !== "unavailable")
            .map((row) => ({ date: row.valuationDate.slice(0, 10), value: row.totalValueBase })),
    [isNetWorthPreview, netWorthHistory, valuationHistory],
  );
  const projected = useMemo(
    () => (projection?.points ?? []).map((point) => ({ date: point.date, value: point.value })),
    [projection],
  );
  const accountNames = useMemo(
    () => new Map([...accounts, ...assets].map((position) => [position.id, position.name])),
    [accounts, assets],
  );

  if (isLoading) {
    return (
      <Page>
        <PageHeader heading={t("scenarios:title")} onBack={goBack} />
        <PageContent>
          <Skeleton className="h-64 w-full" />
        </PageContent>
      </Page>
    );
  }

  if (error || !scenario) {
    return (
      <Page>
        <PageHeader heading={t("scenarios:title")} onBack={goBack} />
        <PageContent>
          <div className="flex flex-col items-center gap-4 py-24 text-center">
            <p className="text-muted-foreground">{t("scenarios:not_found")}</p>
            <Button onClick={goBack}>{t("scenarios:back_to_list")}</Button>
          </div>
        </PageContent>
      </Page>
    );
  }

  const saveInput = (patch: Partial<ScenarioInput>): ScenarioInput => ({
    name: scenario.name,
    description: scenario.description ?? null,
    anchorDate: scenario.anchorDate,
    endDate: scenario.endDate,
    assumptions: scenario.assumptions,
    ...patch,
  });

  const handleEdit = async (input: ScenarioInput) => {
    await updateMutation.mutateAsync({ id: scenario.id, input });
  };
  const handleSaveAssumptions = async (assumptions: ScenarioAssumptions) => {
    await updateMutation.mutateAsync({ id: scenario.id, input: saveInput({ assumptions }) });
  };
  const handleSaveTransactions = async (request: SaveScenarioTransactionsRequest) => {
    await saveTransactionsMutation.mutateAsync({ id: scenario.id, request });
  };
  const handleDeleteTransactions = (ids: string[]) => {
    saveTransactionsMutation.mutate({ id: scenario.id, request: { deleteIds: ids } });
  };
  const handleDuplicate = async () => {
    const copy = await duplicateMutation.mutateAsync({
      id: scenario.id,
      name: t("scenarios:copy_name", { name: scenario.name }).slice(0, 100),
    });
    navigate(`/scenarios/${copy.id}`);
  };
  const handleDelete = async () => {
    await deleteMutation.mutateAsync(scenario.id);
    setDeleteOpen(false);
    navigate("/scenarios");
  };

  const warningText = (warning: ScenarioWarning) =>
    t(`scenarios:warnings.${warning.code}`, {
      account: warning.accountId ? (accountNames.get(warning.accountId) ?? warning.accountId) : "",
      currency: warning.currency ?? "",
      date: warning.date ? formatDate(warning.date) : "",
    });

  const actions = (
    <div className="flex items-center gap-2">
      <Button size="sm" variant="outline" onClick={() => setEditOpen(true)}>
        <Icons.Pencil className="me-1 h-4 w-4" />
        {t("common:edit")}
      </Button>
      <Button
        size="sm"
        variant="outline"
        onClick={handleDuplicate}
        disabled={duplicateMutation.isPending}
      >
        <Icons.Copy className="me-1 h-4 w-4" />
        {t("scenarios:duplicate")}
      </Button>
      <Button
        size="sm"
        variant="outline"
        onClick={() => setDeleteOpen(true)}
        aria-label={t("common:delete")}
      >
        <Icons.Trash className="h-4 w-4" />
      </Button>
    </div>
  );

  return (
    <Page>
      <PageHeader
        heading={scenario.name}
        text={t("scenarios:date_range", {
          start: formatDate(scenario.anchorDate),
          end: formatDate(scenario.endDate),
        })}
        onBack={goBack}
        actions={actions}
      />
      <PageContent className="space-y-6">
        {scenario.description && (
          <p className="text-muted-foreground text-sm">{scenario.description}</p>
        )}

        {projection && projection.warnings.length > 0 && (
          <div className="space-y-1 rounded-lg border border-amber-500/40 bg-amber-500/5 p-3 text-sm">
            {projection.warnings.map((warning, index) => (
              <p key={`${warning.code}-${index}`} className="flex items-start gap-2">
                <Icons.AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-amber-600" />
                <span>{warningText(warning)}</span>
              </p>
            ))}
          </div>
        )}

        <ScenarioPreviewChart
          real={real}
          projected={projected}
          currency={projection?.currency ?? baseCurrency}
          isBalanceHidden={isBalanceHidden}
          isLoading={isProjectionLoading || isHistoryLoading}
          kind={previewKind}
          onKindChange={setPreviewKind}
        />

        <ScenarioTransactionsCard
          scenario={scenario}
          transactions={transactions}
          accounts={accounts}
          assets={assets}
          isBalanceHidden={isBalanceHidden}
          isSaving={saveTransactionsMutation.isPending}
          onEdit={setTransactionTarget}
          onDelete={handleDeleteTransactions}
        />

        <ScenarioAssumptionsCard
          scenario={scenario}
          accounts={accounts}
          assets={assets}
          baseCurrency={baseCurrency}
          isSaving={updateMutation.isPending}
          onSave={handleSaveAssumptions}
        />
      </PageContent>

      <ScenarioFormDialog
        open={editOpen}
        onClose={() => setEditOpen(false)}
        todayISO={todayISO}
        scenario={scenario}
        isSaving={updateMutation.isPending}
        onSubmit={handleEdit}
      />
      {transactionTarget && (
        <ScenarioTransactionDialog
          open
          onClose={() => setTransactionTarget(null)}
          scenario={scenario}
          accounts={accounts}
          assets={assets}
          target={transactionTarget}
          isSaving={saveTransactionsMutation.isPending}
          onSubmit={handleSaveTransactions}
        />
      )}
      <AlertDialog open={deleteOpen} onOpenChange={setDeleteOpen}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("scenarios:delete_title")}</AlertDialogTitle>
            <AlertDialogDescription>
              {t("scenarios:delete_confirm", { name: scenario.name })}
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel disabled={deleteMutation.isPending}>
              {t("common:cancel")}
            </AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              disabled={deleteMutation.isPending}
              variant="destructive"
            >
              {t("common:delete")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Page>
  );
}
