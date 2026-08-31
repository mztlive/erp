"use client"

import * as React from "react"
import { z } from "zod"

import { toFieldErrors, useAppForm } from "@/components/form"
import { getErrorMessage } from "@/lib/api/errors"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { RoleOptionsPanel } from "@/features/admin/components/accounts/role-options-panel"
import { useAdminMutations } from "@/features/admin/hooks/queries"

/** 待调整角色的账号。 */
export type RoleAssignmentTarget = {
    userId: string
    displayName: string
    accountName: string
    roleIds: readonly string[]
}

const schema = z.object({
    role_ids: z.array(z.string()).min(1, "至少选择一个角色"),
})

/**
 * 调整账号的角色绑定。
 *
 * 只改角色：姓名与密码属于账号资料，在「账号管理」维护，
 * 权限工作面不承载登录凭据类操作。
 */
export function RoleAssignmentDialog({
    target,
    roleOptions,
    onOpenChange,
}: {
    target: RoleAssignmentTarget
    roleOptions: readonly { id: string; name: string }[]
    onOpenChange: (open: boolean) => void
}) {
    const { updateAdminRole } = useAdminMutations()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [pending, setPending] = React.useState(false)

    const form = useAppForm({
        defaultValues: { role_ids: [...target.roleIds] },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            setPending(true)
            try {
                await updateAdminRole({
                    id: target.userId,
                    role_ids: value.role_ids,
                })
                onOpenChange(false)
            } catch (error) {
                setSubmitError(getErrorMessage(error, "操作失败，请重试。"))
            } finally {
                setPending(false)
            }
        },
    })

    return (
        <Dialog open onOpenChange={(open) => !open && onOpenChange(false)}>
            <DialogContent
                className="max-h-[88vh] overflow-y-auto sm:max-w-md"
                closeButtonId="operations-access-role-assignment-close"
            >
                <DialogHeader>
                    <DialogTitle>调整角色</DialogTitle>
                    <DialogDescription>
                        {target.displayName}（{target.accountName}
                        ）的角色决定其可访问的页面与动作，保存后立即生效。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-4"
                    onSubmit={(e) => {
                        e.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="role_ids"
                        children={(field) => {
                            const selected = field.state.value ?? []
                            const isInvalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <Field data-invalid={isInvalid || undefined}>
                                    <FieldLabel>
                                        角色
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <RoleOptionsPanel
                                        options={roleOptions}
                                        selected={selected}
                                        invalid={isInvalid}
                                        onToggle={(id, checked) => {
                                            field.handleChange(
                                                checked
                                                    ? [...selected, id]
                                                    : selected.filter(
                                                          (value) =>
                                                              value !== id,
                                                      ),
                                            )
                                            form.validateField(
                                                "role_ids",
                                                "change",
                                            )
                                        }}
                                    />
                                    {isInvalid ? (
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    ) : null}
                                </Field>
                            )
                        }}
                    />
                    {submitError ? (
                        <Alert variant="destructive" role="alert">
                            <AlertTitle>提交失败</AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="operations-access-role-assignment-cancel"
                            type="button"
                            variant="outline"
                            disabled={pending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="operations-access-role-assignment-save"
                                label="保存"
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
