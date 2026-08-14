import { describe, it, expect, vi, beforeEach } from "vitest"
import { renderHook } from "@testing-library/react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useCreatePermission } from "./use-create-permission"

vi.mock("@/features/auth/queries", () => ({
    useAccountProfileQuery: vi.fn(),
}))

const mockedAccountQuery = vi.mocked(useAccountProfileQuery)

function accountState(overrides: Record<string, unknown> = {}) {
    return {
        data: undefined,
        isPending: false,
        isError: false,
        error: null,
        ...overrides,
    } as unknown as ReturnType<typeof useAccountProfileQuery>
}

beforeEach(() => {
    mockedAccountQuery.mockReset()
})

describe("useCreatePermission", () => {
    it("grants create when the account holds the permission", () => {
        mockedAccountQuery.mockReturnValue(
            accountState({
                data: {
                    userid: "u1",
                    account: "acct",
                    name: "张三",
                    subject: "s1",
                    role_ids: [],
                    permissions: ["unit_of_measure:create"],
                    account_kind: "staff",
                },
            }),
        )
        const { result } = renderHook(() =>
            useCreatePermission("unit_of_measure:create"),
        )
        expect(result.current.canCreate).toBe(true)
    })

    it("denies create when the permission is missing", () => {
        mockedAccountQuery.mockReturnValue(
            accountState({
                data: {
                    userid: "u1",
                    account: "acct",
                    name: "张三",
                    subject: "s1",
                    role_ids: [],
                    permissions: ["other:view"],
                    account_kind: "staff",
                },
            }),
        )
        const { result } = renderHook(() =>
            useCreatePermission("unit_of_measure:create"),
        )
        expect(result.current.canCreate).toBe(false)
        expect(result.current.createBlockedReason).toBe(
            "当前账号没有新建此类资料的权限。",
        )
    })

    it("shows a pending message while the profile loads", () => {
        mockedAccountQuery.mockReturnValue(
            accountState({ isPending: true }),
        )
        const { result } = renderHook(() =>
            useCreatePermission("unit_of_measure:create"),
        )
        expect(result.current.canCreate).toBe(false)
        expect(result.current.createBlockedReason).toBe(
            "正在核对创建权限，请稍候。",
        )
    })

    it("surfaces the load error message", () => {
        mockedAccountQuery.mockReturnValue(
            accountState({
                isError: true,
                error: new Error("网络中断"),
            }),
        )
        const { result } = renderHook(() =>
            useCreatePermission("unit_of_measure:create"),
        )
        expect(result.current.canCreate).toBe(false)
        expect(result.current.createBlockedReason).toBe("网络中断")
    })

    it("always denies when no permission is required", () => {
        mockedAccountQuery.mockReturnValue(
            accountState({
                data: {
                    userid: "u1",
                    account: "acct",
                    name: "张三",
                    subject: "s1",
                    role_ids: [],
                    permissions: ["*:*"],
                    account_kind: "staff",
                },
            }),
        )
        const { result } = renderHook(() => useCreatePermission(undefined))
        expect(result.current.canCreate).toBe(false)
        expect(result.current.createBlockedReason).toBe(
            "当前账号没有新建此类资料的权限。",
        )
    })

    it("passes the account query through for the caller", () => {
        const state = accountState({ isPending: true })
        mockedAccountQuery.mockReturnValue(state)
        const { result } = renderHook(() =>
            useCreatePermission("unit_of_measure:create"),
        )
        expect(result.current.accountQuery).toBe(state)
    })
})
