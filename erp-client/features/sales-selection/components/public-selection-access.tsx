"use client"

import * as React from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { z } from "zod"
import { LockKeyhole } from "lucide-react"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { unlockPublicSelection } from "../api"
import { salesSelectionKeys } from "../queries"

/** 访问凭证按公开链接保存在当前浏览器会话；不保存访问密码。 */
export function useSelectionAccess(token: string) {
    const [accessToken, setAccessToken] = React.useState("")
    const [loadedToken, setLoadedToken] = React.useState("")
    const ready = loadedToken === token
    const queryClient = useQueryClient()
    const storageKey = `sales-selection-access:${token}`
    React.useEffect(() => {
        try {
            setAccessToken(sessionStorage.getItem(storageKey) ?? "")
        } catch {
            setAccessToken("")
        }
        setLoadedToken(token)
    }, [storageKey, token])
    const unlock = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: (input: { password: string; voucher_code?: string }) =>
            unlockPublicSelection(token, input),
        onSuccess: (result) => {
            queryClient.setQueryData(
                salesSelectionKeys.public(token, result.access_token),
                result.page,
            )
            try {
                sessionStorage.setItem(storageKey, result.access_token)
            } catch {
                /* 当前页面仍保持访问凭证 */
            }
            setAccessToken(result.access_token)
        },
    })
    const resetUnlock = unlock.reset
    const lock = React.useCallback(() => {
        try {
            sessionStorage.removeItem(storageKey)
            sessionStorage.removeItem(
                `sales-selection-pending:${token}:${accessToken}`,
            )
        } catch {
            /* 清除当前页面的授权与数据 */
        }
        void queryClient.cancelQueries({
            queryKey: salesSelectionKeys.public(token, accessToken),
        })
        queryClient.removeQueries({
            queryKey: salesSelectionKeys.public(token, accessToken),
        })
        queryClient.removeQueries({
            queryKey: salesSelectionKeys.public(token, ""),
            exact: true,
        })
        resetUnlock()
        setAccessToken("")
    }, [accessToken, queryClient, resetUnlock, storageKey, token])
    return { accessToken, ready, unlock, lock }
}

export function PublicSelectionAccess({
    voucherRequired,
    access,
}: {
    voucherRequired: boolean
    access: ReturnType<typeof useSelectionAccess>
}) {
    const form = useAppForm({
        defaultValues: { password: "", voucher_code: "" },
        validators: {
            onSubmit: z
                .object({
                    password: z.string().min(1, "请输入访问密码"),
                    voucher_code: z.string(),
                })
                .superRefine((value, context) => {
                    if (voucherRequired && !value.voucher_code.trim())
                        context.addIssue({
                            code: "custom",
                            path: ["voucher_code"],
                            message: "请输入您的提货码",
                        })
                }),
        },
        onSubmit: async ({ value }) => {
            await access.unlock.mutateAsync({
                password: value.password,
                ...(voucherRequired
                    ? { voucher_code: value.voucher_code.trim() }
                    : {}),
            })
        },
    })
    return (
        <main className="flex min-h-svh items-center justify-center bg-muted px-4 py-8">
            <section className="w-full max-w-sm space-y-5 rounded-2xl border bg-card p-6 shadow-sm">
                <div className="space-y-2 text-center">
                    <LockKeyhole
                        className="mx-auto size-10 text-primary"
                        aria-hidden="true"
                    />
                    <h1 className="text-xl font-semibold">
                        请输入选品册访问密码
                    </h1>
                    <p className="text-sm text-muted-foreground">
                        {voucherRequired
                            ? "使用共同访问密码和您个人的提货码进入选品，每个提货码只能提交一次。"
                            : "请向销售获取访问密码后查看商品。"}
                    </p>
                </div>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit().catch(() => undefined)
                    }}
                >
                    <form.AppField
                        name="password"
                        children={(field) => (
                            <field.TextField
                                id="sales-selection-public-password"
                                label="访问密码"
                                type="password"
                                autoComplete="current-password"
                                required
                            />
                        )}
                    />
                    {voucherRequired && (
                        <form.AppField
                            name="voucher_code"
                            children={(field) => (
                                <field.TextField
                                    id="sales-selection-public-voucher-code"
                                    label="个人提货码"
                                    autoComplete="off"
                                    required
                                />
                            )}
                        />
                    )}
                    {access.unlock.isError && (
                        <p
                            id="sales-selection-public-unlock-error"
                            role="alert"
                            className="text-sm text-destructive"
                        >
                            {access.unlock.error instanceof Error
                                ? access.unlock.error.message
                                : "验证失败，请核对密码和提货码。"}
                        </p>
                    )}
                    <Button
                        id="sales-selection-public-unlock"
                        type="submit"
                        className="w-full"
                        disabled={access.unlock.isPending}
                    >
                        {access.unlock.isPending ? "正在验证…" : "进入选品"}
                    </Button>
                </form>
            </section>
        </main>
    )
}
