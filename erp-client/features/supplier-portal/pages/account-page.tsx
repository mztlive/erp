"use client"
import { useState } from "react"
import { useRouter } from "next/navigation"
import { useQueryClient } from "@tanstack/react-query"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { clearToken } from "@/lib/api/session"
import { portalChangePassword } from "../api"
import { portalKeys, usePortalCommand } from "../hooks/queries"
import { usePortalProfile } from "../components/portal-session"
import { PortalError, PortalSurface } from "../components/surface"
const schema = z
    .object({
        oldPassword: z.string().min(6, "请输入原密码"),
        newPassword: z
            .string()
            .min(6, "新密码至少6位")
            .max(32, "新密码最多32位"),
        confirmation: z.string(),
    })
    .refine((value) => value.newPassword === value.confirmation, {
        message: "两次新密码不一致",
        path: ["confirmation"],
    })
export function PortalAccountPage() {
    const profile = usePortalProfile()
    const router = useRouter()
    const client = useQueryClient()
    const mutation = usePortalCommand(portalChangePassword)
    const [error, setError] = useState<unknown>(null)
    const form = useAppForm({
        defaultValues: { oldPassword: "", newPassword: "", confirmation: "" },
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            setError(null)
            try {
                await mutation.mutateAsync({
                    old_password: value.oldPassword,
                    new_password: value.newPassword,
                })
                clearToken("supplier-portal")
                void client.cancelQueries({ queryKey: portalKeys.all })
                client.removeQueries({ queryKey: portalKeys.all })
                router.replace("/supplier-portal/login")
            } catch (cause) {
                setError(cause)
            }
        },
    })
    return (
        <PortalSurface
            title="账号设置"
            description="修改密码后使用新密码重新登录。"
        >
            <div className="rounded-lg border p-5 text-sm">
                <p>姓名：{profile?.name}</p>
                <p>账号：{profile?.account}</p>
                <p>
                    门户岗位：
                    {profile?.role === "maintainer" ? "供给维护员" : "只读人员"}
                </p>
            </div>
            <form
                className="max-w-lg space-y-4 rounded-lg border p-5"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <PortalError error={error} />
                {(
                    [
                        ["oldPassword", "原密码"],
                        ["newPassword", "新密码"],
                        ["confirmation", "再次输入新密码"],
                    ] as const
                ).map(([name, label]) => (
                    <form.AppField key={name} name={name}>
                        {(field) => (
                            <field.TextField
                                id={`supplier-portal-password-${name}`}
                                label={label}
                                type="password"
                                required
                                disabled={mutation.isPending}
                                autoComplete={
                                    name === "oldPassword"
                                        ? "current-password"
                                        : "new-password"
                                }
                            />
                        )}
                    </form.AppField>
                ))}
                <form.AppForm>
                    <form.SubmitButton
                        id="supplier-portal-password-save"
                        label="修改密码并重新登录"
                        disabled={mutation.isPending}
                    />
                </form.AppForm>
            </form>
        </PortalSurface>
    )
}
