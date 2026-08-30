import { act, cleanup, renderHook, waitFor } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { useProfitLossUrlState } from "@/features/actual-profit-loss/hooks/use-profit-loss-url-state"
import type { ProfitLossPeriodBasisConfig } from "@/features/actual-profit-loss/types"

const navigation = vi.hoisted(() => ({
    pathname: "/finance/actual-profit-loss",
    searchParams: new URLSearchParams(),
    replace: vi.fn(),
    push: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    usePathname: () => navigation.pathname,
    useRouter: () => ({
        replace: navigation.replace,
        push: navigation.push,
    }),
    useSearchParams: () => navigation.searchParams,
}))

const basisConfig: ProfitLossPeriodBasisConfig = {
    configuredPeriodBasis: "recognized_at",
    allowedPeriodBases: [
        {
            code: "recognized_at",
            label: "确认时间",
            explanation: "按收入确认时间归属",
        },
    ],
    configurationVersion: "v1",
}

describe("useProfitLossUrlState", () => {
    beforeEach(() => {
        navigation.searchParams = new URLSearchParams()
        navigation.replace.mockReset()
        navigation.push.mockReset()
    })

    afterEach(cleanup)

    it("includes one-based page and page size in the server query", () => {
        navigation.searchParams = new URLSearchParams({
            from: "2026-08-01",
            to: "2026-08-31",
            periodBasis: "recognized_at",
            page: "2",
            pageSize: "50",
        })

        const { result } = renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )

        expect(result.current.query).toMatchObject({
            from: "2026-08-01",
            to: "2026-08-31",
            periodBasis: "recognized_at",
            page: 2,
            pageSize: 50,
        })

        act(() => {
            result.current.setPagination({ pageIndex: 2, pageSize: 50 })
        })
        expect(navigation.replace).toHaveBeenCalledWith(
            "/finance/actual-profit-loss?from=2026-08-01&to=2026-08-31&periodBasis=recognized_at&page=3&pageSize=50",
            { scroll: false },
        )
    })

    it("writes the configured basis without deleting an explicit period", async () => {
        navigation.searchParams = new URLSearchParams({
            from: "2026-07-01",
            to: "2026-07-31",
        })

        renderHook(() =>
            useProfitLossUrlState({ basisConfig, basisResolved: true }),
        )

        await waitFor(() => {
            expect(navigation.replace).toHaveBeenCalledWith(
                "/finance/actual-profit-loss?from=2026-07-01&to=2026-07-31&periodBasis=recognized_at",
                { scroll: false },
            )
        })
    })
})
