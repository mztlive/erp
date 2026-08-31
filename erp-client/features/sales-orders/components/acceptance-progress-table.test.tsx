import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { AcceptanceProgressTable } from "./acceptance-progress-table"
import type { AcceptanceOrderProgress } from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceEligibleFact } from "@/features/sales-orders/lib/acceptance-types"

afterEach(cleanup)

function fact(
    overrides: Partial<AcceptanceEligibleFact> = {},
): AcceptanceEligibleFact {
    return {
        fulfillmentLineId: "fl-1",
        fulfillmentFactType: "SERVICE",
        fulfillmentNo: "SF-fcc911d964bc4dc89b9ac67815522ea2",
        salesOrderLineId: "sol-1",
        lineNo: 1,
        itemSnapshot: "年节礼包上门派送 (北京)",
        unitCode: "次",
        occurredAt: "2026-08-31T00:00:00.000Z",
        netSuccessfulQuantity: "1",
        netAcceptedAllocatedQuantity: "0",
        eligibleQuantity: "1",
        ...overrides,
    }
}

function progress(
    overrides: Partial<AcceptanceOrderProgress> = {},
): AcceptanceOrderProgress {
    const pending = fact()
    return {
        requiredQuantity: "1",
        deliveredQuantity: "1",
        acceptedQuantity: "0",
        pendingQuantity: "1",
        unitCode: null,
        pendingFactCount: 1,
        lines: [
            {
                salesOrderLineId: "sol-1",
                lineNo: 1,
                itemSnapshot: "年节礼包上门派送 (北京)",
                unitCode: "次",
                requiredQuantity: "1",
                deliveredQuantity: "1",
                acceptedQuantity: "0",
                pendingQuantity: "1",
                pendingFacts: [pending],
                stuckKind: "accept",
                stuckLabel: `服务履约 ${pending.fulfillmentNo} 待验 1`,
            },
        ],
        ...overrides,
    }
}

test("待验批次把履约单号单独截断，不把明细列挤换行", () => {
    render(
        <AcceptanceProgressTable
            progress={progress()}
            pendingHint="还有 4 批待客户验收。"
        />,
    )

    expect(screen.getByText("1 · 年节礼包上门派送 (北京)")).toBeTruthy()
    expect(screen.getByText("服务履约 · 待验 1 次")).toBeTruthy()
    expect(screen.getByText("SF-fcc911d964bc4dc89b9ac67815522ea2")).toBeTruthy()
    expect(screen.getByText("还有 4 批待客户验收。")).toBeTruthy()

    const number = screen.getByText("SF-fcc911d964bc4dc89b9ac67815522ea2")
    expect(number.className).toContain("truncate")
    const stuckCell = number.closest('[data-slot="table-cell"]')
    expect(stuckCell?.className).toContain("max-w-52")
})

test("多种待验批次仍展示汇总文案", () => {
    render(
        <AcceptanceProgressTable
            progress={progress({
                pendingFactCount: 2,
                lines: [
                    {
                        salesOrderLineId: "sol-1",
                        lineNo: 1,
                        itemSnapshot: "礼盒",
                        unitCode: "盒",
                        requiredQuantity: "2",
                        deliveredQuantity: "2",
                        acceptedQuantity: "0",
                        pendingQuantity: "2",
                        pendingFacts: [
                            fact(),
                            fact({
                                fulfillmentLineId: "fl-2",
                                fulfillmentFactType: "SUPPLIER_DIRECT",
                                fulfillmentNo: "DN20260831-000003",
                            }),
                        ],
                        stuckKind: "accept",
                        stuckLabel: "待验 2 批 · 服务履约/代发",
                    },
                ],
            })}
        />,
    )

    expect(screen.getByText("待验 2 批 · 服务履约/代发")).toBeTruthy()
})

test("工作台传入的左右内边距会落到进度分区，并保留默认上下留白", () => {
    const { container } = render(
        <AcceptanceProgressTable progress={progress()} className="px-5" />,
    )
    const section = container.querySelector('[data-slot="document-section"]')
    expect(section?.className).toContain("px-5")
    expect(section?.className).toContain("py-5")
    expect(section?.className).not.toContain("py-0")
})
