import { cn } from "@/lib/utils";
import {
  GainPercent,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@wealthfolio/ui";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import type { ScenarioOverlayState } from "../hooks/use-scenario-overlay";
import { SCENARIO_FUTURE_WINDOWS, type ScenarioFutureWindow } from "../lib/scenario-overlay";

const NO_SCENARIO = "__none__";

interface Props {
  overlay: ScenarioOverlayState;
  /** Real vs projected on the latest day with both, as a ratio. */
  deviation?: number | null;
  className?: string;
}

/** Picks the scenario drawn over a history chart and how far ahead it runs. */
export function ScenarioOverlayControl({ overlay, deviation, className }: Props) {
  const { t } = useTranslation();
  const { scenarios, activeScenario, setActiveScenarioId, futureWindow, setFutureWindow } = overlay;

  const scenarioItems = useMemo(
    () => [
      { value: NO_SCENARIO, label: t("scenarios:overlay.none") },
      ...scenarios.map((scenario) => ({ value: scenario.id, label: scenario.name })),
    ],
    [scenarios, t],
  );
  const windowItems = useMemo(
    () =>
      SCENARIO_FUTURE_WINDOWS.map((value) => ({
        value,
        label: t(`scenarios:overlay.window_${value}`),
      })),
    [t],
  );

  if (scenarios.length === 0) return null;

  return (
    <div className={cn("flex flex-wrap items-center justify-center gap-2", className)}>
      <Select
        items={scenarioItems}
        value={activeScenario?.id ?? NO_SCENARIO}
        onValueChange={(value) =>
          setActiveScenarioId(!value || value === NO_SCENARIO ? null : String(value))
        }
      >
        <SelectTrigger
          className="h-8 w-auto max-w-56 gap-1.5 rounded-full text-xs"
          aria-label={t("scenarios:overlay.label")}
        >
          <Icons.TrendingUp className="h-3.5 w-3.5 shrink-0" />
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {scenarioItems.map((item) => (
            <SelectItem key={item.value} value={item.value}>
              {item.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      {activeScenario && (
        <Select
          items={windowItems}
          value={futureWindow}
          onValueChange={(value) => {
            if (value) setFutureWindow(value as ScenarioFutureWindow);
          }}
        >
          <SelectTrigger
            className="h-8 w-auto gap-1.5 rounded-full text-xs"
            aria-label={t("scenarios:overlay.window_label")}
          >
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {windowItems.map((item) => (
              <SelectItem key={item.value} value={item.value}>
                {item.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      )}
      {activeScenario && deviation !== undefined && deviation !== null && (
        <span className="text-muted-foreground flex items-center gap-1 text-xs">
          {t("scenarios:overlay.real_vs_projected")}
          <GainPercent value={deviation} className="text-xs" />
        </span>
      )}
    </div>
  );
}
