"use client"

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
import { useLoginSubmit } from "@/features/auth/hooks/use-login-submit"

const loginSchema = z.object({
    account: z.string().min(3, "请输入账号").max(32, "账号过长"),
    password: z.string().min(6, "密码至少 6 位").max(32, "密码过长"),
})

/**
 * 后台登录表单：提交走 useLoginSubmit（TanStack Query mutation + 跳转）。
 */
export function LoginForm() {
    const { formError, submit } = useLoginSubmit()

    const form = useAppForm({
        defaultValues: {
            account: "",
            password: "",
        },
        validators: {
            onChange: loginSchema,
        },
        onSubmit: async ({ value }) => {
            await submit({ account: value.account, password: value.password })
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
