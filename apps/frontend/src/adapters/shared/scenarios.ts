// Scenario Commands
import type {
  AccountScope,
  ScenarioGrowthSuggestion,
  SaveScenarioTransactionsRequest,
  Scenario,
  ScenarioInput,
  ScenarioProjection,
  ScenarioTransaction,
} from "@/lib/types";

import { invoke } from "./platform";

export const getScenarios = async (): Promise<Scenario[]> => {
  return invoke<Scenario[]>("get_scenarios");
};

export const getScenario = async (id: string): Promise<Scenario> => {
  return invoke<Scenario>("get_scenario", { id });
};

export const createScenario = async (scenario: ScenarioInput): Promise<Scenario> => {
  return invoke<Scenario>("create_scenario", { scenario });
};

export const updateScenario = async (id: string, scenario: ScenarioInput): Promise<Scenario> => {
  return invoke<Scenario>("update_scenario", { id, scenario });
};

export const deleteScenario = async (id: string): Promise<void> => {
  return invoke<void>("delete_scenario", { id });
};

export const duplicateScenario = async (id: string, name: string): Promise<Scenario> => {
  return invoke<Scenario>("duplicate_scenario", { id, name });
};

export const getScenarioTransactions = async (
  scenarioId: string,
): Promise<ScenarioTransaction[]> => {
  return invoke<ScenarioTransaction[]>("get_scenario_transactions", { scenarioId });
};

export const saveScenarioTransactions = async (
  scenarioId: string,
  request: SaveScenarioTransactionsRequest,
): Promise<ScenarioTransaction[]> => {
  return invoke<ScenarioTransaction[]>("save_scenario_transactions", { scenarioId, request });
};

export const getScenarioProjection = async (
  scenarioId: string,
  filter?: AccountScope,
  startDate?: string,
  endDate?: string,
): Promise<ScenarioProjection> => {
  return invoke<ScenarioProjection>("get_scenario_projection", {
    scenarioId,
    filter,
    startDate,
    endDate,
  });
};

export const getScenarioNetWorthProjection = async (
  scenarioId: string,
  startDate?: string,
  endDate?: string,
): Promise<ScenarioProjection> => {
  return invoke<ScenarioProjection>("get_scenario_net_worth_projection", {
    scenarioId,
    startDate,
    endDate,
  });
};

export const suggestScenarioAccountGrowth = async (
  accountId: string,
): Promise<ScenarioGrowthSuggestion | null> => {
  return invoke<ScenarioGrowthSuggestion | null>("suggest_scenario_account_growth", { accountId });
};
