import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { useImportOpeningUrlState } from "@/features/import-opening/hooks/use-import-opening-url"

const mocks = vi.hoisted(() => ({
    routerReplace: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: vi.fn(),
        replace: mocks.routerReplace,
        back: vi.fn(),
    }),
    useSearchParams: vi.fn(() => new URLSearchParams()),
    usePathname: vi.fn(() => "/test"),
    useParams: vi.fn(() => ({})),
}))

import { usePathname, useSearchParams } from "next/navigation"
import type { ReadonlyURLSearchParams } from "next/navigation"

function params(qs: string): ReadonlyURLSearchParams {
    return new URLSearchParams(qs) as unknown as ReadonlyURLSearchParams
}

beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(usePathname).mockReturnValue("/governance/imports")
    vi.mocked(useSearchParams).mockReturnValue(params(""))
})

describe("useImportOpeningUrlState", () => {
    it("parses defaults from an empty query string", () => {
        const { result } = renderHook(() => useImportOpeningUrlState())
        expect(result.current.urlState).toEqual({
            environment: "VALIDATION",
            section: "overview",
            page: 1,
        })
    })

    it("parses query params into url state", () => {
        vi.mocked(useSearchParams).mockReturnValue(
            params(
                "batchId=b1&section=trial&page=3&status=RECEIVING&issueCode=MAPPING_CONFLICT",
            ),
        )
        const { result } = renderHook(() => useImportOpeningUrlState())
        expect(result.current.urlState).toMatchObject({
            environment: "VALIDATION",
            batchId: "b1",
            section: "trial",
            page: 3,
            status: "RECEIVING",
            issueCode: "MAPPING_CONFLICT",
        })
    })

    it("reads the legacy id alias for batchId", () => {
        vi.mocked(useSearchParams).mockReturnValue(params("id=b9"))
        const { result } = renderHook(() => useImportOpeningUrlState())
        expect(result.current.urlState.batchId).toBe("b9")
    })

    it("normalizes the environment param", () => {
        vi.mocked(useSearchParams).mockReturnValue(
            params("environment=production"),
        )
        const { result } = renderHook(() => useImportOpeningUrlState())
        expect(result.current.urlState.environment).toBe("PRODUCTION")
    })

    it("patchUrl merges into the current state and replaces the URL", () => {
        vi.mocked(useSearchParams).mockReturnValue(
            params("batchId=b1&section=trial&page=1"),
        )
        const { result } = renderHook(() => useImportOpeningUrlState())

        act(() => {
            result.current.patchUrl({ page: 2 })
        })

        expect(mocks.routerReplace).toHaveBeenCalledWith(
            "/governance/imports?batchId=b1&section=trial&page=2",
            { scroll: false },
        )
    })

    it("replaceUrl drops reset state from the URL", () => {
        vi.mocked(useSearchParams).mockReturnValue(
            params("batchId=b1&section=trial&workItemId=w1&page=2&q=x"),
        )
        const { result } = renderHook(() => useImportOpeningUrlState())

        act(() => {
            result.current.replaceUrl({
                ...result.current.urlState,
                batchId: undefined,
                section: "overview",
                workItemId: undefined,
                page: 1,
                q: undefined,
            })
        })

        expect(mocks.routerReplace).toHaveBeenCalledWith(
            "/governance/imports",
            { scroll: false },
        )
    })

    it("keeps environment=VALIDATION out of the URL", () => {
        const { result } = renderHook(() => useImportOpeningUrlState())
        act(() => {
            result.current.patchUrl({ section: "trial", page: 1 })
        })
        expect(mocks.routerReplace).toHaveBeenCalledWith(
            "/governance/imports",
            { scroll: false },
        )
    })
})
