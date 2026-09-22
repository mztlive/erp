"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { ArrowLeftIcon, PencilIcon, ShieldAlertIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { listWorkspaceStyles } from "@/components/business/list-workspace"
import { toFieldErrors, useAppForm } from "@/components/form"
import { getErrorMessage } from "@/lib/api/errors"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldError } from "@/components/ui/field"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { SubjectScopesPanel } from "@/features/organization/components/subject-scopes-panel"
import { PermissionOptionsPanel } from "@/features/admin/components/roles/permission-panel"
import {
    CopyRolePermissions,
    RolePermissionReview,
    type PermissionReviewMode,
} from "@/features/admin/components/roles/role-permission-dialogs"
import {
    useAdminsQuery,
    useRoleMutations,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import { PERMISSION_BY_CODE } from "@/features/admin/lib/permission-catalog"
import {
    diffPermissions,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"
import type { AdminRole } from "@/features/admin/types"

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
            <PageScaffold
                density="compact"
                className={listWorkspaceStyles.page}
            >
                <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                <div className="h-16 animate-pulse rounded-lg bg-muted" />
                <div className="h-96 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (rolesQuery.isError) {
        return (
            <PageScaffold
                density="compact"
                className={listWorkspaceStyles.page}
            >
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
            <PageScaffold
                density="compact"
                className={listWorkspaceStyles.page}
            >
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

/** 表单只管理角色名称和原始权限编码，展示筛选不影响提交内容。 */
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
    const { data: profile } = useAccountProfileQuery()
    const [scopesOpen, setScopesOpen] = React.useState(false)
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const [editingName, setEditingName] = React.useState(role === null)
    const [review, setReview] = React.useState<PermissionReviewMode | null>(
        null,
    )
    const [confirmLeave, setConfirmLeave] = React.useState(false)
    const [view, setView] = React.useState<PermissionView>("all")
    const pending = isCreating || isUpdating
    const { initialSelected, preservedCodes } = React.useMemo(() => {
        const all = [...new Set(role?.permissions ?? [])]
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
                if (role)
                    await updateRole({
                        id: role.id,
                        payload: { name: value.name.trim(), permissions },
                    })
                else await createRole({ name: value.name.trim(), permissions })
                router.push(ROLES_LIST_HREF)
            } catch (error) {
                setSubmitError(getErrorMessage(error, "操作失败，请重试。"))
            }
        },
    })
    const leave = () => {
        const { added, removed } = diffPermissions(
            form.state.values.permissions,
            initialSelected,
        )
        if (
            form.state.values.name.trim() !== (role?.name ?? "") ||
            added.length > 0 ||
            removed.length > 0
        )
            setConfirmLeave(true)
        else router.push(ROLES_LIST_HREF)
    }

    return (
        <PageScaffold
            density="compact"
            className="min-h-0 gap-0 px-4 py-4 md:h-full md:px-6 md:py-5"
        >
            <form
                className="flex min-h-0 flex-1 flex-col"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <header className="flex shrink-0 flex-wrap items-center justify-between gap-3 pb-4">
                    <div className="flex min-w-0 flex-wrap items-center gap-x-3 gap-y-2">
                        <Button
                            id="governance-admin-role-form-back"
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label="返回角色列表"
                            disabled={pending}
                            onClick={leave}
                        >
                            <ArrowLeftIcon className="size-4" />
                        </Button>
                        <form.Subscribe selector={(state) => state.values.name}>
                            {(name) => (
                                <h1 className="text-xl font-semibold tracking-tight">
                                    {role ? name || "未命名角色" : "新建角色"}
                                </h1>
                            )}
                        </form.Subscribe>
                        {role && (
                            <Button
                                id="governance-admin-role-form-rename"
                                type="button"
                                variant="ghost"
                                size="sm"
                                disabled={pending}
                                aria-expanded={editingName}
                                onClick={() => setEditingName(true)}
                            >
                                <PencilIcon
                                    className="size-3.5"
                                    aria-hidden="true"
                                />
                                修改名称
                            </Button>
                        )}
                        <span className="text-xs text-muted-foreground">
                            {role
                                ? `角色权限${boundAccounts === null ? "" : ` · ${boundAccounts} 个关联账号`}`
                                : "设置名称与操作权限"}
                        </span>
                    </div>
                    {role &&
                    hasPermission(profile?.permissions, "data_scope:list") ? (
                        <form.Subscribe selector={(state) => state.values}>
                            {(values) => {
                                const changes = diffPermissions(
                                    values.permissions,
                                    initialSelected,
                                )
                                const dirty =
                                    changes.added.length > 0 ||
                                    changes.removed.length > 0 ||
                                    values.name.trim() !== role.name
                                return (
                                    <div className="flex flex-wrap items-center gap-2">
                                        {dirty ? (
                                            <span className="text-xs text-muted-foreground">
                                                保存角色后可配置数据范围
                                            </span>
                                        ) : null}
                                        <Button
                                            id="role-form-data-scopes"
                                            type="button"
                                            variant="outline"
                                            disabled={dirty || pending}
                                            onClick={() => setScopesOpen(true)}
                                        >
                                            数据范围 · 能看哪些数据
                                        </Button>
                                    </div>
                                )
                            }}
                        </form.Subscribe>
                    ) : null}
                    <form.Subscribe
                        selector={(state) => state.values.permissions}
                    >
                        {(permissions) => (
                            <CopyRolePermissions
                                roles={otherRoles}
                                disabled={hasWildcard || pending}
                                currentCount={permissions.length}
                                onCopy={(codes) => {
                                    form.setFieldValue("permissions", codes)
                                    void form.validateField(
                                        "permissions",
                                        "change",
                                    )
                                }}
                            />
                        )}
                    </form.Subscribe>
                </header>
                {editingName && (
                    <div className="flex shrink-0 items-start gap-2 pb-4">
                        <form.AppField name="name">
                            {(field) => (
                                <field.TextField
                                    id="governance-admin-role-form-name"
                                    label="角色名称"
                                    hideLabel
                                    required
                                    placeholder="请输入角色名称"
                                    className="w-full max-w-xs"
                                    disabled={pending}
                                />
                            )}
                        </form.AppField>
                        {role && (
                            <Button
                                id="governance-admin-role-form-name-done"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={pending}
                                onClick={async () => {
                                    await form.validateField("name", "change")
                                    if (
                                        roleFormSchema.shape.name.safeParse(
                                            form.state.values.name,
                                        ).success
                                    )
                                        setEditingName(false)
                                }}
                            >
                                完成
                            </Button>
                        )}
                    </div>
                )}
                {hasWildcard && (
                    <Alert variant="info" className="mb-3 shrink-0">
                        <ShieldAlertIcon aria-hidden="true" />
                        <AlertTitle>全权角色</AlertTitle>
                        <AlertDescription>
                            该角色拥有全部权限，不按条目配置。如需降权，请新建受限角色并改绑账号。
                        </AlertDescription>
                    </Alert>
                )}
                <form.AppField name="permissions">
                    {(field) => (
                        <Field
                            className="min-h-0 flex-1 gap-0"
                            data-invalid={
                                !field.state.meta.isValid || undefined
                            }
                        >
                            <PermissionOptionsPanel
                                id="governance-admin-role-form-permissions"
                                selected={
                                    hasWildcard
                                        ? [...PERMISSION_BY_CODE.keys()]
                                        : field.state.value
                                }
                                initial={
                                    hasWildcard
                                        ? [...PERMISSION_BY_CODE.keys()]
                                        : initialSelected
                                }
                                preservedCodes={preservedCodes}
                                view={view}
                                onViewChange={setView}
                                disabled={pending || hasWildcard}
                                onChange={(next) => {
                                    field.handleChange(next)
                                    void form.validateField(
                                        "permissions",
                                        "change",
                                    )
                                }}
                            />
                            {!field.state.meta.isValid && (
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            )}
                        </Field>
                    )}
                </form.AppField>
                {submitError && (
                    <Alert
                        variant="destructive"
                        role="alert"
                        className="my-2 shrink-0"
                    >
                        <AlertTitle>保存失败</AlertTitle>
                        <AlertDescription>{submitError}</AlertDescription>
                    </Alert>
                )}
                <form.Subscribe selector={(state) => state.values}>
                    {(values) => {
                        const { added, removed } = diffPermissions(
                            values.permissions,
                            initialSelected,
                        )
                        const dirty =
                            added.length > 0 ||
                            removed.length > 0 ||
                            values.name.trim() !== (role?.name ?? "")
                        const dangerous = hasWildcard
                            ? 0
                            : values.permissions.filter(
                                  (code) =>
                                      PERMISSION_BY_CODE.get(code)?.dangerous,
                              ).length
                        return (
                            <>
                                <UnsavedRefreshGuard
                                    dirty={dirty && !pending}
                                />
                                <footer className="sticky bottom-0 z-10 flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border bg-card py-3">
                                    <div className="flex min-w-0 flex-wrap items-center gap-x-1 gap-y-1 text-xs text-muted-foreground [&_button]:px-1.5 [&_button]:text-xs">
                                        <Button
                                            id="governance-admin-role-form-review-selected"
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={() =>
                                                setReview("selected")
                                            }
                                        >
                                            {hasWildcard
                                                ? "全部权限"
                                                : `已勾选 ${values.permissions.length} 项`}
                                        </Button>
                                        <Button
                                            id="governance-admin-role-form-review-changes"
                                            type="button"
                                            variant="ghost"
                                            size="sm"
                                            onClick={() => setReview("changes")}
                                        >
                                            {dirty
                                                ? `本次变更 · +${added.length} / −${removed.length}`
                                                : "暂无变更"}
                                        </Button>
                                        {dangerous > 0 && (
                                            <Button
                                                id="governance-admin-role-form-review-dangerous"
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                className="text-destructive"
                                                onClick={() =>
                                                    setReview("dangerous")
                                                }
                                            >
                                                <ShieldAlertIcon
                                                    className="size-3.5"
                                                    aria-hidden="true"
                                                />
                                                高风险 {dangerous} 项
                                            </Button>
                                        )}
                                        {preservedCodes.length > 0 &&
                                            !hasWildcard && (
                                                <Button
                                                    id="governance-admin-role-form-review-preserved"
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    onClick={() =>
                                                        setReview("preserved")
                                                    }
                                                >
                                                    特殊授权{" "}
                                                    {preservedCodes.length} 项
                                                </Button>
                                            )}
                                    </div>
                                    <div className="ml-auto flex items-center gap-2">
                                        <Button
                                            id="governance-admin-role-form-cancel"
                                            type="button"
                                            variant="ghost"
                                            disabled={pending}
                                            onClick={leave}
                                        >
                                            取消
                                        </Button>
                                        <form.AppForm>
                                            <form.SubmitButton
                                                id="governance-admin-role-form-submit"
                                                label={
                                                    role
                                                        ? "保存角色"
                                                        : "创建角色"
                                                }
                                                disabled={
                                                    pending ||
                                                    (role !== null && !dirty)
                                                }
                                            />
                                        </form.AppForm>
                                    </div>
                                </footer>
                                <RolePermissionReview
                                    mode={review}
                                    onClose={() => setReview(null)}
                                    selected={values.permissions}
                                    initial={initialSelected}
                                    preservedCodes={preservedCodes}
                                    name={values.name}
                                    initialName={role?.name ?? ""}
                                />
                            </>
                        )
                    }}
                </form.Subscribe>
            </form>
            {role ? (
                <Dialog open={scopesOpen} onOpenChange={setScopesOpen}>
                    <DialogContent
                        className="max-h-[88vh] overflow-y-auto sm:max-w-2xl"
                        closeButtonId="role-scopes-close"
                    >
                        <DialogHeader>
                            <DialogTitle>{role.name} · 数据范围</DialogTitle>
                            <DialogDescription>
                                为该角色配置业务数据范围，保存后对关联账号生效。
                            </DialogDescription>
                        </DialogHeader>
                        {scopesOpen ? (
                            <SubjectScopesPanel
                                roleId={role.id}
                                roleName={role.name}
                            />
                        ) : null}
                    </DialogContent>
                </Dialog>
            ) : null}
            <Dialog open={confirmLeave} onOpenChange={setConfirmLeave}>
                <DialogContent closeButtonId="governance-admin-role-form-leave-close">
                    <DialogHeader>
                        <DialogTitle>放弃未保存的修改？</DialogTitle>
                        <DialogDescription>
                            角色名称和权限的修改尚未保存。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button
                            id="governance-admin-role-form-continue"
                            type="button"
                            variant="outline"
                            onClick={() => setConfirmLeave(false)}
                        >
                            继续编辑
                        </Button>
                        <Button
                            id="governance-admin-role-form-discard"
                            type="button"
                            onClick={() => router.push(ROLES_LIST_HREF)}
                        >
                            放弃并返回
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </PageScaffold>
    )
}

function UnsavedRefreshGuard({ dirty }: { dirty: boolean }) {
    React.useEffect(() => {
        if (!dirty) return
        const guard = (event: BeforeUnloadEvent) => {
            event.preventDefault()
        }
        window.addEventListener("beforeunload", guard)
        return () => window.removeEventListener("beforeunload", guard)
    }, [dirty])
    return null
}
