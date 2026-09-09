export type CustomerAccountPreviewTarget = Readonly<{
    kind: "receivable" | "receipt" | "invoice" | "refund" | "reversal"
    id: string
}>
