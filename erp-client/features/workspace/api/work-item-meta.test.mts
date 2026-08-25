import assert from "node:assert/strict"
import test from "node:test"

import {
    workspaceDocumentBadge,
    workspaceOpenActionLabel,
    workspaceReadActionLabel,
    workspaceTypeLabel,
    // @ts-expect-error TS5097 -- runtime TypeScript module under node:test
} from "./work-item-meta.ts"

test("document approval badges distinguish sales, purchase and receipt", () => {
    assert.deepEqual(
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "sales_order"),
        { label: "销售单", variant: "info" },
    )
    assert.deepEqual(
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "purchase_order"),
        { label: "采购单", variant: "orange" },
    )
    assert.deepEqual(
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "customer_receipt"),
        { label: "回款", variant: "success" },
    )
})

test("common inbox badges use distinct colors", () => {
    const badges = [
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "sales_order"),
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "voucher_sales_order"),
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "purchase_order"),
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "customer_receipt"),
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "stock_adjustment"),
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "supplier_payment"),
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "customer_refund"),
        workspaceDocumentBadge("PROCUREMENT_ORDER_CREATION", "sales_order"),
        workspaceDocumentBadge("PROCUREMENT_CONFIRMATION", "sales_order"),
    ]
    const variants = badges.map((badge) => badge.variant)
    assert.equal(new Set(variants).size, variants.length)
})

test("document approval accepts PascalCase object types", () => {
    assert.equal(
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "SalesOrder").label,
        "销售单",
    )
    assert.equal(
        workspaceDocumentBadge("DOCUMENT_APPROVAL", "VoucherSalesOrder").label,
        "卡券销售",
    )
    assert.equal(
        workspaceTypeLabel("DOCUMENT_APPROVAL", "SalesChangeOrder"),
        "销售变更单审批",
    )
})

test("started approval instances use the document type badge", () => {
    assert.deepEqual(
        workspaceDocumentBadge("APPROVAL_INSTANCE", "stock_adjustment"),
        { label: "库存调整", variant: "teal" },
    )
})

test("procurement creation stays a purchase badge even when the object is a sales order", () => {
    assert.deepEqual(
        workspaceDocumentBadge("PROCUREMENT_ORDER_CREATION", "sales_order"),
        { label: "待建采购", variant: "lime" },
    )
})

test("sales change impact and finance reviews keep distinct badges", () => {
    assert.deepEqual(
        workspaceDocumentBadge(
            "SALES_CHANGE_IMPACT_REVIEW",
            "sales_change_review",
        ),
        { label: "变更履约", variant: "teal" },
    )
    assert.deepEqual(
        workspaceDocumentBadge(
            "SALES_CHANGE_FINANCE_REVIEW",
            "sales_change_review",
        ),
        { label: "变更财务", variant: "violet" },
    )
})

test("unknown types fall back to the provided label", () => {
    assert.deepEqual(
        workspaceDocumentBadge("UNREGISTERED_TASK", "", "自定义任务"),
        { label: "自定义任务", variant: "neutral" },
    )
})

test("read and open actions are named by document and task", () => {
    assert.equal(workspaceReadActionLabel("sales_order"), "查看销售单")
    assert.equal(workspaceReadActionLabel("SalesOrder"), "查看销售单")
    assert.equal(
        workspaceReadActionLabel("voucher_sales_order"),
        "查看卡券销售单",
    )
    assert.equal(workspaceReadActionLabel("purchase_order"), "查看采购单")
    assert.equal(
        workspaceOpenActionLabel("DOCUMENT_APPROVAL", "sales_order"),
        "打开销售单",
    )
    assert.equal(
        workspaceOpenActionLabel("DOCUMENT_APPROVAL", "purchase_order"),
        "打开采购单",
    )
    assert.equal(
        workspaceOpenActionLabel("PROCUREMENT_ORDER_CREATION", "sales_order"),
        "去建采购单",
    )
    assert.equal(
        workspaceOpenActionLabel("PURCHASE_ORDER_REVIEW", "purchase_order"),
        "去审核采购单",
    )
    assert.equal(
        workspaceOpenActionLabel("CUSTOMER_RECEIPT_REVIEW", "customer_receipt"),
        "打开回款单",
    )
})
