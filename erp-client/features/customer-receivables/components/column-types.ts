import type { AllocationMode } from "@/features/customer-receivables/types"

export type CustomerAccountPreviewTarget = Readonly<{
    kind: "receivable" | "receipt" | "invoice"
    id: string
}>

export type ColumnAllocationTarget = Readonly<{
    salesOrderId?: string
    receivableAccountId?: string
}>

export type ColumnActions = Readonly<{
    onPreview: (target: CustomerAccountPreviewTarget) => void
    onStartSession: (
        mode: AllocationMode,
        partyId: string,
        existingFactId?: string,
        target?: ColumnAllocationTarget,
    ) => void | Promise<void>
}>
