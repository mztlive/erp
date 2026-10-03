"use client"

import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import { FulfillmentDeliveryForm } from "./fulfillment-delivery-form"

export function FulfillmentDirectForm(props: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "SUPPLIER_DIRECT" }>
    onChange: (draft: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return <FulfillmentDeliveryForm {...props} />
}
