"use client"
import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useRouter, useSearchParams } from "next/navigation"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { FieldGroup } from "@/components/ui/field"
import { LoginLayout } from "@/features/auth/components/login-layout"
import { LoginModeSwitch } from "@/features/auth/components/login-mode-switch"
import { portalLogin } from "../api"
import { portalKeys } from "../hooks/queries"
import { PortalError } from "../components/surface"
const schema = z.object({
    account: z.string().trim().min(1, "请输入账号"),
    password: z.string().min(1, "请输入密码"),
})
export function PortalLoginPage() {
    const router = useRouter()
    const search = useSearchParams()
    const client = useQueryClient()
    const mutation = useMutation({ mutationFn: portalLogin, retry: false })
    const [error, setError] = useState<unknown>(null)
    const form = useAppForm({
        defaultValues: { account: "", password: "" },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            setError(null)
            try {
                await mutation.mutateAsync(value)
                void client.cancelQueries({ queryKey: portalKeys.all })
                client.removeQueries({ queryKey: portalKeys.all })
                const target = search.get("returnTo")
                router.replace(
                    target?.startsWith("/supplier-portal/") &&
                        !target.startsWith("/supplier-portal/login")
                        ? target
                        : "/supplier-portal/offerings",
                )
            } catch (cause) {
                setError(cause)
            }
        },
    })
    return (
        <LoginLayout mode="supplier">
            <Card className="w-full max-w-md border-0 bg-transparent shadow-none">
                <CardHeader className="gap-4">
                    <LoginModeSwitch mode="supplier" />
                    <div className="flex flex-col gap-1.5">
                        <CardTitle className="text-login-title">
                            供应商登录
                        </CardTitle>
                        <CardDescription>
                            使用采购方开通的实名账号维护供应资料
                        </CardDescription>
                    </div>
                </CardHeader>
                <CardContent>
                    <form
                        className="flex flex-col gap-6"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <PortalError error={error} />
                        <FieldGroup className="gap-4">
                            <form.AppField name="account">
                                {(field) => (
                                    <field.TextField
                                        id="supplier-portal-login-account"
                                        label="账号"
                                        required
                                        placeholder="请输入账号"
                                        autoComplete="username"
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="password">
                                {(field) => (
                                    <field.TextField
                                        id="supplier-portal-login-password"
                                        label="密码"
                                        type="password"
                                        required
                                        placeholder="请输入密码"
                                        autoComplete="current-password"
                                    />
                                )}
                            </form.AppField>
                        </FieldGroup>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-portal-login-submit"
                                label="登录"
                                pendingLabel="登录中…"
                                size="lg"
                                className="w-full"
                            />
                        </form.AppForm>
                    </form>
                </CardContent>
            </Card>
        </LoginLayout>
    )
}
