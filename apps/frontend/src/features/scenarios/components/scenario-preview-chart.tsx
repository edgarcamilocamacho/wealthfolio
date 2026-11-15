import { AmountDisplay, GainPercent, Skeleton, useDateFormatting } from "@wealthfolio/ui";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@wealthfolio/ui/components/ui/card";
import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Line, LineChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import {
  latestDeviation,
  mergeRealAndProjected,
  type ScenarioChartPoint,
} from "../lib/scenario-transactions";

interface Props {
  real: { date: string; value: number }[];
  projected: { date: string; value: number }[];
  currency: string;
  isBalanceHidden: boolean;
  isLoading: boolean;
}

const COLOR = "var(--investments)";

function PreviewTooltip({
  active,
  payload,
  currency,
  isBalanceHidden,
}: {
  active?: boolean;
  payload?: { payload?: ScenarioChartPoint }[];
  currency: string;
  isBalanceHidden: boolean;
}) {
  const { t } = useTranslation();
  const { formatDate } = useDateFormatting();
  const point = payload?.[0]?.payload;
  if (!active || !point) return null;
  return (
    <div className="bg-popover space-y-1 rounded-md border p-2 text-xs shadow-md">
      <p className="font-medium">{formatDate(point.date)}</p>
      {point.real !== undefined && (
        <div className="flex justify-between gap-4">
          <span className="text-muted-foreground">{t("scenarios:chart.real")}</span>
          <AmountDisplay value={point.real} currency={currency} isHidden={isBalanceHidden} />
        </div>
      )}
      {point.projected !== undefined && (
        <div className="flex justify-between gap-4">
          <span className="text-muted-foreground">{t("scenarios:chart.projected")}</span>
          <AmountDisplay value={point.projected} currency={currency} isHidden={isBalanceHidden} />
        </div>
      )}
      {point.real !== undefined && point.projected !== undefined && (
        <div className="flex justify-between gap-4">
          <span className="text-muted-foreground">{t("scenarios:chart.difference")}</span>
          <AmountDisplay
            value={point.real - point.projected}
            currency={currency}
            isHidden={isBalanceHidden}
            colorFormat
          />
        </div>
      )}
    </div>
  );
}

export function ScenarioPreviewChart({
  real,
  projected,
  currency,
  isBalanceHidden,
  isLoading,
}: Props) {
  const { t } = useTranslation();
  const { formatDate } = useDateFormatting();
  const points = useMemo(() => mergeRealAndProjected(real, projected), [real, projected]);
  const deviation = useMemo(() => latestDeviation(points), [points]);
  const lastProjected = projected[projected.length - 1];

  return (
    <Card>
      <CardHeader className="flex flex-row flex-wrap items-start justify-between gap-4">
        <div className="space-y-1">
          <CardTitle className="text-base">{t("scenarios:chart.title")}</CardTitle>
          <CardDescription>{t("scenarios:chart.description")}</CardDescription>
        </div>
        <div className="flex flex-wrap gap-6 text-sm">
          {deviation && deviation.ratio !== null && (
            <div className="text-end">
              <p className="text-muted-foreground text-xs">
                {t("scenarios:chart.real_vs_projected", { date: formatDate(deviation.date) })}
              </p>
              <GainPercent value={deviation.ratio} />
            </div>
          )}
          {lastProjected && (
            <div className="text-end">
              <p className="text-muted-foreground text-xs">
                {t("scenarios:chart.projected_at", { date: formatDate(lastProjected.date) })}
              </p>
              <AmountDisplay
                value={lastProjected.value}
                currency={currency}
                isHidden={isBalanceHidden}
              />
            </div>
          )}
        </div>
      </CardHeader>
      <CardContent>
        {isLoading ? (
          <Skeleton className="h-64 w-full" />
        ) : points.length === 0 ? (
          <p className="text-muted-foreground py-16 text-center text-sm">
            {t("scenarios:chart.empty")}
          </p>
        ) : (
          <div className="h-64 w-full" dir="ltr">
            <ResponsiveContainer width="100%" height="100%">
              <LineChart data={points} margin={{ top: 8, right: 8, left: 8, bottom: 0 }}>
                <XAxis
                  dataKey="time"
                  type="number"
                  scale="time"
                  domain={["dataMin", "dataMax"]}
                  tickFormatter={(time: number) =>
                    formatDate(new Date(time).toISOString().slice(0, 10))
                  }
                  tick={{ fontSize: "0.6875rem" }}
                  axisLine={false}
                  tickLine={false}
                  minTickGap={48}
                />
                <YAxis hide domain={["auto", "auto"]} />
                <Tooltip
                  content={<PreviewTooltip currency={currency} isBalanceHidden={isBalanceHidden} />}
                />
                <Line
                  type="linear"
                  dataKey="real"
                  stroke={COLOR}
                  strokeWidth={2}
                  dot={false}
                  connectNulls
                  isAnimationActive={false}
                />
                <Line
                  type="linear"
                  dataKey="projected"
                  stroke={COLOR}
                  strokeWidth={1.75}
                  strokeOpacity={0.7}
                  strokeDasharray="2 4"
                  dot={false}
                  connectNulls
                  isAnimationActive={false}
                />
              </LineChart>
            </ResponsiveContainer>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
