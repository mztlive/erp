import {
    act,
    cleanup,
    fireEvent,
    render,
    renderHook,
    screen,
} from "@testing-library/react"
import { afterEach, beforeEach, expect, test, vi } from "vitest"

import { useBatchListFilters } from "@/features/import-opening/hooks/use-batch-list-filters"
import { BatchListToolbar } from "@/features/import-opening/components/batch-list-toolbar"
import { parseImportOpeningSearchParams } from "@/features/import-opening/lib/url-state"
import {
    parseIntegrationSearchParams,
    toResolutionQuery,
} from "@/features/integration-errors/lib/url-state"
import { IntegrationQueueToolbar } from "@/features/integration-errors/pages/components/integration-queue-toolbar"
import { useCustomerReceivablesUrlState } from "@/features/customer-receivables/pages/hooks/use-customer-receivables-url-state"
import { useSupplierAccountsFilters } from "@/features/supplier-payables/pages/hooks/use-supplier-accounts-filters"
import { CustomerReceivablesToolbar } from "@/features/customer-receivables/pages/components/customer-receivables-toolbar"
import { SupplierAccountsToolbar } from "@/features/supplier-payables/pages/components/supplier-accounts-toolbar"

const navigation = vi.hoisted(() => ({
    query: "",
    replace: vi.fn(),
    push: vi.fn(),
    fetch: vi.fn(),
}))
vi.mock("next/navigation", () => ({
    usePathname: () => "/finance/accounts",
    useRouter: () => ({ replace: navigation.replace, push: navigation.push }),
    useSearchParams: () => new URLSearchParams(navigation.query),
}))
vi.mock("@/features/supplier-payables/hooks/queries", () => ({
    useSupplierAccountsQuery: (query: unknown) => {
        navigation.fetch(query)
        return { data: undefined }
    },
}))
vi.mock(
    "@/features/customer-receivables/components/receivable-counterparty-search-combobox",
    () => ({
        ReceivableCounterpartySearchCombobox: () => (
            <input aria-label="筛选往来主体" />
        ),
    }),
)
vi.mock("@/features/entity-selectors", () => ({
    SupplierSearchCombobox: () => <input aria-label="供应商" />,
}))
afterEach(cleanup)
beforeEach(() => {
    navigation.query = ""
    vi.clearAllMocks()
})

function submittedParams() {
    return new URL(navigation.replace.mock.calls.at(-1)![0], "http://localhost")
        .searchParams
}

test("批次旧状态不再限制列表；保留批次号、对象查询与分页重置", () => {
    const state = parseImportOpeningSearchParams(
        new URLSearchParams("status=FAILED&q=IMP-1&objectType=SKU&page=5"),
    )
    expect(state.status).toBeUndefined()
    const patchUrl = vi.fn()
    const { result } = renderHook(() =>
        useBatchListFilters({ urlState: state, patchUrl }),
    )
    render(
        <BatchListToolbar
            searchInputRef={result.current.searchInputRef}
            searchDraft={result.current.qDraft}
            setSearchDraft={result.current.setQDraft}
            appliedChips={result.current.appliedChips}
            removeFilter={result.current.removeBatchFilter}
            applyBatchFilters={result.current.applyBatchFilters}
            objectTypeDraft={result.current.objectTypeDraft}
            setObjectTypeDraft={result.current.setObjectTypeDraft}
            clearAllFilters={result.current.clearAllBatchFilters}
        />,
    )
    expect(screen.queryByRole("button", { name: /更多筛选/ })).toBeNull()
    expect(screen.queryByLabelText("批次状态")).toBeNull()
    act(() => result.current.setQDraft(" IMP-2 "))
    expect(patchUrl).not.toHaveBeenCalled()
    act(() => result.current.applyBatchFilters())
    expect(patchUrl).toHaveBeenLastCalledWith({
        q: "IMP-2",
        objectType: "SKU",
        status: undefined,
        page: 1,
    })
})

