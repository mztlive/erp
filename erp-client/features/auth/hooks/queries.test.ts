import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"
import { fetchAccountProfile, login } from "@/features/auth/api"
import { useAccountProfileQuery, useLoginMutation } from "./queries"

vi.mock("@/features/auth/api", () => ({
    fetchAccountProfile: vi.fn(),
    login: vi.fn(),
}))

const mockedFetchAccountProfile = vi.mocked(fetchAccountProfile)
const mockedLogin = vi.mocked(login)

// 与 lib/api/session 的 token 存储键一致，避免 mock 掩盖真实会话行为。
const TOKEN_STORAGE_KEY = "erp.token"

const profileFixture = {
    userid: "u1",
    account: "admin01",
    name: "管理员",
    subject: "subj",
    role_ids: ["r1"],
    permissions: ["*:*"],
    account_kind: "admin" as const,
}

beforeEach(() => {
    localStorage.clear()
    vi.clearAllMocks()
})

describe("useAccountProfileQuery", () => {
    it("caches under the account/profile/current key and fetches once when authenticated", async () => {
        localStorage.setItem(TOKEN_STORAGE_KEY, "jwt-token")
        mockedFetchAccountProfile.mockResolvedValue(profileFixture)
        const queryClient = createFreshQueryClient()

        const { result, rerender } = renderHookWithProviders(
            () => useAccountProfileQuery(),
            { queryClient },
        )

        await waitFor(() => expect(result.current.data).toEqual(profileFixture))
        // 通过缓存读取验证 queryKey 结构稳定，重渲染不会重建查询。
        expect(
            queryClient.getQueryData(["account", "profile", "current"]),
        ).toEqual(profileFixture)
        expect(mockedFetchAccountProfile).toHaveBeenCalledTimes(1)

        rerender()
        await waitFor(() => expect(result.current.data).toEqual(profileFixture))
        expect(mockedFetchAccountProfile).toHaveBeenCalledTimes(1)
    })

    it("stays disabled and never fetches when there is no token", () => {
        mockedFetchAccountProfile.mockResolvedValue(profileFixture)

        const { result } = renderHookWithProviders(() =>
            useAccountProfileQuery(),
        )

        expect(result.current.fetchStatus).toBe("idle")
        expect(result.current.data).toBeUndefined()
        expect(mockedFetchAccountProfile).not.toHaveBeenCalled()
    })

    it("surfaces query errors when the profile request fails", async () => {
        localStorage.setItem(TOKEN_STORAGE_KEY, "jwt-token")
        mockedFetchAccountProfile.mockRejectedValue(new Error("network down"))

        const { result } = renderHookWithProviders(() =>
            useAccountProfileQuery(),
        )

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toBeInstanceOf(Error)
    })
})

describe("useLoginMutation", () => {
    it("wires mutationFn to the login api and clears cached data on success", async () => {
        mockedLogin.mockResolvedValue({ token: "new-jwt" })
        const queryClient = createFreshQueryClient()
        queryClient.setQueryData(["account", "profile"], { stale: true })

        const { result } = renderHookWithProviders(() => useLoginMutation(), {
            queryClient,
        })

        expect(result.current.isIdle).toBe(true)

        let pending: Promise<unknown>
        act(() => {
            pending = result.current.mutateAsync({
                account: "admin01",
                password: "secret1",
            })
        })
        await pending!

        expect(mockedLogin).toHaveBeenCalledWith({
            account: "admin01",
            password: "secret1",
        })
        await waitFor(() => expect(result.current.isSuccess).toBe(true))
        // onSuccess 清空旧会话缓存，登录后不再残留上一账号数据。
        expect(queryClient.getQueryData(["account", "profile"])).toBeUndefined()
    })

    it("keeps the error on the mutation state when login fails", async () => {
        mockedLogin.mockRejectedValue({ kind: "Auth", message: "bad" })
        const queryClient = createFreshQueryClient()

        const { result } = renderHookWithProviders(() => useLoginMutation(), {
            queryClient,
        })

        let pending: Promise<unknown>
        act(() => {
            pending = result.current
                .mutateAsync({ account: "admin01", password: "secret1" })
                .catch(() => undefined)
        })
        await pending!

        await waitFor(() => expect(result.current.isError).toBe(true))
        expect(result.current.error).toEqual({ kind: "Auth", message: "bad" })
    })
})
