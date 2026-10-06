"use client"
import { useState } from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useRouter, useSearchParams } from "next/navigation"
import { z } from "zod"
import { useAppForm } from "@/components/form"
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
        <main className="flex min-h-svh items-center justify-center p-6">
            <section className="w-full max-w-md space-y-6 rounded-xl border bg-card p-8">
                <header>
                    <h1 className="text-2xl font-semibold">供应商登录</h1>
                    <p className="mt-2 text-sm text-muted-foreground">
                        使用内部开通的实名账号维护供应资料。
                    </p>
                </header>
                <PortalError error={error} />
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="account">
                        {(field) => (
                            <field.TextField
                                id="supplier-portal-login-account"
                                label="账号"
                                required
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
                                autoComplete="current-password"
                            />
                        )}
                    </form.AppField>
                    <form.AppForm>
                        <form.SubmitButton
                            id="supplier-portal-login-submit"
                            label="登录"
                            pendingLabel="登录中…"
                            className="w-full"
                        />
                    </form.AppForm>
                </form>
            </section>
        </main>
    )
}
