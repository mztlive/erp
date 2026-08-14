import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import * as api from "@/features/supplier-api-connections/api/connections"
import {
    useBindCredentialMutation,
    useBindEndpointMutation,
    useConnectionCenterQuery,
    useConnectionListQuery,
    useCreateConnectionMutation,
    useDisableConnectionMutation,
    useEnableConnectionMutation,
    useRunHealthCheckMutation,
    useStartCatalogSyncMutation,
    useUpdateCapabilitiesMutation,
} from "@/features/supplier-api-connections/hooks/queries"
import type { FormalOutcome } from "@/features/supplier-api-connections/types"

vi.mock("@/features/supplier-api-connections/api/connections", () => ({
    bindCredentialReference: vi.fn(),
    bindEndpointReference: vi.fn(),
    createConnection: vi.fn(),
    disableConnection: vi.fn(),
    enableConnection: vi.fn(),
    fetchConnectionCenter: vi.fn(),
    fetchConnectionList: vi.fn(),
    runHealthCheck: vi.fn(),
    startCatalogSync: vi.fn(),
    updateCapabilities: vi.fn(),
}))

const succeeded: FormalOutcome = {
    status: "succeeded",
    title: "成功",
    message: "已完成",
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useConnectionListQuery", () => {
    it("fetches the list with the given input under a stable query key", async () => {
        vi.mocked(api.fetchConnectionList).mockResolvedValue({
            metrics: {
                enabled: 0,
                faulted: 0,
                pendingConfig: 0,
                healthAbnormal: 0,
                catalogStale: 0,
            },
            items: [],
            total: 0,
            page: 1,
            pageSize: 20,
            hasModulePermission: true,
            hasDataScope: true,
            projectedAt: "2026-01-01T00:00:00.000Z",
            credentialOpaqueOptions: [],
            endpointOpaqueOptions: [],
        })
        const input = { environment: "PRODUCTION", page: 1 }
        const client = createFreshQueryClient()
        const { result, rerender } = renderHookWithProviders(
            () => useConnectionListQuery(input),
            { queryClient: client },
        )

        expect(result.current.isPending).toBe(true)
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data?.total).toBe(0)
        expect(api.fetchConnectionList).toHaveBeenCalledTimes(1)
        expect(api.fetchConnectionList).toHaveBeenCalledWith(input)
        expect(
            client
                .getQueryCache()
                .getAll()
                .map((q) => q.queryKey),
        ).toEqual([["supplier-api-connections", "list", input]])

        // 同一输入再渲染不重复请求（key 稳定）
        rerender()
        await waitFor(() => expect(client.isFetching()).toBe(0))
        expect(api.fetchConnectionList).toHaveBeenCalledTimes(1)
    })

    it("exposes the error state when the request fails", async () => {
        vi.mocked(api.fetchConnectionList).mockRejectedValue(
            new Error("list failed"),
        )
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useConnectionListQuery({ environment: "ALL", page: 1 }),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
        expect(result.current.data).toBeUndefined()
    })
})

