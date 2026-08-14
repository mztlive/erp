import { waitFor } from "@testing-library/react"
import type { QueryObserverOptions } from "@tanstack/react-query"
import { beforeEach, describe, expect, it, vi } from "vitest"

import * as api from "@/features/supplier-api-connections/api/connections"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { useOpaqueReferenceOptionsQuery } from "@/features/supplier-api-connections/hooks/use-opaque-reference-options"

vi.mock("@/features/supplier-api-connections/api/connections", () => ({
    fetchOpaqueReferenceOptions: vi.fn(),
}))

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useOpaqueReferenceOptionsQuery", () => {
    it("queries credential options under a stable key with the configured staleTime", async () => {
        vi.mocked(api.fetchOpaqueReferenceOptions).mockResolvedValue([
            { referenceId: "r1", alias: "别-1", version: "v1" },
        ])
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useOpaqueReferenceOptionsQuery("credential"),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() =>
            expect(result.current.data).toEqual([
                { referenceId: "r1", alias: "别-1", version: "v1" },
            ]),
        )
        expect(api.fetchOpaqueReferenceOptions).toHaveBeenCalledTimes(1)
        expect(api.fetchOpaqueReferenceOptions).toHaveBeenCalledWith(
            "credential",
        )
        const query = client.getQueryCache().getAll()[0]!
        const options = query.options as QueryObserverOptions<
            Array<{ referenceId: string; alias: string; version: string }>,
            Error,
            Array<{ referenceId: string; alias: string; version: string }>,
            Array<{ referenceId: string; alias: string; version: string }>,
            readonly unknown[]
        >
        expect(query.queryKey).toEqual([
            "supplier-api-connections",
            "opaque-reference-options",
            "credential",
        ])
        expect(options.staleTime).toBe(5 * 60 * 1000)
    })

    it("queries endpoint options with the same key prefix but the endpoint kind", async () => {
        vi.mocked(api.fetchOpaqueReferenceOptions).mockResolvedValue([])
        const client = createFreshQueryClient()
        renderHookWithProviders(
            () => useOpaqueReferenceOptionsQuery("endpoint"),
            { queryClient: client },
        )
        await waitFor(() =>
            expect(api.fetchOpaqueReferenceOptions).toHaveBeenCalledWith(
                "endpoint",
            ),
        )
        expect(client.getQueryCache().getAll()[0]!.queryKey).toEqual([
            "supplier-api-connections",
            "opaque-reference-options",
            "endpoint",
        ])
    })

    it("exposes the error state when the options request fails", async () => {
        vi.mocked(api.fetchOpaqueReferenceOptions).mockRejectedValue(
            new Error("options failed"),
        )
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useOpaqueReferenceOptionsQuery("credential"),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})
