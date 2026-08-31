import * as React from "react"
import { cleanup, render, screen } from "@testing-library/react"
import { afterEach, expect, test } from "vitest"

import { useAcceptanceSelection } from "@/features/sales-orders/hooks/use-acceptance-selection"
import { defaultBatchDraft } from "@/features/sales-orders/lib/acceptance-model"
import type { AcceptanceEligibleFact } from "@/features/sales-orders/lib/acceptance-types"

import { AcceptanceRegisterBatchRow } from "./acceptance-register-batch-row"

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

function BatchRowHarness({
    source,
    skip = false,
}: {
    source: AcceptanceEligibleFact
    skip?: boolean
}) {
    const selection = useAcceptanceSelection()
    const primed = React.useRef(false)
    if (!primed.current) {
        primed.current = true
        if (!skip) {
            selection.replace(
                new Map([
                    [source.fulfillmentLineId, defaultBatchDraft(source)],
                ]),
            )
        }
    }
    return (
        <AcceptanceRegisterBatchRow
            fact={source}
            selection={selection}
            canPost
        />
    )
}

test("服务明细只提供通过、服务不通过和本次不验", () => {
    render(<BatchRowHarness source={fact()} />)

    expect(screen.getByRole("button", { name: "本次不验" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "服务不通过" })).toBeTruthy()
    expect(screen.queryByRole("button", { name: "短少" })).toBeNull()
    expect(screen.queryByRole("button", { name: "拒收" })).toBeNull()
})

test("商品明细只提供通过、短少、拒收和本次不验", () => {
    render(
        <BatchRowHarness
            source={fact({
                fulfillmentFactType: "SUPPLIER_DIRECT",
                fulfillmentNo: "DN-1",
                itemSnapshot: "礼盒",
                unitCode: "盒",
            })}
        />,
    )

    expect(screen.getByRole("button", { name: "本次不验" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "通过" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "短少" })).toBeTruthy()
    expect(screen.getByRole("button", { name: "拒收" })).toBeTruthy()
    expect(screen.queryByRole("button", { name: "服务不通过" })).toBeNull()
})

test("本次不验与结果选项同一组，选中后显示说明", () => {
    render(<BatchRowHarness source={fact()} skip />)

    expect(
        screen.getByText("本批不计入这次验收。需要验收时再选结果。"),
    ).toBeTruthy()
    expect(
        screen
            .getByRole("button", { name: "本次不验" })
            .getAttribute("aria-pressed"),
    ).toBe("true")
})
