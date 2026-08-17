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
                    title: "采购确认已通过 · 已形成采购创建依据",
                    description: "销售单已生效，采购创建依据已形成。",
                    reference: "pcb_1",
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
                        reference: "pcb_1",
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
                name: /采购确认已通过 · 已形成采购创建依据/,
            }),
        ).toBeTruthy()
        expect(screen.getByText("销售单已生效")).toBeTruthy()
        expect(screen.getByText("XS20260814170355")).toBeTruthy()
        expect(screen.queryByRole("alert")).toBeNull()
    })
})
