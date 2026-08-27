import type {
    AllocationSessionView,
    AllocationTrack,
} from "@/features/supplier-payables/types"

export type AllocationSessionIdentity = Readonly<{
    track: AllocationTrack
    supplierId: string
    purchaseOrderId?: string
    existingInvoiceId?: string
    preselectPayableAccountId?: string
}>

/** 判断已存在的核销会话是否属于当前业务对象，禁止跨任务复用草稿。 */
export function allocationSessionMatchesIdentity(
    session: AllocationSessionView,
    identity: AllocationSessionIdentity,
): boolean {
    const expectedPreselectedIds = identity.preselectPayableAccountId
        ? [identity.preselectPayableAccountId]
        : []

    return (
        session.track === identity.track &&
        session.supplierId === identity.supplierId &&
        session.purchaseOrderId === identity.purchaseOrderId &&
        (!identity.existingInvoiceId ||
            session.existingInvoiceId === identity.existingInvoiceId) &&
        session.preselectedPayableAccountIds.length ===
            expectedPreselectedIds.length &&
        expectedPreselectedIds.every((id) =>
            session.preselectedPayableAccountIds.includes(id),
        )
    )
}
