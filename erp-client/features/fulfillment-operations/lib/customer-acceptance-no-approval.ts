import type {
    FulfillmentFormalOutcome,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

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
