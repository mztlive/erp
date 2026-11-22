"use client"

import { reportOperationProblem } from "@/lib/system-feedback"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { Checkbox } from "@/components/ui/checkbox"
import { ROLE_WORKBENCHES } from "@/lib/role-workbenches"
import { useRouter } from "@/lib/navigation"
import { PencilIcon, ShieldAlertIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    detailWorkspaceStyles,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldGroup } from "@/components/ui/field"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
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
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { PERMISSION_BY_CODE } from "@/features/admin/lib/permission-catalog"
import {
    diffPermissions,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"
import { permissionChanges } from "../lib/role-workbench"
import type { AdminRole } from "@/features/admin/types"

type RoleFormValues = {
    name: string
    permissions: string[]
    workbenches: string[]
}

const roleFormSchema = z.object({
    name: z
        .string()
        .trim()
        .min(2, "角色名称长度必须在2-32个字符之间")
        .max(32, "角色名称长度必须在2-32个字符之间"),
    permissions: z.array(z.string()),
    workbenches: z.array(z.string()),
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
export function RoleFormPage({
    roleId,
    returnTo,
}: {
    roleId: string | null
    returnTo?: string
}) {
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const isEdit = roleId !== null
    const canRead = hasPermission(permissions, "role:list")
    const canSave = hasPermission(
        permissions,
        isEdit ? "role:update" : "role:create",
    )
    const rolesQuery = useRolesQuery(canRead && canSave)
    const adminsQuery = useAdminsQuery(
        isEdit &&
            canRead &&
            canSave &&
            hasPermission(permissions, "admin:list"),
    )
    const role = isEdit
        ? (rolesQuery.data?.find((candidate) => candidate.id === roleId) ??
          null)
        : null

    const title = isEdit ? "编辑角色" : "新建角色"
    if (profile.isError || (profile.isSuccess && (!canRead || !canSave))) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind={profile.isError ? undefined : "permission"}
                            title={
                                profile.isError
                                    ? undefined
                                    : "暂无此角色操作的权限"
                            }
                            error={profile.error}
                            onRetry={
                                profile.isError
                                    ? () => void profile.refetch()
                                    : undefined
                            }
                            id="governance-admin-role-form-profile-retry"
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (profile.isPending || rolesQuery.isPending) {
        return <DetailWorkspaceSkeleton title={title} />
    }

    if (rolesQuery.isError && !rolesQuery.data) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
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
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (isEdit && !role) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title={title} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
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
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const boundAccounts = roleId
        ? (adminsQuery.data?.filter((account) =>
              account.role_ids.includes(roleId),
          ).length ?? null)
        : null

    return (
        <RoleForm
            onRefreshRoles={() => rolesQuery.refetch({ throwOnError: true })}
            key={roleId ?? "new"}
            role={role}
            returnTo={returnTo}
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
    onRefreshRoles,
    boundAccounts,
    returnTo,
}: {
    role: AdminRole | null
    otherRoles: readonly AdminRole[]
    onRefreshRoles: () => Promise<unknown>
    boundAccounts: number | null
    returnTo?: string
}) {
    const router = useRouter()
    const returnHref =
        returnTo?.startsWith("/system/accounts/") &&
        !returnTo.includes("?") &&
        !returnTo.includes("#")
            ? returnTo
            : ROLES_LIST_HREF
    const { createRole, updateRole, isCreating, isUpdating } =
        useRoleMutations()
    const [savedMessage, setSavedMessage] = React.useState<string | null>(null)
    const [editingName, setEditingName] = React.useState(role === null)
    const [review, setReview] = React.useState<PermissionReviewMode | null>(
        null,
    )
    const [confirmDiscard, setConfirmDiscard] = React.useState<
        "leave" | "reset" | null
    >(null)
    const [view, setView] = React.useState<PermissionView>("all")
    const pending = isCreating || isUpdating
    const [savedValues, setSavedValues] = React.useState<RoleFormValues>(
        () => ({
            name: role?.name ?? "",
            permissions: [...new Set(role?.permissions ?? [])],
            workbenches: role?.workbenches ?? [],
        }),
    )
    const hasWildcard = role?.permissions.includes(WILDCARD_CODE) ?? false
    const form = useAppForm({
        defaultValues: savedValues,
        validators: { onChange: roleFormSchema },
        onSubmit: async ({ value }) => {
            const permissions = [...new Set(value.permissions)]
            try {
                if (role)
                    await updateRole({
                        id: role.id,
                        payload: {
                            name: value.name.trim(),
                            permissions,
                            workbenches: value.workbenches,
                        },
                    })
                else {
                    const createdId = await createRole({
                        name: value.name.trim(),
                        workbenches: value.workbenches,
                        permissions,
                    })
                    router.replace(
                        `/system/roles/${encodeURIComponent(createdId)}/edit`,
                    )
                    return
                }
                const next = { ...value, name: value.name.trim(), permissions }
                setSavedValues(next)
                form.reset(next)
                setSavedMessage(
                    "角色操作权限已保存，对使用此角色的人员生效。每个人的数据范围请到人员资料中设置。",
                )
            } catch (error) {
                reportOperationProblem(error, { title: "角色未保存" })
            }
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const { added, removed } = diffPermissions(
        values.permissions,
        savedValues.permissions,
    )
    const nameChanged = values.name.trim() !== savedValues.name
    const workbenchChangeCount = new Set([
        ...values.workbenches.filter(
            (id) => !savedValues.workbenches.includes(id),
        ),
        ...savedValues.workbenches.filter(
            (id) => !values.workbenches.includes(id),
        ),
    ]).size
    const dirty =
        nameChanged ||
        workbenchChangeCount > 0 ||
        added.length > 0 ||
        removed.length > 0
    const saveDockOpen = role !== null && dirty
    const effectiveChanges = permissionChanges(
        savedValues.permissions,
        values.permissions,
    )
    const permissionChangeCount =
        effectiveChanges.length || (added.length + removed.length > 0 ? 1 : 0)
    const changeCount =
        permissionChangeCount + (nameChanged ? 1 : 0) + workbenchChangeCount
    const currentPreserved = values.permissions.filter(
        (code) => !PERMISSION_BY_CODE.has(code),
    )
    const dangerous = hasWildcard
        ? 0
        : values.permissions.filter(
              (code) => PERMISSION_BY_CODE.get(code)?.dangerous,
          ).length

    React.useEffect(() => {
        if (!role) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key.toLowerCase() === "s"
            ) {
                if (
                    event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    )
                )
                    return
                event.preventDefault()
                if (
                    dirty &&
                    !pending &&
                    !confirmDiscard &&
                    !review &&
                    form.state.canSubmit &&
                    !form.state.isSubmitting
                ) {
                    void form.handleSubmit()
                }
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [role, dirty, pending, confirmDiscard, review, form])

    const isEdit = role !== null
    React.useEffect(() => {
        if (!isEdit || !editingName) return
        const input = document.getElementById("governance-admin-role-form-name")
        input?.focus()
    }, [isEdit, editingName])

    const submit = (
        <form.AppForm>
            <form.SubmitButton
                loading={pending}
                id="governance-admin-role-form-submit"
                label={role ? "保存本次调整" : "创建角色"}
                disabled={pending || (role !== null && !dirty)}
                size="sm"
                className="px-3.5"
            />
        </form.AppForm>
    )

    return (
        <DetailWorkspacePage>
            <form
                className="flex min-w-0 flex-1 flex-col"
                onSubmit={(event) => {
                    event.preventDefault()
                    if (
                        !pending &&
                        !form.state.isSubmitting &&
                        (!role || dirty)
                    )
                        void form.handleSubmit()
                }}
            >
                <UnsavedRefreshGuard dirty={dirty && !pending} />
                <DetailWorkspaceChrome
                    title={role ? values.name || "未命名角色" : "新建角色"}
                    titleExtra={
                        saveDockOpen ? <DetailWorkspaceDirtyBadge /> : null
                    }
                    meta={
                        <DetailWorkspaceMeta
                            items={[
                                role ? "角色权限" : "设置名称与操作权限",
                                role && boundAccounts !== null ? (
                                    <span key="accounts" className="num">
                                        {boundAccounts} 个关联账号
                                    </span>
                                ) : null,
                            ]}
                        />
                    }
                    secondaryActions={
                        <>
                            {role && (
                                <Button
                                    id="governance-admin-role-form-rename"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={pending}
                                    aria-expanded={editingName}
                                    onClick={() => setEditingName(true)}
                                >
                                    <PencilIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    修改名称
                                </Button>
                            )}
                            <CopyRolePermissions
                                onRefreshRoles={onRefreshRoles}
                                roles={otherRoles}
                                disabled={hasWildcard || pending}
                                currentCount={values.permissions.length}
                                onCopy={(codes) => {
                                    form.setFieldValue("permissions", [
                                        ...form.state.values.permissions.filter(
                                            (code) =>
                                                !PERMISSION_BY_CODE.has(code),
                                        ),
                                        ...codes,
                                    ])
                                    void form.validateField(
                                        "permissions",
                                        "change",
                                    )
                                }}
                            />
                            <Button
                                id="governance-admin-role-form-cancel"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                disabled={pending}
                                onClick={() => {
                                    if (dirty) setConfirmDiscard("leave")
                                    else router.push(returnHref)
                                }}
                            >
                                取消
                            </Button>
                        </>
                    }
                    primaryAction={role ? null : submit}
                />
                <DetailWorkspaceBody
                    saveDockOpen={saveDockOpen}
                    alerts={
                        savedMessage ? (
                            <p
                                role="status"
                                className="text-body-compact text-success"
                            >
                                {savedMessage}
                            </p>
                        ) : null
                    }
                >
                    <DetailWorkspaceSection
                        width="form"
                        title="岗位工作台"
                        headingId="governance-admin-role-form-workbenches-heading"
                    >
                        <fieldset
                            aria-labelledby="governance-admin-role-form-workbenches-heading"
                            className="space-y-2"
                        >
                            <p className="text-xs text-muted-foreground">
                                设置该角色登录后默认使用的岗位工作台（仍需勾选下方对应业务权限后才可进入）。
                            </p>
                            <form.Field name="workbenches">
                                {(field) => (
                                    <div className="flex flex-wrap gap-4">
                                        {ROLE_WORKBENCHES.map((item) => (
                                            <label
                                                key={item.id}
                                                className="flex items-center gap-2 text-xs"
                                                htmlFor={`role-workbench-${item.id.toLowerCase()}`}
                                            >
                                                <Checkbox
                                                    id={`role-workbench-${item.id.toLowerCase()}`}
                                                    disabled={pending}
                                                    checked={field.state.value.includes(
                                                        item.id,
                                                    )}
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        field.handleChange(
                                                            checked
                                                                ? [
                                                                      ...field
                                                                          .state
                                                                          .value,
                                                                      item.id,
                                                                  ]
                                                                : field.state.value.filter(
                                                                      (id) =>
                                                                          id !==
                                                                          item.id,
                                                                  ),
                                                        )
                                                    }
                                                />
                                                {item.name}
                                            </label>
                                        ))}
                                    </div>
                                )}
                            </form.Field>
                        </fieldset>
                        {editingName && (
                            <div data-slot="detail-form-grid">
                                <FieldGroup className="flex-row items-start gap-2">
                                    <form.AppField name="name">
                                        {(field) => (
                                            <field.TextField
                                                id="governance-admin-role-form-name"
                                                label="角色名称"
                                                hideLabel
                                                required
                                                placeholder="请输入角色名称"
                                                className="w-full max-w-xs"
                                                inputClassName="scroll-mt-32"
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
                                                await form.validateField(
                                                    "name",
                                                    "change",
                                                )
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
                                </FieldGroup>
                            </div>
                        )}
                    </DetailWorkspaceSection>
                    <DetailWorkspaceSection
                        title="操作权限"
                        actions={
                            <>
                                <Button
                                    id="governance-admin-role-form-review-selected"
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    onClick={() => setReview("selected")}
                                >
                                    {hasWildcard
                                        ? "全部权限"
                                        : `已勾选 ${values.permissions.length} 项`}
                                </Button>
                                <Button
                                    id="governance-admin-role-form-review-changes"
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    onClick={() => setReview("changes")}
                                >
                                    {dirty
                                        ? `有 ${changeCount} 项未保存调整`
                                        : "暂无变更"}
                                </Button>
                                {dangerous > 0 && (
                                    <Button
                                        id="governance-admin-role-form-review-dangerous"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className="text-destructive"
                                        onClick={() => setReview("dangerous")}
                                    >
                                        <ShieldAlertIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        高风险 {dangerous} 项
                                    </Button>
                                )}
                                {currentPreserved.length > 0 &&
                                    !hasWildcard && (
                                        <Button
                                            id="governance-admin-role-form-review-preserved"
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            onClick={() =>
                                                setReview("preserved")
                                            }
                                        >
                                            整模块授权 {currentPreserved.length}{" "}
                                            项
                                        </Button>
                                    )}
                            </>
                        }
                    >
                        <p className="text-xs text-muted-foreground">
                            设置此角色可以执行的操作。每个人可以处理哪些数据，请到人员资料中设置。
                        </p>
                        {hasWildcard && (
                            <Alert variant="info">
                                <ShieldAlertIcon aria-hidden="true" />
                                <AlertTitle>
                                    超级管理员角色（拥有全部权限）
                                </AlertTitle>
                                <AlertDescription>
                                    该角色默认拥有系统全部操作权限，无需逐项勾选。如需限制权限，请另行新建普通角色并分配给对应账号。
                                </AlertDescription>
                            </Alert>
                        )}
                        <form.AppField name="permissions">
                            {(field) => (
                                <Field
                                    className="flex min-h-0 flex-1 flex-col gap-0 md:h-[clamp(24rem,calc(100dvh-20rem),48rem)]"
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
                                                : savedValues.permissions
                                        }
                                        preservedCodes={field.state.value.filter(
                                            (code) =>
                                                !PERMISSION_BY_CODE.has(code),
                                        )}
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
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
                {role && (
                    <DetailWorkspaceSaveDock
                        open={saveDockOpen}
                        changeCount={changeCount}
                        actions={
                            <>
                                <Button
                                    id="governance-admin-role-form-reset"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={pending}
                                    onClick={() => setConfirmDiscard("reset")}
                                >
                                    取消修改
                                </Button>
                                {submit}
                            </>
                        }
                    />
                )}
            </form>
            <RolePermissionReview
                mode={review}
                onClose={() => setReview(null)}
                selected={values.permissions}
                initial={savedValues.permissions}
                preservedCodes={currentPreserved}
                name={values.name}
                initialName={savedValues.name}
            />
            <Dialog
                open={confirmDiscard !== null}
                onOpenChange={(open) => {
                    if (!open) setConfirmDiscard(null)
                }}
            >
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
                            onClick={() => setConfirmDiscard(null)}
                        >
                            继续编辑
                        </Button>
                        <Button
                            id="governance-admin-role-form-discard"
                            type="button"
                            onClick={() => {
                                if (confirmDiscard === "reset") {
                                    form.reset(savedValues)
                                    setEditingName(false)
                                    setConfirmDiscard(null)
                                } else router.push(returnHref)
                            }}
                        >
                            {confirmDiscard === "reset"
                                ? "放弃修改"
                                : "放弃并返回"}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </DetailWorkspacePage>
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
