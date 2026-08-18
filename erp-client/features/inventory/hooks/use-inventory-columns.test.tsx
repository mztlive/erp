import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import * as React from "react"
import {
    cleanup,
    fireEvent,
    render,
    renderHook,
    screen,
} from "@testing-library/react"
import type { ColumnDef } from "@tanstack/react-table"

import { useInventoryColumns } from "./use-inventory-columns"
import type { InventoryColumnsInput } from "./use-inventory-columns"
import type {
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"

vi.mock("next/link", () => ({
    default: ({
        href,
        children,
        ...props
    }: {
        href?: string
        children?: React.ReactNode
    } & Record<string, unknown>) => (
        <a href={href} {...props}>
            {children}
        </a>
    ),
}))

afterEach(() => {
    cleanup()
})

function makeBalanceRow(
    overrides: Partial<StockBalanceRow> = {},
): StockBalanceRow {
    return {
        balanceId: "b1",
        warehouseId: "wh1",
        warehouseCode: "WH01",
        warehouseName: "主仓",
        skuId: "sku1",
        skuCode: "SKU-1",
        skuName: "示例商品",
        specSummary: "500ml",
        baseUnit: "件",
        onHandQuantity: "10",
        reservedQuantity: "2",
        availableQuantity: "8",
        lockVersion: 1,
        lastMovementId: "",
        lastMovementAt: "",
        lastMovementTypeLabel: "",
        availability: "positive",
        statusLabel: "有可用",
        statusTone: "success",
        hasActiveReservation: false,
        stockKind: "OWN_PHYSICAL",
        allowedActions: ["CREATE_ADJUSTMENT", "VIEW_SOURCE"],
        actionBlockers: [],
        ...overrides,
    }
}

function makeMovementRow(
    overrides: Partial<StockMovementRow> = {},
): StockMovementRow {
    return {
        movementId: "m1",
        balanceId: "b1",
        warehouseId: "wh1",
        warehouseName: "主仓",
        skuId: "sku1",
        skuCode: "SKU-1",
        skuName: "示例商品",
        baseUnit: "件",
        movementType: "PURCHASE_RECEIPT",
        movementTypeLabel: "采购入库",
        direction: "increase",
        quantity: "5",
        occurredAt: "2026-08-14T00:00:00.000Z",
        recordedAt: "2026-08-14T00:01:00.000Z",
        recordedByLabel: "张三",
        sourceDocumentType: "PURCHASE_RECEIPT",
        sourceDocumentId: "pr-1",
        sourceDocumentNo: "PR-1",
        sourceHref: "/fulfillment?sourceDocId=pr-1",
        ...overrides,
    }
}

function makeReservationRow(
    overrides: Partial<StockReservationRow> = {},
): StockReservationRow {
    return {
        reservationId: "r1",
        balanceId: "b1",
        warehouseId: "wh1",
        warehouseName: "主仓",
        skuId: "sku1",
        skuCode: "SKU-1",
        skuName: "示例商品",
        baseUnit: "件",
        salesOrderId: "so1",
        salesOrderNo: "SO-1",
        salesOrderLineId: "sol-1",
        salesOrderLineLabel: "销售明细一",
        establishedQuantity: "3",
        consumedQuantity: "1",
        releasedQuantity: "0",
        remainingQuantity: "2",
        status: "ACTIVE",
        statusLabel: "有效",
        statusTone: "success",
        establishedAt: "",
        fulfillmentHref: "/fulfillment?lane=warehouse",
        ...overrides,
    }
}

function makeAdjustmentRow(
    overrides: Partial<StockAdjustmentRow> = {},
): StockAdjustmentRow {
    return {
        adjustmentId: "adj-1",
        adjustmentNo: "TZ1",
        balanceId: "b1",
        warehouseId: "wh1",
        warehouseName: "主仓",
        skuId: "sku1",
        skuCode: "SKU-1",
        skuName: "示例商品",
        baseUnit: "件",
        reasonType: "COUNT_LOSS",
        reasonTypeLabel: "盘亏",
        direction: "decrease",
        quantity: "2",
        status: "IN_APPROVAL",
        statusLabel: "审批中",
        statusTone: "warning",
        operatorLabel: "张三",
        createdAt: "2026-08-14T00:00:00.000Z",
        ...overrides,
    }
}

type CellContext = { row: { original: unknown } }

function callCell<T>(
    columns: ColumnDef<T>[],
    id: string,
    original: T,
): React.ReactNode {
    const column = columns.find((c) => (c as { id?: string }).id === id)
    if (!column?.cell) throw new Error(`column ${id} has no cell`)
    return (column.cell as (ctx: CellContext) => React.ReactNode)({
        row: { original },
    })
}

function setup(overrides: Partial<InventoryColumnsInput> = {}) {
    const rowFocusRef: InventoryColumnsInput["rowFocusRef"] = {
        current: new Map(),
    }
    const openDetail = vi.fn()
    const startAdjustment = vi.fn().mockResolvedValue(undefined)
    const input: InventoryColumnsInput = {
        isPhoneNarrow: false,
        rowFocusRef,
        openDetail,
        startAdjustment,
        ...overrides,
    }
    const utils = renderHook(
        (props: InventoryColumnsInput) => useInventoryColumns(props),
        { initialProps: input },
    )
    return { ...utils, input, rowFocusRef, openDetail, startAdjustment }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useInventoryColumns", () => {
    it("builds the four view column sets with Chinese headers", () => {
        const { result } = setup()

        expect(
            result.current.balanceColumns.map((c) => (c as { id?: string }).id),
        ).toEqual([
            "identity",
            "onHand",
            "reserved",
            "available",
            "status",
            "lastMovement",
            "actions",
        ])
        expect(
            result.current.movementColumns.map(
                (c) => (c as { id?: string }).id,
            ),
        ).toEqual(["identity", "type", "qty", "occurred", "source", "recorder"])
        expect(
            result.current.reservationColumns.map(
                (c) => (c as { id?: string }).id,
            ),
        ).toEqual(["identity", "sales", "qty", "status", "source", "actions"])
        expect(
            result.current.adjustmentColumns.map(
                (c) => (c as { id?: string }).id,
            ),
        ).toEqual(["doc", "identity", "status", "people", "time"])

        expect(result.current.balanceColumns[1].header).toBe("账面现存")
        expect(result.current.movementColumns[1].header).toBe("流水类型")
        expect(result.current.reservationColumns[2].header).toBe("建立 / 剩余")
        expect(result.current.adjustmentColumns[4].header).toBe(
            "创建 / 确认入账",
        )
    })

    it("keeps column arrays stable across re-renders and rebuilds when the actions change", () => {
        const { result, rerender, input } = setup()

        const first = {
            balance: result.current.balanceColumns,
            movement: result.current.movementColumns,
            reservation: result.current.reservationColumns,
            adjustment: result.current.adjustmentColumns,
        }
        rerender(input)
        expect(result.current.balanceColumns).toBe(first.balance)
        expect(result.current.movementColumns).toBe(first.movement)
        expect(result.current.reservationColumns).toBe(first.reservation)
        expect(result.current.adjustmentColumns).toBe(first.adjustment)

        rerender({
            ...input,
            startAdjustment: vi.fn().mockResolvedValue(undefined),
        })
        expect(result.current.balanceColumns).not.toBe(first.balance)
        expect(result.current.movementColumns).toBe(first.movement)
    })

    it("renders the balance identity cell with warehouse and sku info", () => {
        const { result } = setup()
        const node = callCell(
            result.current.balanceColumns,
            "identity",
            makeBalanceRow(),
        )

        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("主仓")
        expect(container.textContent).toContain("WH01")
        expect(container.textContent).toContain("SKU-1")
        expect(container.textContent).toContain("示例商品")
        expect(container.textContent).toContain("500ml")
    })

    it("marks zero available quantity with a warning badge", () => {
        const { result } = setup()

        const zeroNode = callCell(
            result.current.balanceColumns,
            "available",
            makeBalanceRow({ availableQuantity: "0" }),
        )
        const zero = render(zeroNode as React.ReactElement)
        expect(zero.container.textContent).toContain("零可用")

        const positiveNode = callCell(
            result.current.balanceColumns,
            "available",
            makeBalanceRow({ availableQuantity: "8" }),
        )
        const positive = render(positiveNode as React.ReactElement)
        expect(positive.container.textContent).not.toContain("零可用")
    })

    it("wires the balance actions: detail opens and adjustment starts", () => {
        const { result, rowFocusRef, openDetail, startAdjustment } = setup()
        const row = makeBalanceRow()
        const node = callCell(result.current.balanceColumns, "actions", row)

        const { container } = render(node as React.ReactElement)
        fireEvent.click(screen.getByText("查看"))
        expect(openDetail).toHaveBeenCalledWith("b1")

        fireEvent.click(screen.getByText("库存调整"))
        expect(startAdjustment).toHaveBeenCalledWith(row)

        const focusEntry = rowFocusRef.current.get("b1")
        expect(focusEntry).toBeInstanceOf(HTMLButtonElement)
        expect(container.textContent).toContain("查看")
    })

    it("disables adjustment on narrow screens with the readonly blocker message", () => {
        const { result, startAdjustment } = setup({ isPhoneNarrow: true })
        const node = callCell(
            result.current.balanceColumns,
            "actions",
            makeBalanceRow(),
        )

        render(node as React.ReactElement)
        const button = screen.getByText("库存调整").closest("button")
        expect((button as HTMLButtonElement | null)?.disabled).toBe(true)
        expect(button?.title).toBe("窄屏仅只读，请在桌面发起库存调整")
        fireEvent.click(screen.getByText("库存调整"))
        expect(startAdjustment).not.toHaveBeenCalled()
    })

    it("labels movement direction as increase or decrease", () => {
        const { result } = setup()

        const increase = render(
            callCell(
                result.current.movementColumns,
                "type",
                makeMovementRow({ direction: "increase" }),
            ) as React.ReactElement,
        )
        expect(increase.container.textContent).toContain("增加")

        const decrease = render(
            callCell(
                result.current.movementColumns,
                "type",
                makeMovementRow({ direction: "decrease" }),
            ) as React.ReactElement,
        )
        expect(decrease.container.textContent).toContain("减少")
    })

    it("links the movement source to its document and falls back to plain text", () => {
        const { result } = setup()

        const linked = callCell(
            result.current.movementColumns,
            "source",
            makeMovementRow(),
        )
        const linkedContainer = render(linked as React.ReactElement)
        const anchor = linkedContainer.container.querySelector("a")
        expect(anchor?.getAttribute("href")).toBe(
            "/fulfillment?sourceDocId=pr-1",
        )
        expect(linkedContainer.container.textContent).toContain("PR-1")

        const plain = callCell(
            result.current.movementColumns,
            "source",
            makeMovementRow({ sourceHref: undefined }),
        )
        const plainContainer = render(plain as React.ReactElement)
        expect(plainContainer.container.querySelector("a")).toBeNull()
        expect(plainContainer.container.textContent).toContain("PR-1")
    })

    it("passes reservation status label and tone to the badge", () => {
        const { result } = setup()
        const node = callCell(
            result.current.reservationColumns,
            "status",
            makeReservationRow({
                statusLabel: "部分消耗",
                statusTone: "warning",
            }),
        ) as React.ReactElement<Record<string, unknown>>

        expect(node.props.label).toBe("部分消耗")
        expect(node.props.tone).toBe("warning")
        expect(node.props.context).toBe("list")
    })

    it("links the reservation fulfillment context when a href is available", () => {
        const { result } = setup()

        const linked = callCell(
            result.current.reservationColumns,
            "actions",
            makeReservationRow(),
        )
        const linkedContainer = render(linked as React.ReactElement)
        const anchor = linkedContainer.container.querySelector("a")
        expect(anchor?.getAttribute("href")).toBe("/fulfillment?lane=warehouse")
        expect(linkedContainer.container.textContent).toContain("履约上下文")

        const empty = callCell(
            result.current.reservationColumns,
            "actions",
            makeReservationRow({ fulfillmentHref: undefined }),
        )
        const emptyContainer = render(empty as React.ReactElement)
        expect(emptyContainer.container.querySelector("a")).toBeNull()
    })

    it("renders the adjustment doc cell with number, reason and quantity", () => {
        const { result } = setup()
        const node = callCell(
            result.current.adjustmentColumns,
            "doc",
            makeAdjustmentRow(),
        )

        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("TZ1")
        expect(container.textContent).toContain("盘亏")
        expect(container.textContent).toContain("减少")
        expect(container.textContent).toContain("2")
    })

    it("renders operator roles with dash fallbacks in the people cell", () => {
        const { result } = setup()
        const node = callCell(
            result.current.adjustmentColumns,
            "people",
            makeAdjustmentRow(),
        )

        const { container } = render(node as React.ReactElement)
        expect(container.textContent).toContain("经办 张三")
        expect(container.textContent).toContain("当前节点 —")
        expect(container.textContent).toContain("当前审批人 —")
    })
})
