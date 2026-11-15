import { useEffectOnChange } from "@/hooks/use-effect-on-change";
import type {
  Account,
  SaveScenarioTransactionsRequest,
  Scenario,
  ScenarioTransaction,
  ScenarioTransactionInput,
  ScenarioTransactionKind,
} from "@/lib/types";
import {
  Button,
  DatePickerInput,
  Input,
  Label,
  MoneyInput,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@wealthfolio/ui/components/ui/dialog";
import { Switch } from "@wealthfolio/ui/components/ui/switch";
import { addMonths, format } from "date-fns";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { parseLocalDate } from "@/lib/utils";
import { dayOfMonthOf, type ScenarioSeriesGroup } from "../lib/scenario-transactions";

/** What the dialog edits: nothing (a new row), one row, or a whole series. */
export type ScenarioTransactionTarget =
  | { type: "new" }
  | { type: "single"; row: ScenarioTransaction }
  | { type: "series"; group: ScenarioSeriesGroup };

interface Props {
  open: boolean;
  onClose: () => void;
  scenario: Scenario;
  accounts: Account[];
  target: ScenarioTransactionTarget;
  isSaving: boolean;
  onSubmit: (request: SaveScenarioTransactionsRequest) => Promise<void>;
}

function toDateKey(date: Date | undefined): string {
  return date ? format(date, "yyyy-MM-dd") : "";
}

interface DraftState {
  kind: ScenarioTransactionKind;
  accountId: string;
  toAccountId: string;
  amount: number | undefined;
  toAmount: number | undefined;
  note: string;
  repeat: boolean;
  label: string;
  /** The single row's date, or the series start date. */
  date: string;
  /** The series end date. */
  untilDate: string;
}

function firstFutureDate(scenario: Scenario): string {
  return format(addMonths(parseLocalDate(scenario.anchorDate), 1), "yyyy-MM-dd");
}

function draftFor(target: ScenarioTransactionTarget, scenario: Scenario): DraftState {
  const source =
    target.type === "single" ? target.row : target.type === "series" ? target.group.rows[0] : null;
  return {
    kind: source?.kind ?? "DEPOSIT",
    accountId: source?.accountId ?? "",
    toAccountId: source?.toAccountId ?? "",
    amount: source?.amount,
    toAmount: source?.toAmount ?? undefined,
    note: source?.note ?? "",
    repeat: target.type === "series",
    label: target.type === "series" ? target.group.label : "",
    date:
      target.type === "series"
        ? target.group.firstDate
        : (source?.date ?? firstFutureDate(scenario)),
    untilDate: target.type === "series" ? target.group.lastDate : scenario.endDate,
  };
}

export function ScenarioTransactionDialog({
  open,
  onClose,
  scenario,
  accounts,
  target,
  isSaving,
  onSubmit,
}: Props) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState<DraftState>(() => draftFor(target, scenario));

  useEffectOnChange(() => {
    if (!open) return;
    setDraft(draftFor(target, scenario));
  }, [open, target, scenario]);

  const update = (patch: Partial<DraftState>) => setDraft((current) => ({ ...current, ...patch }));

  const kindItems = useMemo(
    () => [
      { value: "DEPOSIT", label: t("scenarios:kind.DEPOSIT") },
      { value: "WITHDRAWAL", label: t("scenarios:kind.WITHDRAWAL") },
      { value: "TRANSFER", label: t("scenarios:kind.TRANSFER") },
    ],
    [t],
  );
  const accountItems = useMemo(
    () =>
      accounts.map((account) => ({
        value: account.id,
        label: `${account.name} (${account.currency})`,
      })),
    [accounts],
  );

  const isTransfer = draft.kind === "TRANSFER";
  const fromAccount = accounts.find((account) => account.id === draft.accountId);
  const toAccount = accounts.find((account) => account.id === draft.toAccountId);
  const crossCurrency = Boolean(
    isTransfer && fromAccount && toAccount && fromAccount.currency !== toAccount.currency,
  );

  const errors: string[] = [];
  if (!draft.accountId) errors.push(t("scenarios:transaction.error_account"));
  if (isTransfer && (!draft.toAccountId || draft.toAccountId === draft.accountId)) {
    errors.push(t("scenarios:transaction.error_destination"));
  }
  if (!draft.amount || draft.amount <= 0) errors.push(t("scenarios:transaction.error_amount"));
  if (!draft.date) errors.push(t("scenarios:transaction.error_date"));
  if (draft.repeat && !draft.label.trim()) errors.push(t("scenarios:transaction.error_label"));
  if (draft.repeat && (!draft.untilDate || draft.untilDate < draft.date)) {
    errors.push(t("scenarios:transaction.error_until"));
  }
  const canSubmit = errors.length === 0 && !isSaving;
  const beforeAnchor = draft.date !== "" && draft.date <= scenario.anchorDate;

  const buildInput = (): ScenarioTransactionInput => ({
    date: draft.date,
    kind: draft.kind,
    accountId: draft.accountId,
    toAccountId: isTransfer ? draft.toAccountId : null,
    amount: draft.amount ?? 0,
    toAmount: crossCurrency ? (draft.toAmount ?? null) : null,
    note: draft.note.trim() || null,
  });

  const buildRequest = (): SaveScenarioTransactionsRequest => {
    const input = buildInput();
    if (!draft.repeat) {
      if (target.type === "single") return { updates: [{ ...input, id: target.row.id }] };
      if (target.type === "series") {
        return { creates: [input], deleteIds: target.group.rows.map((row) => row.id) };
      }
      return { creates: [input] };
    }
    const { date: startDate, ...fields } = input;
    const rule = {
      ...fields,
      label: draft.label.trim(),
      dayOfMonth: dayOfMonthOf(startDate),
      startDate,
      endDate: draft.untilDate,
      replaceSeriesId: target.type === "series" ? target.group.seriesId : null,
    };
    if (target.type === "single") return { series: [rule], deleteIds: [target.row.id] };
    return { series: [rule] };
  };

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) return;
    try {
      await onSubmit(buildRequest());
      onClose();
    } catch {
      // The mutation reports the error; the dialog stays open to try again.
    }
  };

  const title =
    target.type === "new"
      ? t("scenarios:transaction.add_title")
      : t("scenarios:transaction.edit_title");

  return (
    <Dialog
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen) onClose();
      }}
    >
      <DialogContent className="sm:max-w-lg">
        <form onSubmit={handleSubmit} className="space-y-6">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>{t("scenarios:transaction.description")}</DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <Label>{t("scenarios:transaction.kind")}</Label>
              <Select
                items={kindItems}
                value={draft.kind}
                onValueChange={(value) => {
                  if (value) update({ kind: value as ScenarioTransactionKind });
                }}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {kindItems.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            <div className={isTransfer ? "grid gap-4 sm:grid-cols-2" : "space-y-2"}>
              <div className="space-y-2">
                <Label>
                  {isTransfer ? t("scenarios:transaction.from_account") : t("common:account")}
                </Label>
                <Select
                  items={accountItems}
                  value={draft.accountId}
                  onValueChange={(value) => update({ accountId: String(value ?? "") })}
                >
                  <SelectTrigger>
                    <SelectValue placeholder={t("scenarios:transaction.select_account")} />
                  </SelectTrigger>
                  <SelectContent>
                    {accountItems.map((item) => (
                      <SelectItem key={item.value} value={item.value}>
                        {item.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              {isTransfer && (
                <div className="space-y-2">
                  <Label>{t("scenarios:transaction.to_account")}</Label>
                  <Select
                    items={accountItems}
                    value={draft.toAccountId}
                    onValueChange={(value) => update({ toAccountId: String(value ?? "") })}
                  >
                    <SelectTrigger>
                      <SelectValue placeholder={t("scenarios:transaction.select_account")} />
                    </SelectTrigger>
                    <SelectContent>
                      {accountItems.map((item) => (
                        <SelectItem key={item.value} value={item.value}>
                          {item.label}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )}
            </div>

            <div className={crossCurrency ? "grid gap-4 sm:grid-cols-2" : "space-y-2"}>
              <div className="space-y-2">
                <Label>
                  {fromAccount
                    ? t("scenarios:transaction.amount_in", { currency: fromAccount.currency })
                    : t("common:amount")}
                </Label>
                <MoneyInput
                  value={draft.amount}
                  onValueChange={(value: number | undefined) => update({ amount: value })}
                />
              </div>
              {crossCurrency && toAccount && (
                <div className="space-y-2">
                  <Label>
                    {t("scenarios:transaction.received_in", { currency: toAccount.currency })}
                  </Label>
                  <MoneyInput
                    value={draft.toAmount}
                    onValueChange={(value: number | undefined) => update({ toAmount: value })}
                  />
                  <p className="text-muted-foreground text-xs">
                    {t("scenarios:transaction.received_hint")}
                  </p>
                </div>
              )}
            </div>

            <div className="flex items-center justify-between gap-3 rounded-lg border p-3">
              <div>
                <p className="text-sm font-medium">{t("scenarios:transaction.repeat")}</p>
                <p className="text-muted-foreground text-xs">
                  {t("scenarios:transaction.repeat_hint")}
                </p>
              </div>
              <Switch
                checked={draft.repeat}
                onCheckedChange={(checked) => update({ repeat: checked })}
              />
            </div>

            {draft.repeat && (
              <div className="space-y-2">
                <Label htmlFor="scenario-series-label">{t("scenarios:transaction.label")}</Label>
                <Input
                  id="scenario-series-label"
                  value={draft.label}
                  onChange={(event) => update({ label: event.target.value })}
                  placeholder={t("scenarios:transaction.label_placeholder")}
                  maxLength={100}
                />
              </div>
            )}

            <div className={draft.repeat ? "grid gap-4 sm:grid-cols-2" : "space-y-2"}>
              <div className="space-y-2">
                <Label>
                  {draft.repeat ? t("scenarios:transaction.first_date") : t("common:date")}
                </Label>
                <DatePickerInput
                  value={draft.date}
                  onChange={(date) => update({ date: toDateKey(date) })}
                />
              </div>
              {draft.repeat && (
                <div className="space-y-2">
                  <Label>{t("scenarios:transaction.until_date")}</Label>
                  <DatePickerInput
                    value={draft.untilDate}
                    onChange={(date) => update({ untilDate: toDateKey(date) })}
                  />
                </div>
              )}
            </div>
            {beforeAnchor && (
              <p className="text-muted-foreground text-xs">
                {t("scenarios:transaction.before_anchor_hint")}
              </p>
            )}

            <div className="space-y-2">
              <Label htmlFor="scenario-transaction-note">{t("scenarios:transaction.note")}</Label>
              <Input
                id="scenario-transaction-note"
                value={draft.note}
                onChange={(event) => update({ note: event.target.value })}
                maxLength={500}
              />
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t("common:cancel")}
            </Button>
            <Button type="submit" disabled={!canSubmit} title={errors[0]}>
              {t("common:save")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
