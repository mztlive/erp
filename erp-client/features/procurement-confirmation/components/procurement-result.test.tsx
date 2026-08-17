import type { ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { ProcurementOutcomeFeedback } from "./procurement-result"

afterEach(() => {
    cleanup()
})

vi.mock("next/link", () => ({
    default: ({ href, children }: { href: string; children?: ReactNode }) => (
        <a href={href}>{children}</a>
    ),
}))

describe("ProcurementOutcomeFeedback", () => {
    it("shows the confirmation result in a dialog instead of an inline alert", () => {
        render(
            <ProcurementOutcomeFeedback
                finishedResult={null}
                lastResult={{
                    status: "succeeded",
                    title: "采购确认已通过 · 采购单已生成",
                    description: "销售单已生效，采购单 PO-1 已生成。",
                    reference: "po_1",
                    stayOnItem: true,
                    outcome: {
                        kind: "APPROVED_AND_SALES_EFFECTIVE",
                        procurementConfirmationId: "conf_1",
                        salesOrderId: "so_1",
                        salesOrderNo: "XS20260814170355",
                        submissionId: "sub_1",
                        subjectHash: "sub_1",
                        salesOrderRevisionId: "sr_1",
                        receivableAccountId: "ra_1",
                        procurementCreationBasisId: "pcb_1",
                        purchaseOrders: [
                            { purchaseOrderId: "po_1", purchaseNo: "PO-1" },
                        ],
                        reference: "po_1",
                    },
                }}
                returnTo="/procurement/confirm"
                resultRef={{ current: null }}
                onDismissFinished={() => undefined}
                onDismissLastResult={() => undefined}
                onNext={() => undefined}
            />,
        )

        expect(
            screen.getByRole("dialog", {
                name: /采购确认已通过 · 采购单已生成/,
            }),
        ).toBeTruthy()
        expect(screen.getByText("销售单已生效")).toBeTruthy()
        expect(screen.getByText("XS20260814170355")).toBeTruthy()
        expect(
            screen
                .getByRole("link", { name: "查看采购单" })
                .getAttribute("href"),
        ).toBe("/procurement/orders/po_1?mode=edit")
        expect(screen.queryByRole("alert")).toBeNull()
    })
})
