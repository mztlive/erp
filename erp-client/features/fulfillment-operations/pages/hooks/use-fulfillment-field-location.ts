"use client"

import { useRef } from "react"
import type { ValidationIssue } from "@/components/business"
import type { FulfillmentDeliveryFormHandle } from "../../components/forms/fulfillment-delivery-form"

/** 先展开物流编辑区，再定位字段；其他履约表单保留原定位方式。 */
export function useFulfillmentFieldLocation() {
    const deliveryRef = useRef<FulfillmentDeliveryFormHandle>(null)
    function locateIssue(issue: ValidationIssue) {
        if (!issue.targetId) return
        if (deliveryRef.current?.locate(issue.targetId)) return
        const target = document.getElementById(issue.targetId)
        target?.scrollIntoView({ block: "center" })
        target?.focus({ preventScroll: true })
    }
    return { deliveryRef, locateIssue }
}
