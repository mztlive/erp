import { describe, expect, it, vi } from "vitest"
import * as React from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { renderHook } from "@testing-library/react"

import type { CustomerQualityRow, CustomerQualityView } from "../types"
import { useCustomerQualityColumns } from "./use-customer-quality-columns"

const row: CustomerQualityRow = {
    customerId: "c1",
    customerNo: "NO-001",
    customerName: "示例客户",
    ownerLabels: ["张三", "李四"],
    tags: [
        {
            type: "scale",
            code: "s1",
            label: "头部客户",
            tone: "success",
            ruleVersion: "v1",
            explanation: "成交规模居前",
        },
    ],
    salesGrossAmount: "1,200.00",
    salesOrderCount: 3,
    voucherShare: "20%",
    nonVoucherShare: "80%",
    costCoveredNetRevenue: "800.00",
    costUncoveredNetRevenue: "200.00",
    costCoverageRate: "80.0%",
    actualProfitLossNet: "100.00",
    marginRate: "12%",
    receivableOpenGross: "500.00",
    overdueGross: "50.00",
    averageCollectionDays: "30",
    exceptionCounts: { return: 1, refund: 2 },
    firstBusinessAt: "2026-01-01T00:00:00+08:00",
    latestBusinessAt: "2026-07-01T00:00:00+08:00",
    scaleTierCode: "s1",
    profitTierCode: "p1",
    riskTierCode: "r1",
    cardFundsReviewInsufficient: false,
    allowedDrilldowns: ["W03", "W05", "W11", "W16"],
}

const view: CustomerQualityView = {
    scope: { id: "scope:team:sales-east", label: "华东", permissionVersion: "v1" },
    period: {
        from: "2026-01-01",
        to: "2026-06-30",
        basis: "BUSINESS_DATE",
        timezone: "Asia/Shanghai",
        selectionSource: "EXPLICIT",
    },
    freshness: {
        projectedAt: "2026-07-01T10:00:00+08:00",
        sourceWatermark: "outbox:cq:2026-07-01T10:00:00+08:00",
        state: "fresh",
    },
    coverage: {
        cardFundsReviewRate: "8/10",
        cardFundsReviewPercent: 80,
        reviewedVoucherOrderCount: 8,
        requiredVoucherOrderCount: 10,
        cardFundsState: "partial",
        costCoveredNetRevenue: "800.00",
        costUncoveredNetRevenue: "200.00",
        costCoverageRate: "80.0%",
        costCoveragePercent: 80,
        costCoverageState: "partial",
        costBasis: "ACTUAL",
    },
    metrics: [],
    dimensions: [],
    customers: { items: [row], total: 1, filteredTotal: 1 },
    filterSummary: "全部",
    canExport: true,
    tagRuleCatalog: {
        scale: { ruleVersion: "v1", explanation: "e", labels: {} },
        profit: { ruleVersion: "v1", explanation: "e", labels: {} },
        risk: { ruleVersion: "v1", explanation: "e", labels: {} },
    },
}

function renderColumns(
    data: CustomerQualityView | undefined,
    businessType?: "VOUCHER" | "GOODS_SERVICE",
    onTagClick: (tag: CustomerQualityRow["tags"][number]) => void = () => {},
) {
    return renderHook(() =>
        useCustomerQualityColumns({
            data,
            returnTo: "/analytics/customer-quality?page=1",
            businessType,
            onTagClick,
        }),
    ).result.current
}

function findHref(node: React.ReactNode): string | null {
    if (!React.isValidElement(node)) return null
    const props = node.props as Record<string, unknown>
    if (typeof props.href === "string") return props.href
    if (props.render) {
        const found = findHref(props.render as React.ReactNode)
        if (found) return found
    }
    const children = props.children as
        | React.ReactNode
        | readonly React.ReactNode[]
    if (Array.isArray(children)) {
        for (const child of children) {
            const found = findHref(child)
            if (found) return found
        }
        return null
    }
    return findHref(children as React.ReactNode)
}

