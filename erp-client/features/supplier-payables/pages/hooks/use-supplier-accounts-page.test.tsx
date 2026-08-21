import { act, cleanup } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

let currentUrl = ""

vi.mock("next/navigation", () => ({
    useRouter: vi.fn(() => ({ push: vi.fn(), replace: vi.fn(), back: vi.fn() })),
    useSearchParams: vi.fn(() => new URLSearchParams(currentUrl)),
    usePathname: vi.fn(() => "/test"),
    useParams: vi.fn(() => ({})),
}))

vi.mock("@/features/supplier-payables/hooks/queries", () => ({
    useSupplierAccountsQuery: vi.fn(),
}))

import { usePathname, useRouter } from "next/navigation"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { useSupplierAccountsQuery } from "@/features/supplier-payables/hooks/queries"
import type {
    PayableRow,
    SupplierAccountsListView,
} from "@/features/supplier-payables/types"
import { useSupplierAccountsPage } from "./use-supplier-accounts-page"

const mockedPathname = vi.mocked(usePathname)
const mockedRouter = vi.mocked(useRouter)
const mockedListQuery = vi.mocked(useSupplierAccountsQuery)

let currentListData: SupplierAccountsListView | undefined

// currentListData 在各测试里重新赋值；实现需动态读取而非捕获旧引用
mockedListQuery.mockImplementation(
    () =>
        ({
            data: currentListData,
            isPending: false,
            isError: false,
            error: null,
            refetch: vi.fn(),
        }) as unknown as ReturnType<typeof useSupplierAccountsQuery>,
)

function makePayable(
    overrides: Partial<PayableRow> = {},
): PayableRow {
    return {
        payableAccountId: "PA1",
        supplierId: "S1",
        supplierName: "供应商甲",
        sourceType: "PURCHASE_ORDER",
        sourceTypeLabel: "采购单",
        sourceDocumentId: "PO1",
        sourceDocumentNo: "PO-001",
        primaryEntryId: "E1",
        entryLockVersion: 1,
        accountLockVersion: 1,
        grossTotal: "1000.00",
        settledTotal: "200.00",
        openTotal: "800.00",
        invoicedTotal: "100.00",
        openInvoiceableTotal: "700.00",
        dueDate: "2026-08-20",
        dueState: "not_due",
        dueStateLabel: "未到期",
        status: "OPEN",
        statusLabel: "未结",
        statusTone: "warning",
        allowedActions: [],
        actionBlockers: [],
        ...overrides,
    }
}

function makeListView(
    overrides: Partial<SupplierAccountsListView> = {},
): SupplierAccountsListView {
    return {
        view: "payable",
        metrics: {
            openPayableTotal: "1000.00",
            overduePayableTotal: "300.00",
            unallocatedPaymentTotal: "50.00",
            unallocatedInvoiceTotal: "0.00",
            prepayGateBlockedCount: 0,
        },
        payables: [],
        payments: [],
        invoices: [],
        unallocated: [],
        suppliers: [],
        total: 0,
        filterSummary: "共 0 条",
        permissionVersion: "pv-1",
        dataWatermark: "wm-1",
        queriedAt: "2026-08-14T00:00:00.000Z",
        moduleAllowed: true,
        hasDataScope: true,
        canRegisterPayment: true,
        canRegisterInvoice: true,
        canExport: false,
        payablePriorityPolicy: {
            state: "AVAILABLE",
            mixedAutoAllocationAllowed: true,
        },
        allowFullBankReveal: false,
        ...overrides,
    }
}

function setupRouter() {
    const router = { push: vi.fn(), replace: vi.fn(), back: vi.fn() }
    mockedRouter.mockReturnValue(
        router as unknown as ReturnType<typeof useRouter>,
    )
    return router
}

