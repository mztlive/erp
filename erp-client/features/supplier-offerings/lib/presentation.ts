import type {
    OfferingStatus,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import { compareDecimal } from "@/lib/fixed-decimal"

/** 返回关系状态对应的徽标样式。 */
export const statusVariant = (status: OfferingStatus) => {
    if (status === "ACTIVE") return "success" as const
    if (status === "STOPPED") return "destructive" as const
    return "secondary" as const
}

/** 格式化可选金额。 */
export const money = (value?: string | null): string => {
    return value ? `¥${value}` : "—"
}

/** 判断供给是否处于可用且未耗尽状态。 */
export const isCurrentlyAvailable = (
    offering: SupplierOfferingView,
): boolean => {
    return (
        offering.availability_status === "AVAILABLE" &&
        (offering.available_quantity == null ||
            compareDecimal(offering.available_quantity, "0", 6) > 0)
    )
}