function callCell(
    columns: ReturnType<typeof renderColumns>,
    id: string,
    original: CustomerQualityRow,
): React.ReactNode {
    const column = columns.find((c) => (c as { id?: string }).id === id)
    if (!column?.cell) throw new Error(`column ${id} has no cell`)
    return (
        column.cell as (ctx: unknown) => React.ReactNode
    )({ row: { original } })
}

describe("useCustomerQualityColumns", () => {
    it("builds the expected column set with Chinese headers", () => {
        const columns = renderColumns(view)
        expect(columns.map((c) => (c as { id?: string }).id)).toEqual([
            "customerName",
            "tags",
            "salesGrossAmount",
            "costCoverageRate",
            "actualProfitLossNet",
            "receivableOpenGross",
            "exceptions",
            "latestBusinessAt",
        ])
        expect(columns[0].header).toBe("客户")
        expect(columns[1].header).toBe("经营标签")
        expect(columns[2].header).toBe("成交金额（含税）")
        expect(columns[5].header).toBe("应收 / 逾期（含税）")
        expect(columns[6].header).toBe("异常")
    })

    it("links the customer name only when W03 drilldown is allowed", () => {
        const columns = renderColumns(view)
        const linked = callCell(columns, "customerName", row)
        expect(findHref(linked)).toMatch(/^\/sales\/customers\/c1\?/)
        expect(linked).toMatchObject({ type: "div" })

        const noDrill: CustomerQualityRow = {
            ...row,
            allowedDrilldowns: [],
        }
        const plain = callCell(columns, "customerName", noDrill)
        expect(findHref(plain)).toBeNull()
        const { container } = render(plain as React.ReactElement)
        expect(container.querySelector("a")).toBeNull()
        expect(container.textContent).toContain("示例客户")
        expect(container.textContent).toContain("NO-001")
    })

    it("renders tag badges that open the tag dialog on click", () => {
        const onTagClick = vi.fn()
        const columns = renderColumns(view, undefined, onTagClick)
        const node = callCell(columns, "tags", row) as React.ReactElement
        render(node)
        const button = screen.getByRole("button", {
            name: "头部客户：查看规则说明",
        })
        fireEvent.click(button)
        expect(onTagClick).toHaveBeenCalledWith(row.tags[0])
    })

    it("renders the sales amount with drilldown attrs and a query link", () => {
        const columns = renderColumns(view)
        const node = callCell(columns, "salesGrossAmount", row)
        expect(findHref(node)).toMatch(/^\/sales\/orders\?/)
        const link = node as React.ReactElement<Record<string, unknown>>
        expect(link.props["data-customer-id"]).toBe("c1")
        expect(link.props["data-focus-metric"]).toBe("salesGrossAmount")
        const params = new URLSearchParams(
            String(link.props.href).split("?")[1],
        )
        expect(params.get("nature")).toBeNull()
    })

    it("shows the raw sales amount without a link when W05 is not allowed", () => {
        const columns = renderColumns(view)
        const noDrill: CustomerQualityRow = {
            ...row,
            allowedDrilldowns: ["W03"],
        }
        const node = callCell(columns, "salesGrossAmount", noDrill)
        expect(findHref(node)).toBeNull()
        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("3 单 · 卡券占比 20%")
    })

    it("passes the business type through to the sales orders link", () => {
        const columns = renderColumns(view, "VOUCHER")
        const node = callCell(columns, "salesGrossAmount", row)
        const params = new URLSearchParams(
            String(findHref(node)).split("?")[1],
        )
        expect(params.get("nature")).toBe("card_voucher")
    })

    it("shows a placeholder when cost coverage numbers are missing", () => {
        const columns = renderColumns(view)
        const noCoverage: CustomerQualityRow = {
            ...row,
            costCoveredNetRevenue: null,
            costUncoveredNetRevenue: null,
            costCoverageRate: null,
        }
        const node = callCell(columns, "costCoverageRate", noCoverage)
        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toBe("卡券/未覆盖 — 不显示为 0")
    })

    it("renders coverage money values when present", () => {
        const columns = renderColumns(view)
        const node = callCell(columns, "costCoverageRate", row)
        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("覆盖")
        expect(container.textContent).toContain("未覆盖")
        expect(container.textContent).toContain("80.0%")
    })

    it("shows a placeholder for missing profit numbers and a link otherwise", () => {
        const columns = renderColumns(view)
        const missing: CustomerQualityRow = {
            ...row,
            actualProfitLossNet: null,
            marginRate: null,
        }
        const missingNode = callCell(columns, "actualProfitLossNet", missing)
        const { container } = render(missingNode as React.ReactElement)
        expect(container.textContent).toBe("暂无可靠口径")

        const linked = callCell(columns, "actualProfitLossNet", row)
        expect(findHref(linked)).toMatch(/^\/analytics\/profit-loss\?/)
        const link = linked as React.ReactElement<Record<string, unknown>>
        expect(link.props["data-focus-metric"]).toBe("actualProfitLossNet")
    })

    it("shows the plain profit amount when W16 is not allowed", () => {
        const columns = renderColumns(view)
        const noDrill: CustomerQualityRow = {
            ...row,
            allowedDrilldowns: ["W03"],
        }
        const node = callCell(columns, "actualProfitLossNet", noDrill)
        expect(findHref(node)).toBeNull()
    })

    it("marks insufficient card funds review and links overdue drilldowns", () => {
        const columns = renderColumns(view)
        const insufficient: CustomerQualityRow = {
            ...row,
            cardFundsReviewInsufficient: true,
        }
        const node = callCell(columns, "receivableOpenGross", insufficient)
        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("票款未复核")
        expect(findHref(node)).toMatch(/^\/finance\/customer-accounts\?/)
    })

    it("shows a dash for overdue when drilldown is not allowed", () => {
        const columns = renderColumns(view)
        const noDrill: CustomerQualityRow = {
            ...row,
            overdueGross: null,
            allowedDrilldowns: [],
        }
        const node = callCell(columns, "receivableOpenGross", noDrill)
        expect(findHref(node)).toBeNull()
        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("逾期")
    })

    it("joins exception counts with separators or shows a dash", () => {
        const columns = renderColumns(view)
        const withExceptions = callCell(columns, "exceptions", row)
        const { container } = render(withExceptions as React.ReactElement)
        expect(container.textContent).toBe("退货 1 · 退款 2")

        const none: CustomerQualityRow = {
            ...row,
            exceptionCounts: {},
        }
        const empty = callCell(columns, "exceptions", none)
        const emptyRender = render(empty as React.ReactElement)
        expect(emptyRender.container.textContent).toBe("—")
    })

    it("formats the latest business time or shows a dash", () => {
        const columns = renderColumns(view)
        const withTime = callCell(columns, "latestBusinessAt", row)
        const { container } = render(withTime as React.ReactElement)
        expect(container.textContent).toContain("2026")
        expect(container.textContent).not.toBe("—")

        const none: CustomerQualityRow = {
            ...row,
            latestBusinessAt: undefined,
        }
        const empty = callCell(columns, "latestBusinessAt", none)
        const emptyRender = render(empty as React.ReactElement)
        expect(emptyRender.container.textContent).toBe("—")
    })

    it("keeps drilldown links plain when no view data is provided", () => {
        const columns = renderColumns(undefined)
        const node = callCell(columns, "salesGrossAmount", row)
        expect(findHref(node)).toBeNull()
        const node2 = callCell(columns, "actualProfitLossNet", row)
        expect(findHref(node2)).toBeNull()
        const node3 = callCell(columns, "receivableOpenGross", row)
        expect(findHref(node3)).toBeNull()
    })
})
