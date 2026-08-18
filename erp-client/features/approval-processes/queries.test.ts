import { waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { createApiError } from "@/lib/api/errors"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

import {
    createDefinitionDraft,
    fetchDefinitionCatalog,
    fetchDefinitionDetail,
    publishDefinition,
} from "./api"
import { catalogFixture, detailFixture } from "./fixtures"
import { ok } from "./result"
import {
    approvalProcessKeys,
    invalidateDefinitionQueries,
    useCreateDefinitionDraftMutation,
    useDefinitionCatalogQuery,
    usePublishDefinitionMutation,
} from "./queries"

vi.mock("./api", () => ({
    fetchDefinitionCatalog: vi.fn(),
    fetchDefinitionVersions: vi.fn(),
    fetchDefinitionDetail: vi.fn(),
    fetchEligibleAssignees: vi.fn(),
    createDefinitionDraft: vi.fn(),
    replaceDefinitionNodes: vi.fn(),
    publishDefinition: vi.fn(),
    retireDefinition: vi.fn(),
}))

const mockedCatalog = vi.mocked(fetchDefinitionCatalog)
const mockedCreate = vi.mocked(createDefinitionDraft)
const mockedPublish = vi.mocked(publishDefinition)
const mockedDetail = vi.mocked(fetchDefinitionDetail)

beforeEach(() => {
    vi.clearAllMocks()
})

describe("approval process queries", () => {
    it("uses the fixed catalog query key", async () => {
        const rows = catalogFixture()
        mockedCatalog.mockResolvedValue(ok(rows))
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useDefinitionCatalogQuery(),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.data).toEqual(rows))
        expect(client.getQueryData(approvalProcessKeys.catalog())).toEqual(rows)
        expect(approvalProcessKeys.catalog()).toEqual([
            "approvalProcesses",
            "catalog",
        ])
        expect(approvalProcessKeys.versions("sales_order")).toEqual([
            "approvalProcesses",
            "versions",
            "sales_order",
        ])
        expect(approvalProcessKeys.detail("def-1")).toEqual([
            "approvalProcesses",
            "detail",
            "def-1",
        ])
        expect(
            approvalProcessKeys.eligibleAssignees("sales_order", "张"),
        ).toEqual([
            "approvalProcesses",
            "eligibleAssignees",
            "sales_order",
            "张",
        ])
    })

    it("keeps previous catalog visible while refetching", async () => {
        const first = catalogFixture()
        mockedCatalog.mockResolvedValue(ok(first))
        const { result } = renderHookWithProviders(() =>
            useDefinitionCatalogQuery(),
        )
        await waitFor(() => expect(result.current.data).toEqual(first))
        mockedCatalog.mockImplementation(
            () =>
                new Promise(() => undefined) as ReturnType<
                    typeof fetchDefinitionCatalog
                >,
        )
        void result.current.refetch()
        expect(result.current.data).toEqual(first)
    })

    it("surfaces ResultAsync errors on the query", async () => {
        mockedCatalog.mockResolvedValue({
            ok: false,
            error: createApiError({
                kind: "Http",
                status: 403,
                message: "forbidden",
            }),
        })
        const { result } = renderHookWithProviders(() =>
            useDefinitionCatalogQuery(),
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
    })

    it("invalidates only catalog versions and the returned definition", async () => {
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        await invalidateDefinitionQueries(
            client,
            detailFixture({
                definition_id: "def-9",
                document_type: "stock_adjustment",
            }),
        )
        expect(invalidate).toHaveBeenCalledTimes(3)
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: approvalProcessKeys.catalog(),
        })
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: approvalProcessKeys.versions("stock_adjustment"),
        })
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: approvalProcessKeys.detail("def-9"),
        })
        expect(invalidate).not.toHaveBeenCalledWith({})
    })

    it("create mutation unwraps ResultAsync and invalidates precisely", async () => {
        const created = detailFixture({ definition_id: "def-new" })
        mockedCreate.mockResolvedValue(ok(created))
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useCreateDefinitionDraftMutation(),
            { queryClient: client },
        )
        await result.current.mutateAsync({
            document_type: "stock_adjustment",
            name: "库存",
            draft_source: "EMPTY",
            idempotency_key: "k1",
        })
        expect(invalidate).toHaveBeenCalledWith({
            queryKey: approvalProcessKeys.detail("def-new"),
        })
    })

    it("does not auto overwrite when publish returns 409", async () => {
        mockedPublish.mockResolvedValue({
            ok: false,
            error: createApiError({
                kind: "Http",
                status: 409,
                code: "APPROVAL_DEFINITION_VERSION_CONFLICT",
                message: "stale",
            }),
        })
        mockedDetail.mockResolvedValue(
            ok(detailFixture({ definition_lock_version: "9" })),
        )
        const { result } = renderHookWithProviders(() =>
            usePublishDefinitionMutation(),
        )
        await expect(
            result.current.mutateAsync({
                definitionId: "def-1",
                request: {
                    expected_definition_lock_version: "3",
                    idempotency_key: "old-key",
                },
            }),
        ).rejects.toMatchObject({
            status: 409,
            code: "APPROVAL_DEFINITION_VERSION_CONFLICT",
        })
        expect(result.current.isPending).toBe(false)
    })
})
