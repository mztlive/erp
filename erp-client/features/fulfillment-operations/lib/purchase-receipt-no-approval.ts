import type { BackendPurchaseReceipt } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：采购收货 HTTP DTO 不得携带审批绑定。 */
export const PURCHASE_RECEIPT_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendPurchaseReceipt,
    "approval"
> = true

/** 编译期证明：入库工作单投影不得嵌入审批区。 */
export const PURCHASE_RECEIPT_OPERATION_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentOperation,
    "approval"
> = true

/** 编译期证明：入库正式结果不得携带审批区。 */
export const PURCHASE_RECEIPT_OUTCOME_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentFormalOutcome,
    "approval"
> = true

/**
 * 丢弃采购收货 DTO 上误带的审批字段。PurchaseReceipt 为 NO_APPROVAL，
 * 禁止把绑定带入投影。
 *
 * @param dto 采购收货 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripPurchaseReceiptApprovalField<T extends object>(
    dto: T,
): Omit<T, "approval"> {
    if (!("approval" in dto)) {
        return dto
    }
    const { approval: _discarded, ...rest } = dto as T & {
        approval?: unknown
    }
    void _discarded
    return rest
}
