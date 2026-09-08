import { act, renderHook, cleanup } from "@testing-library/react"
import { afterEach, beforeEach, expect, it, vi } from "vitest"
import { useContractsList } from "@/features/contracts/hooks/use-contracts-list"
import { useLedgerSearch } from "@/features/inventory/pages/hooks/use-ledger-search"
import { useIntegrationSearch } from "@/features/integration-errors/pages/hooks/use-integration-search"

const navigation = vi.hoisted(() => ({
    query: "q=old&page=6",
    replace: vi.fn(),
    fetch: vi.fn(),
}))
vi.mock("next/navigation", () => ({
    usePathname: () => "/sales/contracts",
    useRouter: () => ({ replace: navigation.replace }),
    useSearchParams: () => new URLSearchParams(navigation.query),
}))
vi.mock("@/features/contracts/hooks/queries", () => ({
    useContractsQuery: (query: unknown) => {
        navigation.fetch(query)
        return { data: undefined }
    },
}))
beforeEach(() => {
    navigation.query = "q=old&page=6"
    vi.clearAllMocks()
})
afterEach(() => {
    cleanup()
    document.body.innerHTML = ""
})

it("合同草稿不触发新关键词请求，提交回第一页，历史导航回填聚焦中的输入", () => {
    const { result, rerender } = renderHook(() => useContractsList())
    act(() => result.current.setSearchDraft(" new "))
    expect(navigation.fetch).toHaveBeenLastCalledWith(
        expect.objectContaining({ q: "old", page: 6 }),
    )
    act(() => result.current.applyFilters())
    const url = new URL(navigation.replace.mock.calls[0][0], "http://localhost")
    expect(url.searchParams.get("q")).toBe("new")
    expect(url.searchParams.get("page")).not.toBe("6")
    const input = document.createElement("input")
    document.body.append(input)
    result.current.searchInputRef.current = input
    input.focus()
    navigation.query = "q=history&page=2"
    rerender()
    expect(result.current.searchDraft).toBe("history")
    expect(navigation.fetch).toHaveBeenLastCalledWith(
        expect.objectContaining({ q: "history", page: 2 }),
    )
    act(() => result.current.clearAllFilters())
    const cleared = new URL(
        navigation.replace.mock.calls.at(-1)![0],
        "http://localhost",
    )
    expect(cleared.searchParams.has("q")).toBe(false)
})

it("库存和集成关键词在输入聚焦时仍跟随历史 URL 回填", () => {
    const ledger = renderHook(({ q }) => useLedgerSearch({ qParam: q }), {
        initialProps: { q: "old" },
    })
    const integration = renderHook(({ q }) => useIntegrationSearch({ q }), {
        initialProps: { q: "old" },
    })
    for (const item of [ledger, integration]) {
        const input = document.createElement("input")
        document.body.append(input)
        item.result.current.searchInputRef.current = input
        input.focus()
        item.rerender({ q: "restored" })
        expect(item.result.current.searchDraft).toBe("restored")
    }
})
