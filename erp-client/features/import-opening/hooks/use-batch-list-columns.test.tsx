import type { ReactNode } from "react"
import { describe, expect, it, vi } from "vitest"
import { fireEvent, render, renderHook, screen } from "@testing-library/react"

import { useBatchListColumns } from "@/features/import-opening/hooks/use-batch-list-columns"
import type { ImportBatchListItem } from "@/features/import-opening/types"

const item: ImportBatchListItem = {
    batchId: "b1",
    batchNo: "B-2025-001",
    environment: "PRODUCTION",
    sourceObjectSet: ["CUSTOMER", "SKU"],
    baselineDate: "2025-01-01",
    importRuleVersion: "v9",
    stage: "TRIAL",
    status: "TRIAL_READY",
    progressLabel: "12/20",
    confirmationSummary: "1/3 已确认",
    initiatorLabel: "张三",
    updatedAt: "2025-01-02T10:00:00.000Z",
}

function renderCell(columnId: string, row: ImportBatchListItem) {
    const columns = renderHook(() =>
        useBatchListColumns({ onOpenBatch: vi.fn() }),
    ).result.current
    const column = columns.find((c) => c.id === columnId)
    if (!column) throw new Error(`column ${columnId} not found`)
    const cell = column.cell as unknown as (ctx: {
        row: { original: ImportBatchListItem }
    }) => ReactNode
    return cell({ row: { original: row } })
}

describe("useBatchListColumns", () => {
    it("builds the expected column set", () => {
        const onOpenBatch = vi.fn()
        const { result } = renderHook(() => useBatchListColumns({ onOpenBatch }))
        expect(result.current.map((c) => c.id)).toEqual([
            "batchNo",
            "environment",
            "objects",
            "baseline",
            "stage",
            "rule",
            "progress",
            "confirm",
            "status",
            "updated",
        ])
    })

    it("opens the batch from the batchNo cell", () => {
        const onOpenBatch = vi.fn()
        const { result } = renderHook(() => useBatchListColumns({ onOpenBatch }))
        const column = result.current.find((c) => c.id === "batchNo")!
        const cell = column.cell as unknown as (ctx: {
            row: { original: ImportBatchListItem }
        }) => ReactNode
        const { getByText } = render(<>{cell({ row: { original: item } })}</>)
        fireEvent.click(getByText("B-2025-001"))
        expect(onOpenBatch).toHaveBeenCalledWith("b1")
    })

    it("renders the environment badge", () => {
        const node = renderCell("environment", item)
        const { getByText } = render(<>{node}</>)
        expect(getByText("生产环境")).toBeTruthy()
    })

    it("renders the object set with Chinese labels", () => {
        const node = renderCell("objects", item)
        const { getByText } = render(<>{node}</>)
        expect(getByText("客户、商品 SKU")).toBeTruthy()
    })

    it("renders the status badge label", () => {
        const node = renderCell("status", item)
        const { getByText } = render(<>{node}</>)
        expect(getByText("试算完成")).toBeTruthy()
    })

    it("renders baseline, stage, rule, progress, confirm and updated cells", () => {
        const { getByText } = render(
            <>
                {["baseline", "stage", "rule", "progress", "confirm", "updated"].map(
                    (id) => (
                        <div key={id}>{renderCell(id, item)}</div>
                    ),
                )}
            </>,
        )
        expect(getByText("2025-01-01")).toBeTruthy()
        expect(getByText("业务校验与试算")).toBeTruthy()
        expect(getByText("v9")).toBeTruthy()
        expect(getByText("12/20")).toBeTruthy()
        expect(getByText("1/3 已确认")).toBeTruthy()
        expect(screen.getAllByText(/2025/).length).toBeGreaterThanOrEqual(2)
    })

    it("recomputes columns only when the open callback changes", () => {
        const onOpenA = vi.fn()
        const onOpenB = vi.fn()
        const { result, rerender } = renderHook(
            ({ onOpenBatch }: { onOpenBatch: (batchId: string) => void }) =>
                useBatchListColumns({ onOpenBatch }),
            { initialProps: { onOpenBatch: onOpenA } },
        )
        const first = result.current
        rerender({ onOpenBatch: onOpenA })
        expect(result.current).toBe(first)
        rerender({ onOpenBatch: onOpenB })
        expect(result.current).not.toBe(first)
    })
})
