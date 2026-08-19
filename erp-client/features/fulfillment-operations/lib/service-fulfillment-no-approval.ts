import type { BackendServiceFulfillment } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/** 服务履约单作为合同 DocumentType 的固定值。 */
export const SERVICE_FULFILLMENT_DOCUMENT_TYPE = "ServiceFulfillment" as const

/** 工作项上的服务履约对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const SERVICE_FULFILLMENT_OBJECT_TYPE = "service_fulfillment" as const

/** 合同 §4.3 对 ServiceFulfillment 的固定政策。 */
export const SERVICE_FULFILLMENT_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

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

/** 服务履约业务动作白名单；不得混入审批决定或流程写入口。 */
const SERVICE_FULFILLMENT_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "SAVE",
    "POST",
    "CONFIRM",
    "SKIP",
    "DISCARD",
])

/**
 * 判断当前任务是否属于服务履约单。ServiceFulfillment 为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 ServiceFulfillment / service_fulfillment 时为 true。
 */
export const isServiceFulfillmentWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === SERVICE_FULFILLMENT_DOCUMENT_TYPE ||
    workItem?.businessObjectType === SERVICE_FULFILLMENT_OBJECT_TYPE

/**
 * 判断当前履约工作单或正式结果是否为服务履约。
 * 作业类型 `SERVICE` 与事实类型 `SERVICE_FULFILLMENT` 都不接入审批区。
 *
 * @param operation 履约工作单或结果投影；只读 `operationType` / `factType`。
 * @returns 作业类型为 SERVICE 或事实类型为 SERVICE_FULFILLMENT 时为 true。
 */
export const isServiceFulfillmentOperation = (operation?: {
    operationType?: string
    factType?: string
}): boolean =>
    operation?.operationType === "SERVICE" ||
    operation?.factType === "SERVICE_FULFILLMENT"

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

/**
 * 服务履约允许动作是否只含服务业务入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为服务履约业务动作时为 true。
 */
export const serviceFulfillmentActionsExcludeApproval = (
    actions: readonly string[],
): boolean =>
    actions.every((action) => SERVICE_FULFILLMENT_BUSINESS_ACTIONS.has(action))
