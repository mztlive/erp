import type { BackendDelivery } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：仓发 HTTP DTO 不得携带审批绑定。 */
export const DELIVERY_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendDelivery,
    "approval"
> = true

/** 编译期证明：仓发工作单投影不得嵌入审批区。 */
export const DELIVERY_OPERATION_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentOperation,
    "approval"
> = true

/** 编译期证明：仓发正式结果不得携带审批区。 */
export const DELIVERY_OUTCOME_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentFormalOutcome,
    "approval"
> = true

/**
 * 丢弃仓发 DTO 上误带的审批字段。Delivery 为 NO_APPROVAL，
 * 禁止把绑定带入投影。
 *
 * @param dto 仓发 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripDeliveryApprovalField<T extends object>(
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
