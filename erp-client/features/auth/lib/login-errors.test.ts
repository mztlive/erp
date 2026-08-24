import { describe, expect, it } from "vitest"

import { loginErrorMessage } from "./login-errors"

describe("loginErrorMessage", () => {
    it("maps Auth errors to the retry hint", () => {
        expect(loginErrorMessage({ kind: "Auth", message: "401" })).toBe(
            "账号或密码不正确，请重试",
        )
    })

    it("uses the backend message for Validation errors", () => {
        expect(
            loginErrorMessage({
                kind: "Validation",
                message: "账号格式不正确",
            }),
        ).toBe("账号格式不正确")
    })

    it("falls back to a generic hint when a Validation error has no message", () => {
        expect(loginErrorMessage({ kind: "Validation" })).toBe(
            "登录信息未通过检查，请修改后重试。",
        )
    })

    it("maps Network errors to the connectivity hint", () => {
        expect(loginErrorMessage({ kind: "Network", message: "x" })).toBe(
            "网络连接失败，请检查网络后重试。",
        )
    })

    it("passes through any other structured message", () => {
        expect(
            loginErrorMessage({ kind: "Unknown", message: "系统繁忙" }),
        ).toBe("系统繁忙")
    })

    it("replaces technical Error messages with the login fallback", () => {
        expect(loginErrorMessage(new Error("boom"))).toBe(
            "登录失败，请稍后重试。",
        )
    })

    it("falls back for non-object errors", () => {
        expect(loginErrorMessage("oops")).toBe("登录失败，请稍后重试。")
        expect(loginErrorMessage(undefined)).toBe("登录失败，请稍后重试。")
        expect(loginErrorMessage(null)).toBe("登录失败，请稍后重试。")
    })
})
