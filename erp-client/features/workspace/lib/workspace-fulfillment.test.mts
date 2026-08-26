import assert from "node:assert/strict"
import test from "node:test"

import {
    workspaceFulfillmentDescriptor,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./workspace-fulfillment.ts"

const BASE = {
    workItemType: "FULFILLMENT_OPERATION",
    handlerKey: "fulfillment_operation",
} as const

test("warehouse receipt resolves to one warehouse operation", () => {
    assert.deepEqual(
        workspaceFulfillmentDescriptor({
            ...BASE,
            businessObjectType: "purchase_receipt",
            ownerRole: "warehouse_inbound_handler",
            reasonCode: "PURCHASE_RECEIPT_READY",
        }),
        { role: "warehouse", operationTypes: ["RECEIPT"] },
    )
})

test("purchase order owner direct delivery resolves to procurement", () => {
    assert.deepEqual(
        workspaceFulfillmentDescriptor({
            ...BASE,
            businessObjectType: "delivery",
            ownerRole: "purchase_order_owner",
            reasonCode: "SUPPLIER_DIRECT_DELIVERY_READY",
        }),
        { role: "procurement", operationTypes: ["SUPPLIER_DIRECT"] },
    )
})

test("warehouse ship resolves to one warehouse operation", () => {
    assert.deepEqual(
        workspaceFulfillmentDescriptor({
            ...BASE,
            businessObjectType: "delivery",
            ownerRole: "warehouse_outbound_handler",
            reasonCode: "WAREHOUSE_DELIVERY_READY",
        }),
        { role: "warehouse", operationTypes: ["WAREHOUSE_SHIP"] },
    )
})

test("electronic and service fulfillment stay with the purchase order owner", () => {
    assert.deepEqual(
        workspaceFulfillmentDescriptor({
            ...BASE,
            businessObjectType: "electronic_delivery",
            ownerRole: "purchase_order_owner",
            reasonCode: "ELECTRONIC_DELIVERY_READY",
        }),
        { role: "procurement", operationTypes: ["ELECTRONIC"] },
    )
    assert.deepEqual(
        workspaceFulfillmentDescriptor({
            ...BASE,
            businessObjectType: "service_fulfillment",
            ownerRole: "purchase_order_owner",
            reasonCode: "SERVICE_FULFILLMENT_READY",
        }),
        { role: "procurement", operationTypes: ["SERVICE"] },
    )
})

test("inconsistent responsibility identity fails closed", () => {
    assert.equal(
        workspaceFulfillmentDescriptor({
            ...BASE,
            businessObjectType: "delivery",
            ownerRole: "warehouse_outbound_handler",
            reasonCode: "SUPPLIER_DIRECT_DELIVERY_READY",
        }),
        null,
    )
})
