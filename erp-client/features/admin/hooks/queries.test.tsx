import { act, waitFor } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    createAdmin,
    createRole,
    deleteAdmin,
    deleteRole,
    fetchAssignableRoles,
    fetchRoles,
    updateAdmin,
    updateAdminRole,
    updateRole,
} from "@/features/admin/api/admin"
import type { AdminRole } from "@/features/admin/types"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import {
    useAdminMutations,
    useAssignableRolesQuery,
    useRoleMutations,
    useRolesQuery,
} from "./queries"

vi.mock("@/features/admin/api/admin", () => ({
    fetchRoles: vi.fn(),
    fetchAssignableRoles: vi.fn(),
    createAdmin: vi.fn(),
    updateAdmin: vi.fn(),
    updateAdminRole: vi.fn(),
    deleteAdmin: vi.fn(),
    createRole: vi.fn(),
    updateRole: vi.fn(),
    deleteRole: vi.fn(),
}))

const sampleRoles: AdminRole[] = [
    {
        id: "role-1",
        name: "管理员",
        permissions: ["admin:list"],
        created_at: 1,
    },
    { id: "role-2", name: "运营", permissions: [], created_at: 2 },
]

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useRolesQuery", () => {
    it("fetches the role list under the stable roles query key", async () => {
        vi.mocked(fetchRoles).mockResolvedValue(sampleRoles)
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(() => useRolesQuery(), {
            queryClient: client,
        })

        expect(result.current.isPending).toBe(true)
        expect(result.current.data).toBeUndefined()

        await waitFor(() => expect(result.current.data).toEqual(sampleRoles))
        expect(result.current.isSuccess).toBe(true)
        expect(fetchRoles).toHaveBeenCalledTimes(1)
        // v5 queryFn 接收 QueryFunctionContext；校验其挂载的 queryKey
        const [context] = vi.mocked(fetchRoles).mock.calls[0] as unknown as [
            { queryKey: readonly unknown[] },
        ]
        expect(context.queryKey).toEqual(["admin", "roles"])
        expect(client.getQueryData(["admin", "roles"])).toEqual(sampleRoles)
    })

    it("surfaces failures as error state", async () => {
        vi.mocked(fetchRoles).mockRejectedValue(new Error("网络不可用"))
        const { result } = renderHookWithProviders(() => useRolesQuery())

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useAssignableRolesQuery", () => {
    it("fetches assignable roles under their own query key", async () => {
        vi.mocked(fetchAssignableRoles).mockResolvedValue([sampleRoles[0]])
        const client = createFreshQueryClient()
        const { result } = renderHookWithProviders(
            () => useAssignableRolesQuery(),
            { queryClient: client },
        )

        await waitFor(() =>
            expect(result.current.data).toEqual([sampleRoles[0]]),
        )
        expect(fetchAssignableRoles).toHaveBeenCalledTimes(1)
        expect(fetchRoles).not.toHaveBeenCalled()
        expect(
            client.getQueryData(["admin", "roles", "assignable"]),
        ).toEqual([sampleRoles[0]])
    })
})

