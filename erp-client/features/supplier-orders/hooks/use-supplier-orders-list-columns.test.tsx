import { cleanup, fireEvent, render, renderHook } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import type { ReactNode } from "react"

afterEach(cleanup)

vi.mock("next/link", () => ({
    default: (props: {
        href?: string
        className?: string
        children?: ReactNode
    }) => <a {...props}>{props.children}</a>,
}))

import type { ColumnDef } from "@tanstack/react-table"
import type { SupplierOrderListRow } from "@/features/supplier-orders/types"
import { useSupplierOrdersListColumns } from "./use-supplier-orders-list-columns"

function makeRow(
    overrides: Partial<SupplierOrderListRow> = {},
): SupplierOrderListRow {
    return {
        orderId: "so_1",
        orderNo: "SFO-1001",
        mallOrderId: "mall_1",
        mallOrderNo: "MO-2001",
        supplierId: "sup_1",
        supplierName: "华东供应商",
        externalOrderNo: "EXT-3001",
        fulfillmentStatus: "SUBMITTING",
        fulfillmentLabel: "提交中",
        fulfillmentTone: "info",
        cancelStatus: "NONE",
        cancelLabel: "未发起",
        cancelTone: "neutral",
        refundStatus: "NONE",
        refundLabel: "未发起",
        refundTone: "neutral",
        paidAt: "2026-08-01T00:00:00.000Z",
        updatedAt: "2026-08-02T00:00:00.000Z",
        lastBusinessAt: "2026-08-02T00:00:00.000Z",
        itemCount: 2,
        allowedActions: ["OPEN_CENTER", "NOTE"],
        actionBlockers: [],
        priority: 70,
        ...overrides,
    }
}

type ColumnsProps = Parameters<typeof useSupplierOrdersListColumns>[0]

function makeProps(overrides: Partial<ColumnsProps> = {}): ColumnsProps {
    return {
        rows: [makeRow()],
        focusedIndex: 0,
        rowRefs: { current: new Map<string, HTMLElement>() },
        onPreview: vi.fn(),
        onQueryResult: vi.fn(async () => {}),
        queryPending: false,
        ...overrides,
    }
}

function renderColumns(props: ColumnsProps) {
    return renderHook(() => useSupplierOrdersListColumns(props))
}

function renderCell(
    columns: ColumnDef<SupplierOrderListRow>[],
    id: string,
    row: SupplierOrderListRow,
) {
    const column = columns.find((c) => c.id === id)
    if (!column?.cell) throw new Error(`column ${id} missing cell`)
    const cell = column.cell as (ctx: {
        row: { original: SupplierOrderListRow }
    }) => ReactNode
    return render(<>{cell({ row: { original: row } })}</>)
}

describe("useSupplierOrdersListColumns — column shape", () => {
    it("exposes the pinned identity and action columns in order", () => {
        const { result } = renderColumns(makeProps())

        expect(result.current.map((c) => c.id)).toEqual([
            "identity",
            "mall",
            "tracks",
            "external",
            "updated",
            "itemCount",
            "actions",
        ])
        expect(result.current.map((c) => c.header)).toEqual([
            "供应商订单",
            "商城单号",
            "履约 / 取消 / 退款",
            "外部单号",
            "更新时间",
            "商品数",
            "操作",
        ])
    })

    it("disables sorting only on the tracks and actions columns", () => {
        const { result } = renderColumns(makeProps())
        for (const column of result.current) {
            const expected = column.id === "tracks" || column.id === "actions"
            expect(column.enableSorting).toBe(expected ? false : undefined)
        }
    })

    it("marks the focused row with tabIndex 0 and data-focused", () => {
        const props = makeProps({
            rows: [makeRow({ orderId: "so_a" }), makeRow({ orderId: "so_b" })],
            focusedIndex: 1,
        })
        const { result } = renderColumns(props)

        const first = renderCell(result.current, "identity", props.rows[0]!)
        expect(first.container.querySelector("[data-focused]")).toBeNull()
        first.unmount()

        const second = renderCell(result.current, "identity", props.rows[1]!)
        expect(
            second.container
                .querySelector('[data-focused="true"]')
                ?.getAttribute("tabindex"),
        ).toBe("0")
    })

    it("registers the row element in rowRefs for focus restoration", () => {
        const props = makeProps()
        const { result } = renderColumns(props)

        const view = renderCell(result.current, "identity", props.rows[0]!)
        expect(props.rowRefs.current.get("so_1")).toBeInstanceOf(HTMLDivElement)
        view.unmount()
        expect(props.rowRefs.current.get("so_1")).toBeUndefined()
    })
})

