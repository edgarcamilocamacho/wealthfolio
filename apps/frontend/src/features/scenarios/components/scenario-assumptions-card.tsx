import { useEffectOnChange } from "@/hooks/use-effect-on-change";
import type { Account, Scenario, ScenarioAssumptions, ScenarioGrowthModel } from "@/lib/types";
import { format } from "date-fns";
import type { ScenarioPosition } from "../lib/scenario-positions";
import {
  Button,
  DatePickerInput,
  QuantityInput,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@wealthfolio/ui/components/ui/card";
import { suggestScenarioAccountGrowth } from "@/adapters";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

interface Props {
  scenario: Scenario;
  accounts: Account[];
  /** Alternative assets (properties, vehicles...) that can appreciate. */
  assets?: ScenarioPosition[];
  baseCurrency: string;
  isSaving: boolean;
  onSave: (assumptions: ScenarioAssumptions) => Promise<void>;
}

/** Rates are edited in percent and stored as fractions. */
interface AccountDraft {
  model: "flat" | "rate";
  annualPct?: number;
  interestTaxPct?: number;
  outflowTaxPct?: number;
  /** YYYY-MM-DD; growth starts on this day. */
  growthFrom?: string;
}

const toPct = (fraction: number) => Math.round(fraction * 1_000_000) / 10_000;
const toFraction = (pct: number | undefined) => (pct ?? 0) / 100;

function accountDraft(model: ScenarioGrowthModel | undefined): AccountDraft {
  if (model?.model === "rate") {
    return {
      model: "rate",
      annualPct: toPct(model.annualRate),
      interestTaxPct: toPct(model.interestTaxRate),
      outflowTaxPct: toPct(model.outflowTaxRate),
      growthFrom: model.growthFrom ?? undefined,
    };
  }
  return { model: "flat" };
}

function draftsFor(scenario: Scenario, positions: ScenarioPosition[]) {
  const accountDrafts: Record<string, AccountDraft> = {};
  for (const position of positions) {
    accountDrafts[position.id] = accountDraft(scenario.assumptions.accounts[position.id]);
  }
  const fxDrafts: Record<string, number | undefined> = {};
  for (const [currency, drift] of Object.entries(scenario.assumptions.fx)) {
    fxDrafts[currency] = toPct(drift);
  }
  return { accountDrafts, fxDrafts };
}

function PercentField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number | undefined;
  onChange: (value: number | undefined) => void;
}) {
  return (
    <label className="flex min-w-0 flex-col gap-1">
      <span className="text-muted-foreground text-xs">{label}</span>
      <div className="flex items-center gap-1">
        <QuantityInput
          value={value}
          allowNegative
          maxDecimalPlaces={4}
          onValueChange={(next: number | undefined) => onChange(next)}
        />
        <span className="text-muted-foreground text-sm">%</span>
      </div>
    </label>
  );
}

