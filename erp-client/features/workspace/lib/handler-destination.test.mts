import assert from "node:assert/strict"
import test from "node:test"

import {
    HANDLER_REGISTRY,
    buildHandlerHref,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./handler-destination.ts"

const REQUIRED_CONTEXT = {
    businessObjectId: "object / 42",
    workItemId: "wi-42",
    queueContextId: "queue-42",
    taskVersion: "must-not-cross-navigation",
    allowedActions: ["APPROVE"],
} as const

function parsedHref(href: string | null): URL {
    assert.ok(href, "registered handler must produce a URL")
    return new URL(href, "https://erp.test")
}

function assertStableContext(url: URL): void {
    assert.equal(url.searchParams.get("from"), "workspace")
    assert.equal(url.searchParams.get("workItemId"), "wi-42")
    assert.equal(url.searchParams.get("queueContextId"), "queue-42")
    assert.equal(url.searchParams.has("taskVersion"), false)
    assert.equal(url.searchParams.has("allowedActions"), false)
}

test("inactive handlers are absent and fail closed", () => {
    for (const handlerKey of [
        "procurement_confirmation",
        "low_margin_manager",
        "card_sales_manager_approval",
        "card_sales_operations_approval",
        "ownership_sales",
        "ownership_finance",
        "finance_correction",
    ]) {
        assert.equal(HANDLER_REGISTRY[handlerKey], undefined)
        assert.equal(
            buildHandlerHref({
                ...REQUIRED_CONTEXT,
                handlerKey,
                destinationWorkspaceId: "W05",
            }),
            null,
        )
    }
    assert.equal(
        HANDLER_REGISTRY.procurement_order_creation?.family,
        "procurement",
    )
})

test("sales change review opens its root sales order instead of the review id", () => {
    for (const handlerKey of [
        "sales_change_impact_review",
        "sales_change_finance_review",
    ] as const) {
        const url = parsedHref(
            buildHandlerHref({
                ...REQUIRED_CONTEXT,
                rootBusinessObjectId: "sales-order / 7",
                handlerKey,
                destinationWorkspaceId: "W05",
            }),
        )
        assert.equal(url.pathname, "/sales/orders/sales-order%20%2F%207")
        assert.equal(url.searchParams.get("section"), "change-review")
        assertStableContext(url)
    }
    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "sales_change_impact_review",
            destinationWorkspaceId: "W05",
        }),
        null,
    )
})

test("procurement creation opens W08 with sales order and work item context", () => {
    const url = parsedHref(
        buildHandlerHref({
            businessObjectId: "sales-order / 7",
            workItemId: "wi-42",
            handlerKey: "procurement_order_creation",
            destinationWorkspaceId: "W08",
        }),
    )

    assert.equal(url.pathname, "/procurement/orders")
    assert.equal(url.searchParams.get("action"), "create")
    assert.equal(url.searchParams.get("salesOrderId"), "sales-order / 7")
    assert.equal(url.searchParams.get("workItemId"), "wi-42")
    assert.equal(url.searchParams.has("queueContextId"), false)
})

test("fulfillment operation stays in W01 and focuses the exact task", () => {
    const url = parsedHref(
        buildHandlerHref({
            businessObjectId: "delivery / 7",
            workItemId: "wi-42",
            handlerKey: "fulfillment_operation",
            destinationWorkspaceId: "W01",
        }),
    )

    assert.equal(url.pathname, "/workspace")
    assert.equal(url.searchParams.get("family"), "fulfillment")
    assert.equal(url.searchParams.get("currentWorkItemId"), "wi-42")
    assert.equal(url.searchParams.has("queueContextId"), false)
})

test("customer acceptance opens the exact sales order W06 section", () => {
    const url = parsedHref(
        buildHandlerHref({
            businessObjectId: "sales-order / 7",
            workItemId: "wi-42",
            queueContextId: "queue-42",
            handlerKey: "customer_acceptance_registration",
            destinationWorkspaceId: "W06",
        }),
    )

    assert.equal(url.pathname, "/sales/orders/sales-order%20%2F%207")
    assert.equal(url.searchParams.get("section"), "acceptance")
    assert.equal(url.searchParams.get("from"), "W01")
    assert.equal(url.searchParams.get("workItemId"), "wi-42")
    assert.equal(url.searchParams.get("queueContextId"), "queue-42")
    assert.equal(
        url.searchParams.get("returnTo"),
        "/workspace?currentWorkItemId=wi-42",
    )
})

test("purchase order review opens the exact W08 review mode", () => {
    const url = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "po_review",
            destinationWorkspaceId: "W08",
        }),
    )

    assert.equal(url.pathname, "/procurement/orders/object%20%2F%2042")
    assert.equal(url.searchParams.get("mode"), "review")
    assertStableContext(url)
})

test("supplier payment execution opens W12 with the exact payable preselected", () => {
    const url = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            businessObjectId: "payable / 42",
            rootBusinessObjectId: "purchase / 7",
            handlerKey: "supplier_payment_execution",
            destinationWorkspaceId: "W12",
        }),
    )

    assert.equal(url.pathname, "/finance/supplier-accounts")
    assert.equal(url.searchParams.get("from"), "W01")
    assert.equal(url.searchParams.get("view"), "payable")
    assert.equal(url.searchParams.get("session"), "payment")
    assert.equal(url.searchParams.get("purchaseOrderId"), "purchase / 7")
    assert.equal(url.searchParams.get("detailId"), "payable / 42")
    assert.equal(url.searchParams.get("previewKind"), "payable")
    assert.equal(url.searchParams.get("currentWorkItemId"), "wi-42")
    assert.equal(url.searchParams.get("queueContextId"), "queue-42")

    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "supplier_payment_execution",
            destinationWorkspaceId: "W12",
        }),
        null,
    )
})

