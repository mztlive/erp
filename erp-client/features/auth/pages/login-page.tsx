"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { useLoginMutation } from "@/features/auth/queries"
import type { ApiError } from "@/lib/api/errors"

const loginSchema = z.object({
    account: z.string().min(3, "请输入账号").max(32, "账号过长"),
    password: z.string().min(6, "密码至少 6 位").max(32, "密码过长"),
})

function loginErrorMessage(error: unknown): string {
    const err = error as Partial<ApiError> | undefined
    if (err?.kind === "Auth") return "账号或密码不正确，请重试"
    if (err?.kind === "Validation") return err.message || "登录信息未通过校验"
    if (err?.kind === "Network") return "无法连接服务器，请确认后端已启动"
    if (err?.message) return err.message
    return "登录失败，请稍后重试"
}

/**
 * 后台登录页（公开路由）。
 */
export function LoginPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const loginMutation = useLoginMutation()
    const [formError, setFormError] = React.useState<string | null>(null)

    const form = useAppForm({
        defaultValues: {
            account: "",
            password: "",
        },
        validators: {
            onChange: loginSchema,
        },
        onSubmit: async ({ value }) => {
            setFormError(null)
            try {
                await loginMutation.mutateAsync({
                    account: value.account,
                    password: value.password,
                    account_kind: "admin",
                })
                const raw = searchParams.get("returnTo")
                const target =
                    raw && raw.startsWith("/") && !raw.startsWith("//")
                        ? raw
                        : "/workspace/tasks"
                router.replace(target)
            } catch (error) {
                setFormError(loginErrorMessage(error))
            }
        },
    })

    return (
        <div className="flex min-h-svh items-center justify-center bg-muted/30 p-4">
            <Card className="w-full max-w-md shadow-sm">
                <CardHeader className="space-y-1">
                    <CardTitle className="text-xl">登录</CardTitle>
                    <CardDescription>
                        员工福利 ERP · 使用后台账号进入工作台
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <form
                        className="flex flex-col gap-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        {formError ? (
                            <Alert variant="destructive">
                                <AlertTitle>无法登录</AlertTitle>
                                <AlertDescription>{formError}</AlertDescription>
                            </Alert>
                        ) : null}

                        <form.AppField
                            name="account"
                            children={(field) => (
                                <field.TextField
                                    label="账号"
                                    placeholder="请输入账号"
                                    autoComplete="username"
                                />
                            )}
                        />
                        <form.AppField
                            name="password"
                            children={(field) => (
                                <field.TextField
                                    label="密码"
                                    type="password"
                                    placeholder="请输入密码"
                                    autoComplete="current-password"
                                />
                            )}
                        />

                        <form.AppForm>
                            <form.SubmitButton
                                label="登录"
                                pendingLabel="登录中…"
                                className="w-full"
                            />
                        </form.AppForm>
                    </form>
                </CardContent>
            </Card>
        </div>
    )
}
