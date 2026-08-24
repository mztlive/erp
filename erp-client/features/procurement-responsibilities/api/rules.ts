import { apiGet, apiPost, apiPut } from "@/lib/api"
import {
    mapProcurementResponsibilityRule,
    mapSaveProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/api/mapping"
import type {
    BackendProcurementResponsibilityRuleList,
    BackendSaveProcurementResponsibilityRule,
} from "@/features/procurement-responsibilities/api/wire-types"
import type {
    ProcurementResponsibilityRule,
    SaveProcurementResponsibilityRuleInput,
} from "@/features/procurement-responsibilities/types"

const RULES_PATH = "/admin/procurement-responsibility-rules"

export async function fetchProcurementResponsibilityRules(): Promise<
    readonly ProcurementResponsibilityRule[]
> {
    const response =
        await apiGet<BackendProcurementResponsibilityRuleList>(RULES_PATH)
    const rows = Array.isArray(response) ? response : response.items
    return rows.map(mapProcurementResponsibilityRule)
}

export async function saveProcurementResponsibilityRule(
    input: SaveProcurementResponsibilityRuleInput,
): Promise<ProcurementResponsibilityRule> {
    const payload = mapSaveProcurementResponsibilityRule(input)
    const response = input.ruleId
        ? await apiPut<BackendSaveProcurementResponsibilityRule>(
              `${RULES_PATH}/${encodeURIComponent(input.ruleId)}`,
              payload,
          )
        : await apiPost<BackendSaveProcurementResponsibilityRule>(
              RULES_PATH,
              payload,
          )
    return mapProcurementResponsibilityRule(response)
}
