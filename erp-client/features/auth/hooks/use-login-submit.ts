"use client"

import { useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"

import { loginErrorMessage } from "@/features/auth/lib/login-errors"
import { resolveReturnTarget } from "@/features/auth/lib/return-to"
import { useLoginMutation } from "./queries"

export type LoginSubmitInput = {
    account: string
    password: string
}

/**
 * 登录提交流程：调用登录 mutation，成功后按 returnTo 参数跳转，
 * 失败时把统一错误转换为用户可读提示。
 */
export function useLoginSubmit() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const loginMutation = useLoginMutation()
    const [formError, setFormError] = useState<string | null>(null)

    const submit = async ({ account, password }: LoginSubmitInput) => {
        setFormError(null)
        try {
            await loginMutation.mutateAsync({
                account,
                password,
                account_kind: "admin",
            })
            router.replace(resolveReturnTarget(searchParams.get("returnTo")))
        } catch (error) {
            setFormError(loginErrorMessage(error))
        }
    }

    return { formError, submit, isPending: loginMutation.isPending }
}
