import { useEffectOnChange } from "@/hooks/use-effect-on-change";
import type { Scenario, ScenarioInput } from "@/lib/types";
import { Button, DatePickerInput, Input, Label } from "@wealthfolio/ui";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@wealthfolio/ui/components/ui/dialog";
import { Textarea } from "@wealthfolio/ui/components/ui/textarea";
import { addYears, format } from "date-fns";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { parseLocalDate } from "@/lib/utils";

const DEFAULT_HORIZON_YEARS = 5;

interface Props {
  open: boolean;
  onClose: () => void;
  /** Today in the app's time zone (YYYY-MM-DD). The anchor cannot be later. */
  todayISO: string;
  /** Present when editing; absent when creating. */
  scenario?: Scenario;
  isSaving: boolean;
  onSubmit: (input: ScenarioInput) => Promise<void>;
}

function toDateKey(date: Date | undefined): string {
  return date ? format(date, "yyyy-MM-dd") : "";
}

function defaultEndDate(anchorDate: string): string {
  return format(addYears(parseLocalDate(anchorDate), DEFAULT_HORIZON_YEARS), "yyyy-MM-dd");
}

export function ScenarioFormDialog({
  open,
  onClose,
  todayISO,
  scenario,
  isSaving,
  onSubmit,
}: Props) {
  const { t } = useTranslation();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [anchorDate, setAnchorDate] = useState(todayISO);
  const [endDate, setEndDate] = useState(defaultEndDate(todayISO));

  useEffectOnChange(() => {
    if (!open) return;
    setName(scenario?.name ?? "");
    setDescription(scenario?.description ?? "");
    setAnchorDate(scenario?.anchorDate ?? todayISO);
    setEndDate(scenario?.endDate ?? defaultEndDate(todayISO));
  }, [open, scenario, todayISO]);

  const trimmedName = name.trim();
  const anchorError = anchorDate > todayISO ? t("scenarios:form.anchor_after_today") : null;
  const endError =
    endDate && anchorDate && endDate <= anchorDate ? t("scenarios:form.end_before_anchor") : null;
  const canSubmit =
    Boolean(trimmedName && anchorDate && endDate) && !anchorError && !endError && !isSaving;

  const handleSubmit = async (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!canSubmit) return;
    try {
      await onSubmit({
        name: trimmedName,
        description: description.trim() || null,
        anchorDate,
        endDate,
        assumptions: scenario?.assumptions ?? { accounts: {}, fx: {} },
      });
      onClose();
    } catch {
      // The mutation reports the error; the dialog stays open to try again.
    }
  };

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
            <DialogTitle>
              {scenario ? t("scenarios:form.edit_title") : t("scenarios:form.create_title")}
            </DialogTitle>
            <DialogDescription>{t("scenarios:form.description")}</DialogDescription>
          </DialogHeader>

          <div className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="scenario-name">{t("scenarios:form.name")}</Label>
              <Input
                id="scenario-name"
                value={name}
                onChange={(event) => setName(event.target.value)}
                placeholder={t("scenarios:form.name_placeholder")}
                maxLength={100}
                autoFocus
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="scenario-description">{t("scenarios:form.notes")}</Label>
              <Textarea
                id="scenario-description"
                value={description}
                onChange={(event) => setDescription(event.target.value)}
                maxLength={1000}
                rows={2}
              />
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
              <div className="space-y-2">
                <Label>{t("scenarios:form.anchor_date")}</Label>
                <DatePickerInput
                  value={anchorDate}
                  onChange={(date) => setAnchorDate(toDateKey(date))}
                />
                <p
                  className={
                    anchorError ? "text-destructive text-xs" : "text-muted-foreground text-xs"
                  }
                >
                  {anchorError ?? t("scenarios:form.anchor_hint")}
                </p>
              </div>
              <div className="space-y-2">
                <Label>{t("scenarios:form.end_date")}</Label>
                <DatePickerInput value={endDate} onChange={(date) => setEndDate(toDateKey(date))} />
                {endError && <p className="text-destructive text-xs">{endError}</p>}
              </div>
            </div>
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={onClose}>
              {t("common:cancel")}
            </Button>
            <Button type="submit" disabled={!canSubmit}>
              {scenario ? t("common:save") : t("scenarios:form.create")}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}