describe("useConnectionCenterQuery", () => {
    it("stays disabled when no connectionId is provided", () => {
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useConnectionCenterQuery(undefined),
            { queryClient: client },
        )
        expect(result.current.isPending).toBe(true)
        expect(result.current.fetchStatus).toBe("idle")
        expect(api.fetchConnectionCenter).not.toHaveBeenCalled()
    })

    it('fetches the center view and resolves null as a valid "not found" result', async () => {
        vi.mocked(api.fetchConnectionCenter).mockResolvedValue(null)
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useConnectionCenterQuery("c1"),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        expect(result.current.data).toBe(null)
        expect(api.fetchConnectionCenter).toHaveBeenCalledWith({
            connectionId: "c1",
        })
    })

    it("exposes the error state when the detail request fails", async () => {
        vi.mocked(api.fetchConnectionCenter).mockRejectedValue(
            new Error("center failed"),
        )
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useConnectionCenterQuery("c1"),
            { queryClient: client },
        )
        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("mutation hooks", () => {
    it("wires createConnection and invalidates all connection queries on success", async () => {
        vi.mocked(api.createConnection).mockResolvedValue(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useCreateConnectionMutation(),
            { queryClient: client },
        )
        const input = {
            connectionCode: "CONN-1",
            supplierId: "s1",
            supplierName: "供应商",
            environment: "PRODUCTION" as const,
            idempotencyKey: "create_1",
        }
        let outcome: unknown
        await act(async () => {
            outcome = await result.current.mutateAsync(input)
        })
        expect(outcome).toBe(succeeded)
        expect(api.createConnection).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })

    it("does not invalidate when createConnection is rejected", async () => {
        const rejected: FormalOutcome = {
            status: "rejected",
            code: "X",
            title: "被拒绝",
            message: "不满足条件",
        }
        vi.mocked(api.createConnection).mockResolvedValue(rejected)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useCreateConnectionMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync({
                connectionCode: "CONN-1",
                supplierId: "s1",
                supplierName: "供应商",
                environment: "STAGING",
                idempotencyKey: "create_2",
            })
        })
        expect(invalidate).not.toHaveBeenCalled()
    })

    it("wires bindCredentialReference and invalidates on success", async () => {
        vi.mocked(api.bindCredentialReference).mockResolvedValue(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useBindCredentialMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            opaqueReferenceId: "r1",
            expectedVersion: "3",
            idempotencyKey: "cred_1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.bindCredentialReference).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })

    it("wires bindEndpointReference and invalidates on success", async () => {
        vi.mocked(api.bindEndpointReference).mockResolvedValue(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useBindEndpointMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            opaqueReferenceId: "r2",
            expectedVersion: "3",
            idempotencyKey: "endpoint_1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.bindEndpointReference).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })

    it("wires updateCapabilities and invalidates on success", async () => {
        vi.mocked(api.updateCapabilities).mockResolvedValue(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useUpdateCapabilitiesMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            changes: [{ code: "CATALOG" as const, enabled: true }],
            expectedConnectionVersion: "3",
            expectedCapabilityVersions: { CATALOG: "2" },
            reasonCode: "ADMIN_CONFIG",
            operationId: "op_1",
            idempotencyKey: "cap_1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.updateCapabilities).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })

    it("wires runHealthCheck and invalidates for processing outcomes", async () => {
        const processing: FormalOutcome = {
            status: "processing",
            title: "已创建",
            message: "后台执行中",
            jobId: "j1",
            jobNo: "J-1",
        }
        vi.mocked(api.runHealthCheck).mockResolvedValue(processing)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useRunHealthCheckMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            expectedVersion: "3",
            idempotencyKey: "health_1",
            checkType: "CONNECTIVITY" as const,
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.runHealthCheck).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })

    it("does not invalidate when a health check is rejected", async () => {
        const rejected: FormalOutcome = {
            status: "rejected",
            code: "X",
            title: "被拒绝",
            message: "不满足条件",
        }
        vi.mocked(api.runHealthCheck).mockResolvedValue(rejected)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useRunHealthCheckMutation(),
            { queryClient: client },
        )
        await act(async () => {
            await result.current.mutateAsync({
                connectionId: "c1",
                expectedVersion: "3",
                idempotencyKey: "health_2",
                checkType: "AUTHENTICATION",
            })
        })
        expect(invalidate).not.toHaveBeenCalled()
    })

    it("wires startCatalogSync and invalidates on success; skips invalidation on failure", async () => {
        vi.mocked(api.startCatalogSync).mockResolvedValueOnce(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useStartCatalogSyncMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            expectedVersion: "3",
            idempotencyKey: "catalog_1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.startCatalogSync).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )

        invalidate.mockClear()
        const failed: FormalOutcome = {
            status: "failed",
            code: "X",
            title: "失败",
            message: "未执行",
        }
        vi.mocked(api.startCatalogSync).mockResolvedValueOnce(failed)
        await act(async () => {
            await result.current.mutateAsync({
                connectionId: "c1",
                expectedVersion: "3",
                idempotencyKey: "catalog_2",
            })
        })
        expect(invalidate).not.toHaveBeenCalled()
    })

    it("wires disableConnection and invalidates on success", async () => {
        vi.mocked(api.disableConnection).mockResolvedValue(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useDisableConnectionMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            expectedVersion: "3",
            reasonCode: "ADMIN_DISABLE",
            idempotencyKey: "disable_1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.disableConnection).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })

    it("wires enableConnection and invalidates on success", async () => {
        vi.mocked(api.enableConnection).mockResolvedValue(succeeded)
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(
            () => useEnableConnectionMutation(),
            { queryClient: client },
        )
        const input = {
            connectionId: "c1",
            expectedVersion: "3",
            idempotencyKey: "enable_1",
        }
        await act(async () => {
            await result.current.mutateAsync(input)
        })
        expect(api.enableConnection).toHaveBeenCalledWith(
            input,
            expect.anything(),
        )
        await waitFor(() =>
            expect(invalidate).toHaveBeenCalledWith({
                queryKey: ["supplier-api-connections"],
            }),
        )
    })
})