export function ScenarioAssumptionsCard({
  scenario,
  accounts,
  assets = [],
  baseCurrency,
  isSaving,
  onSave,
}: Props) {
  const { t } = useTranslation();
  const positions = useMemo<ScenarioPosition[]>(() => [...accounts, ...assets], [accounts, assets]);
  const [{ accountDrafts, fxDrafts }, setDrafts] = useState(() => draftsFor(scenario, positions));
  const [dirty, setDirty] = useState(false);

  useEffectOnChange(() => {
    setDrafts(draftsFor(scenario, positions));
    setDirty(false);
  }, [scenario, positions]);

  const modelItems = useMemo(
    () => [
      { value: "flat", label: t("scenarios:assumptions.model_flat") },
      { value: "rate", label: t("scenarios:assumptions.model_rate") },
    ],
    [t],
  );
  const assetModelItems = useMemo(
    () => [
      { value: "flat", label: t("scenarios:assumptions.model_flat") },
      { value: "rate", label: t("scenarios:assumptions.model_appreciation") },
    ],
    [t],
  );

  const currencies = useMemo(
    () =>
      [...new Set(positions.map((position) => position.currency))]
        .filter((currency) => currency !== baseCurrency)
        .sort(),
    [positions, baseCurrency],
  );

  const setAccount = (id: string, patch: Partial<AccountDraft>) => {
    setDrafts((current) => ({
      fxDrafts: current.fxDrafts,
      accountDrafts: { ...current.accountDrafts, [id]: { ...current.accountDrafts[id], ...patch } },
    }));
    setDirty(true);
  };
  const setFx = (currency: string, value: number | undefined) => {
    setDrafts((current) => ({
      accountDrafts: current.accountDrafts,
      fxDrafts: { ...current.fxDrafts, [currency]: value },
    }));
    setDirty(true);
  };

  const [suggestingId, setSuggestingId] = useState<string | null>(null);
  const handleSuggest = async (position: ScenarioPosition) => {
    setSuggestingId(position.id);
    try {
      const suggestion = await suggestScenarioAccountGrowth(position.id);
      if (!suggestion) {
        toast.info(t("scenarios:assumptions.suggest_none", { account: position.name }));
        return;
      }
      setAccount(position.id, {
        model: "rate",
        annualPct: toPct(suggestion.annualRate),
        interestTaxPct: toPct(suggestion.interestTaxRate),
      });
      toast.success(
        t("scenarios:assumptions.suggest_done", {
          account: position.name,
          count: suggestion.interestPayments,
        }),
      );
    } catch (error) {
      toast.error(error instanceof Error ? error.message : String(error));
    } finally {
      setSuggestingId(null);
    }
  };

  const handleSave = async () => {
    // Keep models for accounts this list does not show (archived, other purposes).
    const nextAccounts: Record<string, ScenarioGrowthModel> = { ...scenario.assumptions.accounts };
    for (const [id, draft] of Object.entries(accountDrafts)) {
      if (draft.model === "rate") {
        nextAccounts[id] = {
          model: "rate",
          annualRate: toFraction(draft.annualPct),
          interestTaxRate: toFraction(draft.interestTaxPct),
          outflowTaxRate: toFraction(draft.outflowTaxPct),
          ...(draft.growthFrom ? { growthFrom: draft.growthFrom } : {}),
        };
      } else {
        delete nextAccounts[id];
      }
    }
    const nextFx: Record<string, number> = {};
    for (const [currency, pct] of Object.entries(fxDrafts)) {
      if (pct !== undefined && pct !== 0) nextFx[currency] = toFraction(pct);
    }
    try {
      await onSave({ accounts: nextAccounts, fx: nextFx });
    } catch {
      // The mutation reports the error; the edits stay to try again.
    }
  };

  const renderPosition = (position: ScenarioPosition, isAsset: boolean) => {
    const draft = accountDrafts[position.id] ?? { model: "flat" };
    return (
      <div key={position.id} className="space-y-3 p-3">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div className="min-w-0">
            <p className="truncate text-sm font-medium">{position.name}</p>
            <p className="text-muted-foreground text-xs">{position.currency}</p>
          </div>
          <div className="flex items-center gap-2">
            {!isAsset && (
              <Button
                size="sm"
                variant="ghost"
                onClick={() => handleSuggest(position)}
                disabled={suggestingId !== null}
                title={t("scenarios:assumptions.suggest_hint")}
              >
                <Icons.Sparkles className="me-1 h-3.5 w-3.5" />
                {t("scenarios:assumptions.suggest")}
              </Button>
            )}
            <Select
              items={isAsset ? assetModelItems : modelItems}
              value={draft.model}
              onValueChange={(value) =>
                setAccount(position.id, { model: value === "rate" ? "rate" : "flat" })
              }
            >
              <SelectTrigger className="w-56">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {(isAsset ? assetModelItems : modelItems).map((item) => (
                  <SelectItem key={item.value} value={item.value}>
                    {item.label}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        </div>
        {draft.model === "rate" &&
          (isAsset ? (
            <div className="grid gap-3 sm:grid-cols-3">
              <PercentField
                label={t("scenarios:assumptions.appreciation")}
                value={draft.annualPct}
                onChange={(value) => setAccount(position.id, { annualPct: value })}
              />
              <label className="flex min-w-0 flex-col gap-1 sm:col-span-2">
                <span className="text-muted-foreground text-xs">
                  {t("scenarios:assumptions.growth_from")}
                </span>
                <DatePickerInput
                  value={draft.growthFrom}
                  onChange={(date) =>
                    setAccount(position.id, {
                      growthFrom: date ? format(date, "yyyy-MM-dd") : undefined,
                    })
                  }
                />
              </label>
            </div>
          ) : (
            <div className="grid gap-3 sm:grid-cols-3">
              <PercentField
                label={t("scenarios:assumptions.annual_rate")}
                value={draft.annualPct}
                onChange={(value) => setAccount(position.id, { annualPct: value })}
              />
              <PercentField
                label={t("scenarios:assumptions.interest_tax")}
                value={draft.interestTaxPct}
                onChange={(value) => setAccount(position.id, { interestTaxPct: value })}
              />
              <PercentField
                label={t("scenarios:assumptions.outflow_tax")}
                value={draft.outflowTaxPct}
                onChange={(value) => setAccount(position.id, { outflowTaxPct: value })}
              />
            </div>
          ))}
      </div>
    );
  };

  return (
    <Card>
      <CardHeader className="flex flex-row items-start justify-between gap-4">
        <div className="space-y-1">
          <CardTitle className="text-base">{t("scenarios:assumptions.title")}</CardTitle>
          <CardDescription>{t("scenarios:assumptions.description")}</CardDescription>
        </div>
        <Button size="sm" onClick={handleSave} disabled={!dirty || isSaving}>
          {t("common:save")}
        </Button>
      </CardHeader>
      <CardContent className="space-y-6">
        <div className="divide-y rounded-lg border">
          {accounts.map((account) => renderPosition(account, false))}
        </div>

        {assets.length > 0 && (
          <div className="space-y-3">
            <div>
              <p className="text-sm font-medium">{t("scenarios:assumptions.assets_title")}</p>
              <p className="text-muted-foreground text-xs">
                {t("scenarios:assumptions.assets_description")}
              </p>
            </div>
            <div className="divide-y rounded-lg border">
              {assets.map((asset) => renderPosition(asset, true))}
            </div>
          </div>
        )}

        {currencies.length > 0 && (
          <div className="space-y-3">
            <div>
              <p className="text-sm font-medium">{t("scenarios:assumptions.fx_title")}</p>
              <p className="text-muted-foreground text-xs">
                {t("scenarios:assumptions.fx_description", { currency: baseCurrency })}
              </p>
            </div>
            <div className="grid gap-3 sm:grid-cols-3">
              {currencies.map((currency) => (
                <PercentField
                  key={currency}
                  label={currency}
                  value={fxDrafts[currency]}
                  onChange={(value) => setFx(currency, value)}
                />
              ))}
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
