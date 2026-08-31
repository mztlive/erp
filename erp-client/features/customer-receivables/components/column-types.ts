import type { AllocationMode } from "@/features/customer-receivables/types"

export type CustomerAccountPreviewTarget = Readonly<{
    kind: "receivable" | "receipt" | "invoice" | "refund" | "reversal"
    id: string
}>

export type ColumnAllocationTarget = Readonly<{
    salesOrderId?: string
    receivableAccountId?: string
}>

export type ColumnActions = Readonly<{
    onPreview: (target: CustomerAccountPreviewTarget) => void
    canStartSession?: (mode: AllocationMode) => boolean
    startSessionPending?: boolean
    permissionReason?: string
    onStartSession: (
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: ColumnAllocationTarget,
    ) => void | Promise<void>
}>
