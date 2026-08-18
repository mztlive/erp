import assert from "node:assert/strict"
import test from "node:test"

import {
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

for (const handlerKey of [
    "card_sales_manager_approval",
    "card_sales_operations_approval",
] as const) {
    test(`${handlerKey} opens the exact W05 sales order approval section`, () => {
        const url = parsedHref(
            buildHandlerHref({
                ...REQUIRED_CONTEXT,
                handlerKey,
                destinationWorkspaceId: "W05",
            }),
        )

        assert.equal(url.pathname, "/sales/orders/object%20%2F%2042")
        assert.equal(url.searchParams.get("section"), "approval")
        assertStableContext(url)
    })
}

test("low margin handler opens the exact W05 rejection section", () => {
    const url = parsedHref(
        buildHandlerHref({
            ...REQUIRED_CONTEXT,
            handlerKey: "low_margin_manager",
            destinationWorkspaceId: "W05",
        }),
    )

    assert.equal(url.pathname, "/sales/orders/object%20%2F%2042")
    assert.equal(url.searchParams.get("section"), "procurement-rejection")
    assertStableContext(url)
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
