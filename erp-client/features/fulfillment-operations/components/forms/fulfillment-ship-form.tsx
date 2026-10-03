"use client"

import type { FulfillmentDraft, FulfillmentOperation } from "../../types"
import { FulfillmentDeliveryForm } from "./fulfillment-delivery-form"

export function FulfillmentShipForm(props: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "WAREHOUSE_SHIP" }>
    onChange: (draft: FulfillmentDraft) => void
    disabled?: boolean
}) {
    return <FulfillmentDeliveryForm {...props} />
}
