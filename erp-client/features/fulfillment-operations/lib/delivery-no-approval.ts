import type { BackendDelivery } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/** 仓发单作为合同 DocumentType 的固定值。 */
export const DELIVERY_DOCUMENT_TYPE = "Delivery" as const

/** 工作项上的仓发对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const DELIVERY_OBJECT_TYPE = "delivery" as const

/** 合同 §4.3 对 Delivery 的固定政策。 */
export const DELIVERY_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

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

/** 仓发业务动作白名单；不得混入审批决定或流程写入口。 */
const DELIVERY_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "SAVE",
    "POST",
    "CONFIRM",
    "SKIP",
    "DISCARD",
])

/**
 * 判断当前任务是否属于仓发单。Delivery 为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 Delivery / delivery 时为 true。
 */
export const isDeliveryWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === DELIVERY_DOCUMENT_TYPE ||
    workItem?.businessObjectType === DELIVERY_OBJECT_TYPE

/**
 * 判断当前履约工作单是否为仓发或直发。发货路径不接入审批区。
 *
 * @param operation 履约工作单；只读 `operationType`。
 * @returns 作业类型为 WAREHOUSE_SHIP 或 SUPPLIER_DIRECT 时为 true。
 */
export const isDeliveryOperation = (operation?: {
    operationType?: string
}): boolean =>
    operation?.operationType === "WAREHOUSE_SHIP" ||
    operation?.operationType === "SUPPLIER_DIRECT"

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

/**
 * 仓发允许动作是否只含发货业务入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为仓发业务动作时为 true。
 */
export const deliveryActionsExcludeApproval = (
    actions: readonly string[],
): boolean => actions.every((action) => DELIVERY_BUSINESS_ACTIONS.has(action))
