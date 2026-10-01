import type { BackendServiceFulfillment } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：服务履约 HTTP DTO 不得携带审批绑定。 */
export const SERVICE_FULFILLMENT_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendServiceFulfillment,
    "approval"
> = true

/** 编译期证明：服务履约工作单投影不得嵌入审批区。 */
export const SERVICE_FULFILLMENT_OPERATION_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentOperation,
    "approval"
> = true

/** 编译期证明：服务履约正式结果不得携带审批区。 */
export const SERVICE_FULFILLMENT_OUTCOME_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentFormalOutcome,
    "approval"
> = true

/**
 * 丢弃服务履约 DTO 上误带的审批字段。ServiceFulfillment 为 NO_APPROVAL，
 * 禁止把绑定带入投影。
 *
 * @param dto 服务履约 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripServiceFulfillmentApprovalField<T extends object>(
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
