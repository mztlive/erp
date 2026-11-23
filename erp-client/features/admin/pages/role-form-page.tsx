"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import {
    LockIcon,
    ShieldAlertIcon,
    ShieldCheckIcon,
    ShieldIcon,
    ShieldPlusIcon,
    Trash2Icon,
} from "lucide-react"
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
import {
    Alert,
    AlertAction,
    AlertDescription,
    AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError } from "@/components/ui/field"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toast } from "@/components/ui/toast"
import { DeleteRoleDialog } from "@/features/admin/components/roles/delete-role-dialog"
import { PermissionOptionsPanel } from "@/features/admin/components/roles/permission-panel"
import { RoleDetailSidebar } from "@/features/admin/components/roles/role-detail-sidebar"
import { RoleMembersPanel } from "@/features/admin/components/roles/role-members-panel"
import {
    CopyRolePermissions,
    RolePermissionReview,
    type PermissionReviewMode,
} from "@/features/admin/components/roles/role-permission-dialogs"
import { RoleWorkbenchPicker } from "@/features/admin/components/roles/role-workbench-picker"
import {
    useAdminsQuery,
    useRoleMutations,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import type { FocusedPermissionGroup } from "@/features/admin/hooks/use-permission-panel"
import { PERMISSION_BY_CODE } from "@/features/admin/lib/permission-catalog"
import {
    diffPermissions,
    type PermissionView,
} from "@/features/admin/lib/permission-editor"
import {
    diffWorkbenches,
    evaluateWorkbenchReadiness,
    summarizeBoundAccounts,
    summarizeRolePermissions,
    WILDCARD_CODE,
} from "@/features/admin/lib/role-detail-view"
import { permissionChanges } from "@/features/admin/lib/role-workbench"
import type { AdminRole } from "@/features/admin/types"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { formatDateTime } from "@/lib/datetime"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { ROLE_WORKBENCHES, roleWorkbench } from "@/lib/role-workbenches"
import { reportOperationProblem } from "@/lib/system-feedback"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"

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

/** 角色返回列表地址（角色与权限 · 角色视图）。 */
const ROLES_LIST_HREF = "/system/access-audit?view=roles"

type RoleDetailTab = "permissions" | "settings" | "members"

/**
 * 角色详情 / 新建页。
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
    const canDelete = hasPermission(permissions, "role:delete")
    const canReadAdmins = hasPermission(permissions, "admin:list")
    const rolesQuery = useRolesQuery(canRead && canSave)
    const adminsQuery = useAdminsQuery(
        isEdit && canRead && canSave && canReadAdmins,
    )
    const role = isEdit
        ? (rolesQuery.data?.find((candidate) => candidate.id === roleId) ??
          null)
        : null

    const title = isEdit ? "角色详情" : "新建角色"
    if (profile.isError || (profile.isSuccess && (!canRead || !canSave))) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title={title}
                    back={{
                        id: "governance-admin-role-form-back",
                        label: "角色与权限",
                        href: ROLES_LIST_HREF,
                    }}
                />
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
        return (
            <DetailWorkspaceSkeleton
                title={title}
                description="正在读取角色配置…"
            />
        )
    }

    if (rolesQuery.isError && !rolesQuery.data) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title={title}
                    back={{
                        id: "governance-admin-role-form-back",
                        label: "角色与权限",
                        href: ROLES_LIST_HREF,
                    }}
                />
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
                <DetailWorkspaceChrome
                    title={title}
                    back={{
                        id: "governance-admin-role-form-back",
                        label: "角色与权限",
                        href: ROLES_LIST_HREF,
                    }}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            kind="system"
                            title="未找到角色"
                            description="该角色不存在或已被删除，可返回角色列表重新选择。"
                            action={
                                <Button
                                    id="governance-admin-role-form-missing-back"
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

    return (
        <RoleForm
            key={roleId ?? "new"}
            role={role}
            otherRoles={(rolesQuery.data ?? []).filter(
                (candidate) => candidate.id !== roleId,
            )}
            onRefreshRoles={() => rolesQuery.refetch({ throwOnError: true })}
            adminsQuery={adminsQuery}
            canReadAdmins={canReadAdmins}
            canDelete={canDelete}
            returnTo={returnTo}
        />
    )
}

/** 表单只管理角色名称、岗位工作台和原始权限编码，展示筛选不影响提交内容。 */
function RoleForm({
    role,
    otherRoles,
    onRefreshRoles,
    adminsQuery,
    canReadAdmins,
    canDelete,
    returnTo,
}: {
    role: AdminRole | null
    otherRoles: readonly AdminRole[]
    onRefreshRoles: () => Promise<unknown>
    adminsQuery: ReturnType<typeof useAdminsQuery>
    canReadAdmins: boolean
    canDelete: boolean
    returnTo?: string
}) {
    const router = useRouter()
    const isAccountReturn =
        Boolean(returnTo?.startsWith("/system/accounts/")) &&
        !returnTo?.includes("?") &&
        !returnTo?.includes("#")
    const returnHref = isAccountReturn ? returnTo! : ROLES_LIST_HREF

    const { createRole, updateRole, isCreating, isUpdating } =
        useRoleMutations()
    const [tab, setTab] = React.useState<RoleDetailTab>("permissions")
    const [review, setReview] = React.useState<PermissionReviewMode | null>(
        null,
    )
    const [confirmDiscard, setConfirmDiscard] = React.useState<
        "leave" | "reset" | null
    >(null)
    const [deleting, setDeleting] = React.useState(false)
    const [view, setView] = React.useState<PermissionView>("all")
    const [focusedGroup, setFocusedGroup] =
        React.useState<FocusedPermissionGroup | null>(null)
    const focusSeqRef = React.useRef(0)

    const pending = isCreating || isUpdating
    const isSystem = Boolean(role?.system)
    const hasWildcard = role?.permissions.includes(WILDCARD_CODE) ?? false
    const isReadOnly = isSystem || hasWildcard

    const [savedValues, setSavedValues] = React.useState<RoleFormValues>(
        () => ({
            name: role?.name ?? "",
            permissions: [...new Set(role?.permissions ?? [])],
            workbenches: role?.workbenches ?? [],
        }),
    )

    const form = useAppForm({
        defaultValues: savedValues,
        validators: { onChange: roleFormSchema },
        onSubmit: async ({ value }) => {
            if (isReadOnly) return
            const permissions = [...new Set(value.permissions)]
            const trimmedName = value.name.trim()
            try {
                if (role) {
                    await updateRole({
                        id: role.id,
                        payload: {
                            name: trimmedName,
                            permissions,
                            workbenches: value.workbenches,
                        },
                    })
                } else {
                    const createdId = await createRole({
                        name: trimmedName,
                        workbenches: value.workbenches,
                        permissions,
                    })
                    toast.add({
                        title: "角色已创建",
                        type: "success",
                        timeout: 3000,
                    })
                    router.replace(
                        `/system/roles/${encodeURIComponent(createdId)}/edit`,
                    )
                    return
                }
                const next = {
                    ...value,
                    name: trimmedName,
                    permissions,
                }
                setSavedValues(next)
                form.reset(next)
                toast.add({
                    title: "角色配置已保存",
                    description:
                        "对使用此角色的人员生效；每个人的数据范围请到人员资料中设置。",
                    type: "success",
                    timeout: 4500,
                })
            } catch (error) {
                reportOperationProblem(error, { title: "角色未保存" })
            }
        },
    })

    const values = useStore(form.store, (state) => state.values)

    const overview = React.useMemo(
        () =>
            summarizeRolePermissions(
                values.permissions,
                savedValues.permissions,
            ),
        [values.permissions, savedValues.permissions],
    )

    const { added, removed } = diffPermissions(
        values.permissions,
        savedValues.permissions,
    )
    const nameValid = roleFormSchema.shape.name.safeParse(values.name).success
    const nameChanged = values.name.trim() !== savedValues.name
    const workbenchDiff = diffWorkbenches(
        values.workbenches,
        savedValues.workbenches,
    )
    const effectiveChanges = permissionChanges(
        savedValues.permissions,
        values.permissions,
    )
    const permissionChangeCount =
        effectiveChanges.length || (added.length + removed.length > 0 ? 1 : 0)
    const settingsChangeCount = (nameChanged ? 1 : 0) + workbenchDiff.count
    const changeCount = permissionChangeCount + settingsChangeCount
    const dirty = !isReadOnly && changeCount > 0
    const saveDockOpen = role !== null && dirty

    useWorkspaceDirtyState({ dirty: dirty && !pending, busy: pending })

    const boundSummary = React.useMemo(
        () =>
            role && canReadAdmins && adminsQuery.data
                ? summarizeBoundAccounts(adminsQuery.data, role.id)
                : null,
        [adminsQuery.data, canReadAdmins, role],
    )

    const missingWorkbenchItems = React.useMemo(
        () =>
            ROLE_WORKBENCHES.filter((item) =>
                values.workbenches.includes(item.id),
            )
                .map((item) => ({
                    item,
                    readiness: evaluateWorkbenchReadiness(
                        item.id,
                        values.permissions,
                    ),
                }))
                .filter((entry) => !entry.readiness.ready),
        [values.permissions, values.workbenches],
    )

    const activeTab: RoleDetailTab =
        tab === "members" && !canReadAdmins ? "permissions" : tab

    const triggerSubmit = React.useCallback(() => {
        if (
            isReadOnly ||
            pending ||
            confirmDiscard !== null ||
            review !== null ||
            deleting ||
            form.state.isSubmitting
        ) {
            return
        }
        if (role && !dirty) return
        const nameValid = roleFormSchema.shape.name.safeParse(
            form.state.values.name,
        ).success
        if (!nameValid && role) {
            setTab("settings")
            window.requestAnimationFrame(() => {
                document
                    .getElementById("governance-admin-role-form-name")
                    ?.focus()
            })
        }
        void form.handleSubmit()
    }, [
        confirmDiscard,
        deleting,
        dirty,
        form,
        isReadOnly,
        pending,
        review,
        role,
    ])

    const handleLeave = () => {
        if (dirty) setConfirmDiscard("leave")
        else router.push(returnHref)
    }

    const handleGrantPermissions = (codes: readonly string[]) => {
        if (isReadOnly || pending) return
        const grantable = codes.filter((code) => PERMISSION_BY_CODE.has(code))
        if (grantable.length === 0) return
        form.setFieldValue("permissions", [
            ...new Set([...form.state.values.permissions, ...grantable]),
        ])
        void form.validateField("permissions", "change")
    }

    const handleJumpToModule = (groupName: string) => {
        focusSeqRef.current += 1
        setView("all")
        setFocusedGroup({ name: groupName, seq: focusSeqRef.current })
        setTab("permissions")
    }

    const selectedWorkbenchNames = values.workbenches.flatMap((id) => {
        const workbench = roleWorkbench(id)
        return workbench ? [workbench.name] : []
    })

    const submitButton = (
        <form.AppForm>
            <form.SubmitButton
                loading={pending}
                id="governance-admin-role-form-submit"
                label={role ? "保存本次调整" : "创建角色"}
                disabled={pending || isReadOnly || (role !== null && !dirty)}
                size="sm"
                className="px-3.5"
            />
        </form.AppForm>
    )

    const permissionReviewActions = (
        <>
            <Button
                id="governance-admin-role-form-review-selected"
                type="button"
                variant="ghost"
                size="xs"
                className={detailWorkspaceStyles.quietButton}
                onClick={() => setReview("selected")}
            >
                {hasWildcard
                    ? "查看全部权限"
                    : `已授权明细（${overview.effectiveCount}）`}
            </Button>
            {role ? (
                <Button
                    id="governance-admin-role-form-review-changes"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={
                        dirty
                            ? "text-warning-soft-foreground hover:bg-warning-soft/70 hover:text-warning-soft-foreground"
                            : detailWorkspaceStyles.quietButton
                    }
                    onClick={() => setReview("changes")}
                >
                    {dirty ? `变更核对（${changeCount}）` : "暂无变更"}
                </Button>
            ) : null}
            {overview.dangerousCount > 0 ? (
                <Button
                    id="governance-admin-role-form-review-dangerous"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className="text-destructive hover:bg-destructive-soft/70 hover:text-destructive-soft-foreground"
                    onClick={() => setReview("dangerous")}
                >
                    <ShieldAlertIcon
                        data-icon="inline-start"
                        aria-hidden="true"
                    />
                    高风险 {overview.dangerousCount} 项
                </Button>
            ) : null}
            {overview.preservedCodes.length > 0 && !hasWildcard ? (
                <Button
                    id="governance-admin-role-form-review-preserved"
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={detailWorkspaceStyles.quietButton}
                    onClick={() => setReview("preserved")}
                >
                    整模块授权 {overview.preservedCodes.length} 项
                </Button>
            ) : null}
        </>
    )

    const permissionSection = (
        <DetailWorkspaceSection
            title="操作权限"
            headingId="governance-admin-role-form-permissions-heading"
            width="full"
            badge={
                <Badge variant="secondary" className="num">
                    {hasWildcard
                        ? "全部权限"
                        : `${overview.effectiveCount} / ${overview.totalCatalogCount} 项`}
                </Badge>
            }
            subtitle="设置此角色可执行的功能操作；每个人能查看哪些数据请在人员资料的「数据范围」中配置。"
            actions={permissionReviewActions}
        >
            <form.AppField name="permissions">
                {(field) => (
                    <Field
                        className={cn(
                            "flex min-h-0 flex-1 flex-col gap-0",
                            role
                                ? "md:h-[clamp(28rem,calc(100dvh-16rem),52rem)]"
                                : "md:h-[clamp(26rem,calc(100dvh-18rem),46rem)]",
                        )}
                        data-invalid={!field.state.meta.isValid || undefined}
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
                            preservedCodes={overview.preservedCodes}
                            view={view}
                            onViewChange={setView}
                            focusedGroup={focusedGroup}
                            disabled={pending || isReadOnly}
                            onChange={(next) => {
                                field.handleChange(next)
                                void form.validateField("permissions", "change")
                            }}
                        />
                        {!field.state.meta.isValid ? (
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        ) : null}
                    </Field>
                )}
            </form.AppField>
        </DetailWorkspaceSection>
    )

    const roleAlerts = (
        <>
            {hasWildcard ? (
                <Alert variant="info">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>超级管理员角色（拥有全部权限）</AlertTitle>
                    <AlertDescription>
                        该角色默认拥有系统全部操作权限，受系统保护不可修改或删除。如需按岗位限制权限，请另行新建普通角色并分配给对应账号。
                    </AlertDescription>
                </Alert>
            ) : isSystem ? (
                <Alert variant="info">
                    <ShieldIcon aria-hidden="true" />
                    <AlertTitle>系统内置角色（只读保护）</AlertTitle>
                    <AlertDescription>
                        该角色受系统保护，不可修改或删除。如需基于此角色定制权限，可新建角色并使用「从其他角色复制」作为起点。
                    </AlertDescription>
                </Alert>
            ) : null}

            {!isReadOnly && missingWorkbenchItems.length > 0 ? (
                <Alert variant="warning">
                    <ShieldAlertIcon aria-hidden="true" />
                    <AlertTitle>已选岗位工作台缺少入口权限</AlertTitle>
                    <AlertDescription>
                        {missingWorkbenchItems
                            .map((entry) => entry.item.name)
                            .join("、")}
                        尚未勾选全部最低入口权限，保存后关联人员将暂时无法进入该工作台。
                    </AlertDescription>
                    {role && activeTab !== "settings" ? (
                        <AlertAction>
                            <Button
                                id="governance-admin-role-form-alert-fix-workbench"
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => setTab("settings")}
                            >
                                查看并补齐
                            </Button>
                        </AlertAction>
                    ) : null}
                </Alert>
            ) : null}
        </>
    )

    const hasAlerts =
        hasWildcard ||
        isSystem ||
        (!isReadOnly && missingWorkbenchItems.length > 0)

    const mediaTile = (
        <div
            aria-hidden="true"
            className={cn(
                "flex size-11 shrink-0 items-center justify-center rounded-lg border",
                hasWildcard
                    ? "border-warning-border/70 bg-warning-soft text-warning-soft-foreground"
                    : isSystem
                      ? "border-indigo-border/70 bg-indigo-soft text-indigo-soft-foreground"
                      : "border-info-border/70 bg-info-soft text-info-soft-foreground",
            )}
        >
            {!role ? (
                <ShieldPlusIcon className="size-5" />
            ) : hasWildcard ? (
                <ShieldAlertIcon className="size-5" />
            ) : isSystem ? (
                <ShieldIcon className="size-5" />
            ) : (
                <ShieldCheckIcon className="size-5" />
            )}
        </div>
    )

    const backLink = {
        id: "governance-admin-role-form-back",
        label: isAccountReturn ? "返回人员资料" : "角色与权限",
        visibility: isAccountReturn
            ? ("always" as const)
            : ("standalone-only" as const),
        onClick: handleLeave,
    }

    const headerMeta = role ? (
        <DetailWorkspaceMeta
            items={[
                hasWildcard ? (
                    "拥有全部业务与系统操作权限"
                ) : (
                    <span key="perms" className="num">
                        已授权 {overview.effectiveCount} 项（覆盖{" "}
                        {overview.coveredGroups.length} 个模块）
                    </span>
                ),
                boundSummary !== null ? (
                    <span key="accounts" className="num">
                        {boundSummary.members.length} 个关联账号
                    </span>
                ) : null,
                selectedWorkbenchNames.length > 0
                    ? `工作台：${selectedWorkbenchNames.join("、")}`
                    : null,
                role.created_at ? (
                    <span key="created" className="num">
                        创建于{" "}
                        {formatDateTime(
                            new Date(role.created_at * 1000).toISOString(),
                            "full",
                        )}
                    </span>
                ) : null,
            ]}
        />
    ) : (
        <DetailWorkspaceMeta
            items={[
                "设置角色名称、岗位工作台与操作权限",
                overview.effectiveCount > 0 ? (
                    <span key="perms" className="num">
                        已选 {overview.effectiveCount} 项权限（覆盖{" "}
                        {overview.coveredGroups.length} 个模块）
                    </span>
                ) : (
                    "尚未勾选操作权限"
                ),
                selectedWorkbenchNames.length > 0
                    ? `工作台：${selectedWorkbenchNames.join("、")}`
                    : null,
            ]}
        />
    )

    const headerSecondaryActions = (
        <>
            {!isReadOnly ? (
                <CopyRolePermissions
                    onRefreshRoles={onRefreshRoles}
                    roles={otherRoles}
                    disabled={isReadOnly || pending}
                    currentCount={overview.effectiveCount}
                    onCopy={(codes) => {
                        form.setFieldValue("permissions", [
                            ...form.state.values.permissions.filter(
                                (code) => !PERMISSION_BY_CODE.has(code),
                            ),
                            ...codes,
                        ])
                        void form.validateField("permissions", "change")
                    }}
                />
            ) : null}
            {role && canDelete && !isSystem ? (
                <Button
                    id="governance-admin-role-form-delete"
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.quietDangerButton}
                    disabled={pending}
                    onClick={() => setDeleting(true)}
                >
                    <Trash2Icon data-icon="inline-start" aria-hidden="true" />
                    删除角色
                </Button>
            ) : null}
            {!role ? (
                <Button
                    id="governance-admin-role-form-cancel"
                    type="button"
                    variant="ghost"
                    size="sm"
                    className={detailWorkspaceStyles.quietButton}
                    disabled={pending}
                    onClick={handleLeave}
                >
                    取消
                </Button>
            ) : null}
        </>
    )

    const formContent = role ? (
        <Tabs
            value={activeTab}
            onValueChange={(next) => setTab(next as RoleDetailTab)}
            className="min-w-0 flex-1 gap-0"
        >
            <DetailWorkspaceChrome
                back={backLink}
                media={mediaTile}
                title={values.name.trim() || role.name || "未命名角色"}
                primaryStatus={
                    hasWildcard
                        ? { tone: "warning", label: "全部权限" }
                        : overview.effectiveCount === 0
                          ? { tone: "neutral", label: "未配置权限" }
                          : { tone: "success", label: "启用" }
                }
                titleExtra={
                    isSystem || saveDockOpen ? (
                        <div className="flex flex-wrap items-center gap-1.5">
                            {isSystem ? (
                                <Badge variant="indigo">系统内置</Badge>
                            ) : null}
                            {saveDockOpen ? (
                                <DetailWorkspaceDirtyBadge />
                            ) : null}
                        </div>
                    ) : null
                }
                meta={headerMeta}
                secondaryActions={headerSecondaryActions}
                primaryAction={null}
                tabs={
                    <TabsList
                        variant="line"
                        className={detailWorkspaceStyles.tabsList}
                        aria-label="角色详情分区"
                    >
                        <TabsTrigger
                            id="governance-admin-role-tab-permissions"
                            className={detailWorkspaceStyles.tab}
                            value="permissions"
                        >
                            <span>操作权限</span>
                            {permissionChangeCount > 0 ? (
                                <span
                                    aria-label="操作权限有未保存修改"
                                    className="size-1.5 rounded-full bg-warning"
                                />
                            ) : (
                                <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                    {hasWildcard
                                        ? "全部"
                                        : overview.effectiveCount}
                                </span>
                            )}
                        </TabsTrigger>
                        <TabsTrigger
                            id="governance-admin-role-tab-settings"
                            className={detailWorkspaceStyles.tab}
                            value="settings"
                        >
                            <span>基础与工作台</span>
                            {!nameValid ? (
                                <span
                                    aria-label="角色名称填写有误"
                                    className="size-1.5 rounded-full bg-destructive"
                                />
                            ) : settingsChangeCount > 0 ? (
                                <span
                                    aria-label="基础与工作台有未保存修改"
                                    className="size-1.5 rounded-full bg-warning"
                                />
                            ) : values.workbenches.length > 0 ? (
                                <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                    {values.workbenches.length}
                                </span>
                            ) : null}
                        </TabsTrigger>
                        {canReadAdmins ? (
                            <TabsTrigger
                                id="governance-admin-role-tab-members"
                                className={detailWorkspaceStyles.tab}
                                value="members"
                            >
                                <span>关联人员</span>
                                {boundSummary !== null ? (
                                    <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                        {boundSummary.members.length}
                                    </span>
                                ) : null}
                            </TabsTrigger>
                        ) : null}
                    </TabsList>
                }
            />

            <DetailWorkspaceBody
                saveDockOpen={saveDockOpen}
                alerts={hasAlerts ? roleAlerts : null}
            >
                <TabsContent
                    value="permissions"
                    keepMounted
                    className="data-[hidden]:hidden"
                >
                    {permissionSection}
                </TabsContent>

                <TabsContent
                    value="settings"
                    keepMounted
                    className="data-[hidden]:hidden"
                >
                    <div
                        data-slot="detail-workspace-split"
                        className="grid items-start gap-x-6 gap-y-0 lg:grid-cols-[minmax(0,1fr)_280px] xl:grid-cols-[minmax(0,1fr)_320px] 2xl:grid-cols-[minmax(0,1fr)_360px]"
                    >
                        <div className="min-w-0">
                            <DetailWorkspaceSection
                                title="基本信息"
                                headingId="governance-admin-role-form-basic-heading"
                                width="form"
                            >
                                {isReadOnly ? (
                                    <div className="max-w-md space-y-1.5">
                                        <p className="text-xs font-medium text-foreground/75">
                                            角色名称
                                        </p>
                                        <div className="flex h-control items-center gap-2 rounded-md border border-border/60 bg-surface-sunken px-3 text-body-compact text-foreground">
                                            <LockIcon
                                                className="size-3.5 shrink-0 text-muted-foreground"
                                                aria-hidden="true"
                                            />
                                            <span className="min-w-0 truncate font-medium">
                                                {role.name}
                                            </span>
                                        </div>
                                        <p className="text-xs text-muted-foreground">
                                            系统内置角色名称受保护，不可修改。
                                        </p>
                                    </div>
                                ) : (
                                    <div data-slot="detail-form-grid">
                                        <form.AppField name="name">
                                            {(field) => (
                                                <field.TextField
                                                    id="governance-admin-role-form-name"
                                                    label="角色名称"
                                                    required
                                                    description="用于岗位识别与人员角色分配，2 至 32 个字符。"
                                                    placeholder="请输入角色名称"
                                                    className="max-w-md"
                                                    inputClassName="scroll-mt-32"
                                                    disabled={pending}
                                                />
                                            )}
                                        </form.AppField>
                                    </div>
                                )}
                            </DetailWorkspaceSection>

                            <DetailWorkspaceSection
                                title="岗位工作台"
                                headingId="governance-admin-role-form-workbenches-heading"
                                width="form"
                                badge={
                                    values.workbenches.length > 0 ? (
                                        <Badge
                                            variant="secondary"
                                            className="num"
                                        >
                                            已选 {values.workbenches.length} 个
                                        </Badge>
                                    ) : null
                                }
                                subtitle="设置该角色登录后默认使用的岗位工作台；需同时具备对应入口权限才可进入。"
                            >
                                <fieldset
                                    aria-labelledby="governance-admin-role-form-workbenches-heading"
                                    className="min-w-0"
                                >
                                    <form.Field name="workbenches">
                                        {(field) => (
                                            <RoleWorkbenchPicker
                                                value={field.state.value}
                                                permissions={values.permissions}
                                                disabled={pending || isReadOnly}
                                                onChange={(next) =>
                                                    field.handleChange(next)
                                                }
                                                onGrantPermissions={
                                                    isReadOnly
                                                        ? undefined
                                                        : handleGrantPermissions
                                                }
                                            />
                                        )}
                                    </form.Field>
                                </fieldset>
                            </DetailWorkspaceSection>
                        </div>

                        <aside
                            aria-label="权限覆盖与角色属性"
                            className="min-w-0 border-t border-border/60 py-5 lg:self-stretch lg:border-t-0 lg:border-l lg:border-border/60 lg:pl-6"
                        >
                            <RoleDetailSidebar
                                role={role}
                                permissions={values.permissions}
                                overview={overview}
                                workbenchCount={values.workbenches.length}
                                boundAccountCount={
                                    boundSummary
                                        ? boundSummary.members.length
                                        : null
                                }
                                canReadAdmins={canReadAdmins}
                                onJumpToPermissions={() =>
                                    setTab("permissions")
                                }
                                onJumpToModule={handleJumpToModule}
                                onOpenMembers={
                                    canReadAdmins
                                        ? () => setTab("members")
                                        : undefined
                                }
                                onOpenReview={setReview}
                            />
                        </aside>
                    </div>
                </TabsContent>

                {canReadAdmins ? (
                    <TabsContent
                        value="members"
                        keepMounted
                        className="data-[hidden]:hidden"
                    >
                        <RoleMembersPanel
                            role={role}
                            adminsQuery={adminsQuery}
                        />
                    </TabsContent>
                ) : null}
            </DetailWorkspaceBody>
        </Tabs>
    ) : (
        <>
            <DetailWorkspaceChrome
                back={backLink}
                media={mediaTile}
                title="新建角色"
                meta={headerMeta}
                secondaryActions={headerSecondaryActions}
                primaryAction={submitButton}
            />
            <DetailWorkspaceBody alerts={hasAlerts ? roleAlerts : null}>
                <DetailWorkspaceSection
                    title="基本信息"
                    headingId="governance-admin-role-form-basic-heading"
                    width="form"
                >
                    <div data-slot="detail-form-grid">
                        <form.AppField name="name">
                            {(field) => (
                                <field.TextField
                                    id="governance-admin-role-form-name"
                                    label="角色名称"
                                    required
                                    description="用于岗位识别与人员角色分配，2 至 32 个字符。"
                                    placeholder="请输入角色名称"
                                    className="max-w-md"
                                    inputClassName="scroll-mt-32"
                                    disabled={pending}
                                />
                            )}
                        </form.AppField>
                    </div>

                    <fieldset
                        aria-labelledby="governance-admin-role-form-workbenches-heading"
                        className="space-y-2.5 pt-2"
                    >
                        <div className="space-y-0.5">
                            <h3
                                id="governance-admin-role-form-workbenches-heading"
                                className="font-semibold text-foreground"
                            >
                                岗位工作台
                            </h3>
                            <p className="text-xs text-muted-foreground">
                                可选。设置该角色登录后默认使用的岗位工作台（仍需勾选对应入口权限后才可进入）。
                            </p>
                        </div>
                        <form.Field name="workbenches">
                            {(field) => (
                                <RoleWorkbenchPicker
                                    value={field.state.value}
                                    permissions={values.permissions}
                                    disabled={pending}
                                    onChange={(next) =>
                                        field.handleChange(next)
                                    }
                                    onGrantPermissions={handleGrantPermissions}
                                />
                            )}
                        </form.Field>
                    </fieldset>
                </DetailWorkspaceSection>

                {permissionSection}
            </DetailWorkspaceBody>
        </>
    )

    return (
        <DetailWorkspacePage>
            <form
                className="flex min-w-0 flex-1 flex-col"
                onSubmit={(event) => {
                    event.preventDefault()
                    triggerSubmit()
                }}
            >
                {formContent}

                {role && !isReadOnly ? (
                    <DetailWorkspaceSaveDock
                        open={saveDockOpen}
                        changeCount={changeCount}
                        description={
                            !nameValid
                                ? "请先在「基础与工作台」修正角色名称"
                                : boundSummary && boundSummary.activeCount > 0
                                  ? `保存后对 ${boundSummary.activeCount} 个启用账号即时生效`
                                  : "保存后立即生效"
                        }
                        onSave={triggerSubmit}
                        saving={pending}
                        actions={
                            <>
                                <Button
                                    id="governance-admin-role-form-dock-review"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.quietButton
                                    }
                                    disabled={pending}
                                    onClick={() => setReview("changes")}
                                >
                                    核对变更
                                </Button>
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
                                {submitButton}
                            </>
                        }
                    />
                ) : null}
            </form>

            <RolePermissionReview
                mode={review}
                onClose={() => setReview(null)}
                selected={values.permissions}
                initial={savedValues.permissions}
                preservedCodes={overview.preservedCodes}
                name={values.name}
                initialName={savedValues.name}
                workbenches={values.workbenches}
                initialWorkbenches={savedValues.workbenches}
            />

            {deleting && role && canDelete && !isSystem ? (
                <DeleteRoleDialog
                    role={role}
                    onOpenChange={(open) => {
                        if (!open) setDeleting(false)
                    }}
                    onDeleted={() => {
                        toast.add({
                            title: "角色已删除",
                            type: "success",
                            timeout: 3000,
                        })
                        router.push(ROLES_LIST_HREF)
                    }}
                />
            ) : null}

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
                            角色名称、岗位工作台或操作权限的修改尚未保存。
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
                            variant="destructive"
                            onClick={() => {
                                if (confirmDiscard === "reset") {
                                    form.reset(savedValues)
                                    setConfirmDiscard(null)
                                } else {
                                    router.push(returnHref)
                                }
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
