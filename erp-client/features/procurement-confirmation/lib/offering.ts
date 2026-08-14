import type { ProcurementSupplyOption } from "@/features/procurement-confirmation/api"
import type { FulfillmentMode } from "@/features/procurement-confirmation/types"

import { capabilityCodeForMode } from "./supply-cost"

/** 按供给修订 ID 在当前选项集中定位供给。 */
export function findOffering(
    options: readonly ProcurementSupplyOption[],
    offeringRevisionId: string | null,
): ProcurementSupplyOption | undefined {
    return options.find(
        (option) => option.offeringRevisionId === offeringRevisionId,
    )
}

/** 交付方式恰好匹配一项能力时返回该项，否则返回 undefined（要求人工选择）。 */
export function singleCapabilityForMode(
    offering: ProcurementSupplyOption | undefined,
    fulfillmentMode: FulfillmentMode,
) {
    const capabilities =
        offering?.capabilities.filter(
            (capability) =>
                capability.capabilityCode ===
                capabilityCodeForMode(fulfillmentMode),
        ) ?? []
    return capabilities.length === 1 ? capabilities[0] : undefined
}
