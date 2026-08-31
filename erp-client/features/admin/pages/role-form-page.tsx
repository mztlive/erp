"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { ArrowLeftIcon, CopyIcon, ShieldAlertIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    OptionCombobox,
    PageHeader,
    PageScaffold,
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { getErrorMessage } from "@/lib/api/errors"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    Field,
    FieldError,
    FieldGroup,
    FieldLabel,
} from "@/components/ui/field"
import { PermissionOptionsPanel } from "@/features/admin/components/roles/permission-panel"
import {
    useAdminsQuery,
    useRoleMutations,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import {
    PERMISSION_BY_CODE,
    permissionGroupSegment,
    selectedItemsByGroup,
    summarizePermissions,
} from "@/features/admin/lib/permission-catalog"
import type { AdminRole } from "@/features/admin/types"
import { cn } from "@/lib/utils"

type RoleFormValues = {
    name: string
    permissions: string[]
}

const roleFormSchema = z.object({
    name: z
        .string()
        .trim()
        .min(2, "角色名称长度必须在2-32个字符之间")
        .max(32, "角色名称长度必须在2-32个字符之间"),
    permissions: z.array(z.string()),
})

/** 角色返回列表地址（权限与审计 · 角色视图）。 */
const ROLES_LIST_HREF = "/system/access-audit?view=roles"

/** 通配全权编码：不在权限目录内，界面不可勾选，保存时原样保留。 */
const WILDCARD_CODE = "*:*"

/**
 * 角色新建 / 编辑页。
 *
 * 这一层只做数据闸门：角色与账号加载完成后才挂载表单，
 * 表单实例带 key 重建，保证 `defaultValues` 一次到位（表单初始值不会被后到的数据覆盖）。
 *
 * @param roleId 编辑目标角色 ID；null 表示新建。
 */
export function RoleFormPage({ roleId }: { roleId: string | null }) {
    const router = useRouter()
    const rolesQuery = useRolesQuery()
    const adminsQuery = useAdminsQuery()

    const isEdit = roleId !== null
    const role = isEdit
        ? (rolesQuery.data?.find((candidate) => candidate.id === roleId) ??
          null)
        : null

    if (rolesQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (rolesQuery.isError) {
        return (
            <PageScaffold density="compact">
                <PageHeader title={isEdit ? "编辑角色" : "新建角色"} />
                <BusinessFailureState
                    error={rolesQuery.error}
                    title="角色信息加载失败"
                    action={
                        <Button
                            id="governance-admin-role-form-retry"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() => void rolesQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (isEdit && !role) {
        return (
            <PageScaffold density="compact">
                <PageHeader title="编辑角色" />
                <BusinessFailureState
                    kind="system"
                    title="未找到角色"
                    description="该角色不存在或已被删除，可返回角色列表重新选择。"
                    action={
                        <Button
                            id="governance-admin-role-form-back"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() => router.push(ROLES_LIST_HREF)}
                        >
                            返回角色列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const boundAccounts = roleId
        ? (adminsQuery.data?.filter((account) =>
              account.role_ids.includes(roleId),
          ).length ?? null)
        : null

    return (
        <RoleForm
            key={roleId ?? "new"}
            role={role}
            otherRoles={(rolesQuery.data ?? []).filter(
                (candidate) => candidate.id !== roleId,
            )}
            boundAccounts={boundAccounts}
        />
    )
}

/**
 * 角色表单本体。挂载时角色数据已就绪，`defaultValues` 即最终初始值。
 *
 * @param role 编辑目标角色；null 表示新建。
 * @param otherRoles 其它角色，用于「复制权限」。
 * @param boundAccounts 绑定该角色的账号数；未知时为 null。
 */
function RoleForm({
    role,
    otherRoles,
    boundAccounts,
}: {
    role: AdminRole | null
    otherRoles: readonly AdminRole[]
    boundAccounts: number | null
}) {
    const router = useRouter()
    const { createRole, updateRole, isCreating, isUpdating } =
        useRoleMutations()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const isEdit = role !== null
    const pending = isCreating || isUpdating

    /**
     * 目录内可勾选的权限，与目录外权限（含通配全权）分开：
     * 后者界面不展示勾选，保存时原样带回，避免静默丢权限。
     */
    const { initialSelected, preservedCodes } = React.useMemo(() => {
        const all = role?.permissions ?? []
        return {
            initialSelected: all.filter((code) => PERMISSION_BY_CODE.has(code)),
            preservedCodes: all.filter((code) => !PERMISSION_BY_CODE.has(code)),
        }
    }, [role])
    const hasWildcard = preservedCodes.includes(WILDCARD_CODE)

    const form = useAppForm({
        defaultValues: {
            name: role?.name ?? "",
            permissions: initialSelected,
        } satisfies RoleFormValues,
        validators: { onChange: roleFormSchema },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            const permissions = [
                ...new Set([...preservedCodes, ...value.permissions]),
            ]
            try {
                if (role) {
                    await updateRole({
                        id: role.id,
                        payload: { name: value.name.trim(), permissions },
                    })
                } else {
                    await createRole({
                        name: value.name.trim(),
                        permissions,
                    })
                }
                router.push(ROLES_LIST_HREF)
            } catch (error) {
                setSubmitError(getErrorMessage(error, "操作失败，请重试。"))
            }
        },
    })

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={isEdit ? "编辑角色" : "新建角色"}
                description={
                    isEdit
                        ? "调整角色名称与权限；保存后立即对绑定该角色的账号生效。"
                        : "创建角色并勾选权限；权限决定该角色可访问的页面与可执行的动作。"
                }
                metadata={
                    isEdit && boundAccounts !== null ? (
                        <span className="text-xs text-muted-foreground">
                            绑定账号{" "}
                            <span className="num">{boundAccounts}</span> 个
                        </span>
                    ) : null
                }
                actions={
                    <Button
                        id="governance-admin-role-form-back"
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={pending}
                        onClick={() => router.push(ROLES_LIST_HREF)}
                    >
                        <ArrowLeftIcon className="size-4" aria-hidden="true" />
                        返回角色列表
                    </Button>
                }
            />

            <form
                className={cn(
                    surfacePanelClassName,
                    "flex min-w-0 flex-col overflow-hidden",
                )}
                onSubmit={(e) => {
                    e.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <div className="flex flex-col gap-4 p-4 md:p-5">
                    <FieldGroup className="gap-4">
                        <div className="flex flex-wrap items-end gap-3">
                            <form.AppField
                                name="name"
                                children={(field) => (
                                    <field.TextField
                                        id="governance-admin-role-form-name"
                                        label="角色名称"
                                        required
                                        placeholder="如：销售经理"
                                        className="w-full max-w-xs"
                                    />
                                )}
                            />
                            <form.Subscribe
                                selector={(state) => state.values.permissions}
                                children={(permissions) => (
                                    <CopyFromRole
                                        roles={otherRoles}
                                        disabled={hasWildcard}
                                        onCopy={(codes) => {
                                            form.setFieldValue(
                                                "permissions",
                                                codes,
                                            )
                                            form.validateField(
                                                "permissions",
                                                "change",
                                            )
                                        }}
                                        currentCount={permissions.length}
                                    />
                                )}
                            />
                        </div>

                        {hasWildcard ? (
                            <Alert variant="info">
                                <ShieldAlertIcon aria-hidden="true" />
                                <AlertTitle>全权角色</AlertTitle>
                                <AlertDescription>
                                    该角色拥有全部权限，不按条目配置。此处的勾选不会缩小它的权限范围；如需降权，请新建受限角色并改绑账号。
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        <form.AppField
                            name="permissions"
                            children={(field) => {
                                const selected = field.state.value ?? []
                                const isInvalid =
                                    field.state.meta.isTouched &&
                                    !field.state.meta.isValid
                                const errors = toFieldErrors(
                                    field.state.meta.errors,
                                )
                                return (
                                    <Field
                                        data-invalid={isInvalid || undefined}
                                    >
                                        <FieldLabel>权限</FieldLabel>
                                        <PermissionOptionsPanel
                                            id="governance-admin-role-form-permissions"
                                            selected={selected}
                                            onChange={(next) => {
                                                field.handleChange(next)
                                                form.validateField(
                                                    "permissions",
                                                    "change",
                                                )
                                            }}
                                        />
                                        <SelectionReview
                                            selected={selected}
                                            initial={initialSelected}
                                            onRemoveGroup={(codes) => {
                                                const drop = new Set(codes)
                                                field.handleChange(
                                                    selected.filter(
                                                        (code) =>
                                                            !drop.has(code),
                                                    ),
                                                )
                                            }}
                                        />
                                        {preservedCodes.length > 0 &&
                                        !hasWildcard ? (
                                            <p className="text-xs text-muted-foreground">
                                                另有{" "}
                                                <span className="num">
                                                    {preservedCodes.length}
                                                </span>{" "}
                                                项权限不在当前权限目录内，保存时保持不变。
                                            </p>
                                        ) : null}
                                        {isInvalid ? (
                                            <FieldError errors={errors} />
                                        ) : null}
                                    </Field>
                                )
                            }}
                        />
                    </FieldGroup>
                    {submitError ? (
                        <Alert variant="destructive" role="alert">
                            <AlertTitle>提交失败</AlertTitle>
                            <AlertDescription>{submitError}</AlertDescription>
                        </Alert>
                    ) : null}
                </div>

                <form.Subscribe
                    selector={(state) => state.values.permissions}
                    children={(permissions) => (
                        <StickyFormBar
                            selected={permissions}
                            initial={initialSelected}
                            isEdit={isEdit}
                            boundAccounts={boundAccounts}
                            pending={pending}
                            onCancel={() => router.push(ROLES_LIST_HREF)}
                        >
                            <form.AppForm>
                                <form.SubmitButton
                                    id="governance-admin-role-form-submit"
                                    label={isEdit ? "保存" : "创建"}
                                />
                            </form.AppForm>
                        </StickyFormBar>
                    )}
                />
            </form>
        </PageScaffold>
    )
}

/** 从现有角色复制权限：新建角色时不必从 0 勾选。 */
function CopyFromRole({
    roles,
    disabled,
    currentCount,
    onCopy,
}: {
    roles: readonly AdminRole[]
    disabled: boolean
    currentCount: number
    onCopy: (codes: string[]) => void
}) {
    const [sourceId, setSourceId] = React.useState<string | null>(null)
    const source = roles.find((role) => role.id === sourceId) ?? null
    const copyable = source
        ? source.permissions.filter((code) => PERMISSION_BY_CODE.has(code))
        : []

    if (roles.length === 0) return null

    return (
        <div className="flex min-w-0 flex-1 flex-wrap items-end gap-2">
            <div className="flex min-w-0 flex-col gap-1.5">
                <span className="text-sm">复制现有角色的权限</span>
                <OptionCombobox
                    id="governance-admin-role-form-copy-source"
                    className="w-56"
                    value={sourceId}
                    onValueChange={setSourceId}
                    options={roles.map((role) => ({
                        value: role.id,
                        label: role.name,
                    }))}
                    placeholder="选择角色"
                    aria-label="复制权限的来源角色"
                    disabled={disabled}
                />
            </div>
            <Button
                id="governance-admin-role-form-copy"
                type="button"
                variant="outline"
                disabled={disabled || !source || copyable.length === 0}
                onClick={() => onCopy(copyable)}
                title={
                    currentCount > 0 ? "复制会覆盖当前已勾选的权限" : undefined
                }
            >
                <CopyIcon data-icon="inline-start" aria-hidden="true" />
                复制
                {source ? (
                    <span className="num text-muted-foreground">
                        {copyable.length}
                    </span>
                ) : null}
            </Button>
        </div>
    )
}

/** 已选摘要：按权限组归并，保存前可整组复核与移除。 */
function SelectionReview({
    selected,
    initial,
    onRemoveGroup,
}: {
    selected: readonly string[]
    initial: readonly string[]
    onRemoveGroup: (codes: readonly string[]) => void
}) {
    const groups = React.useMemo(
        () => selectedItemsByGroup(selected),
        [selected],
    )
    const { added, removed } = diffCodes(selected, initial)

    if (selected.length === 0) {
        return (
            <p className="text-xs text-muted-foreground">
                未勾选任何权限的角色仅保留基础登录能力，可在创建后再次编辑。
            </p>
        )
    }

    return (
        <Collapsible className={cn(surfaceInsetClassName, "overflow-hidden")}>
            <CollapsibleTrigger
                id="governance-admin-role-form-selection-toggle"
                className="group flex w-full items-center gap-2 px-3 py-2 text-left text-xs hover:bg-muted/40"
            >
                <span className="font-medium text-foreground">已选摘要</span>
                <span className="text-muted-foreground">
                    共 <span className="num">{selected.length}</span> 项 ·{" "}
                    <span className="num">{groups.length}</span> 个模块
                </span>
                {added.length > 0 || removed.length > 0 ? (
                    <span className="text-muted-foreground">
                        本次 {added.length > 0 ? `新增 ${added.length}` : ""}
                        {added.length > 0 && removed.length > 0 ? " · " : ""}
                        {removed.length > 0 ? `移除 ${removed.length}` : ""}
                    </span>
                ) : null}
                <span className="ml-auto text-muted-foreground group-aria-expanded:hidden">
                    展开
                </span>
            </CollapsibleTrigger>
            <CollapsibleContent className="border-t border-grid px-3 py-2">
                <ul className="flex flex-wrap gap-1.5">
                    {groups.map((group) => (
                        <li key={group.name}>
                            <Badge variant="outline" className="gap-1">
                                {group.name}
                                <span className="num">
                                    {group.items.length}
                                </span>
                                <button
                                    id={`governance-admin-role-form-remove-${permissionGroupSegment(group.name)}`}
                                    type="button"
                                    className="text-muted-foreground hover:text-destructive"
                                    aria-label={`移除 ${group.name} 的全部权限`}
                                    onClick={() =>
                                        onRemoveGroup(
                                            group.items.map(
                                                (item) => item.code,
                                            ),
                                        )
                                    }
                                >
                                    ×
                                </button>
                            </Badge>
                        </li>
                    ))}
                </ul>
            </CollapsibleContent>
        </Collapsible>
    )
}

/** 吸底操作条：已选计数、与原配置的差异、影响面与提交按钮。 */
function StickyFormBar({
    selected,
    initial,
    isEdit,
    boundAccounts,
    pending,
    onCancel,
    children,
}: {
    selected: readonly string[]
    initial: readonly string[]
    isEdit: boolean
    boundAccounts: number | null
    pending: boolean
    onCancel: () => void
    children: React.ReactNode
}) {
    const { added, removed } = diffCodes(selected, initial)
    const summary = summarizePermissions(selected)
    const dangerous = selected.filter(
        (code) => PERMISSION_BY_CODE.get(code)?.dangerous,
    ).length

    return (
        <div className="sticky bottom-0 z-10 flex flex-wrap items-center justify-between gap-3 border-t border-grid bg-card/95 px-4 py-3 backdrop-blur md:px-5">
            <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                <span>
                    已选 <span className="num">{selected.length}</span> 项 ·{" "}
                    <span className="num">{summary.groups.length}</span> 个模块
                </span>
                {isEdit && (added.length > 0 || removed.length > 0) ? (
                    <span>
                        {added.length > 0 ? (
                            <span className="text-success">
                                新增 <span className="num">{added.length}</span>
                            </span>
                        ) : null}
                        {added.length > 0 && removed.length > 0 ? " · " : ""}
                        {removed.length > 0 ? (
                            <span className="text-destructive">
                                移除{" "}
                                <span className="num">{removed.length}</span>
                            </span>
                        ) : null}
                    </span>
                ) : null}
                {dangerous > 0 ? (
                    <span className="text-destructive">
                        含高风险权限 <span className="num">{dangerous}</span> 项
                    </span>
                ) : null}
                {isEdit && boundAccounts !== null && boundAccounts > 0 ? (
                    <span>
                        保存后影响 <span className="num">{boundAccounts}</span>{" "}
                        个账号
                    </span>
                ) : null}
            </div>
            <div className="flex items-center gap-2">
                <Button
                    id="governance-admin-role-form-cancel"
                    type="button"
                    variant="ghost"
                    disabled={pending}
                    onClick={onCancel}
                >
                    取消
                </Button>
                {children}
            </div>
        </div>
    )
}

/** 与原权限的差异。 */
function diffCodes(
    selected: readonly string[],
    initial: readonly string[],
): { added: string[]; removed: string[] } {
    const before = new Set(initial)
    const after = new Set(selected)
    return {
        added: selected.filter((code) => !before.has(code)),
        removed: initial.filter((code) => !after.has(code)),
    }
}
