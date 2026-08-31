import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { defaultBatchDraft } from "@/features/sales-orders/lib/acceptance-model"
import type {
    AcceptanceEligibleFact,
    AcceptanceHistoryItem,
} from "@/features/sales-orders/lib/acceptance-types"

import { AcceptanceDialogs } from "./acceptance-dialogs"

afterEach(cleanup)

function fact(
    overrides: Partial<AcceptanceEligibleFact> &
        Pick<AcceptanceEligibleFact, "fulfillmentLineId" | "salesOrderLineId">,
): AcceptanceEligibleFact {
    return {
        fulfillmentFactType: "WAREHOUSE_SHIP",
        fulfillmentNo: "FH-1",
        lineNo: 1,
        itemSnapshot: "打印纸",
        unitCode: "包",
        occurredAt: "2026-08-01T00:00:00.000Z",
        netSuccessfulQuantity: "10",
        netAcceptedAllocatedQuantity: "0",
        eligibleQuantity: "10",
        ...overrides,
    }
}

test("确认层回显上级勾选的批次和结果", () => {
    const pass = fact({
        fulfillmentLineId: "f-1",
        salesOrderLineId: "sol-1",
        fulfillmentNo: "FH-1",
    })
    const short = fact({
        fulfillmentLineId: "f-2",
        salesOrderLineId: "sol-1",
        fulfillmentNo: "FH-2",
    })
    const selected = new Map([
        [pass.fulfillmentLineId, defaultBatchDraft(pass)],
        [
            short.fulfillmentLineId,
            {
                ...defaultBatchDraft(short),
                result: "SHORT" as const,
                exceptionQty: "2",
                reason: "外箱破损少两包",
            },
        ],
    ])

    render(
        <AcceptanceDialogs
            confirmOpen
            onConfirmOpenChange={() => undefined}
            selected={selected}
            overallPreview="SHORT"
            onConfirmAcceptance={async () => undefined}
            reverseTarget={null}
            onReverseOpenChange={() => undefined}
            reverseReason=""
            onReverseReasonChange={() => undefined}
            onConfirmReverse={async () => undefined}
            exitDiscardOpen={false}
            onExitDiscardOpenChange={() => undefined}
            onConfirmExit={() => undefined}
        />,
    )

    const title = screen.getByText("确认客户验收")
    const status = screen.getByLabelText("状态变化")
    const firstItem = screen.getAllByText("打印纸")[0]
    const header = title.closest("[data-slot=alert-dialog-header]")
    expect(firstItem).toBeTruthy()
    expect(header?.contains(status)).toBe(true)
    expect(header?.contains(firstItem as Node)).toBe(false)
    expect(
        title.compareDocumentPosition(status) &
            Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0)
    expect(
        status.compareDocumentPosition(firstItem as Node) &
            Node.DOCUMENT_POSITION_FOLLOWING,
    ).not.toBe(0)
    expect(screen.getByText("核对下面 2 个批次的验收结果。")).toBeTruthy()
    expect(screen.getByLabelText("本次验收批次")).toBeTruthy()
    expect(screen.getByText("仓发 FH-1")).toBeTruthy()
    expect(screen.getByText("通过 10 包")).toBeTruthy()
    expect(screen.getByText("仓发 FH-2")).toBeTruthy()
    expect(screen.getByText("短少 2 包、通过 8 包")).toBeTruthy()
    expect(screen.getByText("外箱破损少两包")).toBeTruthy()
    expect(screen.queryByText("记下本次客户验收结果")).toBeNull()
    expect(screen.queryByText("下一责任部门")).toBeNull()
    expect(screen.queryByText("请核对状态变化和业务影响后再继续。")).toBeNull()
})

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
        recordedBy: "u1",
        version: 1,
        factOnlyNotice: "",
        ...overrides,
    }
}

test("冲正确认层只保留状态变化和理由", () => {
    render(
        <AcceptanceDialogs
            confirmOpen={false}
            onConfirmOpenChange={() => undefined}
            selected={new Map()}
            overallPreview="PASS"
            onConfirmAcceptance={async () => undefined}
            reverseTarget={historyItem()}
            onReverseOpenChange={() => undefined}
            reverseReason=""
            onReverseReasonChange={() => undefined}
            onConfirmReverse={async () => undefined}
            exitDiscardOpen={false}
            onExitDiscardOpenChange={() => undefined}
            onConfirmExit={() => undefined}
        />,
    )

    const title = screen.getByText("冲正 YS-1？")
    expect(title).toBeTruthy()
    const reason = screen.getByLabelText(/冲正理由/)
    const header = title.closest("[data-slot=alert-dialog-header]")
    expect(header?.contains(reason)).toBe(false)
    expect(
        screen.getByText(
            "原记录会保留，并增加一条冲正记录；对应批次重新变为待验。",
        ),
    ).toBeTruthy()
    expect(screen.getByText("已冲正")).toBeTruthy()
    expect(screen.getByPlaceholderText("说明误录原因")).toBeTruthy()
    expect(screen.queryByText("提交后锁定字段")).toBeNull()
    expect(screen.queryByText("本次动作产生的影响")).toBeNull()
    expect(screen.queryByText("下一责任部门")).toBeNull()
    expect(screen.queryByText("请核对状态变化和业务影响后再继续。")).toBeNull()
    expect(screen.queryByText("请填写冲正理由")).toBeNull()
    expect(
        screen
            .getByRole("button", { name: "确认冲正" })
            .hasAttribute("disabled"),
    ).toBe(true)
    expect(screen.getByRole("button", { name: "取消" })).toBeTruthy()
})

test("冲正确认层不把内部单号写进标题", () => {
    render(
        <AcceptanceDialogs
            confirmOpen={false}
            onConfirmOpenChange={() => undefined}
            selected={new Map()}
            overallPreview="PASS"
            onConfirmAcceptance={async () => undefined}
            reverseTarget={historyItem({
                acceptanceNo: "REV-REV-YS-91b74dafa79333f7f1dda09f7",
            })}
            onReverseOpenChange={() => undefined}
            reverseReason=""
            onReverseReasonChange={() => undefined}
            onConfirmReverse={async () => undefined}
            exitDiscardOpen={false}
            onExitDiscardOpenChange={() => undefined}
            onConfirmExit={() => undefined}
        />,
    )

    expect(screen.getByText("冲正这条验收记录？")).toBeTruthy()
    expect(screen.queryByText(/91b74dafa79333f7f1dda09f7/)).toBeNull()
})
