import { describe, it, expect, vi, afterEach } from "vitest"
import { cleanup, fireEvent, render, renderHook } from "@testing-library/react"
import type { CellContext } from "@tanstack/react-table"

import {
    useBrandListColumns,
    useUnitOfMeasureListColumns,
    useVoucherCategoryListColumns,
    useWarehouseListColumns,
} from "./use-dictionary-list-columns"
import type { MasterDataListItem } from "@/features/master-data/types"

afterEach(cleanup)

const baseRow: MasterDataListItem = {
    objectType: "unit-of-measures",
    stableId: "uom-1",
    stableNo: "UOM-001",
    name: "箱",
    lifecycleStatus: "ENABLED",
    lifecycleStatusLabel: "当前启用",
    lifecycleTone: "success",
    revisionTiming: "CURRENT",
    revisionTimingLabel: "当前生效",
    currentRevisionId: "rev-1",
    displayedRevisionId: "rev-1",
    revisionNo: 2,
    effectiveFrom: "2026-01-01",
    keyFacts: [{ label: "单位符号", value: "箱" }],
    selectorEligibility: [],
    allowedActions: ["DISABLE"],
    actionBlockers: [],
    lockVersion: 1,
    metricTags: [],
}

function cellContext(
    row: MasterDataListItem,
): CellContext<MasterDataListItem, unknown> {
    return { row: { original: row } } as CellContext<MasterDataListItem, unknown>
}

function renderCell(
    columns: { id?: string; cell?: unknown }[],
    columnId: string,
    row: MasterDataListItem,
) {
    const column = columns.find((c) => c.id === columnId)
    if (typeof column?.cell !== "function") {
        throw new Error(`cell renderer missing for ${columnId}`)
    }
    const element = column.cell(cellContext(row)) as React.ReactElement
    return render(element)
}

const lastFocusedRowId = { current: null as string | null }

describe("useBrandListColumns", () => {
    it("builds the brand columns with a blocker column when any row is blocked", () => {
        const blockedRow = { ...baseRow, primaryBlocker: "停用中不可用" }
        const { result } = renderHook(() =>
            useBrandListColumns({
                lastFocusedRowId,
                rows: [blockedRow],
                onDisableTarget: vi.fn(),
            }),
        )
        expect(result.current.map((c) => c.id)).toEqual([
            "stableNo",
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "blocker",
            "actions",
        ])
    })

    it("omits the blocker column when no row is blocked", () => {
        const { result } = renderHook(() =>
            useBrandListColumns({
                lastFocusedRowId,
                rows: [baseRow],
                onDisableTarget: vi.fn(),
            }),
        )
        expect(result.current.map((c) => c.id)).toEqual([
            "stableNo",
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "actions",
        ])
    })

    it("renders the disable action and reports the clicked row", () => {
        const onDisableTarget = vi.fn()
        const { result } = renderHook(() =>
            useBrandListColumns({
                lastFocusedRowId,
                rows: [baseRow],
                onDisableTarget,
            }),
        )
        const screen = renderCell(result.current, "actions", baseRow)
        const button = screen.getByRole("button", { name: "停用" })
        expect(button).not.toHaveProperty("disabled", true)
        fireEvent.click(button)
        expect(onDisableTarget).toHaveBeenCalledWith(baseRow)
        expect(lastFocusedRowId.current).toBe("uom-1")
    })

    it("disables the stop button when DISABLE is not allowed", () => {
        const { result } = renderHook(() =>
            useBrandListColumns({
                lastFocusedRowId,
                rows: [{ ...baseRow, allowedActions: [] }],
                onDisableTarget: vi.fn(),
            }),
        )
        const screen = renderCell(result.current, "actions", {
            ...baseRow,
            allowedActions: [],
        })
        const button = screen.getByRole("button", { name: "停用" })
        expect(button).toHaveProperty("disabled", true)
    })
})

describe("useUnitOfMeasureListColumns", () => {
    it("builds the unit columns without an effective period column", () => {
        const { result } = renderHook(() =>
            useUnitOfMeasureListColumns({
                lastFocusedRowId,
                rows: [baseRow],
                onDisableTarget: vi.fn(),
            }),
        )
        expect(result.current.map((c) => c.id)).toEqual([
            "stableNo",
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "actions",
        ])
    })

    it("renders the lifecycle label in the lifecycle cell", () => {
        const { result } = renderHook(() =>
            useUnitOfMeasureListColumns({
                lastFocusedRowId,
                rows: [baseRow],
                onDisableTarget: vi.fn(),
            }),
        )
        const screen = renderCell(result.current, "lifecycle", baseRow)
        expect(screen.getByText("当前启用")).toBeTruthy()
    })
})

describe("useVoucherCategoryListColumns", () => {
    it("builds columns with an effective period and update-only action", () => {
        const onReviseTarget = vi.fn()
        const { result } = renderHook(() =>
            useVoucherCategoryListColumns({
                lastFocusedRowId,
                rows: [baseRow],
                onReviseTarget,
            }),
        )
        expect(result.current.map((c) => c.id)).toEqual([
            "stableNo",
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "period",
            "actions",
        ])

        const reviseRow = {
            ...baseRow,
            allowedActions: ["CREATE_REVISION"],
        }
        const screen = renderCell(result.current, "actions", reviseRow)
        fireEvent.click(screen.getByRole("button", { name: "更新资料" }))
        expect(onReviseTarget).toHaveBeenCalledWith(reviseRow)
    })
})

describe("useWarehouseListColumns", () => {
    it("builds warehouse columns with full actions", () => {
        const onPreview = vi.fn()
        const onReviseTarget = vi.fn()
        const onDisableTarget = vi.fn()
        const { result } = renderHook(() =>
            useWarehouseListColumns({
                lastFocusedRowId,
                rows: [baseRow],
                onPreview,
                onReviseTarget,
                onDisableTarget,
            }),
        )
        expect(result.current.map((c) => c.id)).toEqual([
            "stableNo",
            "name",
            "revisionNo",
            "lifecycle",
            "revisionTiming",
            "period",
            "actions",
        ])

        const screen = renderCell(result.current, "actions", baseRow)
        fireEvent.click(screen.getByRole("button", { name: "查看" }))
        expect(onPreview).toHaveBeenCalledWith("uom-1")
        expect(lastFocusedRowId.current).toBe("uom-1")
    })
})