describe("useSupplierOrdersListColumns — identity cell", () => {
    it("shows the order number and supplier name and opens the preview", () => {
        const props = makeProps()
        const { result } = renderColumns(props)
        const view = renderCell(result.current, "identity", props.rows[0]!)

        expect(view.getByText("SFO-1001")).toBeTruthy()
        expect(view.getByText("华东供应商")).toBeTruthy()

        fireEvent.click(view.getByLabelText("预览 SFO-1001"))
        expect(props.onPreview).toHaveBeenCalledWith("so_1")
    })
})

describe("useSupplierOrdersListColumns — tracks cell", () => {
    it("renders the three track statuses", () => {
        const { result } = renderColumns(makeProps())
        const view = renderCell(result.current, "tracks", makeRow())

        expect(view.getByText("履约")).toBeTruthy()
        expect(view.getByText("提交中")).toBeTruthy()
        expect(view.getByText("取消")).toBeTruthy()
        expect(view.getByText("退款")).toBeTruthy()
    })
})

describe("useSupplierOrdersListColumns — external cell", () => {
    it("falls back to 尚未返回 without an external order number", () => {
        const { result } = renderColumns(makeProps())
        const view = renderCell(
            result.current,
            "external",
            makeRow({ externalOrderNo: undefined }),
        )
        expect(view.getByText("尚未返回")).toBeTruthy()
    })
})

describe("useSupplierOrdersListColumns — actions cell", () => {
    it("offers query for unknown results and wires the handler", () => {
        const props = makeProps({
            rows: [
                makeRow({
                    fulfillmentStatus: "RESULT_UNKNOWN",
                    allowedActions: ["OPEN_CENTER", "QUERY_RESULT"],
                }),
            ],
        })
        const { result } = renderColumns(props)
        const view = renderCell(result.current, "actions", props.rows[0]!)

        fireEvent.click(view.getByText("查询原结果"))
        expect(props.onQueryResult).toHaveBeenCalledWith(props.rows[0])
    })

    it("disables the query button without permission and explains the blocker", () => {
        const props = makeProps({
            rows: [
                makeRow({
                    fulfillmentStatus: "RESULT_UNKNOWN",
                    allowedActions: ["OPEN_CENTER"],
                    actionBlockers: [
                        {
                            action: "QUERY_RESULT",
                            code: "NO_QUERY",
                            message: "需先进入任务处理",
                            destinationWorkspaceId: "ws_errors",
                        },
                    ],
                }),
            ],
        })
        const { result } = renderColumns(props)
        const view = renderCell(result.current, "actions", props.rows[0]!)

        expect(
            (view.getByText("查询原结果") as HTMLButtonElement).disabled,
        ).toBe(true)
        expect(view.getByText(/需先进入任务处理/)).toBeTruthy()
        expect(view.getByText("前往接口错误中心")).toBeTruthy()
    })

    it("hides query controls for non-unknown fulfillment states", () => {
        const props = makeProps()
        const { result } = renderColumns(props)
        const view = renderCell(result.current, "actions", props.rows[0]!)

        expect(view.queryByText("查询原结果")).toBeNull()
        expect(view.getByText("预览")).toBeTruthy()
        expect(view.getByText("详情")).toBeTruthy()

        fireEvent.click(view.getByText("预览"))
        expect(props.onPreview).toHaveBeenCalledWith("so_1")
    })
})
