import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/** 客户验收单作为合同 DocumentType 的固定值。 */
export const CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE = "CustomerAcceptance" as const

/** 工作项上的客户验收对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const CUSTOMER_ACCEPTANCE_OBJECT_TYPE = "customer_acceptance" as const

/** 合同 §4.3 对 CustomerAcceptance 的固定政策。 */
export const CUSTOMER_ACCEPTANCE_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

/**
 * 履约结果面板交给客户验收的后续步骤投影。
 * CustomerAcceptance 为 NO_APPROVAL，不得携带审批绑定。
 */
export type CustomerAcceptanceHandoff = {
    salesOrderId: string
    acceptanceRequired: true
    acceptanceNextStep: string
}

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：客户验收交接投影不得携带审批绑定。 */
export const CUSTOMER_ACCEPTANCE_DTO_HAS_NO_APPROVAL: ForbidKey<
    CustomerAcceptanceHandoff,
    "approval"
> = true

/** 编译期证明：履约工作单投影不得嵌入客户验收审批区。 */
export const CUSTOMER_ACCEPTANCE_OPERATION_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentOperation,
    "approval"
> = true

/** 编译期证明：履约正式结果不得携带客户验收审批区。 */
export const CUSTOMER_ACCEPTANCE_OUTCOME_HAS_NO_APPROVAL: ForbidKey<
    FulfillmentFormalOutcome,
    "approval"
> = true

/** 客户验收业务动作白名单；不得混入审批决定或流程写入口。 */
const CUSTOMER_ACCEPTANCE_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "CREATE_ACCEPTANCE",
    "SAVE_DRAFT",
    "POST_ACCEPTANCE",
    "REVERSE_ACCEPTANCE",
])

/**
 * 判断当前任务是否属于客户验收单。CustomerAcceptance 为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 CustomerAcceptance / customer_acceptance 时为 true。
 */
export const isCustomerAcceptanceWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === CUSTOMER_ACCEPTANCE_DOCUMENT_TYPE ||
    workItem?.businessObjectType === CUSTOMER_ACCEPTANCE_OBJECT_TYPE

/**
 * 判断履约正式结果是否进入客户验收交接。验收交接不接入审批区。
 *
 * @param outcome 履约正式结果；只读 `acceptanceRequired`。
 * @returns 需要销售登记客户验收时为 true。
 */
export const isCustomerAcceptanceHandoff = (outcome?: {
    acceptanceRequired?: boolean
}): boolean => outcome?.acceptanceRequired === true

/**
 * 丢弃客户验收交接投影上误带的审批字段。CustomerAcceptance 为 NO_APPROVAL，
 * 禁止把绑定带入履约结果。
 *
 * @param dto 客户验收交接或履约结果载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripCustomerAcceptanceApprovalField<T extends object>(
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
 * 客户验收允许动作是否只含验收业务入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为客户验收业务动作时为 true。
 */
export const customerAcceptanceActionsExcludeApproval = (
    actions: readonly string[],
): boolean =>
    actions.every((action) => CUSTOMER_ACCEPTANCE_BUSINESS_ACTIONS.has(action))
