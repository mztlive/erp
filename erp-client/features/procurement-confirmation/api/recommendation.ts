/** 后端计算的最低可执行成本采购方案。 */

import { apiGet } from "@/lib/api"

import { mapRecommendation } from "./mapping"
import type { BackendRecommendation } from "./backend-types"
import type { ProcurementRecommendation } from "@/features/procurement-confirmation/types"

export async function fetchProcurementRecommendation(
    confirmationId: string,
): Promise<ProcurementRecommendation> {
    const recommendation = await apiGet<BackendRecommendation>(
        `/admin/procurement-confirmations/${encodeURIComponent(confirmationId)}/recommendation`,
    )
    return mapRecommendation(recommendation)
}
