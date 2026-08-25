import { expect, test } from "vitest"

import {
    purchaseOrderSectionHref,
    resolvePurchaseOrderDetailSection,
} from "./purchase-order-detail-helpers"

test("概览分区不带 section 查询参数", () => {
    expect(purchaseOrderSectionHref("po-1", "overview")).toBe(
        "/procurement/orders/po-1",
    )
})

test("非概览分区写入 section，并保留现有查询参数", () => {
    expect(
        purchaseOrderSectionHref("po-1", "lines", "mode=edit&workItemId=w-1"),
    ).toBe("/procurement/orders/po-1?mode=edit&workItemId=w-1&section=lines")
})

test("切回概览时去掉 section、保留其余参数", () => {
    expect(
        purchaseOrderSectionHref("po-1", "overview", "section=lines&mode=edit"),
    ).toBe("/procurement/orders/po-1?mode=edit")
})

test("未知 section 回落到概览", () => {
    expect(resolvePurchaseOrderDetailSection("unknown")).toBe("overview")
    expect(resolvePurchaseOrderDetailSection("payable")).toBe("payable")
})
