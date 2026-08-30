import assert from "node:assert/strict"
import test from "node:test"

import {
    invoiceExecutionIsComplete,
    workspaceInvoiceDescriptor,
    workspaceInvoiceMatchesReceivable,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./workspace-invoice.ts"

const BASE = {
    workItemType: "SALES_INVOICE_EXECUTION",
    handlerKey: "sales_invoice_execution",
    businessObjectType: "receivable_account",
    businessObjectId: "receivable-42",
    rootBusinessObjectId: "sales-7",
    ownerRole: "role-finance",
    reasonCode: "RECEIVABLE_INVOICE_REQUIRED",
} as const

test("sales receivable invoice task resolves to one account", () => {
    assert.deepEqual(workspaceInvoiceDescriptor(BASE), {
        receivableAccountId: "receivable-42",
        salesOrderId: "sales-7",
    })
})

test("red invoice and sales change reopened tasks stay executable", () => {
    assert.deepEqual(
        workspaceInvoiceDescriptor({
            ...BASE,
            reasonCode: "INVOICEABLE_REOPENED_BY_RED_INVOICE",
        }),
        {
            receivableAccountId: "receivable-42",
            salesOrderId: "sales-7",
        },
    )
    assert.deepEqual(
        workspaceInvoiceDescriptor({
            ...BASE,
            reasonCode: "INVOICEABLE_REOPENED_BY_SALES_CHANGE",
        }),
        {
            receivableAccountId: "receivable-42",
            salesOrderId: "sales-7",
        },
    )
})

test("inconsistent invoice identity fails closed", () => {
    assert.equal(
        workspaceInvoiceDescriptor({
            ...BASE,
            ownerRole: "role-sales",
        }),
        null,
    )
    assert.equal(
        workspaceInvoiceDescriptor({
            ...BASE,
            rootBusinessObjectId: "receivable-42",
        }),
        null,
    )
    assert.equal(
        workspaceInvoiceDescriptor({
            ...BASE,
            workItemType: "DOCUMENT_APPROVAL",
        }),
        null,
    )
})

test("receivable source must match the frozen sales order", () => {
    const descriptor = workspaceInvoiceDescriptor(BASE)
    assert.ok(descriptor)
    assert.equal(
        workspaceInvoiceMatchesReceivable(descriptor, {
            accountId: "receivable-42",
            salesOrderId: "sales-7",
        }),
        true,
    )
    assert.equal(
        workspaceInvoiceMatchesReceivable(descriptor, {
            accountId: "receivable-42",
            salesOrderId: "sales-99",
        }),
        false,
    )
    assert.equal(
        workspaceInvoiceMatchesReceivable(descriptor, {
            accountId: "receivable-99",
            salesOrderId: "sales-7",
        }),
        false,
    )
})

test("partial invoice keeps the execution task open", () => {
    assert.equal(invoiceExecutionIsComplete("30.00", "100.00"), false)
    assert.equal(invoiceExecutionIsComplete("100.00", "100.00"), true)
    assert.equal(invoiceExecutionIsComplete("100", "100.00"), true)
    assert.equal(invoiceExecutionIsComplete("not-an-amount", "100.00"), false)
})