describe("useAdminMutations", () => {
    const payload = {
        account: "boss",
        password: "secret123",
        name: "老板",
        role_ids: ["role-1"],
    }

    it("wires every admin write to the api and invalidates admin queries on success", async () => {
        vi.mocked(createAdmin).mockResolvedValue(undefined)
        vi.mocked(updateAdmin).mockResolvedValue(undefined)
        vi.mocked(updateAdminRole).mockResolvedValue(undefined)
        vi.mocked(deleteAdmin).mockResolvedValue(undefined)

        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(() => useAdminMutations(), {
            queryClient: client,
        })

        await act(async () => {
            await result.current.createAdmin(payload)
        })
        expect(createAdmin).toHaveBeenCalledWith(payload)

        await act(async () => {
            await result.current.updateAdmin({
                id: "admin-1",
                payload: { name: "新名字" },
            })
        })
        expect(updateAdmin).toHaveBeenCalledWith("admin-1", { name: "新名字" })

        await act(async () => {
            await result.current.updateAdminRole({
                id: "admin-1",
                role_ids: ["role-2"],
            })
        })
        expect(updateAdminRole).toHaveBeenCalledWith("admin-1", {
            role_ids: ["role-2"],
        })

        await act(async () => {
            await result.current.deleteAdmin("admin-1")
        })
        expect(deleteAdmin).toHaveBeenCalledWith("admin-1")

        expect(invalidate).toHaveBeenCalledTimes(4)
        expect(invalidate).toHaveBeenCalledWith({ queryKey: ["admin"] })
    })

    it("exposes pending state while a mutation is in flight", async () => {
        let resolveCreate!: (value: void) => void
        vi.mocked(createAdmin).mockImplementation(
            () =>
                new Promise<void>((resolve) => {
                    resolveCreate = resolve
                }),
        )
        const { result } = renderHookWithProviders(() => useAdminMutations())

        let promise!: Promise<void>
        act(() => {
            promise = result.current.createAdmin(payload)
        })
        // mutation 状态通知走 setTimeout 调度，用 waitFor 等待落地
        await waitFor(() => expect(result.current.isCreating).toBe(true))
        expect(result.current.isUpdating).toBe(false)
        expect(result.current.isDeleting).toBe(false)

        await act(async () => {
            resolveCreate()
            await promise
        })
        await waitFor(() => expect(result.current.isCreating).toBe(false))
    })

    it("propagates errors without invalidating queries", async () => {
        vi.mocked(deleteAdmin).mockRejectedValue(new Error("forbidden"))
        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(() => useAdminMutations(), {
            queryClient: client,
        })

        let error: unknown
        await act(async () => {
            try {
                await result.current.deleteAdmin("admin-1")
            } catch (e) {
                error = e
            }
        })
        expect(error).toBeInstanceOf(Error)
        expect(invalidate).not.toHaveBeenCalled()
    })
})

describe("useRoleMutations", () => {
    it("wires create/update/delete role to the api and invalidates admin queries", async () => {
        vi.mocked(createRole).mockResolvedValue(undefined)
        vi.mocked(updateRole).mockResolvedValue(undefined)
        vi.mocked(deleteRole).mockResolvedValue(undefined)

        const client = createFreshQueryClient()
        const invalidate = vi.spyOn(client, "invalidateQueries")
        const { result } = renderHookWithProviders(() => useRoleMutations(), {
            queryClient: client,
        })

        await act(async () => {
            await result.current.createRole({
                name: "销售经理",
                permissions: ["order:list"],
            })
        })
        expect(createRole).toHaveBeenCalledWith({
            name: "销售经理",
            permissions: ["order:list"],
        })

        await act(async () => {
            await result.current.updateRole({
                id: "role-1",
                payload: { permissions: [] },
            })
        })
        expect(updateRole).toHaveBeenCalledWith("role-1", {
            permissions: [],
        })

        await act(async () => {
            await result.current.deleteRole("role-1")
        })
        expect(deleteRole).toHaveBeenCalledWith("role-1")

        expect(invalidate).toHaveBeenCalledTimes(3)
        expect(invalidate).toHaveBeenCalledWith({ queryKey: ["admin"] })
    })

    it("exposes isDeleting while the delete mutation is in flight", async () => {
        let resolveDelete!: (value: void) => void
        vi.mocked(deleteRole).mockImplementation(
            () =>
                new Promise<void>((resolve) => {
                    resolveDelete = resolve
                }),
        )
        const { result } = renderHookWithProviders(() => useRoleMutations())

        let promise!: Promise<void>
        act(() => {
            promise = result.current.deleteRole("role-1")
        })
        await waitFor(() => expect(result.current.isDeleting).toBe(true))
        expect(result.current.isCreating).toBe(false)
        expect(result.current.isUpdating).toBe(false)

        await act(async () => {
            resolveDelete()
            await promise
        })
        await waitFor(() => expect(result.current.isDeleting).toBe(false))
    })
})
