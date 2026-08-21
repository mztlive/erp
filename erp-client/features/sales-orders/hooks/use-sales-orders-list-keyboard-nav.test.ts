import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { parseSalesOrdersSearchParams } from "@/features/sales-orders/lib/url-state"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { useSalesOrdersListKeyboardNav } from "./use-sales-orders-list-keyboard-nav"

const makeUrl = (raw = "") =>
    parseSalesOrdersSearchParams(new URLSearchParams(raw))

function makeSalesOrderListItem(
    id: string,
    overrides: Partial<SalesOrderListItem> = {},
): SalesOrderListItem {
    return {
        id,
        documentNumber: `SO-${id}`,
        customerName: "示例客户",
        contractId: "",
        contractNumber: "",
        contractCompanyName: "",
        contractRevisionLabel: "",
        nature: "physical_service",
        originSystem: "erp",
        primaryStatus: { code: "effective", label: "已生效", tone: "success" },
        fulfillment: { label: "未开始", tone: "neutral" },
        collection: { label: "未收", tone: "neutral" },
        invoicing: { label: "未开", tone: "neutral" },
        amountGross: "1000.00",
        amountNet: "900.00",
        taxAmount: "100.00",
        receivedAmount: "0.00",
        invoicedAmount: "0.00",
        ownerName: "张三",
        submittedAt: "2026-01-01 10:00",
        welfareScene: "",
        version: 1,
        lockVersion: 1,
        settlementEntity: "主体",
        sellerEntity: "主体",
        paymentTerms: "月结",
        fulfillmentDeadline: "",
        lineItems: [],
        related: {
            purchaseOrders: 0,
            fulfillments: 0,
            receipts: 0,
            invoices: 0,
        },
        closeEligibility: {
            fulfillmentComplete: false,
            receivableSettled: false,
            invoiceComplete: false,
            eligibleToClose: false,
            blockers: [],
            note: "",
        },
        natureLocked: true,
        commercialReadOnly: false,
        revisions: [],
        procurementRejection: null,
        activeLowMarginManagerConfirmation: null,
        activeChangeOrder: null,
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

const items = [
    makeSalesOrderListItem("so-a"),
    makeSalesOrderListItem("so-b"),
    makeSalesOrderListItem("so-c"),
]

const pressKey = (key: string, target?: EventTarget) => {
    const event = new KeyboardEvent("keydown", { key, bubbles: true })
    if (target) {
        target.dispatchEvent(event)
    } else {
        window.dispatchEvent(event)
    }
}

let searchInput: HTMLInputElement

beforeEach(() => {
    vi.clearAllMocks()
    searchInput = document.createElement("input")
    searchInput.setAttribute("data-slot", "so-list-search")
    document.body.appendChild(searchInput)
})

afterEach(() => {
    searchInput.remove()
    cleanup()
})

describe("useSalesOrdersListKeyboardNav", () => {
    it("j / ArrowDown 向下移动聚焦行并在末尾封顶", () => {
        const onRowNavigate = vi.fn()
        const { result } = renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate,
            }),
        )

        act(() => {
            pressKey("j")
        })
        expect(result.current.focusedIndex).toBe(1)
        act(() => {
            pressKey("ArrowDown")
        })
        act(() => {
            pressKey("ArrowDown")
        })
        expect(result.current.focusedIndex).toBe(2)
    })

    it("k / ArrowUp 向上移动聚焦行并在顶部封底", () => {
        const { result } = renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate: vi.fn(),
            }),
        )

        act(() => {
            pressKey("ArrowDown")
            pressKey("ArrowDown")
        })
        expect(result.current.focusedIndex).toBe(2)
        act(() => {
            pressKey("k")
        })
        expect(result.current.focusedIndex).toBe(1)
        act(() => {
            pressKey("ArrowUp")
            pressKey("ArrowUp")
            pressKey("ArrowUp")
        })
        expect(result.current.focusedIndex).toBe(0)
    })

    it("Enter 打开当前聚焦行", () => {
        const onRowNavigate = vi.fn()
        renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate,
            }),
        )

        act(() => {
            pressKey("ArrowDown")
        })
        act(() => {
            pressKey("Enter")
        })

        expect(onRowNavigate).toHaveBeenCalledWith("so-b")
    })

    it("列表为空时导航键无效", () => {
        const onRowNavigate = vi.fn()
        const { result } = renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items: [],
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate,
            }),
        )

        act(() => {
            pressKey("ArrowDown")
            pressKey("Enter")
        })

        expect(result.current.focusedIndex).toBe(0)
        expect(onRowNavigate).not.toHaveBeenCalled()
    })

    it("/ 聚焦搜索框", () => {
        const focus = vi.spyOn(searchInput, "focus")
        renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate: vi.fn(),
            }),
        )

        act(() => {
            pressKey("/")
        })

        expect(focus).toHaveBeenCalledTimes(1)
    })

    it("弹层打开时 / 不聚焦背景搜索框", () => {
        const focus = vi.spyOn(searchInput, "focus")
        const dialog = document.createElement("div")
        dialog.setAttribute("role", "dialog")
        document.body.appendChild(dialog)
        renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate: vi.fn(),
            }),
        )

        act(() => {
            pressKey("/")
        })

        expect(focus).not.toHaveBeenCalled()
        dialog.remove()
    })

    it("在输入框内按键不触发列表导航", () => {
        const onRowNavigate = vi.fn()
        const { result } = renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: null,
                onPaperChange: vi.fn(),
                onRowNavigate,
            }),
        )
        const input = document.createElement("input")
        document.body.appendChild(input)

        act(() => {
            pressKey("ArrowDown", input)
            pressKey("/", input)
        })

        expect(result.current.focusedIndex).toBe(0)
        expect(onRowNavigate).not.toHaveBeenCalled()
        input.remove()
    })

    it("Escape 关闭纸质预览并把焦点还给原行", async () => {
        const onPaperChange = vi.fn()
        const { result } = renderHook(() =>
            useSalesOrdersListKeyboardNav({
                items,
                url: makeUrl(),
                paperId: "so-a",
                onPaperChange,
                onRowNavigate: vi.fn(),
            }),
        )
        const rowEl = document.createElement("div")
        const focus = vi.spyOn(rowEl, "focus")
        result.current.rowRefs.current.set("so-a", rowEl)

        act(() => {
            pressKey("Escape")
        })

        expect(onPaperChange).toHaveBeenCalledWith(null)
        await waitFor(() => expect(focus).toHaveBeenCalledTimes(1))
    })

    it("筛选或分页变化时聚焦回到第一行", () => {
        const { result, rerender } = renderHook(
            ({ url }: { url: ReturnType<typeof makeUrl> }) =>
                useSalesOrdersListKeyboardNav({
                    items,
                    url,
                    paperId: null,
                    onPaperChange: vi.fn(),
                    onRowNavigate: vi.fn(),
                }),
            { initialProps: { url: makeUrl() } },
        )

        act(() => {
            pressKey("ArrowDown")
            pressKey("ArrowDown")
        })
        expect(result.current.focusedIndex).toBe(2)

        rerender({ url: makeUrl("page=2") })
        expect(result.current.focusedIndex).toBe(0)
    })
})
