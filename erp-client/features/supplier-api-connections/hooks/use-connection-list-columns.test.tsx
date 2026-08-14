import { renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { useConnectionListColumns } from "@/features/supplier-api-connections/hooks/use-connection-list-columns"
import type { ConnectionListItem } from "@/features/supplier-api-connections/types"

function accessorOf(
    columns: ReturnType<typeof useConnectionListColumns>,
    columnId: string,
) {
    const column = columns.find((c) => c.id === columnId)
    if (!column || !("accessorFn" in column) || !column.accessorFn) {
        throw new Error(`column ${columnId} has no accessorFn`)
    }
    return column.accessorFn
}

function item(overrides: Partial<ConnectionListItem> = {}): ConnectionListItem {
    return {
        connectionId: "c1",
        connectionCode: "CONN-1",
        supplier: { id: "s1", name: "供应商甲" },
        environment: "PRODUCTION",
        environmentLabel: "生产",
        status: "ENABLED",
        statusLabel: "启用",
        statusTone: "success",
        capabilitySummary: "商品目录、价格",
        healthResult: "SUCCESS",
        healthLabel: "成功",
        healthTone: "success",
        catalogState: "NEVER",
        catalogLabel: "从未同步",
        nextStep: "连接已启用",
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

describe("useConnectionListColumns", () => {
    it("produces the fixed column set with stable accessors", () => {
        const onOpen = vi.fn()
        const { result } = renderHook(() => useConnectionListColumns(onOpen))
        const columns = result.current

        expect(columns.map((column) => column.id)).toEqual([
            "identity",
            "environment",
            "status",
            "capabilities",
            "health",
            "catalog",
            "nextStep",
            "owners",
            "actions",
        ])

        const row = item({
            businessOwner: "采购甲",
            technicalOwner: "技术乙",
        })
        const byId = new Map(columns.map((column) => [column.id, column]))
        expect(accessorOf(columns, "identity")(row, 0)).toBe("CONN-1")
        expect(accessorOf(columns, "environment")(row, 0)).toBe("生产")
        expect(accessorOf(columns, "status")(row, 0)).toBe("启用")
        expect(accessorOf(columns, "capabilities")(row, 0)).toBe(
            "商品目录、价格",
        )
        expect(accessorOf(columns, "health")(row, 0)).toBe("成功")
        expect(accessorOf(columns, "catalog")(row, 0)).toBe("从未同步")
        expect(accessorOf(columns, "nextStep")(row, 0)).toBe("连接已启用")
        expect(accessorOf(columns, "owners")(row, 0)).toBe("采购甲 / 技术乙")
        expect("accessorFn" in byId.get("actions")!).toBe(false)
        expect(byId.get("actions")!.enableSorting).toBe(false)
    })

    it("derives owner accessors with fallbacks when owners are absent", () => {
        const { result } = renderHook(() => useConnectionListColumns(vi.fn()))
        expect(accessorOf(result.current, "owners")(item(), 0)).toBe("— / —")
    })

    it("keeps the columns memoized for the same callback", () => {
        const onOpen = vi.fn()
        const { result, rerender } = renderHook(() =>
            useConnectionListColumns(onOpen),
        )
        const first = result.current
        rerender()
        expect(result.current).toBe(first)
    })
})
