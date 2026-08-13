import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import type { FulfillmentMode } from "@/features/procurement-confirmation/types"

/** 交付方式对应的供应商能力编码。 */
export function capabilityCodeForMode(mode: FulfillmentMode) {
    if (mode === "ELECTRONIC") return "virtual"
    if (mode === "SERVICE") return "offline_service"
    return "physical"
}

/** 按数量判定报价档位：达到集采起订量用集采价，否则用一件代发价。 */
export function supplyCostForQuantity(
    option: ProcurementSupplyOption,
    quantity: string,
) {
    return Number(quantity) >= Number(option.bulkMinimumOrderQuantity)
        ? option.bulkCostGross
        : option.dropshipCostGross
}
