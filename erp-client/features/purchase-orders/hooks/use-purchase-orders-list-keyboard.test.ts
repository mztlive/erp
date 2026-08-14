import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"
import * as React from "react"

import type { PurchaseOrderListItem } from "@/features/purchase-orders/types"
import { usePurchaseOrdersListKeyboard } from "./use-purchase-orders-list-keyboard"

function makeRow(id: string): PurchaseOrderListItem {
    return {
        purchaseOrderId: id,
        purchaseNo: id,
        status: "DRAFT",
        statusLabel: "草稿",
        statusTone: "neutral",
        reviewStatus: "NONE",
        reviewLabel: "—",
        salesOrderId: "so_1",
        salesOrderNo: "SO-1",
        supplierId: "sup_1",
        supplierName: "供应商A",
        purchaseType: "PHYSICAL",
        fulfillmentResponsibility: "WAREHOUSE",
        paymentTermCode: "",
        paymentTermLabel: "—",
        ownerName: "—",
        grossAmount: "0",
        netAmount: "0",
        taxAmount: "0",
        costMasked: false,
        paymentProgress: "未付",
        invoiceProgress: "未收",
        fulfillmentProgress: "未开始",
        paymentGate: "NOT_APPLICABLE",
        updatedAt: "2026-08-14T00:00:00.000Z",
        allowedActions: [],
        actionBlockers: [],
    }
}

function renderWithState(
    pageRows: readonly PurchaseOrderListItem[],
    createOpen = false,
) {
    return renderHook(() => {
        const [focusedIndex, setFocusedIndex] = React.useState(0)
        const [previewId, setPreviewId] = React.useState<string | null>(null)
        usePurchaseOrdersListKeyboard({
            pageRows,
            focusedIndex,
            previewId,
            createOpen,
            onFocusIndex: setFocusedIndex,
            onOpenPreview: setPreviewId,
            onClosePreview: () => setPreviewId(null),
        })
        return { focusedIndex, previewId }
    })
}

function key(keyName: string, target?: EventTarget) {
    const event = new KeyboardEvent("keydown", {
        key: keyName,
        bubbles: true,
        cancelable: true,
    })
    ;(target ?? window).dispatchEvent(event)
    return event
}

beforeEach(() => {
    document.body.innerHTML = ""
})

afterEach(() => {
    cleanup()
})

describe("usePurchaseOrdersListKeyboard", () => {
    const rows = [makeRow("po_1"), makeRow("po_2"), makeRow("po_3")]

    it("j / ArrowDown 向下移动并在末尾停留", () => {
        const { result } = renderWithState(rows)
        act(() => {
            key("j")
        })
        expect(result.current.focusedIndex).toBe(1)
        act(() => {
            key("ArrowDown")
        })
        expect(result.current.focusedIndex).toBe(2)
        act(() => {
            key("j")
        })
        expect(result.current.focusedIndex).toBe(2)
    })

    it("k / ArrowUp 向上移动并在顶部停留", () => {
        const { result } = renderWithState(rows)
        act(() => {
            key("j")
            key("j")
        })
        expect(result.current.focusedIndex).toBe(2)
        act(() => {
            key("k")
        })
        expect(result.current.focusedIndex).toBe(1)
        act(() => {
            key("ArrowUp")
        })
        expect(result.current.focusedIndex).toBe(0)
        act(() => {
            key("k")
        })
        expect(result.current.focusedIndex).toBe(0)
    })

    it("Enter 打开焦点行的预览", () => {
        const { result } = renderWithState(rows)
        act(() => {
            key("j")
        })
        act(() => {
            key("Enter")
        })
        expect(result.current.previewId).toBe("po_2")
    })

    it("预览打开时忽略 j/k/Enter，Escape 关闭", () => {
        const { result } = renderWithState(rows)
        act(() => {
            key("Enter")
        })
        expect(result.current.previewId).toBe("po_1")

        act(() => {
            key("j")
            key("Enter")
        })
        expect(result.current.focusedIndex).toBe(0)
        expect(result.current.previewId).toBe("po_1")

        let escapeEvent: KeyboardEvent
        act(() => {
            escapeEvent = key("Escape")
        })
        expect(escapeEvent!.defaultPrevented).toBe(true)
        expect(result.current.previewId).toBeNull()
    })

    it("/ 聚焦搜索框并阻止默认行为", () => {
        const search = document.createElement("input")
        search.setAttribute("data-slot", "po-list-search")
        const focus = vi.fn()
        search.focus = focus
        document.body.appendChild(search)

        renderWithState(rows)
        act(() => {
            key("/")
        })
        expect(focus).toHaveBeenCalledTimes(1)
    })

    it("无搜索框时 / 不抛错", () => {
        renderWithState(rows)
        expect(() => {
            act(() => {
                key("/")
            })
        }).not.toThrow()
    })

    it("建单弹框打开时列表不响应按键", () => {
        const { result } = renderWithState(rows, true)
        act(() => {
            key("j")
            key("Enter")
        })
        expect(result.current.focusedIndex).toBe(0)
        expect(result.current.previewId).toBeNull()
    })

    it("空列表不响应按键", () => {
        const { result } = renderWithState([])
        act(() => {
            key("j")
            key("Enter")
        })
        expect(result.current.focusedIndex).toBe(0)
        expect(result.current.previewId).toBeNull()
    })

    it("输入框内按键不触发行导航", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)

        const { result } = renderWithState(rows)
        act(() => {
            key("j", input)
            key("Enter", input)
        })
        expect(result.current.focusedIndex).toBe(0)
        expect(result.current.previewId).toBeNull()
    })

    it("预览打开时输入框内 Escape 仍可关闭预览", () => {
        const input = document.createElement("input")
        document.body.appendChild(input)

        const { result } = renderWithState(rows)
        act(() => {
            key("Enter")
        })
        expect(result.current.previewId).toBe("po_1")
        act(() => {
            key("Escape", input)
        })
        expect(result.current.previewId).toBeNull()
    })
})
