import type { BackendPurchaseReceipt } from "@/features/fulfillment-operations/api/documents"
import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/** 采购收货单作为合同 DocumentType 的固定值。 */
export const PURCHASE_RECEIPT_DOCUMENT_TYPE = "PurchaseReceipt" as const

/** 工作项上的采购收货对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const PURCHASE_RECEIPT_OBJECT_TYPE = "purchase_receipt" as const

/** 合同 §4.3 对 PurchaseReceipt 的固定政策。 */
export const PURCHASE_RECEIPT_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

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

/** 采购收货业务动作白名单；不得混入审批决定或流程写入口。 */
const PURCHASE_RECEIPT_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "SAVE",
    "POST",
    "CONFIRM",
    "SKIP",
    "DISCARD",
])

/**
 * 判断当前任务是否属于采购收货。采购收货为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 PurchaseReceipt / purchase_receipt 时为 true。
 */
export const isPurchaseReceiptWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === PURCHASE_RECEIPT_DOCUMENT_TYPE ||
    workItem?.businessObjectType === PURCHASE_RECEIPT_OBJECT_TYPE

/**
 * 判断当前履约工作单是否为采购入库。入库路径不接入审批区。
 *
 * @param operation 履约工作单；只读 `operationType`。
 * @returns 作业类型为 RECEIPT 时为 true。
 */
export const isPurchaseReceiptOperation = (operation?: {
    operationType?: string
}): boolean => operation?.operationType === "RECEIPT"

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

/**
 * 采购收货允许动作是否只含入库业务入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为采购收货业务动作时为 true。
 */
export const purchaseReceiptActionsExcludeApproval = (
    actions: readonly string[],
): boolean =>
    actions.every((action) => PURCHASE_RECEIPT_BUSINESS_ACTIONS.has(action))
