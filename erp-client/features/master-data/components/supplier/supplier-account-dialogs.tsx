"use client"

import { useRef, useState } from "react"
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
import {
    commandFailureDisposition,
    type CommandFailureDisposition,
} from "@/lib/api/command-recovery"
import { PortalAdminCommandConflict } from "@/features/supplier-portal/components/admin-command-conflict"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { commandKey } from "@/features/supplier-portal/lib/presentation"
import {
    createPortalAccount,
    updatePortalAccount,
    type PortalAccount,
} from "@/features/master-data/api/supplier-portal-accounts"
import { usePortalAccountCommand } from "@/features/master-data/hooks/use-supplier-portal-accounts"
const roles = [
    { value: "maintainer", label: "供给维护员" },
    { value: "read_only", label: "只读人员" },
]
const createSchema = z.object({
    account: z.string().trim().min(3, "账号至少3位").max(32, "账号最多32位"),
    name: z.string().trim().min(1, "请填写实名"),
    password: z.string().min(6, "密码至少6位").max(32, "密码最多32位"),
    role: z.enum(["maintainer", "read_only"]),
})

export function SupplierAccountCreateDialog({
    supplierId,
    supplierEnabled,
    onReload,
    onSaved,
    onClose,
}: {
    supplierId: string
    supplierEnabled: boolean
    onReload: () => Promise<unknown>
    onSaved: () => void
    onClose: () => void
}) {
    const [error, setError] = useState<unknown>(null)
    const [failure, setFailure] = useState<CommandFailureDisposition | null>(
        null,
    )
    const intent = useRef<{
        body: Record<string, unknown>
        key: string
    } | null>(null)
    const mutation = usePortalAccountCommand(createPortalAccount)
    const form = useAppForm({
        defaultValues: {
            account: "",
            name: "",
            password: "",
            role: "maintainer",
        },
        validators: { onSubmit: createSchema },
        onSubmit: async ({ value }) => {
            if (failure === "conflict") return
            if (!supplierEnabled && !intent.current) return
            setError(null)
            const body = {
                supplier_id: supplierId,
                account: value.account.trim(),
                name: value.name.trim(),
                password: value.password,
                role: value.role,
            }
            if (
                intent.current &&
                JSON.stringify(intent.current.body) !== JSON.stringify(body)
            ) {
                setError(new Error("请先点击「重试提交」并确认结果"))
                return
            }
            if (!intent.current) {
                intent.current = { body, key: commandKey("account-create") }
            }
            try {
                await mutation.mutateAsync({
                    ...intent.current.body,
                    idempotency_key: intent.current.key,
                })
                intent.current = null
                setFailure(null)
                form.reset()
                onSaved()
            } catch (cause) {
                const disposition = commandFailureDisposition(cause)
                if (disposition === "rejected") intent.current = null
                setFailure(disposition)
                setError(cause)
            }
        },
    })
    const locked = mutation.isPending || !!intent.current
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !locked) onClose()
            }}
        >
            <DialogContent
                closeButtonId="supplier-portal-account-create-close"
                showCloseButton={!locked}
                className="max-h-[85svh] overflow-y-auto sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>开通供应商账号</DialogTitle>
                    <DialogDescription>
                        账号绑定当前供应商。开通后，将登录账号与初始密码交给对应人员。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <PortalError error={error} />
                    {failure === "conflict" && (
                        <PortalAdminCommandConflict
                            idPrefix="supplier-portal-account-create-conflict"
                            onReload={() => onReload()}
                            onConfirmed={() => {
                                intent.current = null
                                setFailure(null)
                                setError(null)
                            }}
                        />
                    )}
                    {failure === "unknown" && (
                        <p className="text-body-compact">
                            开通结果尚未确认，请保留本次填写并点击「重试提交」。
                        </p>
                    )}
                    <div className="grid gap-4 sm:grid-cols-2">
                        {(
                            [
                                ["account", "登录账号"],
                                ["name", "真实姓名"],
                                ["password", "初始密码"],
                            ] as const
                        ).map(([name, label]) => (
                            <form.AppField key={name} name={name}>
                                {(field) => (
                                    <field.TextField
                                        id={`supplier-portal-account-create-${name}`}
                                        label={label}
                                        required
                                        type={
                                            name === "password"
                                                ? "password"
                                                : "text"
                                        }
                                        disabled={
                                            mutation.isPending ||
                                            !!intent.current ||
                                            !supplierEnabled
                                        }
                                    />
                                )}
                            </form.AppField>
                        ))}
                        <form.AppField name="role">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-portal-account-create-role"
                                    label="门户岗位"
                                    required
                                    options={roles}
                                    allowClear={false}
                                    disabled={
                                        mutation.isPending ||
                                        !!intent.current ||
                                        !supplierEnabled
                                    }
                                />
                            )}
                        </form.AppField>
                    </div>
                    <DialogFooter>
                        <Button
                            id="supplier-portal-account-create-cancel"
                            type="button"
                            variant="outline"
                            disabled={locked}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-portal-account-create-save"
                                label={intent.current ? "重试提交" : "开通账号"}
                                disabled={
                                    mutation.isPending ||
                                    failure === "conflict" ||
                                    (!supplierEnabled && !intent.current)
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

export function SupplierAccountEditDialog({
    account,
    onReload,
    onClose,
}: {
    account: PortalAccount
    onReload: () => Promise<PortalAccount>
    onClose: () => void
}) {
    const mutation = usePortalAccountCommand((body: Record<string, unknown>) =>
        updatePortalAccount(account.account_id, body),
    )
    const [error, setError] = useState<unknown>(null)
    const [failure, setFailure] = useState<CommandFailureDisposition | null>(
        null,
    )
    const intent = useRef<{
        body: Record<string, unknown>
        key: string
    } | null>(null)
    const expectedVersions = useRef({
        account: account.account_version,
        binding: account.binding_version,
    })
    const reviewedAccount = useRef<PortalAccount | null>(null)
    const initialValues = useRef({
        role: account.role as string,
        active: account.active ? "active" : "inactive",
    })
    const form = useAppForm({
        defaultValues: initialValues.current,
        onSubmit: async ({ value }) => {
            if (failure === "conflict") return
            setError(null)
            const body = {
                expected_account_version: expectedVersions.current.account,
                expected_binding_version: expectedVersions.current.binding,
                role: value.role,
                active: value.active === "active",
            }
            intent.current ??= { body, key: commandKey("account-update") }
            try {
                await mutation.mutateAsync({
                    ...intent.current.body,
                    idempotency_key: intent.current.key,
                })
                intent.current = null
                setFailure(null)
                onClose()
            } catch (cause) {
                const disposition = commandFailureDisposition(cause)
                if (disposition === "rejected") intent.current = null
                setFailure(disposition)
                setError(cause)
            }
        },
    })
    const locked = mutation.isPending || !!intent.current
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !locked) onClose()
            }}
        >
            <DialogContent
                closeButtonId="supplier-portal-account-edit-close"
                showCloseButton={!locked}
                className="max-h-[85svh] overflow-y-auto sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>配置账号</DialogTitle>
                    <DialogDescription>
                        {account.name} · {account.account}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <PortalError error={error} />
                    {failure === "conflict" && (
                        <PortalAdminCommandConflict
                            idPrefix="supplier-portal-account-edit-conflict"
                            onReload={async () => {
                                reviewedAccount.current = null
                                reviewedAccount.current = await onReload()
                            }}
                            onConfirmed={() => {
                                const currentAccount = reviewedAccount.current
                                if (!currentAccount) return
                                expectedVersions.current = {
                                    account: currentAccount.account_version,
                                    binding: currentAccount.binding_version,
                                }
                                intent.current = null
                                setFailure(null)
                                setError(null)
                            }}
                        />
                    )}
                    {failure === "unknown" && (
                        <p className="text-body-compact">
                            配置结果尚未确认，请保留填写并点击「重试提交」。
                        </p>
                    )}
                    {failure === "conflict" && (
                        <p className="text-xs text-muted-foreground">
                            当前读取的岗位：
                            {account.role === "maintainer"
                                ? "供给维护员"
                                : "只读人员"}
                            ；账号状态：{account.active ? "启用" : "停用"}
                            。您的本次填写仍保留。
                        </p>
                    )}
                    <div className="grid gap-4 md:grid-cols-2">
                        <form.AppField name="role">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-portal-account-edit-role"
                                    label="门户岗位"
                                    required
                                    options={roles}
                                    allowClear={false}
                                    disabled={
                                        mutation.isPending || !!intent.current
                                    }
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="active">
                            {(field) => (
                                <field.SelectField
                                    id="supplier-portal-account-edit-active"
                                    label="账号状态"
                                    required
                                    options={[
                                        { value: "active", label: "启用" },
                                        { value: "inactive", label: "停用" },
                                    ]}
                                    allowClear={false}
                                    disabled={
                                        mutation.isPending || !!intent.current
                                    }
                                />
                            )}
                        </form.AppField>
                    </div>
                    <p className="text-xs text-muted-foreground">
                        停用后，该人员将无法继续访问供应商门户。
                    </p>
                    <DialogFooter>
                        <Button
                            id="supplier-portal-account-edit-cancel"
                            type="button"
                            variant="outline"
                            disabled={locked}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-portal-account-edit-save"
                                label={intent.current ? "重试提交" : "保存配置"}
                                disabled={
                                    mutation.isPending || failure === "conflict"
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
