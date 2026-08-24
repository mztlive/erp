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
    const pageSize = 200
    const fetchPage = (page: number) =>
        apiGet<BackendProcurementResponsibilityRuleList>(
            `${RULES_PATH}?page=${page}&page_size=${pageSize}`,
        )
    const first = await fetchPage(1)
    if (Array.isArray(first)) {
        return first.map(mapProcurementResponsibilityRule)
    }
    const pageCount = Math.ceil(first.total / first.page_size)
    const rest =
        pageCount > 1
            ? await Promise.all(
                  Array.from({ length: pageCount - 1 }, (_, index) =>
                      fetchPage(index + 2),
                  ),
              )
            : []
    const rows = [
        ...first.items,
        ...rest.flatMap((page) => (Array.isArray(page) ? page : page.items)),
    ]
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