test("接口错误类别由 q 提交，旧独立类别不再与关键词叠加", () => {
    const urlState = parseIntegrationSearchParams(
        new URLSearchParams("errorClass=result-unknown&q=临时故障"),
    )
    expect(toResolutionQuery(urlState)).toMatchObject({ q: "临时故障" })
    expect(toResolutionQuery(urlState).errorClass).toBeUndefined()
    const patchUrl = vi.fn()
    render(
        <IntegrationQueueToolbar
            urlState={urlState}
            searchDraft=" 调用次数受限 "
            onSearchDraftChange={vi.fn()}
            searchInputRef={{ current: null }}
            autoNext={false}
            patchUrl={patchUrl}
            onClearFilters={vi.fn()}
        />,
    )
    expect(screen.getByRole("button", { name: /更多筛选/ })).toBeTruthy()
    expect(screen.getByPlaceholderText(/错误类别/)).toBeTruthy()
    fireEvent.submit(screen.getByRole("form", { name: "接口错误队列查询" }))
    expect(patchUrl).toHaveBeenLastCalledWith(
        expect.objectContaining({ q: "调用次数受限", errorClass: null }),
    )
})

test.each(["receipt", "sales_invoice", "unallocated"])(
    "客户往来 %s 只使用主体关键词，旧主体条件不再请求",
    (view) => {
        navigation.query = `view=${view}&counterpartyId=party-old&q=客户甲&page=5`
        const { result } = renderHook(() => useCustomerReceivablesUrlState())
        expect(result.current.query).toMatchObject({
            q: "客户甲",
            counterpartyPartyId: undefined,
        })
        expect(result.current.panelOpen).toBe(false)
        const state = result.current
        render(
            <CustomerReceivablesToolbar
                {...state}
                appliedChips={[]}
                loading={false}
                failed={false}
            />,
        )
        expect(screen.queryByRole("button", { name: /更多筛选/ })).toBeNull()
        act(() => result.current.setSearchDraft(" 客户乙 "))
        act(() => result.current.applyFilters())
        expect(submittedParams().get("q")).toBe("客户乙")
        expect(submittedParams().has("counterpartyId")).toBe(false)
        expect(submittedParams().has("page")).toBe(false)
    },
)

test.each(["payment", "purchase_invoice", "unallocated"])(
    "供应商往来 %s 只使用供应商关键词，旧供应商条件不再请求",
    (view) => {
        navigation.query = `view=${view}&supplierId=supplier-old&q=供应商甲&page=5`
        const { result } = renderHook(() => useSupplierAccountsFilters())
        expect(navigation.fetch).toHaveBeenLastCalledWith(
            expect.objectContaining({ q: "供应商甲", supplierId: undefined }),
        )
        expect(result.current.panelOpen).toBe(false)
        const state = result.current
        render(
            <SupplierAccountsToolbar
                {...state}
                onSearchInputChange={state.setSearchInput}
                clearAllFilters={state.clearFilters}
                hasPendingChanges={state.hasPendingChanges}
                loading={false}
                failed={false}
            />,
        )
        if (view === "unallocated") {
            expect(
                screen.getByRole("button", { name: /更多筛选/ }),
            ).toBeTruthy()
        } else {
            expect(
                screen.queryByRole("button", { name: /更多筛选/ }),
            ).toBeNull()
        }
        act(() => result.current.setSearchInput(" 供应商乙 "))
        act(() => result.current.applyFilters())
        expect(submittedParams().get("q")).toBe("供应商乙")
        expect(submittedParams().has("supplierId")).toBe(false)
        expect(submittedParams().has("page")).toBe(false)
    },
)

test("应收与应付台账仍保留多条件面板和主体筛选", () => {
    navigation.query = "view=receivable&counterpartyId=party-old"
    const customer = renderHook(() => useCustomerReceivablesUrlState())
    expect(customer.result.current.query.counterpartyPartyId).toBe("party-old")
    render(
        <CustomerReceivablesToolbar
            {...customer.result.current}
            appliedChips={[]}
            loading={false}
            failed={false}
        />,
    )
    expect(screen.getByRole("button", { name: /更多筛选/ })).toBeTruthy()
    cleanup()
    navigation.query = "view=payable&supplierId=supplier-old"
    const supplier = renderHook(() => useSupplierAccountsFilters())
    const state = supplier.result.current
    expect(navigation.fetch).toHaveBeenLastCalledWith(
        expect.objectContaining({ supplierId: "supplier-old" }),
    )
    render(
        <SupplierAccountsToolbar
            {...state}
            onSearchInputChange={state.setSearchInput}
            clearAllFilters={state.clearFilters}
            loading={false}
            failed={false}
        />,
    )
    expect(screen.getByRole("button", { name: /更多筛选/ })).toBeTruthy()
})
