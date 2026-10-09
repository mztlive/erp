"use client"

import * as React from "react"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

const passwordSchema = z.object({
    password: z
        .string()
        .min(8, "访问密码至少 8 位")
        .max(64, "访问密码最多 64 位"),
})

export function AccessPasswordDialog({
    open,
    onOpenChange,
    busy,
    disabledReason,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    busy: boolean
    disabledReason?: string
    onSubmit: (password: string) => Promise<void>
}) {
    const disabled = busy || Boolean(disabledReason)
    const form = useAppForm({
        defaultValues: { password: "" },
        validators: { onChange: passwordSchema },
        onSubmit: async ({ value }) => {
            if (disabled) return
            await onSubmit(passwordSchema.parse(value).password)
            onOpenChange(false)
            form.reset()
        },
    })
    const wasOpen = React.useRef(false)
    React.useEffect(() => {
        if (open && !wasOpen.current) form.reset()
        wasOpen.current = open
    }, [form, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent closeButtonId="sales-selection-access-password-close">
                <DialogHeader>
                    <DialogTitle>设置访问密码</DialogTitle>
                    <DialogDescription>
                        设置 8 到 64
                        位密码。客户需填写密码后访问选品册，修改后已打开的页面也需要重新验证。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="grid gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (disabled) return
                        void form.handleSubmit()
                    }}
                >
                    {disabledReason && (
                        <p
                            id="sales-selection-access-password-disabled-reason"
                            role="status"
                            className="text-sm text-muted-foreground"
                        >
                            {disabledReason}
                        </p>
                    )}
                    <form.AppField
                        name="password"
                        children={(field) => (
                            <field.TextField
                                id="sales-selection-access-password-input"
                                label="访问密码"
                                required
                                type="password"
                                autoComplete="new-password"
                                disabled={disabled}
                                description="请与选品链接分别发送给客户。"
                            />
                        )}
                    />
                    <DialogFooter>
                        <Button
                            id="sales-selection-access-password-cancel"
                            type="button"
                            variant="outline"
                            disabled={busy}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-selection-access-password-submit"
                                label="保存密码"
                                pendingLabel="保存中…"
                                disabled={disabled}
                                loading={busy}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
