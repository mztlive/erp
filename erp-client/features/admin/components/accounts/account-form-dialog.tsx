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
import { Label } from "@/components/ui/label"
import { RoleOptionsPanel } from "@/features/admin/components/accounts/role-options-panel"
import { useAdminMutations } from "@/features/admin/hooks/queries"

/** 账号表单所需的最小账号信息（编辑模式）。 */
export type AccountDraft = {
    id: string
    account: string
    name: string
    role_ids: string[]
}

/** 新建 / 编辑账号提交值。 */
type AccountFormValues = {
    account: string
    name: string
    password: string
    role_ids: string[]
}

const createSchema = z.object({
    account: z
        .string()
        .trim()
        .min(3, "账号长度必须在3-32个字符之间")
        .max(32, "账号长度必须在3-32个字符之间"),
    name: z.string().trim().min(1, "请输入姓名"),
    password: z
        .string()
        .min(6, "密码长度必须在6-32个字符之间")
        .max(32, "密码长度必须在6-32个字符之间"),
    role_ids: z.array(z.string()).min(1, "至少选择一个角色"),
})

const editSchema = z.object({
    account: z.string(),
    name: z.string().trim().min(1, "请输入姓名"),
    password: z
        .string()
        .max(32, "密码长度不能超过32个字符")
        .refine((value) => value === "" || value.length >= 6, {
            message: "密码长度必须在6-32个字符之间",
        }),
    role_ids: z.array(z.string()).min(1, "至少选择一个角色"),
})

/**
 * 新建 / 编辑账号对话框。
 *
 * @param mode create 时需要账号与密码；edit 时账号只读、密码留空表示不修改。
 * @param account 编辑目标账号；create 传 null。
 * @param roleOptions 可分配角色选项（API 层已回落全部角色）。
 */
export function AccountFormDialog({
    mode,
    account,
    roleOptions,
    onOpenChange,
}: {
    mode: "create" | "edit"
    account: AccountDraft | null
    roleOptions: readonly { id: string; name: string }[]
    onOpenChange: (open: boolean) => void
}) {
    const { createAdmin, updateAdmin, isCreating, isUpdating } =
        useAdminMutations()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const isEdit = mode === "edit"
    const pending = isCreating || isUpdating

    const form = useAppForm({
        defaultValues: {
            account: isEdit ? (account?.account ?? "") : "",
            name: account?.name ?? "",
            password: "",
            role_ids: account?.role_ids ?? [],
        } satisfies AccountFormValues,
        validators: {
            onChange: isEdit ? editSchema : createSchema,
        },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            try {
                if (isEdit && account) {
                    await updateAdmin({
                        id: account.id,
                        payload: {
                            name: value.name.trim(),
                            role_ids: value.role_ids,
                            password: value.password || undefined,
                        },
                    })
                } else {
                    await createAdmin({
                        account: value.account.trim(),
                        password: value.password,
                        name: value.name.trim(),
                        role_ids: value.role_ids,
                    })
                }
                onOpenChange(false)
            } catch (error) {
                setSubmitError(getErrorMessage(error, "操作失败，请重试。"))
            }
        },
    })

    return (
        <Dialog open onOpenChange={(open) => !open && onOpenChange(false)}>
            <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>
                        {isEdit ? "编辑账号" : "新建账号"}
                    </DialogTitle>
                    <DialogDescription>
                        {isEdit
                            ? "修改姓名、角色或密码；密码留空表示不修改。"
                            : "创建后台管理员账号并绑定角色；账号创建后不可修改。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex flex-col gap-3"
                    onSubmit={(e) => {
                        e.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {isEdit ? (
                        <div className="space-y-1.5">
                            <Label>账号</Label>
                            <div className="rounded-lg border bg-muted/40 px-3 py-2 text-sm">
                                {account?.account}
                            </div>
                        </div>
                    ) : (
                        <form.AppField
                            name="account"
                            children={(field) => (
                                <field.TextField
                                    label="账号"
                                    placeholder="登录账号，3-32 个字符"
                                    autoComplete="off"
                                />
                            )}
                        />
                    )}
                    <form.AppField
                        name="name"
                        children={(field) => (
                            <field.TextField
                                label="姓名"
                                placeholder="管理员姓名"
                            />
                        )}
                    />
                    <form.AppField
                        name="password"
                        children={(field) => (
                            <field.TextField
                                label={isEdit ? "新密码" : "密码"}
                                type="password"
                                placeholder={
                                    isEdit ? "留空则不修改" : "6-32 个字符"
                                }
                                autoComplete="new-password"
                            />
                        )}
                    />
                    <form.AppField
                        name="role_ids"
                        mode="array"
                        children={(field) => {
                            const selected = field.state.value ?? []
                            const isInvalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            const errors = toFieldErrors(
                                field.state.meta.errors,
                            )
                            return (
                                <Field data-invalid={isInvalid || undefined}>
                                    <FieldLabel>角色</FieldLabel>
                                    <RoleOptionsPanel
                                        options={roleOptions}
                                        selected={selected}
                                        invalid={isInvalid}
                                        onToggle={(id, checked) => {
                                            const next = checked
                                                ? [...selected, id]
                                                : selected.filter(
                                                      (value) => value !== id,
                                                  )
                                            field.handleChange(next)
                                            form.validateField(
                                                "role_ids",
                                                "change",
                                            )
                                        }}
                                    />
                                    {isInvalid ? (
                                        <FieldError errors={errors} />
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
                            type="button"
                            variant="outline"
                            disabled={pending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                label={isEdit ? "保存" : "创建"}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
