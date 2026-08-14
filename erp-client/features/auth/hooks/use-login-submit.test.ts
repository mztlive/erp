import { beforeEach, describe, expect, it, vi } from "vitest"
import { act, waitFor } from "@testing-library/react"

import { renderHookWithProviders } from "@/features/test-utils"
import { login } from "@/features/auth/api"
import { useLoginSubmit } from "./use-login-submit"

const mocks = vi.hoisted(() => ({
    replace: vi.fn(),
    searchParams: new URLSearchParams(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({ push: vi.fn(), replace: mocks.replace, back: vi.fn() }),
    useSearchParams: () => mocks.searchParams,
    usePathname: () => "/test",
    useParams: () => ({}),
}))

vi.mock("@/features/auth/api", () => ({
    fetchAccountProfile: vi.fn(),
    login: vi.fn(),
}))

const mockedLogin = vi.mocked(login)

beforeEach(() => {
    vi.clearAllMocks()
    mocks.searchParams = new URLSearchParams()
})

describe("useLoginSubmit", () => {
    it("logs in with account_kind admin and lands on the default workspace page", async () => {
        mockedLogin.mockResolvedValue({ token: "jwt" })

        const { result } = renderHookWithProviders(() => useLoginSubmit())

        expect(result.current.formError).toBeNull()
        expect(result.current.isPending).toBe(false)

        await act(async () => {
            await result.current.submit({
                account: "admin01",
                password: "secret1",
            })
        })

        expect(mockedLogin).toHaveBeenCalledWith({
            account: "admin01",
            password: "secret1",
            account_kind: "admin",
        })
        expect(mocks.replace).toHaveBeenCalledWith("/workspace/tasks")
        expect(result.current.formError).toBeNull()
    })

    it("honors an absolute in-app returnTo param", async () => {
        mockedLogin.mockResolvedValue({ token: "jwt" })
        mocks.searchParams = new URLSearchParams("returnTo=/workspace/mall")

        const { result } = renderHookWithProviders(() => useLoginSubmit())

        await act(async () => {
            await result.current.submit({
                account: "admin01",
                password: "secret1",
            })
        })

        expect(mocks.replace).toHaveBeenCalledWith("/workspace/mall")
    })

    it("rejects protocol-relative returnTo values to prevent open redirects", async () => {
        mockedLogin.mockResolvedValue({ token: "jwt" })
        mocks.searchParams = new URLSearchParams("returnTo=//evil.example")

        const { result } = renderHookWithProviders(() => useLoginSubmit())

        await act(async () => {
            await result.current.submit({
                account: "admin01",
                password: "secret1",
            })
        })

        expect(mocks.replace).toHaveBeenCalledWith("/workspace/tasks")
    })

    it("falls back to the default page for non-absolute returnTo values", async () => {
        mockedLogin.mockResolvedValue({ token: "jwt" })
        mocks.searchParams = new URLSearchParams(
            "returnTo=https://evil.example",
        )

        const { result } = renderHookWithProviders(() => useLoginSubmit())

        await act(async () => {
            await result.current.submit({
                account: "admin01",
                password: "secret1",
            })
        })

        expect(mocks.replace).toHaveBeenCalledWith("/workspace/tasks")
    })

    it("maps an Auth failure to a user-facing error and stays on the page", async () => {
        mockedLogin.mockRejectedValue({
            kind: "Auth",
            message: "unauthorized",
        })

        const { result } = renderHookWithProviders(() => useLoginSubmit())

        await act(async () => {
            await result.current
                .submit({ account: "admin01", password: "wrong!" })
                .catch(() => undefined)
        })

        expect(result.current.formError).toBe("账号或密码不正确，请重试")
        expect(mocks.replace).not.toHaveBeenCalled()
    })

    it("clears a previous error on the next successful attempt", async () => {
        mockedLogin
            .mockRejectedValueOnce({ kind: "Network", message: "down" })
            .mockResolvedValueOnce({ token: "jwt" })

        const { result } = renderHookWithProviders(() => useLoginSubmit())

        await act(async () => {
            await result.current
                .submit({ account: "admin01", password: "secret1" })
                .catch(() => undefined)
        })
        expect(result.current.formError).toBe(
            "无法连接服务器，请确认后端已启动",
        )

        await act(async () => {
            await result.current.submit({
                account: "admin01",
                password: "secret1",
            })
        })

        await waitFor(() => expect(result.current.formError).toBeNull())
        expect(mocks.replace).toHaveBeenCalledWith("/workspace/tasks")
    })
})
