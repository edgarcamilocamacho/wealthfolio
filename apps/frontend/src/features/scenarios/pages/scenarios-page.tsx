import { formatZonedDateKey } from "@/features/spending/lib/timezone";
import { useSettingsContext } from "@/lib/settings-provider";
import type { ScenarioInput } from "@/lib/types";
import {
  Button,
  Page,
  PageContent,
  PageHeader,
  Skeleton,
  useDateFormatting,
} from "@wealthfolio/ui";
import { Card, CardContent } from "@wealthfolio/ui/components/ui/card";
import { Icons } from "@wealthfolio/ui/components/ui/icons";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate } from "react-router-dom";
import { ScenarioFormDialog } from "../components/scenario-form-dialog";
import { useScenarioMutations, useScenarios } from "../hooks/use-scenarios";

export default function ScenariosPage() {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const { formatDate } = useDateFormatting();
  const { settings } = useSettingsContext();
  const todayISO = formatZonedDateKey(new Date(), settings?.timezone);
  const { data: scenarios = [], isLoading } = useScenarios();
  const { createMutation } = useScenarioMutations();
  const [createOpen, setCreateOpen] = useState(false);

  const handleCreate = async (input: ScenarioInput) => {
    const scenario = await createMutation.mutateAsync(input);
    navigate(`/scenarios/${scenario.id}`);
  };

  const newButton = (
    <Button size="sm" onClick={() => setCreateOpen(true)}>
      <Icons.Plus className="me-1 h-4 w-4" />
      {t("scenarios:new_scenario")}
    </Button>
  );

  return (
    <Page>
      <PageHeader
        heading={t("scenarios:title")}
        text={t("scenarios:subtitle")}
        actions={newButton}
      />
      <PageContent>
        {isLoading ? (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            <Skeleton className="h-28" />
            <Skeleton className="h-28" />
          </div>
        ) : scenarios.length === 0 ? (
          <div className="flex flex-col items-center justify-center gap-5 py-28 text-center">
            <div className="bg-muted/60 flex h-16 w-16 items-center justify-center rounded-2xl">
              <Icons.TrendingUp className="text-muted-foreground h-8 w-8" />
            </div>
            <div className="space-y-2">
              <p className="text-lg font-semibold">{t("scenarios:empty_title")}</p>
              <p className="text-muted-foreground max-w-md text-sm leading-relaxed">
                {t("scenarios:empty_description")}
              </p>
            </div>
            {newButton}
          </div>
        ) : (
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {scenarios.map((scenario) => (
              <Link key={scenario.id} to={`/scenarios/${scenario.id}`} className="block">
                <Card className="hover:bg-muted/40 h-full transition-colors">
                  <CardContent className="space-y-2 p-4">
                    <p className="truncate font-medium">{scenario.name}</p>
                    {scenario.description && (
                      <p className="text-muted-foreground line-clamp-2 text-sm">
                        {scenario.description}
                      </p>
                    )}
                    <p className="text-muted-foreground text-xs">
                      {t("scenarios:date_range", {
                        start: formatDate(scenario.anchorDate),
                        end: formatDate(scenario.endDate),
                      })}
                    </p>
                  </CardContent>
                </Card>
              </Link>
            ))}
          </div>
        )}
      </PageContent>
      <ScenarioFormDialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        todayISO={todayISO}
        isSaving={createMutation.isPending}
        onSubmit={handleCreate}
      />
    </Page>
  );
}
