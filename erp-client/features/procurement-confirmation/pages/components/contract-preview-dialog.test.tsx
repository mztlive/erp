import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it } from "vitest"

import { ContractPreviewDialog } from "./contract-preview-dialog"

afterEach(() => {
    cleanup()
})

describe("ContractPreviewDialog", () => {
    it("shows the sales submission contract and customer snapshot", () => {
        render(
            <ContractPreviewDialog
                open
                onOpenChange={() => undefined}
                contractSnapshot="HT-2026-001"
                customerSnapshot="演示客户"
                paymentTermLabel="月结30天"
            />,
        )
        expect(screen.getByText("销售提交中的合同快照")).toBeTruthy()
        expect(screen.getByText("HT-2026-001")).toBeTruthy()
        expect(screen.getByText("演示客户")).toBeTruthy()
        expect(screen.getByText("月结30天")).toBeTruthy()
        expect(
            screen.getByText(
                "采购确认以本次销售提交中的合同与客户为准，不读取客户主数据。",
            ),
        ).toBeTruthy()
    })
})
