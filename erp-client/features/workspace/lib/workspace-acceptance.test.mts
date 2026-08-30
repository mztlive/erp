import assert from "node:assert/strict"
import test from "node:test"

import {
    workspaceAcceptanceDescriptor,
    workspaceAcceptanceTaskIdentity,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./workspace-acceptance.ts"

const BASE = {
    workItemType: "CUSTOMER_ACCEPTANCE_REGISTRATION",
    handlerKey: "customer_acceptance_registration",
    businessObjectType: "sales_order",
    businessObjectId: "sales-7",
    ownerRole: "sales_order_owner",
    reasonCode: "CUSTOMER_ACCEPTANCE_REQUIRED",
    destinationWorkspaceId: "W06",
} as const

test("customer acceptance task resolves to one sales order", () => {
    assert.deepEqual(workspaceAcceptanceDescriptor(BASE), {
        salesOrderId: "sales-7",
    })
})

test("reversal reopened acceptance task stays executable", () => {
    assert.deepEqual(
        workspaceAcceptanceDescriptor({
            ...BASE,
            reasonCode: "CUSTOMER_ACCEPTANCE_REOPENED_BY_REVERSAL",
        }),
        { salesOrderId: "sales-7" },
    )
})

test("inconsistent acceptance identity fails closed", () => {
    assert.equal(
        workspaceAcceptanceDescriptor({
            ...BASE,
            ownerRole: "role-finance",
        }),
        null,
    )
    assert.equal(
        workspaceAcceptanceDescriptor({
            ...BASE,
            destinationWorkspaceId: "W01" as const,
        }),
        null,
    )
    assert.equal(
        workspaceAcceptanceDescriptor({
            ...BASE,
            businessObjectType: "customer_acceptance",
        }),
        null,
    )
    assert.equal(
        workspaceAcceptanceDescriptor({
            ...BASE,
            workItemType: "FULFILLMENT_OPERATION",
        }),
        null,
    )
    assert.equal(
        workspaceAcceptanceDescriptor({
            ...BASE,
            reasonCode: "WAREHOUSE_DELIVERY_READY",
        }),
        null,
    )
    assert.equal(
        workspaceAcceptanceDescriptor({
            ...BASE,
            businessObjectId: "  ",
        }),
        null,
    )
})

test("task identity keeps the frozen work item fields for W06 commit", () => {
    assert.deepEqual(
        workspaceAcceptanceTaskIdentity({
            ...BASE,
            workItemId: "wi-42",
            status: "OPEN",
            taskVersion: "3",
            allowedActions: ["PROCESS"],
        }),
        {
            workItemId: "wi-42",
            workItemType: "CUSTOMER_ACCEPTANCE_REGISTRATION",
            handlerKey: "customer_acceptance_registration",
            destinationWorkspaceId: "W06",
            businessObjectType: "sales_order",
            businessObjectId: "sales-7",
            status: "OPEN",
            taskVersion: "3",
            allowedActions: ["PROCESS"],
        },
    )
})
