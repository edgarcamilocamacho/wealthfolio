import { useEffectOnChange } from "@/hooks/use-effect-on-change";
import type { Account, Scenario, ScenarioAssumptions, ScenarioGrowthModel } from "@/lib/types";
import {
  Button,
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
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

interface Props {
  scenario: Scenario;
  accounts: Account[];
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
    };
  }
  return { model: "flat" };
}

function draftsFor(scenario: Scenario, accounts: Account[]) {
  const accountDrafts: Record<string, AccountDraft> = {};
  for (const account of accounts) {
    accountDrafts[account.id] = accountDraft(scenario.assumptions.accounts[account.id]);
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
  baseCurrency,
  isSaving,
  onSave,
}: Props) {
  const { t } = useTranslation();
  const [{ accountDrafts, fxDrafts }, setDrafts] = useState(() => draftsFor(scenario, accounts));
  const [dirty, setDirty] = useState(false);

  useEffectOnChange(() => {
    setDrafts(draftsFor(scenario, accounts));
    setDirty(false);
  }, [scenario, accounts]);

  const modelItems = useMemo(
    () => [
      { value: "flat", label: t("scenarios:assumptions.model_flat") },
      { value: "rate", label: t("scenarios:assumptions.model_rate") },
    ],
    [t],
  );

  const currencies = useMemo(
    () =>
      [...new Set(accounts.map((account) => account.currency))]
        .filter((currency) => currency !== baseCurrency)
        .sort(),
    [accounts, baseCurrency],
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
          {accounts.map((account) => {
            const draft = accountDrafts[account.id] ?? { model: "flat" };
            return (
              <div key={account.id} className="space-y-3 p-3">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">{account.name}</p>
                    <p className="text-muted-foreground text-xs">{account.currency}</p>
                  </div>
                  <Select
                    items={modelItems}
                    value={draft.model}
                    onValueChange={(value) =>
                      setAccount(account.id, { model: value === "rate" ? "rate" : "flat" })
                    }
                  >
                    <SelectTrigger className="w-56">
                      <SelectValue />
                    </SelectTrigger>
                    <SelectContent>
                      {modelItems.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
                {draft.model === "rate" && (
                  <div className="grid gap-3 sm:grid-cols-3">
                    <PercentField
                      label={t("scenarios:assumptions.annual_rate")}
                      value={draft.annualPct}
                      onChange={(value) => setAccount(account.id, { annualPct: value })}
                    />
                    <PercentField
                      label={t("scenarios:assumptions.interest_tax")}
                      value={draft.interestTaxPct}
                      onChange={(value) => setAccount(account.id, { interestTaxPct: value })}
                    />
                    <PercentField
                      label={t("scenarios:assumptions.outflow_tax")}
                      value={draft.outflowTaxPct}
                      onChange={(value) => setAccount(account.id, { outflowTaxPct: value })}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>

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
