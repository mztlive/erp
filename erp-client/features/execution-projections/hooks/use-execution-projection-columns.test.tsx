import { describe, it, expect, vi, afterEach } from "vitest"
import {
    render,
    screen,
    fireEvent,
    renderHook,
    cleanup,
} from "@testing-library/react"
import type { ReactNode } from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { useExecutionProjectionColumns } from "./use-execution-projection-columns"
import type { ProjectionRowCommandAction } from "./use-execution-projection-columns"
import type { ExecutionProjectionRow } from "../types"

afterEach(() => {
    cleanup()
})

function makeRow(
    overrides: Partial<ExecutionProjectionRow> = {},
): ExecutionProjectionRow {
    return {
        projectionId: "proj_0001",
        projectionNo: "PROJ_0001",
        projectionRevisionId: "rev_0001",
        projectionRevisionNo: 2,
        projectionSource: "ERP_SALES_REVISION",
        salesOrderId: "so_0001",
        salesOrderNo: "SO-2026-0001",
        salesOrderRevisionId: "sor_0001",
        salesOrderRevisionNo: 2,
        salesOrderStatus: "—",
        salesOrderStatusTone: "neutral",
        customerLabel: "客户甲",
        targetMallId: "mall_0001",
        targetMallName: "测试商城",
        currentAckedRevisionNo: 1,
        delivery: {
            deliveryId: "dlv_0001",
            status: "FAILED",
            statusLabel: "失败",
            statusTone: "destructive",
            attemptCount: 2,
            lastAttemptAt: "2026-08-01T00:00:00.000Z",
        },
        latencyBand: "normal",
        reconciliationStatus: "NONE",
        pendingDurationLabel: "—",
        ownerLabel: "—",
        allowedActions: ["RETRY", "QUERY_RESULT"],
        actionBlockers: [],
        objectVersion: "5",
        whitelistPreview: {
            voucherCategoryErpName: "—",
            faceValue: "0",
            cardCount: "0",
            cardForm: "—",
            voucherExpiryAt: "—",
        },
        ...overrides,
    }
}

type CellContextLike = {
    row: {
        original: ExecutionProjectionRow
        getIsSelected: () => boolean
        toggleSelected: (value: boolean) => void
    }
    table?: Record<string, unknown>
}

function renderCell(
    column: ColumnDef<ExecutionProjectionRow>,
    context: CellContextLike | Record<string, unknown>,
): ReactNode {
    const fn = column.cell as unknown as (ctx: unknown) => ReactNode
    return fn(context)
}

describe("useExecutionProjectionColumns", () => {
    it("返回固定顺序的九列且每列带展示元信息", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const ids = result.current.map((c) => c.id)
        expect(ids).toEqual([
            "select",
            "salesOrder",
            "source",
            "mall",
            "delivery",
            "acked",
            "attempt",
            "error",
            "actions",
        ])
        expect(result.current[1]?.meta).toEqual({
            label: "销售单",
            width: "default",
        })
    })

    it("选择列表头渲染全选复选框并反映页面选中态", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const selectCol = result.current[0]!
        const headerFn = selectCol.header as unknown as (
            ctx: unknown,
        ) => ReactNode
        render(
            <>
                {headerFn({
                    table: {
                        getIsAllPageRowsSelected: () => true,
                        getIsSomePageRowsSelected: () => false,
                        toggleAllPageRowsSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(
            screen
                .getByRole("checkbox", { name: "全选本页可选项" })
                .hasAttribute("data-checked"),
        ).toBe(true)
    })

    it("销售单列展示单号与客户", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const col = result.current.find((c) => c.id === "salesOrder")!
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow(),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(screen.getByText("SO-2026-0001")).toBeTruthy()
        expect(screen.getByText("客户甲")).toBeTruthy()
    })

    it("来源列按投影来源显示迁移基线徽标", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const col = result.current.find((c) => c.id === "source")!
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow({
                            projectionSource: "MIGRATION_BASELINE",
                        }),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(screen.getByText("迁移基线")).toBeTruthy()
    })

    it("接收状态列展示状态与超时标注", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const col = result.current.find((c) => c.id === "delivery")!
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow({ latencyBand: "over_sla" }),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(screen.getByText("失败")).toBeTruthy()
        expect(screen.getByText("已超时")).toBeTruthy()
    })

    it("商城已确认版列显示版本或尚未确认", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const col = result.current.find((c) => c.id === "acked")!
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow({ currentAckedRevisionNo: 3 }),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(screen.getByText("v3")).toBeTruthy()

        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow({
                            currentAckedRevisionNo: undefined,
                        }),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(screen.getByText("尚未确认")).toBeTruthy()
    })

    it("操作列：打开按钮写 URL、命令按钮回传行、销售单链接指向协同", () => {
        const replaceParams = vi.fn()
        const onRowCommand = vi.fn()
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams,
                commandPending: false,
                onRowCommand,
            }),
        )
        const col = result.current.find((c) => c.id === "actions")!
        const row = makeRow()
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: row,
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )

        fireEvent.click(screen.getByRole("button", { name: "打开" }))
        expect(replaceParams).toHaveBeenCalledWith({
            projectionId: "proj_0001",
            revision: null,
        })

        fireEvent.click(screen.getByRole("button", { name: "查询结果" }))
        expect(onRowCommand).toHaveBeenCalledWith({
            kind: "QUERY_RESULT",
            row,
            objectVersion: "5",
        } satisfies ProjectionRowCommandAction)

        fireEvent.click(screen.getByRole("button", { name: "重试" }))
        expect(onRowCommand).toHaveBeenCalledWith({
            kind: "RETRY",
            row,
            objectVersion: "5",
        } satisfies ProjectionRowCommandAction)

        expect(
            screen.getByRole("button", { name: "销售单" }).getAttribute("href"),
        ).toBe("/sales/orders/so_0001?section=collaboration")
    })

    it("操作列：有关联错误任务时显示错误中心入口链接", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const col = result.current.find((c) => c.id === "actions")!
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow({
                            delivery: {
                                deliveryId: "dlv_0001",
                                status: "FAILED",
                                statusLabel: "失败",
                                statusTone: "destructive",
                                attemptCount: 2,
                                lastAttemptAt: "2026-08-01T00:00:00.000Z",
                                workItemId: "wi_9",
                            },
                        }),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(
            screen
                .getByRole("button", { name: "打开接口错误与对账中心" })
                .getAttribute("href"),
        ).toBe("/governance/integration-errors?workItemId=wi_9&from=W23")
    })

    it("操作列：无允许动作的行不渲染命令按钮", () => {
        const { result } = renderHook(() =>
            useExecutionProjectionColumns({
                replaceParams: vi.fn(),
                commandPending: false,
                onRowCommand: vi.fn(),
            }),
        )
        const col = result.current.find((c) => c.id === "actions")!
        render(
            <>
                {renderCell(col, {
                    row: {
                        original: makeRow({ allowedActions: [] }),
                        getIsSelected: () => false,
                        toggleSelected: vi.fn(),
                    },
                })}
            </>,
        )
        expect(screen.queryByRole("button", { name: "查询结果" })).toBeNull()
        expect(screen.queryByRole("button", { name: "重试" })).toBeNull()
    })
})
