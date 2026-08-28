import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { PayablePreviewBody } from "./payable-preview-body"
import type { PayableActivityItem } from "@/features/supplier-payables/lib/payable-preview-activity"
import type { PayableRow } from "@/features/supplier-payables/types"

afterEach(() => {
    cleanup()
})

const payable: PayableRow = {
    payableAccountId: "pa-1",
    supplierId: "sup-1",
    supplierName: "华东供应商",
    sourceType: "PURCHASE_ORDER",
    sourceTypeLabel: "采购单",
    sourceDocumentId: "po-1",
    sourceDocumentNo: "PO-1001",
    sourceHref: "/procurement/orders/po-1",
    primaryEntryId: "pe-1",
    entryLockVersion: 1,
    accountLockVersion: 1,
    grossTotal: "20.00",
    settledTotal: "8.00",
    openTotal: "12.00",
    invoicedTotal: "4.00",
    openInvoiceableTotal: "16.00",
    dueDate: "2026-09-01",
    dueState: "overdue",
    dueStateLabel: "已到期",
    status: "PARTIAL",
    statusLabel: "部分结清",
    statusTone: "warning",
    paymentGateSummary: {
        state: "BLOCKED",
        message: "先款未达门槛",
        required: "10.00",
        allocated: "8.00",
        gap: "2.00",
    },
    allowedActions: ["VIEW_DETAIL"],
    actionBlockers: [],
}

describe("PayablePreviewBody", () => {
    it("展示双轨进度、构成和往来，先款未满足时给出警示", () => {
        const activity: PayableActivityItem[] = [
            {
                id: "payment:a1",
                track: "payment",
                trackLabel: "付款",
                actionLabel: "核销",
                documentNo: "FK-12",
                href: "/procurement/orders/po-1",
                amount: "8.00",
                occurredAt: "2026-03-12T00:00:00.000Z",
            },
        ]

        render(
            <PayablePreviewBody
                payable={payable}
                entries={[
                    {
                        entryId: "pe-1",
                        entryTypeLabel: "原始应付",
                        direction: "increase",
                        amount: "20.00",
                        sourceLabel: "PO-1001",
                        dueDate: "2026-09-01",
                        occurredAt: "2026-03-01T00:00:00.000Z",
                    },
                ]}
                activity={activity}
                paymentBlockedReason="付款需从工作台的供应商付款任务进入。"
            />,
        )

        expect(screen.getByText("付款进度")).toBeTruthy()
        expect(screen.getByText("收票进度")).toBeTruthy()
        expect(screen.getByText("先款条件未满足")).toBeTruthy()
        expect(screen.getByText("原始应付")).toBeTruthy()
        expect(screen.getByText(/付款 · 核销/)).toBeTruthy()
        expect(screen.getByRole("link")).toBeTruthy()
        expect(
            screen.getByText("付款需从工作台的供应商付款任务进入。"),
        ).toBeTruthy()
        expect(screen.queryByRole("button", { name: "登记付款" })).toBeNull()
    })

    it("先款已满足时不展示警示，无往来时给出空态", () => {
        render(
            <PayablePreviewBody
                payable={{
                    ...payable,
                    paymentGateSummary: {
                        state: "SATISFIED",
                        message: "已满足",
                        required: "10.00",
                        allocated: "10.00",
                        gap: "0.00",
                    },
                }}
                entries={[]}
                activity={[]}
            />,
        )

        expect(screen.queryByText("先款条件未满足")).toBeNull()
        expect(screen.getByText("暂无分录")).toBeTruthy()
        expect(screen.getByText("尚无付款或进项核销记录")).toBeTruthy()
    })
})