test("sales invoice execution opens W11 with the exact receivable preselected", () => {
    const url = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            businessObjectId: "receivable / 42",
            rootBusinessObjectId: "sales / 7",
            handlerKey: "sales_invoice_execution",
            destinationWorkspaceId: "W11",
        }),
    )

    assert.equal(url.pathname, "/finance/customer-accounts")
    assert.equal(url.searchParams.get("from"), "W01")
    assert.equal(url.searchParams.get("view"), "sales_invoice")
    assert.equal(url.searchParams.get("register"), "invoice")
    assert.equal(url.searchParams.get("receivableAccountId"), "receivable / 42")
    assert.equal(url.searchParams.get("salesOrderId"), "sales / 7")
    assert.equal(url.searchParams.get("previewKind"), "receivable")
    assert.equal(url.searchParams.get("previewId"), "receivable / 42")
    assert.equal(url.searchParams.get("currentWorkItemId"), "wi-42")
    assert.equal(url.searchParams.get("queueContextId"), "queue-42")

    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "sales_invoice_execution",
            destinationWorkspaceId: "W11",
        }),
        null,
    )
})

test("supplier settlement review opens the exact W27 statement", () => {
    const url = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "supplier_settlement",
            destinationWorkspaceId: "W27",
        }),
    )

    assert.equal(url.pathname, "/supplier-api/settlements/object%20%2F%2042")
    assert.equal(url.searchParams.get("section"), "review")
    assertStableContext(url)
})

test("existing queue handlers keep their registered focus parameters", () => {
    const cardFunds = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "card_funds",
            destinationWorkspaceId: "W13",
        }),
    )
    assert.equal(cardFunds.pathname, "/finance/card-funds-review")
    assert.equal(cardFunds.searchParams.get("currentWorkItemId"), "wi-42")
    assertStableContext(cardFunds)

    const supplierOrder = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "supplier_fulfillment_investigation",
            destinationWorkspaceId: "W26",
        }),
    )
    assert.equal(
        supplierOrder.pathname,
        "/supplier-api/orders/object%20%2F%2042",
    )
    assertStableContext(supplierOrder)
})

test("W18 requires and carries its registered confirmation scope", () => {
    const url = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "import_business_confirmation",
            destinationWorkspaceId: "W18",
            routeContext: { confirmationScope: "FINANCE" },
        }),
    )
    assert.equal(url.pathname, "/governance/imports")
    assert.equal(url.searchParams.get("section"), "confirm")
    assert.equal(url.searchParams.get("batchId"), "object / 42")
    assert.equal(url.searchParams.get("confirmationScope"), "FINANCE")
    assertStableContext(url)

    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "import_business_confirmation",
            destinationWorkspaceId: "W18",
        }),
        null,
    )
})

test("document_approval opens the destination document without requiring queueContextId", () => {
    const sales = parsedHref(
        buildHandlerHref({
            businessObjectType: "sales_order",
            businessObjectId: "object / 42",
            workItemId: "wi-42",
            handlerKey: "document_approval",
            destinationWorkspaceId: "W05",
        }),
    )
    assert.equal(sales.pathname, "/sales/orders/object%20%2F%2042")
    assert.equal(sales.searchParams.get("section"), "approval")
    assert.equal(sales.searchParams.get("from"), "workspace")
    assert.equal(sales.searchParams.get("workItemId"), "wi-42")

    const receipt = parsedHref(
        buildHandlerHref({
            businessObjectType: "customer_receipt",
            businessObjectId: "object / 42",
            workItemId: "wi-42",
            handlerKey: "document_approval",
            destinationWorkspaceId: "W11",
        }),
    )
    assert.equal(receipt.pathname, "/finance/customer-accounts")
    assert.equal(receipt.searchParams.get("previewId"), "object / 42")
})

test("started payment reversal opens its W12 detail without pretending to be an approval task", () => {
    const reversal = parsedHref(
        buildHandlerHref({
            handlerKey: "document_approval",
            destinationWorkspaceId: "W12",
            businessObjectType: "payment_reversal",
            businessObjectId: "reversal / 42",
            workItemId: "instance-42",
            approvalInstanceId: "instance-42",
            trackingOnly: true,
        }),
    )

    assert.equal(reversal.pathname, "/finance/supplier-accounts")
    assert.equal(reversal.searchParams.get("from"), "workspace")
    assert.equal(reversal.searchParams.get("view"), "payable")
    assert.equal(reversal.searchParams.get("previewKind"), "reversal")
    assert.equal(reversal.searchParams.get("detailId"), "reversal / 42")
    assert.equal(reversal.searchParams.get("approvalInstanceId"), "instance-42")
    assert.equal(reversal.searchParams.has("workItemId"), false)
    assert.equal(reversal.searchParams.has("currentWorkItemId"), false)
})

test("unknown, mismatched, and incomplete handlers fail closed", () => {
    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "unknown_handler",
            destinationWorkspaceId: "W05",
        }),
        null,
    )
    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "po_review",
            destinationWorkspaceId: "W05",
        }),
        null,
    )
    assert.equal(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            queueContextId: undefined,
            handlerKey: "po_review",
            destinationWorkspaceId: "W08",
        }),
        null,
    )
})
