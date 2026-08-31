import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { AcceptanceHistoryList } from "./acceptance-history-list"
import { FACT_ONLY_NOTICE } from "@/features/sales-orders/lib/acceptance-types"
import type { AcceptanceHistoryItem } from "@/features/sales-orders/lib/acceptance-types"

afterEach(cleanup)

function historyItem(
    overrides: Partial<AcceptanceHistoryItem> = {},
): AcceptanceHistoryItem {
    return {
        acceptanceId: "acc-1",
        acceptanceNo: "YS-1",
        status: "POSTED",
        acceptedAt: "2026-08-01T00:00:00.000Z",
        postedAt: "2026-08-01T00:00:00.000Z",
        overallResult: "PASS",
        lines: [],
        recordedBy: "张三",
        version: 1,
        factOnlyNotice: FACT_ONLY_NOTICE,
        ...overrides,
    }
}

test("没有记录时展示空状态", () => {
    render(
        <AcceptanceHistoryList
            history={[]}
            canReverse={false}
            onReverse={() => undefined}
        />,
    )

    expect(screen.getByText("还没有验收记录。")).toBeTruthy()
    expect(screen.queryByLabelText("验收记录时间线")).toBeNull()
})

test("按验收时间由新到旧排成时间线，并保留冲正入口", () => {
    render(
        <AcceptanceHistoryList
            history={[
                historyItem({
                    acceptanceId: "acc-old",
                    acceptanceNo: "YS-1",
                    acceptedAt: "2026-08-01T00:00:00.000Z",
                }),
                historyItem({
                    acceptanceId: "acc-new",
                    acceptanceNo: "YS-2",
                    acceptedAt: "2026-08-10T00:00:00.000Z",
                    overallResult: "SHORT",
                    comment: "少两包",
                    lines: [
                        {
                            salesOrderLineId: "sol-1",
                            lineNo: 1,
                            itemSnapshot: "年节礼包",
                            unitCode: "包",
                            acceptedQuantity: "8",
                            shortQuantity: "2",
                            rejectedQuantity: "0",
                            allocations: [],
                        },
                    ],
                }),
            ]}
            canReverse
            onReverse={() => undefined}
        />,
    )

    const timeline = screen.getByLabelText("验收记录时间线")
    expect(timeline.getAttribute("data-slot")).toBe("timeline")
    const numbers = screen.getAllByText(/YS-\d/)
    expect(numbers[0]?.textContent).toContain("YS-2")
    expect(numbers[1]?.textContent).toContain("YS-1")
    expect(screen.getByText("少两包")).toBeTruthy()
    expect(screen.getByText("1 · 年节礼包 · 通过 8 包、短少 2 包")).toBeTruthy()
    expect(screen.getAllByText("张三")).toHaveLength(2)
    expect(
        document.getElementById(
            "sales-orders-acceptance-history-acc-new-reverse",
        ),
    ).toBeTruthy()
    expect(
        document.getElementById(
            "sales-orders-acceptance-history-acc-old-reverse",
        ),
    ).toBeTruthy()
})

test("已冲正和冲正记录不展示冲正按钮，并互相引用单号", () => {
    render(
        <AcceptanceHistoryList
            history={[
                historyItem({
                    acceptanceId: "acc-orig",
                    acceptanceNo: "YS-1",
                    status: "REVERSED",
                    reversedByAcceptanceId: "acc-rev",
                }),
                historyItem({
                    acceptanceId: "acc-rev",
                    acceptanceNo: "YS-2",
                    reversalOfAcceptanceId: "acc-orig",
                    acceptedAt: "2026-08-02T00:00:00.000Z",
                }),
            ]}
            canReverse
            onReverse={() => undefined}
        />,
    )

    expect(screen.queryByRole("button", { name: "冲正误录" })).toBeNull()
    expect(screen.getByText("冲正 YS-1")).toBeTruthy()
    expect(screen.getByText("已被 YS-2 冲正")).toBeTruthy()
    expect(screen.getByText("冲正记录")).toBeTruthy()
    expect(screen.getByText("已冲正")).toBeTruthy()
})

test("时间线不展示内部验收单号", () => {
    render(
        <AcceptanceHistoryList
            history={[
                historyItem({
                    acceptanceNo: "REV-REV-YS-91b74dafa79333f7f1dda09f7",
                }),
            ]}
            canReverse={false}
            onReverse={() => undefined}
        />,
    )

    expect(screen.getByText("通过")).toBeTruthy()
    expect(screen.queryByText(/91b74dafa79333f7f1dda09f7/)).toBeNull()
})
