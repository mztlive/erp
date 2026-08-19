import type { BackendElectronicDelivery } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/** 电子交付单作为合同 DocumentType 的固定值。 */
export const ELECTRONIC_DELIVERY_DOCUMENT_TYPE = "ElectronicDelivery" as const

/** 工作项上的电子交付对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const ELECTRONIC_DELIVERY_OBJECT_TYPE = "electronic_delivery" as const

/** 合同 §4.3 对 ElectronicDelivery 的固定政策。 */
export const ELECTRONIC_DELIVERY_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：电子交付 HTTP DTO 不得携带审批绑定。 */
export const ELECTRONIC_DELIVERY_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendElectronicDelivery,
    "approval"
> = true

/** 编译期证明：电子交付工作单投影不得嵌入审批区。 */
export const ELECTRONIC_DELIVERY_OPERATION_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentOperation,
    "approval"
> = true

/** 编译期证明：电子交付正式结果不得携带审批区。 */
export const ELECTRONIC_DELIVERY_OUTCOME_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentFormalOutcome,
    "approval"
> = true

/** 电子交付业务动作白名单；不得混入审批决定或流程写入口。 */
const ELECTRONIC_DELIVERY_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "SAVE",
    "POST",
    "CONFIRM",
    "SKIP",
    "DISCARD",
])

/**
 * 判断当前任务是否属于电子交付单。ElectronicDelivery 为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 ElectronicDelivery / electronic_delivery 时为 true。
 */
export const isElectronicDeliveryWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === ELECTRONIC_DELIVERY_DOCUMENT_TYPE ||
    workItem?.businessObjectType === ELECTRONIC_DELIVERY_OBJECT_TYPE

/**
 * 判断当前履约工作单是否为电子交付。电子交付路径不接入审批区。
 *
 * @param operation 履约工作单；只读 `operationType`。
 * @returns 作业类型为 ELECTRONIC 时为 true。
 */
export const isElectronicDeliveryOperation = (operation?: {
    operationType?: string
}): boolean => operation?.operationType === "ELECTRONIC"

/**
 * 丢弃电子交付 DTO 上误带的审批字段。ElectronicDelivery 为 NO_APPROVAL，
 * 禁止把绑定带入投影。
 *
 * @param dto 电子交付 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripElectronicDeliveryApprovalField<T extends object>(
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
 * 电子交付允许动作是否只含交付业务入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为电子交付业务动作时为 true。
 */
export const electronicDeliveryActionsExcludeApproval = (
    actions: readonly string[],
): boolean =>
    actions.every((action) => ELECTRONIC_DELIVERY_BUSINESS_ACTIONS.has(action))
