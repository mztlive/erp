import * as React from "react"
import { cleanup, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { useAcceptanceForm } from "@/features/sales-orders/hooks/use-acceptance-form"
import { useAcceptanceSelection } from "@/features/sales-orders/hooks/use-acceptance-selection"
import {
    pendingAsPassSelection,
    pendingFactsOf,
} from "@/features/sales-orders/lib/acceptance-model"
import type {
    AcceptanceEligibleFact,
    AcceptanceSalesLineGroup,
} from "@/features/sales-orders/lib/acceptance-types"

import { AcceptanceRegisterDialog } from "./acceptance-register-dialog"

afterEach(cleanup)

function fact(
    overrides: Partial<AcceptanceEligibleFact> = {},
): AcceptanceEligibleFact {
    return {
        fulfillmentLineId: "fl-1",
        fulfillmentFactType: "SERVICE",
        fulfillmentNo: "SF-1",
        salesOrderLineId: "sol-1",
        lineNo: 1,
        itemSnapshot: "上门安装",
        unitCode: "次",
        occurredAt: "2026-08-31T00:00:00.000Z",
        netSuccessfulQuantity: "1",
        netAcceptedAllocatedQuantity: "0",
        eligibleQuantity: "1",
        ...overrides,
    }
}

function line(
    overrides: Partial<AcceptanceSalesLineGroup> &
        Pick<AcceptanceSalesLineGroup, "salesOrderLineId" | "lineNo">,
): AcceptanceSalesLineGroup {
    const itemSnapshot = overrides.itemSnapshot ?? `明细 ${overrides.lineNo}`
    return {
        requiredQuantity: "1",
        netAcceptedQuantity: "0",
        unitCode: "次",
        itemSnapshot,
        fulfillmentFacts: [
            fact({
                salesOrderLineId: overrides.salesOrderLineId,
                lineNo: overrides.lineNo,
                itemSnapshot,
                fulfillmentLineId: `fl-${overrides.lineNo}`,
                fulfillmentNo: `SF-${overrides.lineNo}`,
            }),
        ],
        ...overrides,
    }
}

function DialogHarness({
    salesLines,
}: {
    salesLines: AcceptanceSalesLineGroup[]
}) {
    const selection = useAcceptanceSelection()
    const primed = React.useRef(false)
    if (!primed.current) {
        primed.current = true
        selection.replace(pendingAsPassSelection(salesLines))
    }
    const { form, clientIssues } = useAcceptanceForm({
        selected: selection.selected,
        onValidSubmit: () => undefined,
    })
    return (
        <AcceptanceRegisterDialog
            open
            form={form}
            salesLines={salesLines}
            selection={selection}
            canPost
            ownerLabel="张三"
            isOwner
            clientIssues={clientIssues}
            pendingCount={pendingFactsOf(salesLines).length}
            postPending={false}
            onOpenChange={() => undefined}
        />
    )
}

test("多条明细时左侧切换，右侧只展示当前明细的批次", () => {
    render(
        <DialogHarness
            salesLines={[
                line({
                    salesOrderLineId: "sol-1",
                    lineNo: 1,
                    itemSnapshot: "上门派送",
                }),
                line({
                    salesOrderLineId: "sol-2",
                    lineNo: 2,
                    itemSnapshot: "上门安装",
                    unitCode: "次",
                }),
            ]}
        />,
    )

    expect(screen.getByRole("navigation", { name: "待验收明细" })).toBeTruthy()
    expect(screen.getByText("明细 1 · 上门派送")).toBeTruthy()
    expect(screen.queryByText("明细 2 · 上门安装")).toBeNull()

    fireEvent.click(screen.getByRole("button", { name: "明细 2，上门安装" }))
    expect(screen.getByText("明细 2 · 上门安装")).toBeTruthy()
    expect(screen.queryByText("明细 1 · 上门派送")).toBeNull()
})