function renderPage(url: string) {
    currentUrl = url
    mockedPathname.mockReturnValue("/test")
    const router = setupRouter()
    const client = createFreshQueryClient()
    const view = renderHookWithProviders(() => useSupplierAccountsPage(), {
        queryClient: client,
    })
    return { view, router, client }
}

function lastReplaceHref(router: {
    replace: ReturnType<typeof vi.fn>
}): URLSearchParams | null {
    const last = router.replace.mock.calls.at(-1)?.[0]
    if (typeof last !== "string") return null
    const idx = last.indexOf("?")
    return new URLSearchParams(idx >= 0 ? last.slice(idx + 1) : "")
}

function lastPushHref(router: { push: ReturnType<typeof vi.fn> }): string {
    return String(router.push.mock.calls.at(-1)?.[0] ?? "")
}

beforeEach(() => {
    vi.clearAllMocks()
    currentUrl = ""
    currentListData = makeListView()
})

afterEach(() => {
    cleanup()
    vi.useRealTimers()
})

describe("useSupplierAccountsPage", () => {
    it("defaults from an empty URL: payable view, no filters, first page", () => {
        const { view } = renderPage("")

        expect(view.result.current.view).toBe("payable")
        expect(view.result.current.searchInput).toBe("")
        expect(view.result.current.pagination).toEqual({
            pageIndex: 0,
            pageSize: 20,
        })
        expect(view.result.current.trackFilter).toBe("all")
        expect(view.result.current.hasActiveFilters).toBe(false)
        expect(view.result.current.panelOpen).toBe(false)
        expect(view.result.current.appliedChips).toEqual([])
        expect(view.result.current.session).toBeNull()
        expect(view.result.current.previewPayableId).toBeNull()
        expect(view.result.current.sortedPayables).toEqual([])
    })

    it("parses every URL param into the list query", () => {
        const { view } = renderPage(
            "view=payment&q=abc&supplierId=S1&sourceType=SUPPLIER_SETTLEMENT&status=OPEN&due=overdue&paymentGate=unsatisfied&purchaseOrderId=PO1&page=3&track=purchase_invoice",
        )

        expect(view.result.current.view).toBe("payment")
        expect(view.result.current.trackFilter).toBe("purchase_invoice")
        expect(view.result.current.pagination.pageIndex).toBe(2)
        expect(view.result.current.hasActiveFilters).toBe(true)

        const query = mockedListQuery.mock.calls.at(-1)?.[0]
        expect(query).toEqual(
            expect.objectContaining({
                view: "payment",
                q: "abc",
                supplierId: "S1",
                sourceType: "SUPPLIER_SETTLEMENT",
                status: "OPEN",
                due: "overdue",
                paymentGate: "unsatisfied",
                purchaseOrderId: "PO1",
            }),
        )
    })

    it("drops invalid enum values and clamps page to a safe minimum", () => {
        const { view } = renderPage(
            "sourceType=NONSENSE&due=all&paymentGate=all&page=abc",
        )

        const query = mockedListQuery.mock.calls.at(-1)?.[0]
        expect(query?.sourceType).toBeUndefined()
        expect(query?.due).toBeUndefined()
        expect(query?.paymentGate).toBeUndefined()
        expect(view.result.current.pagination.pageIndex).toBe(0)
        // 非法枚举值解析时降级为默认：不激活筛选、不进 chip
        expect(view.result.current.hasActiveFilters).toBe(false)
        expect(view.result.current.appliedChips).toEqual([])

        const { view: view2 } = renderPage("page=-5")
        expect(view2.result.current.pagination.pageIndex).toBe(0)

        const { view: view3 } = renderPage("page=2")
        expect(view3.result.current.pagination.pageIndex).toBe(1)

        const { view: view4 } = renderPage("due=all&paymentGate=all")
        expect(view4.result.current.hasActiveFilters).toBe(false)
    })

    it("drops an invalid status value and keeps valid ones in the query", () => {
        const { view } = renderPage("status=NONSENSE")
        const query = mockedListQuery.mock.calls.at(-1)?.[0]
        expect(query?.status).toBeUndefined()
        expect(view.result.current.hasActiveFilters).toBe(false)

        const { view: view2 } = renderPage("status=PARTIAL")
        const query2 = mockedListQuery.mock.calls.at(-1)?.[0]
        expect(query2?.status).toBe("PARTIAL")
        // hasActiveFilters 覆盖接口实际消费的 status
        expect(view2.result.current.hasActiveFilters).toBe(true)
    })

    it("writes the page param via replace, omitting the first page", () => {
        const { view, router } = renderPage("")

        act(() => {
            view.result.current.handlePaginationChange({
                pageIndex: 2,
                pageSize: 20,
            })
        })
        expect(lastReplaceHref(router)?.get("page")).toBe("3")

        act(() => {
            view.result.current.handlePaginationChange({
                pageIndex: 0,
                pageSize: 20,
            })
        })
        const href = lastReplaceHref(router)
        expect(href?.has("page")).toBe(false)
    })

    it("clearFilters resets drafts, panel and all filter params, keeping the view", () => {
        const { view, router } = renderPage(
            "q=abc&supplierId=S1&status=OPEN&due=overdue&track=payment&purchaseOrderId=PO1&page=2&view=payable",
        )

        expect(view.result.current.hasActiveFilters).toBe(true)
        expect(view.result.current.panelOpen).toBe(true)
        act(() => {
            view.result.current.setSourceTypeDraft("SUPPLIER_SETTLEMENT")
        })
        act(() => {
            view.result.current.clearFilters()
        })

        expect(view.result.current.searchInput).toBe("")
        expect(view.result.current.supplierDraft).toBeNull()
        expect(view.result.current.statusDraft).toBe("all")
        expect(view.result.current.sourceTypeDraft).toBe("all")
        expect(view.result.current.panelOpen).toBe(false)

        // 模拟 URL 已被 replace 更新后的下一次渲染
        currentUrl = "view=payable"
        act(() => {
            view.rerender()
        })
        expect(view.result.current.hasActiveFilters).toBe(false)
        // 清空走 router.replace(scroll:false)，不膨胀浏览历史
        const href = lastReplaceHref(router)
        expect(href?.get("view")).toBe("payable")
        expect(href?.has("q")).toBe(false)
        expect(href?.has("supplierId")).toBe(false)
        expect(href?.has("status")).toBe(false)
        expect(href?.has("due")).toBe(false)
        expect(href?.has("track")).toBe(false)
        expect(href?.has("purchaseOrderId")).toBe(false)
        expect(href?.has("page")).toBe(false)
        expect(router.push).not.toHaveBeenCalled()
    })

    it("keeps drafts local: no URL writes or requests while drafts change", () => {
        const { view, router } = renderPage("")

        act(() => {
            view.result.current.setSearchInput("abc")
            view.result.current.setSupplierDraft("S1")
            view.result.current.setStatusDraft("PARTIAL")
        })

        expect(router.replace).not.toHaveBeenCalled()
        expect(router.push).not.toHaveBeenCalled()
        expect(view.result.current.hasActiveFilters).toBe(false)
    })

    it("applyFilters writes all drafts in one replace, resets pagination and closes the panel", () => {
        const { view, router } = renderPage("page=3")

        act(() => {
            view.result.current.setSearchInput("  PO-100  ")
            view.result.current.setSupplierDraft("S1")
            view.result.current.setSourceTypeDraft("SUPPLIER_SETTLEMENT")
            view.result.current.setStatusDraft("PARTIAL")
            view.result.current.setDueDraft("overdue")
            view.result.current.setPaymentGateDraft("unsatisfied")
        })
        act(() => {
            view.result.current.applyFilters()
        })

        const href = lastReplaceHref(router)
        expect(href?.get("q")).toBe("PO-100")
        expect(href?.get("supplierId")).toBe("S1")
        expect(href?.get("sourceType")).toBe("SUPPLIER_SETTLEMENT")
        expect(href?.get("status")).toBe("PARTIAL")
        expect(href?.get("due")).toBe("overdue")
        expect(href?.get("paymentGate")).toBe("unsatisfied")
        expect(href?.has("page")).toBe(false)
        expect(view.result.current.panelOpen).toBe(false)
        expect(router.push).not.toHaveBeenCalled()
    })

    it("applyFilters drops 全部 drafts from the URL instead of writing defaults", () => {
        const { view, router } = renderPage("status=OPEN&page=2")

        act(() => {
            view.result.current.setStatusDraft("all")
            view.result.current.setSupplierDraft(null)
        })
        act(() => {
            view.result.current.applyFilters()
        })

        const href = lastReplaceHref(router)
        expect(href?.has("status")).toBe(false)
        expect(href?.has("supplierId")).toBe(false)
        expect(href?.has("page")).toBe(false)
        expect(href?.has("q")).toBe(false)
    })

    it("resetMoreFilters clears structured conditions but keeps q and purchaseOrderId", () => {
        const { view, router } = renderPage(
            "q=abc&supplierId=S1&status=OPEN&purchaseOrderId=PO1",
        )

        expect(view.result.current.panelOpen).toBe(true)
        act(() => {
            view.result.current.resetMoreFilters()
        })

        const href = lastReplaceHref(router)
        expect(href?.get("q")).toBe("abc")
        expect(href?.get("purchaseOrderId")).toBe("PO1")
        expect(href?.has("supplierId")).toBe(false)
        expect(href?.has("status")).toBe(false)
        expect(view.result.current.supplierDraft).toBeNull()
        expect(view.result.current.statusDraft).toBe("all")
        // 局部重置保持面板展开
        expect(view.result.current.panelOpen).toBe(true)
    })

    it("removeFilter removes a single condition, its draft, and resets the page", () => {
        const { view, router } = renderPage("q=abc&supplierId=S1&page=2")

        act(() => {
            view.result.current.removeFilter("supplierId")
        })

        const href = lastReplaceHref(router)
        expect(href?.has("supplierId")).toBe(false)
        expect(href?.get("q")).toBe("abc")
        expect(href?.has("page")).toBe(false)
        expect(view.result.current.supplierDraft).toBeNull()
    })

    it("opens the panel initially when the deep link carries structured filters", () => {
        const { view } = renderPage("supplierId=S1&status=OPEN")
        expect(view.result.current.panelOpen).toBe(true)

        const { view: view2 } = renderPage("q=abc&purchaseOrderId=PO1")
        // 仅关键词/来源锁定不强制展开面板
        expect(view2.result.current.panelOpen).toBe(false)
    })

    it("does not force the panel open again on URL backfill after a successful submit", () => {
        const { view, router } = renderPage("")

        act(() => {
            view.result.current.setPanelOpen(true)
            view.result.current.setSupplierDraft("S1")
        })
        act(() => {
            view.result.current.applyFilters()
        })
        expect(view.result.current.panelOpen).toBe(false)

        // 提交后 URL 回填只同步草稿，不得重新展开面板
        currentUrl = "supplierId=S1"
        act(() => {
            view.rerender()
        })
        expect(view.result.current.supplierDraft).toBe("S1")
        expect(view.result.current.panelOpen).toBe(false)
        expect(router.push).not.toHaveBeenCalled()
    })

    it("builds applied chips for every consumed filter, including source locks", () => {
        currentListData = makeListView({
            suppliers: [
                {
                    supplierId: "S1",
                    supplierName: "供应商甲",
                    openPayableTotal: "0",
                    unallocatedPaymentTotal: "0",
                    unallocatedInvoiceTotal: "0",
                },
            ],
        })
        const { view } = renderPage(
            "q=abc&supplierId=S1&sourceType=PURCHASE_ORDER&status=PARTIAL&due=overdue&paymentGate=unsatisfied&track=payment&purchaseOrderId=PO1",
        )

        expect(view.result.current.appliedChips).toEqual([
            { key: "q", label: "搜索：abc" },
            { key: "supplierId", label: "供应商：供应商甲" },
            { key: "sourceType", label: "来源类型：采购单" },
            { key: "status", label: "状态：部分结清" },
            { key: "due", label: "到期：已到期" },
            { key: "paymentGate", label: "先款条件：未满足" },
            { key: "track", label: "轨道：付款" },
            { key: "purchaseOrderId", label: "采购单：PO1" },
        ])
    })

    it("syncs drafts when URL params change (back/forward, quick filters)", () => {
        const { view, router } = renderPage("")

        currentUrl = "q=hello&supplierId=S1&status=OPEN"
        act(() => {
            view.rerender()
        })
        expect(view.result.current.searchInput).toBe("hello")
        expect(view.result.current.supplierDraft).toBe("S1")
        expect(view.result.current.statusDraft).toBe("OPEN")
        // 回填只改草稿，不写 URL
        expect(router.replace).not.toHaveBeenCalled()
        expect(router.push).not.toHaveBeenCalled()
    })

    it("openPreview/closePreview toggle preview state and the detailId param", () => {
        const { view, router } = renderPage("")

        act(() => {
            view.result.current.openPreview("PA9")
        })
        expect(view.result.current.previewPayableId).toBe("PA9")
        expect(lastReplaceHref(router)?.get("detailId")).toBe("PA9")

        act(() => {
            view.result.current.closePreview()
        })
        expect(view.result.current.previewPayableId).toBeNull()
        expect(lastReplaceHref(router)?.has("detailId")).toBe(false)
    })

    it("openRefundPreview writes refund previewKind and clears other previews", () => {
        const { view, router } = renderPage("previewKind=payment&detailId=PMT-1")

        act(() => {
            view.result.current.openRefundPreview("srf-1")
        })
        expect(view.result.current.previewRefundId).toBe("srf-1")
        expect(view.result.current.previewPaymentId).toBeNull()
        expect(view.result.current.previewPayableId).toBeNull()
        expect(view.result.current.previewReversalId).toBeNull()
        const href = lastReplaceHref(router)
        expect(href?.get("detailId")).toBe("srf-1")
        expect(href?.get("previewKind")).toBe("refund")
    })

    it("openReversalPreview writes reversal previewKind and clears other previews", () => {
        const { view, router } = renderPage("previewKind=refund&detailId=srf-1")

        act(() => {
            view.result.current.openReversalPreview("pr-1")
        })
        expect(view.result.current.previewReversalId).toBe("pr-1")
        expect(view.result.current.previewRefundId).toBeNull()
        expect(view.result.current.previewPaymentId).toBeNull()
        expect(view.result.current.previewPayableId).toBeNull()
        const href = lastReplaceHref(router)
        expect(href?.get("detailId")).toBe("pr-1")
        expect(href?.get("previewKind")).toBe("reversal")
    })

    it("openSession/closeSession write and clear session URL params", () => {
        const { view, router } = renderPage("")

        act(() => {
            view.result.current.openSession({
                track: "payment",
                supplierId: "S1",
                existingPaymentId: "PMT-1",
            })
        })
        expect(view.result.current.session).toEqual({
            track: "payment",
            supplierId: "S1",
            existingPaymentId: "PMT-1",
        })
        let href = lastReplaceHref(router)
        expect(href?.get("session")).toBe("payment")
        expect(href?.get("supplierId")).toBe("S1")
        expect(href?.get("paymentId")).toBe("PMT-1")
        expect(href?.has("detailId")).toBe(false)
        expect(href?.has("invoiceId")).toBe(false)

        act(() => {
            view.result.current.closeSession()
        })
        expect(view.result.current.session).toBeNull()
        href = lastReplaceHref(router)
        expect(href?.has("session")).toBe(false)
        expect(href?.has("paymentId")).toBe(false)
        expect(href?.has("invoiceId")).toBe(false)
    })

    it("opens a session from a session deep link once data is available", () => {
        const { view } = renderPage(
            "session=payment&supplierId=S1&purchaseOrderId=PO1&returnTo=%2Fback&from=W08&paymentId=PMT-2",
        )

        expect(view.result.current.session).toEqual({
            track: "payment",
            supplierId: "S1",
            purchaseOrderId: "PO1",
            returnTo: "/back",
            fromWorkspace: "W08",
            existingPaymentId: "PMT-2",
        })
    })

    it("resolves the supplier from the purchase order on a W08 deep link and writes the URL once", () => {
        currentListData = makeListView({
            payables: [
                makePayable({
                    payableAccountId: "PA9",
                    supplierId: "S9",
                    sourceType: "PURCHASE_ORDER",
                    sourceDocumentId: "PO-77",
                }),
            ],
        })
        const { view, router } = renderPage("from=W08&purchaseOrderId=PO-77")

        expect(view.result.current.session).toEqual({
            track: "payment",
            supplierId: "S9",
            purchaseOrderId: "PO-77",
            returnTo: undefined,
            fromWorkspace: "W08",
            preselectPayableAccountId: "PA9",
        })
        expect(lastReplaceHref(router)?.get("session")).toBe("payment")
        expect(lastReplaceHref(router)?.get("supplierId")).toBe("S9")

        const replaceCalls = router.replace.mock.calls.length
        act(() => {
            view.rerender()
        })
        expect(router.replace.mock.calls.length).toBe(replaceCalls)
    })

    it("sorts payables client-side by due date ascending and supplier name descending", () => {
        currentListData = makeListView({
            payables: [
                makePayable({
                    payableAccountId: "PA1",
                    supplierName: "乙公司",
                    dueDate: "2026-08-20",
                }),
                makePayable({
                    payableAccountId: "PA2",
                    supplierName: "甲公司",
                    dueDate: "2026-08-10",
                }),
            ],
        })
        const { view } = renderPage("")

        act(() => {
            view.result.current.setSorting([{ id: "due", desc: false }])
        })
        expect(
            view.result.current.sortedPayables.map((p) => p.payableAccountId),
        ).toEqual(["PA2", "PA1"])

        act(() => {
            view.result.current.setSorting([{ id: "supplier", desc: true }])
        })
        expect(
            view.result.current.sortedPayables.map((p) => p.payableAccountId),
        ).toEqual(["PA1", "PA2"])

        act(() => {
            view.result.current.setSorting([])
        })
        expect(view.result.current.sortedPayables).toHaveLength(2)
    })

    it("pushes the settlement link carrying the current URL as returnTo", () => {
        const { view, router } = renderPage("view=payable&supplierId=S1")

        act(() => {
            view.result.current.openSettlements()
        })

        const href = lastPushHref(router)
        expect(href.startsWith("/supplier-api/settlements?")).toBe(true)
        const params = new URLSearchParams(href.slice(href.indexOf("?") + 1))
        expect(params.get("supplierId")).toBe("S1")
        expect(params.get("returnTo")).toBe(
            "/test?view=payable&supplierId=S1",
        )
    })

    it("derives empty state from an error list query", () => {
        mockedListQuery.mockImplementation(
            () =>
                ({
                    data: undefined,
                    isPending: false,
                    isError: true,
                    error: new Error("boom"),
                    refetch: vi.fn(),
                }) as unknown as ReturnType<typeof useSupplierAccountsQuery>,
        )
        const { view } = renderPage("")

        expect(view.result.current.listQuery.isError).toBe(true)
        expect(view.result.current.data).toBeUndefined()
        expect(view.result.current.sortedPayables).toEqual([])
    })
})
