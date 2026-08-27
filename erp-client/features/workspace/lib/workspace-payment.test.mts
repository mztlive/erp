import assert from "node:assert/strict"
import test from "node:test"

import {
    workspacePaymentDescriptor,
    workspacePaymentMatchesPayable,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./workspace-payment.ts"

const BASE = {
    workItemType: "SUPPLIER_PAYMENT_EXECUTION",
    handlerKey: "supplier_payment_execution",
    businessObjectType: "payable_account",
    businessObjectId: "payable-42",
    rootBusinessObjectId: "purchase-7",
    ownerRole: "role-finance",
    reasonCode: "PAYABLE_PAYMENT_REQUIRED",
} as const

test("purchase payable payment task resolves to one account", () => {
    assert.deepEqual(workspacePaymentDescriptor(BASE), {
        payableAccountId: "payable-42",
        purchaseOrderId: "purchase-7",
    })
})

test("reversal reopened payment task stays executable", () => {
    assert.deepEqual(
        workspacePaymentDescriptor({
            ...BASE,
            reasonCode: "PAYABLE_REOPENED_BY_REVERSAL",
        }),
        {
            payableAccountId: "payable-42",
            purchaseOrderId: "purchase-7",
        },
    )
})

test("inconsistent payment identity fails closed", () => {
    assert.equal(
        workspacePaymentDescriptor({
            ...BASE,
            ownerRole: "role-procurement",
        }),
        null,
    )
    assert.equal(
        workspacePaymentDescriptor({
            ...BASE,
            rootBusinessObjectId: "payable-42",
        }),
        null,
    )
    assert.equal(
        workspacePaymentDescriptor({
            ...BASE,
            workItemType: "DOCUMENT_APPROVAL",
        }),
        null,
    )
})

test("payable source must match the frozen purchase order", () => {
    const descriptor = workspacePaymentDescriptor(BASE)
    assert.ok(descriptor)
    assert.equal(
        workspacePaymentMatchesPayable(descriptor, {
            payableAccountId: "payable-42",
            sourceType: "PURCHASE_ORDER",
            sourceDocumentId: "purchase-7",
        }),
        true,
    )
    assert.equal(
        workspacePaymentMatchesPayable(descriptor, {
            payableAccountId: "payable-42",
            sourceType: "SUPPLIER_SETTLEMENT",
            sourceDocumentId: "purchase-7",
        }),
        false,
    )
    assert.equal(
        workspacePaymentMatchesPayable(descriptor, {
            payableAccountId: "payable-99",
            sourceType: "PURCHASE_ORDER",
            sourceDocumentId: "purchase-7",
        }),
        false,
    )
})
