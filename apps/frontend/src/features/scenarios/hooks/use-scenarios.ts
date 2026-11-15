import {
  createScenario,
  deleteScenario,
  duplicateScenario,
  getScenario,
  getScenarioProjection,
  getScenarios,
  getScenarioTransactions,
  saveScenarioTransactions,
  updateScenario,
} from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import type {
  AccountScope,
  SaveScenarioTransactionsRequest,
  Scenario,
  ScenarioInput,
  ScenarioProjection,
  ScenarioTransaction,
} from "@/lib/types";
import { keepPreviousData, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

export function useScenarios() {
  return useQuery<Scenario[], Error>({
    queryKey: [QueryKeys.SCENARIOS],
    queryFn: getScenarios,
  });
}

export function useScenario(id: string | undefined) {
  return useQuery<Scenario, Error>({
    queryKey: QueryKeys.scenario(id ?? ""),
    queryFn: () => getScenario(id ?? ""),
    enabled: Boolean(id),
  });
}

export function useScenarioTransactions(id: string | undefined) {
  return useQuery<ScenarioTransaction[], Error>({
    queryKey: QueryKeys.scenarioTransactions(id ?? ""),
    queryFn: () => getScenarioTransactions(id ?? ""),
    enabled: Boolean(id),
  });
}

export function useScenarioProjection(
  id: string | undefined,
  filter: AccountScope = { type: "all" },
  range?: { startDate?: string; endDate?: string },
) {
  return useQuery<ScenarioProjection, Error>({
    queryKey: [
      QueryKeys.SCENARIO_PROJECTION,
      id ?? "",
      filter,
      range?.startDate ?? null,
      range?.endDate ?? null,
    ],
    queryFn: () => getScenarioProjection(id ?? "", filter, range?.startDate, range?.endDate),
    enabled: Boolean(id),
    placeholderData: keepPreviousData,
  });
}

/** Everything that depends on a scenario's inputs. */
function useInvalidateScenario() {
  const queryClient = useQueryClient();
  return (id?: string) => {
    queryClient.invalidateQueries({ queryKey: [QueryKeys.SCENARIOS] });
    queryClient.invalidateQueries({ queryKey: [QueryKeys.SCENARIO_PROJECTION] });
    if (id) {
      queryClient.invalidateQueries({ queryKey: QueryKeys.scenario(id) });
      queryClient.invalidateQueries({ queryKey: QueryKeys.scenarioTransactions(id) });
    }
  };
}

export function useScenarioMutations() {
  const { t } = useTranslation();
  const invalidate = useInvalidateScenario();
  const onError = (error: unknown) =>
    toast.error(t("scenarios:save_error"), {
      description: error instanceof Error ? error.message : String(error),
    });

  const createMutation = useMutation({
    mutationFn: (input: ScenarioInput) => createScenario(input),
    onSuccess: (scenario) => invalidate(scenario.id),
    onError,
  });

  const updateMutation = useMutation({
    mutationFn: ({ id, input }: { id: string; input: ScenarioInput }) => updateScenario(id, input),
    onSuccess: (scenario) => {
      invalidate(scenario.id);
      toast.success(t("scenarios:saved"));
    },
    onError,
  });

  const deleteMutation = useMutation({
    mutationFn: (id: string) => deleteScenario(id),
    onSuccess: (_, id) => {
      invalidate(id);
      toast.success(t("scenarios:deleted"));
    },
    onError,
  });

  const duplicateMutation = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => duplicateScenario(id, name),
    onSuccess: (scenario) => invalidate(scenario.id),
    onError,
  });

  const saveTransactionsMutation = useMutation({
    mutationFn: ({ id, request }: { id: string; request: SaveScenarioTransactionsRequest }) =>
      saveScenarioTransactions(id, request),
    onSuccess: (_, { id }) => invalidate(id),
    onError,
  });

  return {
    createMutation,
    updateMutation,
    deleteMutation,
    duplicateMutation,
    saveTransactionsMutation,
  };
}
