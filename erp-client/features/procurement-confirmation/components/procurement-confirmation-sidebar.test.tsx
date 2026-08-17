import type { ReactNode } from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"

import { makeTask } from "../pages/hooks/test-data"
import { ProcurementConfirmationSidebar } from "./procurement-confirmation-sidebar"

vi.mock("next/link", () => ({
    default: ({ href, children }: { href: string; children?: ReactNode }) => (
        <a href={href}>{children}</a>
    ),
}))

afterEach(() => {
    cleanup()
})

const noop = async () => undefined

function renderSidebar(
    allowedActions: readonly string[],
    formalPending = false,
) {
    render(
        <ProcurementConfirmationSidebar
            task={makeTask({ allowedActions })}
            headingRef={{ current: null }}
            formalPending={formalPending}
            onReject={noop}
            onConfirm={noop}
            onStartProcessing={noop}
            onReleaseToTeam={noop}
            coverage={[]}
            estimatedPurchase={undefined}
            lineDrafts={[]}
            recommendation={undefined}
            clientBlocking={[]}
            salesOrderHref="/sales/orders/so_1"
        />,
    )
}

describe("ProcurementConfirmationSidebar", () => {
    it("shows 确认通过 once the operator can save the confirmation plan", () => {
        renderSidebar(["SAVE", "REJECT"])
        expect(screen.getByRole("button", { name: "确认通过" })).toBeTruthy()
        expect(
            screen.getByText("打开确认方案后，补齐供应商与数量再提交通过。"),
        ).toBeTruthy()
    })

    it("does not show 确认通过 before the operator can work the confirmation", () => {
        renderSidebar(["START_PROCESSING"])
        expect(screen.queryByRole("button", { name: "确认通过" })).toBeNull()
        expect(screen.getByRole("button", { name: "开始处理" })).toBeTruthy()
    })
})
