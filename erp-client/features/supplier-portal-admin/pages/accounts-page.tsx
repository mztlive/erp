"use client"
import { useMemo, useRef, useState } from "react"
import { z } from "zod"
import type { ColumnDef } from "@tanstack/react-table"
import { useAppForm } from "@/components/form"
import { DataTable } from "@/components/business"
import { ListWorkSurface } from "@/components/business/list-workspace"
import { SupplierSearchCombobox } from "@/features/entity-selectors"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    commandFailureDisposition,
    type CommandFailureDisposition,
} from "@/lib/api/command-recovery"
import { PortalCommandConflict } from "../components/command-conflict"
import { PortalError } from "@/features/supplier-portal/components/surface"
import { commandKey } from "@/features/supplier-portal/lib/presentation"
import {
    createPortalAccount,
    updatePortalAccount,
    type PortalAccount,
} from "../api"
import {
    usePortalAccounts,
    usePortalAdminAccess,
    usePortalAdminCommand,
} from "../hooks"
import { PortalAdminFrame } from "../components/admin-frame"
const roles = [
    { value: "maintainer", label: "供给维护员" },
    { value: "read_only", label: "只读人员" },
]
const createSchema = z.object({
    supplierId: z.string().min(1, "请选择启用的供应商"),
    account: z.string().trim().min(3, "账号至少3位").max(32, "账号最多32位"),
    name: z.string().trim().min(1, "请填写实名"),
    password: z.string().min(6, "密码至少6位").max(32, "密码最多32位"),
    role: z.enum(["maintainer", "read_only"]),
})
export function PortalAdminAccountsPage() {
    const access = usePortalAdminAccess()
    const [pagination, setPagination] = useState({ pageIndex: 0, pageSize: 50 })
    const [supplierId, setSupplierId] = useState("")
    const [editing, setEditing] = useState<PortalAccount | null>(null)
    const query = usePortalAccounts(
        {
            supplier_id: supplierId,
            page: pagination.pageIndex + 1,
            page_size: pagination.pageSize,
        },
        access.can("supplier_portal_account:list") && !!supplierId,
    )
    const [error, setError] = useState<unknown>(null)
    const [saved, setSaved] = useState(false)
    const [failure, setFailure] = useState<CommandFailureDisposition | null>(
        null,
    )
    const intent = useRef<{
        body: Record<string, unknown>
        key: string
    } | null>(null)
    const mutation = usePortalAdminCommand(createPortalAccount)
    const form = useAppForm({
        defaultValues: {
            supplierId: "",
            account: "",
            name: "",
            password: "",
            role: "maintainer",
        },
        validators: { onSubmit: createSchema },
        onSubmit: async ({ value }) => {
            if (failure === "conflict") return
            setError(null)
            setSaved(false)
            const body = {
                supplier_id: value.supplierId,
                account: value.account.trim(),
                name: value.name.trim(),
                password: value.password,
                role: value.role,
            }
            if (
                intent.current &&
                JSON.stringify(intent.current.body) !== JSON.stringify(body)
            ) {
                setError(new Error("请先重试原开通操作并核对结果"))
                return
            }
            if (!intent.current) {
                setSupplierId(value.supplierId)
                setPagination((current) => ({ ...current, pageIndex: 0 }))
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
                setSaved(true)
            } catch (cause) {
                const disposition = commandFailureDisposition(cause)
                if (disposition === "rejected") intent.current = null
                setFailure(disposition)
                setError(cause)
            }
        },
    })
    const columns = useMemo<ColumnDef<PortalAccount>[]>(
        () => [
            { accessorKey: "name", header: "实名" },
            { accessorKey: "account", header: "账号" },
            {
                accessorKey: "supplier_name",
                header: "供应商",
                cell: ({ row }) => row.original.supplier_name ?? "已绑定供应商",
            },
            {
                accessorKey: "role",
                header: "门户岗位",
                cell: ({ row }) =>
                    row.original.role === "maintainer"
                        ? "供给维护员"
                        : "只读人员",
            },
            {
                accessorKey: "active",
                header: "状态",
                cell: ({ row }) => (row.original.active ? "启用" : "停用"),
            },
            {
                id: "actions",
                header: "操作",
                enableHiding: false,
                cell: ({ row }) => (
                    <Button
                        id={`supplier-portal-account-edit-${toAutomationIdSegment(row.original.account_id)}`}
                        variant="outline"
                        size="sm"
                        disabled={
                            !access.can("supplier_portal_account:update") ||
                            !!editing ||
                            mutation.isPending ||
                            !!intent.current
                        }
                        onClick={() => setEditing(row.original)}
                    >
                        配置岗位与启停
                    </Button>
                ),
            },
        ],
        [access, editing, mutation.isPending],
    )
    if (
        !access.can("supplier_portal_account:list") &&
        !access.profile.isPending
    )
        return (
            <PortalAdminFrame title="门户账号">
                <p>当前账号没有门户账号查看权限。</p>
            </PortalAdminFrame>
        )
    if (!access.can("supplier:list"))
        return (
            <PortalAdminFrame title="门户账号">
                <p className="text-sm">
                    需供应商列表查看权限才能选择供应商；请联系管理员配置资格，具体审核可从工作台进入。
                </p>
            </PortalAdminFrame>
        )
    return (
        <PortalAdminFrame title="门户账号">
            <ListWorkSurface
                ariaLabel="门户账号列表"
                toolbar={
                    <div className="w-full max-w-lg space-y-2">
                        <label
                            htmlFor="supplier-portal-accounts-filter-supplier"
                            className="text-sm"
                        >
                            查看供应商账号
                        </label>
                        <SupplierSearchCombobox
                            id="supplier-portal-accounts-filter-supplier"
                            value={supplierId || undefined}
                            disabled={
                                !!editing ||
                                mutation.isPending ||
                                !!intent.current
                            }
                            onValueChange={(value) => {
                                setSupplierId(value ?? "")
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                        />
                    </div>
                }
                selectionBar={
                    <span className="text-sm">
                        共 {query.data?.total ?? 0} 个实名账号
                    </span>
                }
                table={
                    <DataTable
                        id="supplier-portal-admin-accounts-table"
                        data={query.data?.items ?? []}
                        columns={columns}
                        getRowId={(row) => row.account_id}
                        pagination={pagination}
                        onPaginationChange={setPagination}
                        rowCount={query.data?.total ?? 0}
                        loading={query.isFetching}
                        errorState={
                            <PortalError
                                error={query.error}
                                retry={() => void query.refetch()}
                                id="supplier-portal-admin-accounts-retry"
                            />
                        }
                        emptyState={
                            <p className="p-6 text-sm">
                                {supplierId
                                    ? "此供应商暂无门户账号。"
                                    : "请选择供应商查看账号。"}
                            </p>
                        }
                    />
                }
            />
            {editing && (
                <PortalAccountEdit
                    key={editing.account_id}
                    account={
                        query.data?.items.find(
                            (item) => item.account_id === editing.account_id,
                        ) ?? editing
                    }
                    onReload={async () => {
                        const result = await query.refetch({
                            throwOnError: true,
                        })
                        const currentAccount = result.data?.items.find(
                            (item) => item.account_id === editing.account_id,
                        )
                        if (!currentAccount) {
                            throw new Error(
                                "此账号已不在当前列表中，请核对绑定或查看资格后继续。",
                            )
                        }
                        return currentAccount
                    }}
                    onClose={() => setEditing(null)}
                />
            )}
            {access.can("supplier_portal_account:create") && (
                <form
                    className="space-y-4 rounded-xl border p-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <h2 className="font-semibold">开通供应商实名账号</h2>
                    <p className="text-sm text-muted-foreground">
                        账号只绑定所选供应商；门户岗位与内部角色分别管理。
                    </p>
                    <PortalError error={error} />
                    {failure === "conflict" && (
                        <PortalCommandConflict
                            idPrefix="supplier-portal-account-create-conflict"
                            onReload={() =>
                                query.refetch({ throwOnError: true })
                            }
                            onConfirmed={() => {
                                intent.current = null
                                setFailure(null)
                                setError(null)
                            }}
                        />
                    )}
                    {failure === "unknown" && (
                        <p className="text-sm">
                            开通结果尚未确认，请保留本次填写并重试原开通操作。
                        </p>
                    )}
                    {saved && <p role="status">账号已开通。</p>}
                    <form.AppField name="supplierId">
                        {(field) => (
                            <div className="space-y-2">
                                <label
                                    htmlFor="supplier-portal-account-supplier"
                                    className="text-sm"
                                >
                                    启用的供应商
                                </label>
                                <SupplierSearchCombobox
                                    id="supplier-portal-account-supplier"
                                    value={field.state.value || undefined}
                                    onValueChange={(value) =>
                                        field.handleChange(value ?? "")
                                    }
                                    disabled={
                                        mutation.isPending ||
                                        !!intent.current ||
                                        !!editing
                                    }
                                    allowClear={false}
                                />
                                <p className="text-xs text-destructive">
                                    {field.state.meta.errors
                                        .map((item) =>
                                            typeof item === "string"
                                                ? item
                                                : item?.message,
                                        )
                                        .join("，")}
                                </p>
                            </div>
                        )}
                    </form.AppField>
                    <div className="grid gap-4 md:grid-cols-2">
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
                                            !!editing
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
                                    options={roles}
                                    allowClear={false}
                                    disabled={
                                        mutation.isPending ||
                                        !!intent.current ||
                                        !!editing
                                    }
                                />
                            )}
                        </form.AppField>
                    </div>
                    <form.AppForm>
                        <form.SubmitButton
                            id="supplier-portal-account-create-save"
                            label={intent.current ? "重试原开通" : "开通账号"}
                            disabled={
                                mutation.isPending ||
                                failure === "conflict" ||
                                !!editing
                            }
                        />
                    </form.AppForm>
                </form>
            )}
        </PortalAdminFrame>
    )
}
function PortalAccountEdit({
    account,
    onReload,
    onClose,
}: {
    account: PortalAccount
    onReload: () => Promise<PortalAccount>
    onClose: () => void
}) {
    const mutation = usePortalAdminCommand((body: Record<string, unknown>) =>
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
    return (
        <form
            className="space-y-4 rounded-xl border p-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h2 className="font-semibold">
                配置 {account.name} 的门户岗位与启停
            </h2>
            <PortalError error={error} />
            {failure === "conflict" && (
                <PortalCommandConflict
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
                <p className="text-sm">
                    配置结果尚未确认，请保留填写并重试原配置。
                </p>
            )}
            <p className="text-sm text-muted-foreground">
                当前读取的岗位：
                {account.role === "maintainer" ? "供给维护员" : "只读人员"}
                ；账号状态：{account.active ? "启用" : "停用"}
                。您的本次填写仍保留。
            </p>
            <div className="grid gap-4 md:grid-cols-2">
                <form.AppField name="role">
                    {(field) => (
                        <field.SelectField
                            id="supplier-portal-account-edit-role"
                            label="门户岗位"
                            options={roles}
                            allowClear={false}
                            disabled={mutation.isPending || !!intent.current}
                        />
                    )}
                </form.AppField>
                <form.AppField name="active">
                    {(field) => (
                        <field.SelectField
                            id="supplier-portal-account-edit-active"
                            label="账号状态"
                            options={[
                                { value: "active", label: "启用" },
                                { value: "inactive", label: "停用" },
                            ]}
                            allowClear={false}
                            disabled={mutation.isPending || !!intent.current}
                        />
                    )}
                </form.AppField>
            </div>
            <p className="text-sm text-muted-foreground">
                停用或岗位变化后，旧会话的后续请求立即重新校验。
            </p>
            <div className="flex gap-2">
                <form.AppForm>
                    <form.SubmitButton
                        id="supplier-portal-account-edit-save"
                        label={intent.current ? "重试原配置" : "保存配置"}
                        disabled={mutation.isPending || failure === "conflict"}
                    />
                </form.AppForm>
                <Button
                    id="supplier-portal-account-edit-cancel"
                    type="button"
                    variant="outline"
                    disabled={mutation.isPending || !!intent.current}
                    onClick={onClose}
                >
                    取消
                </Button>
            </div>
        </form>
    )
}
