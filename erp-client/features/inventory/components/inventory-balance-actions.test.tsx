import { render, screen } from "@testing-library/react"
import type { ReactNode } from "react"
import { describe, expect, it, vi } from "vitest"

import type {
    BalanceDetailView,
    StockBalanceRow,
} from "@/features/inventory/types"

import { buildBalanceColumns } from "./columns/balance-columns"
import { InventoryBalancePreview } from "./inventory-balance-preview"

vi.mock("@/components/business", async (importOriginal) => {
    const original =
        await importOriginal<typeof import("@/components/business")>()
    return {
        ...original,
        QuickPreviewSheet: ({
            children,
            footer,
        }: {
            children: ReactNode
            footer?: ReactNode
        }) => (
            <div>
                <div>{children}</div>
                <div>{footer}</div>
            </div>
        ),
    }
})

const balance: StockBalanceRow = {
    balanceId: "balance-1",
    warehouseId: "warehouse-1",
    warehouseCode: "WH-1",
    warehouseName: "一号仓",
    skuId: "sku-1",
    skuCode: "SKU-1",
    skuName: "商品一",
    specSummary: "",
    baseUnit: "件",
    onHandQuantity: "10",
    reservedQuantity: "2",
    availableQuantity: "8",
    lockVersion: "7",
    lastMovementId: "movement-1",
    lastMovementAt: "2026-09-01T00:00:00.000Z",
    lastMovementTypeLabel: "采购入库",
    availability: "reserved",
    statusLabel: "有预占",
    statusTone: "info",
    hasActiveReservation: true,
    stockKind: "OWN_PHYSICAL",
    allowedActions: [],
    actionBlockers: [],
}

function renderBalanceActions(row: StockBalanceRow) {
    const actionColumn = buildBalanceColumns({
        isPhoneNarrow: false,
        rowFocusRef: { current: new Map() },
        openDetail: vi.fn(),
        startAdjustment: vi.fn(),
    }).find((column) => column.id === "actions")
    if (typeof actionColumn?.cell !== "function") {
        throw new Error("balance action column is not renderable")
    }
    const cell = actionColumn.cell as (context: {
        row: { original: StockBalanceRow }
    }) => ReactNode
    render(cell({ row: { original: row } }))
}

function renderBalancePreview(row: StockBalanceRow) {
    const detail: BalanceDetailView = {
        balance: row,
        recentMovements: [],
        reservations: [],
        sourceDocuments: [],
        pendingAdjustments: [],
        queriedAt: "2026-09-01T00:00:00.000Z",
    }
    render(
        <InventoryBalancePreview
            open
            detail={detail}
            isPending={false}
            onClose={vi.fn()}
            onViewMovements={vi.fn()}
            onStartAdjustment={vi.fn()}
        />,
    )
}

describe("stock balance create adjustment entry", () => {
    it("does not render the row action without a server-issued action", () => {
        renderBalanceActions(balance)

        expect(screen.getByRole("button", { name: "查看" })).toBeTruthy()
        expect(screen.queryByRole("button", { name: "库存调整" })).toBeNull()
    })

    it("renders the row action when the server issues it", () => {
        renderBalanceActions({
            ...balance,
            allowedActions: ["CREATE_ADJUSTMENT"],
        })

        expect(screen.getByRole("button", { name: "库存调整" })).toBeTruthy()
    })

    it("does not render the preview action without a server-issued action", () => {
        renderBalancePreview(balance)

        expect(
            screen.queryByRole("button", { name: "发起库存调整" }),
        ).toBeNull()
    })

    it("renders the preview action when the server issues it", () => {
        renderBalancePreview({
            ...balance,
            allowedActions: ["CREATE_ADJUSTMENT"],
        })

        expect(
            screen.getByRole("button", { name: "发起库存调整" }),
        ).toBeTruthy()
    })
})
