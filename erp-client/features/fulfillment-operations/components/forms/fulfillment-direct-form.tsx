"use client"

import type { Ref } from "react"
import type { FulfillmentDeliveryFormHandle } from "./fulfillment-delivery-form"
import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import { FulfillmentDeliveryForm } from "./fulfillment-delivery-form"

export function FulfillmentDirectForm(props: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "SUPPLIER_DIRECT" }>
    onChange: (draft: FulfillmentDraft) => void
    ref?: Ref<FulfillmentDeliveryFormHandle>
    disabled?: boolean
}) {
    return <FulfillmentDeliveryForm {...props} />
}
