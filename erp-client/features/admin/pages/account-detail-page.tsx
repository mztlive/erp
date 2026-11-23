"use client"

import * as React from "react"
import Link from "next/link"
import {
    BanIcon,
    CheckIcon,
    CircleCheckIcon,
    CopyIcon,
    EllipsisIcon,
    PencilIcon,
    Trash2Icon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    Alert,
    AlertAction,
    AlertDescription,
    AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { isPermissionDeniedError } from "@/lib/api/errors"
import { formatDateTime } from "@/lib/datetime"
import { useRouter } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { AccessCheckPanel } from "../components/accounts/access-check-panel"
import { AccountDetailSidebar } from "../components/accounts/account-detail-sidebar"
import { AccountFormDialog } from "../components/accounts/account-form-dialog"
import { AccountOperationPermissions } from "../components/accounts/account-operation-permissions"
import {
    AccountProfileEditor,
    type AccountProfileSnapshot,
} from "../components/accounts/account-profile-editor"
import { AccountProfileOverview } from "../components/accounts/account-profile-overview"
import { DeleteAdminDialog } from "../components/accounts/delete-admin-dialog"
import { PersonalBusinessPermissions } from "../components/accounts/personal-business-permissions"
import { ToggleAdminStatusDialog } from "../components/accounts/toggle-admin-status-dialog"
import {
    useAdminsQuery,
    useAssignableRolesQuery,
    useRolesQuery,
} from "../hooks/queries"
import { usePersonScopes } from "../hooks/use-person-data-scopes"
import { usePersonalGrantDraft } from "../hooks/use-personal-grant-draft"
import {
    accountInitial,
    summarizePersonScopeViews,
} from "../lib/account-detail-view"
import { accountPermissionGroups } from "../lib/account-permission-preview"
import { adminAccountStatusBadge } from "../lib/account-status"

const backToAccounts = {
    id: "account-detail-back",
    label: "人员账号",
    href: "/system/accounts",
} as const

function CopyAccountInlineButton({ value }: { value: string }) {
    const [copied, setCopied] = React.useState(false)

    const handleCopy = React.useCallback(() => {
        if (typeof navigator === "undefined" || !navigator.clipboard) return
        void navigator.clipboard.writeText(value).then(() => {
            setCopied(true)
            toast.add({
                title: "已复制登录账号",
                type: "success",
                timeout: 2000,
            })
            window.setTimeout(() => setCopied(false), 2000)
        })
    }, [value])

    return (
        <Button
            id="account-detail-copy-account"
            type="button"
            variant="ghost"
            size="xs"
            className="h-5 gap-1 px-1.5 text-xs text-muted-foreground hover:text-foreground"
            aria-label="复制登录账号"
            onClick={handleCopy}
        >
            {copied ? (
                <CheckIcon className="size-3 text-success" aria-hidden="true" />
            ) : (
                <CopyIcon className="size-3" aria-hidden="true" />
            )}
            <span>{copied ? "已复制" : "复制"}</span>
        </Button>
    )
}

export function AccountDetailPage({ accountId }: { accountId: string }) {
    const { draft: businessDraft, setDraft: setBusinessDraft } =
        usePersonalGrantDraft(accountId)
    const accounts = useAdminsQuery()
    const profile = useAccountProfileQuery()
    const can = (permission: string) =>
        hasPermission(profile.data?.permissions, permission)

    const canReadOrg = can("org_unit:list")
    const canManageOrg = can("org_unit:manage")
    const canReadRoles = can("role:list")
    const canUpdateAdmin = can("admin:update")
    const canEditRoles = canUpdateAdmin && can("role:assignable_list")
    const canReadScopes = can("data_scope:list")
    const canCreateScopes = can("data_scope:create") && canManageOrg

    const roles = useRolesQuery(canReadRoles)
    const options = useAssignableRolesQuery(canEditRoles)
    const org = useOrganizationStateQuery(canReadOrg)
    const scopes = usePersonScopes(accountId, canReadScopes)

    const [profileDraft, setProfileSnapshot] =
        React.useState<AccountProfileSnapshot | null>(null)
    const profileSnapshot =
        profileDraft?.account.id === accountId ? profileDraft : null
    const account =
        accounts.data?.find((row) => row.id === accountId) ??
        (profileSnapshot?.account.id === accountId
            ? profileSnapshot.account
            : null)

    const router = useRouter()
    const [editing, setEditing] = React.useState<"password" | null>(null)
    const [deleting, setDeleting] = React.useState(false)
    const [togglingStatus, setTogglingStatus] = React.useState(false)
    const [tab, setTab] = React.useState("basic")
    const [focusedScopeResource, setFocusedScopeResource] = React.useState<
        string | null
    >(null)

    const permissionPreview = React.useMemo(
        () =>
            account && canReadRoles && roles.data
                ? accountPermissionGroups(account, roles.data)
                : null,
        [account, canReadRoles, roles.data],
    )

    const scopeSummary = React.useMemo(() => {
        if (!canReadScopes || !scopes.data) return null
        const labels = new Map(
            (org.data?.units ?? []).map((unit) => [unit.id, unit.name]),
        )
        return summarizePersonScopeViews(scopes.data, labels)
    }, [canReadScopes, org.data?.units, scopes.data])

    const notifySuccess = React.useCallback((title: string) => {
        toast.add({ title, type: "success", timeout: 4000 })
    }, [])

    if (accounts.isPending && !account) {
        return (
            <DetailWorkspaceSkeleton
                title="人员资料"
                description="正在读取人员资料…"
                sidebar
            />
        )
    }

    if (accounts.isError && !profileSnapshot) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome title="人员资料" back={backToAccounts} />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessFailureState
                            title="人员资料加载失败"
                            error={accounts.error}
                            onRetry={() => void accounts.refetch()}
                            id="account-detail-retry"
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    if (!account) {
        return (
            <DetailWorkspacePage>
                <DetailWorkspaceChrome
                    title="人员不存在或无权查看"
                    back={backToAccounts}
                />
                <DetailWorkspaceBody>
                    <DetailWorkspaceSection width="form">
                        <BusinessEmptyState
                            kind="no-data"
                            title="账号不存在或无权查看"
                            description="可能账号已被删除，或当前登录账号无权查看该人员资料。"
                            action={
                                <Button
                                    id="account-detail-missing-back"
                                    size="sm"
                                    render={<Link href="/system/accounts" />}
                                >
                                    返回人员账号
                                </Button>
                            }
                        />
                    </DetailWorkspaceSection>
                </DetailWorkspaceBody>
            </DetailWorkspacePage>
        )
    }

    const hasSystemRole = account.role_ids.some((id) =>
        roles.data?.some((role) => role.id === id && role.system),
    )
    const isCurrentUser = profile.data?.userid === account.id
    const isActive = account.status === "active"

    const deleteDisabledReason = hasSystemRole
        ? "系统角色账号不可删除"
        : !roles.isSuccess
          ? "角色尚未读取，暂不能删除"
          : undefined
    const canToggleStatus = canUpdateAdmin && canReadRoles
    const canDelete = can("admin:delete") && canReadRoles
    const statusDisabledReason = hasSystemRole
        ? "系统角色账号不可停用或启用"
        : !roles.isSuccess
          ? "角色尚未读取，暂不能变更账号状态"
          : isActive && isCurrentUser
            ? "不能停用当前登录账号"
            : undefined

    const editDisabledReason =
        canReadOrg && canManageOrg && (org.isPending || org.isFetching)
            ? "正在读取组织信息，请稍后编辑"
            : undefined

    const openEditProfile = () => {
        setProfileSnapshot({
            account,
            view: org.data,
        })
    }

    const activeTab =
        (tab === "permissions" && !canReadRoles) ||
        ((tab === "access" || tab === "check") && !canReadScopes)
            ? "basic"
            : tab

    const visibleTabCount = 1 + (canReadRoles ? 1 : 0) + (canReadScopes ? 2 : 0)

    const overviewAlerts = (
        <>
            {!isActive ? (
                <Alert variant="warning">
                    <BanIcon aria-hidden="true" />
                    <AlertTitle>
                        该账号当前处于
                        {adminAccountStatusBadge(account.status).label}状态
                    </AlertTitle>
                    <AlertDescription>
                        停用期间该账号无法登录后台，既有会话请求将被拒绝；角色、部门与数据范围均已保留。
                    </AlertDescription>
                    {canToggleStatus && !statusDisabledReason ? (
                        <AlertAction>
                            <Button
                                id="account-detail-alert-enable"
                                type="button"
                                variant="outline"
                                size="xs"
                                onClick={() => setTogglingStatus(true)}
                            >
                                启用账号
                            </Button>
                        </AlertAction>
                    ) : null}
                </Alert>
            ) : null}
            {canReadRoles &&
            roles.isSuccess &&
            account.role_ids.length === 0 ? (
                <Alert variant="warning">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>尚未分配角色</AlertTitle>
                    <AlertDescription>
                        未分配角色的账号无法执行任何业务操作，请通过「编辑资料」为其分配角色。
                    </AlertDescription>
                    {canUpdateAdmin ? (
                        <AlertAction>
                            <Button
                                id="account-detail-alert-assign-role"
                                type="button"
                                variant="outline"
                                size="xs"
                                disabled={Boolean(editDisabledReason)}
                                onClick={openEditProfile}
                            >
                                分配角色
                            </Button>
                        </AlertAction>
                    ) : null}
                </Alert>
            ) : null}
        </>
    )

    const hasAlerts =
        !isActive ||
        (canReadRoles && roles.isSuccess && account.role_ids.length === 0)

    const hasMoreActions = canDelete || canToggleStatus

    const openScopesTab = (resource?: string) => {
        if (resource) setFocusedScopeResource(resource)
        setTab("access")
    }

    return (
        <DetailWorkspacePage>
            <Tabs
                value={activeTab}
                onValueChange={(next) => setTab(String(next))}
                className="min-w-0 flex-1 gap-0"
            >
                <DetailWorkspaceChrome
                    back={backToAccounts}
                    media={
                        <div
                            aria-hidden="true"
                            className={cn(
                                "flex size-11 shrink-0 items-center justify-center rounded-lg border text-section-title font-semibold",
                                isActive
                                    ? "border-info-border/70 bg-info-soft text-info-soft-foreground"
                                    : "border-neutral-border bg-neutral-soft text-neutral-soft-foreground",
                            )}
                        >
                            {accountInitial(account.name, account.account)}
                        </div>
                    }
                    title={account.name}
                    primaryStatus={adminAccountStatusBadge(account.status)}
                    titleExtra={
                        hasSystemRole || isCurrentUser ? (
                            <div className="flex flex-wrap items-center gap-1.5">
                                {hasSystemRole ? (
                                    <Badge variant="indigo">系统角色</Badge>
                                ) : null}
                                {isCurrentUser ? (
                                    <Badge variant="info">当前登录</Badge>
                                ) : null}
                            </div>
                        ) : null
                    }
                    meta={
                        <DetailWorkspaceMeta
                            items={[
                                <span
                                    key="account"
                                    className="inline-flex items-center gap-1"
                                    data-workspace-document-number={
                                        account.account
                                    }
                                >
                                    <span className="num">
                                        登录账号：{account.account}
                                    </span>
                                    <CopyAccountInlineButton
                                        value={account.account}
                                    />
                                </span>,
                                <span key="created" className="num">
                                    创建于{" "}
                                    {formatDateTime(
                                        new Date(
                                            account.created_at * 1000,
                                        ).toISOString(),
                                        "full",
                                    )}
                                </span>,
                            ]}
                        />
                    }
                    secondaryActions={
                        hasMoreActions ? (
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    id="account-detail-more"
                                    render={
                                        <Button
                                            variant="ghost"
                                            size="sm"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                        />
                                    }
                                >
                                    <EllipsisIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    更多
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                    align="end"
                                    className="w-56"
                                >
                                    {canToggleStatus ? (
                                        <DropdownMenuItem
                                            id="account-detail-toggle-status"
                                            disabled={Boolean(
                                                statusDisabledReason,
                                            )}
                                            aria-describedby={
                                                statusDisabledReason
                                                    ? "account-detail-status-reason"
                                                    : undefined
                                            }
                                            onClick={() =>
                                                setTogglingStatus(true)
                                            }
                                        >
                                            {isActive ? (
                                                <BanIcon aria-hidden="true" />
                                            ) : (
                                                <CircleCheckIcon aria-hidden="true" />
                                            )}
                                            {isActive ? "停用账号" : "启用账号"}
                                        </DropdownMenuItem>
                                    ) : null}
                                    {canToggleStatus && statusDisabledReason ? (
                                        <p
                                            id="account-detail-status-reason"
                                            className="px-2 py-1.5 text-xs text-muted-foreground"
                                        >
                                            {statusDisabledReason}
                                        </p>
                                    ) : null}
                                    {canDelete ? (
                                        <DropdownMenuItem
                                            id="account-detail-delete"
                                            variant="destructive"
                                            disabled={Boolean(
                                                deleteDisabledReason,
                                            )}
                                            aria-describedby={
                                                deleteDisabledReason
                                                    ? "account-detail-delete-reason"
                                                    : undefined
                                            }
                                            onClick={() => setDeleting(true)}
                                        >
                                            <Trash2Icon aria-hidden="true" />
                                            删除账号
                                        </DropdownMenuItem>
                                    ) : null}
                                    {canDelete && deleteDisabledReason ? (
                                        <p
                                            id="account-detail-delete-reason"
                                            className="px-2 py-1.5 text-xs text-muted-foreground"
                                        >
                                            {deleteDisabledReason}
                                        </p>
                                    ) : null}
                                </DropdownMenuContent>
                            </DropdownMenu>
                        ) : null
                    }
                    primaryAction={
                        canUpdateAdmin ? (
                            <Button
                                id="account-profile-edit"
                                type="button"
                                size="sm"
                                className="px-3.5"
                                disabled={Boolean(editDisabledReason)}
                                title={editDisabledReason ?? "编辑账号资料"}
                                onClick={openEditProfile}
                            >
                                <PencilIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                编辑资料
                            </Button>
                        ) : null
                    }
                    tabs={
                        visibleTabCount >= 2 ? (
                            <TabsList
                                variant="line"
                                className={detailWorkspaceStyles.tabsList}
                                aria-label="人员设置分区"
                            >
                                <TabsTrigger
                                    id="account-detail-basic-tab"
                                    className={detailWorkspaceStyles.tab}
                                    value="basic"
                                >
                                    基本资料
                                </TabsTrigger>
                                {canReadRoles ? (
                                    <TabsTrigger
                                        id="account-detail-permissions-tab"
                                        className={detailWorkspaceStyles.tab}
                                        value="permissions"
                                    >
                                        <span>操作权限</span>
                                        {permissionPreview &&
                                        permissionPreview.permissionCount >
                                            0 ? (
                                            <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                                {permissionPreview.allPermissions
                                                    ? "全部"
                                                    : permissionPreview.permissionCount}
                                            </span>
                                        ) : null}
                                    </TabsTrigger>
                                ) : null}
                                {canReadScopes ? (
                                    <TabsTrigger
                                        id="account-detail-access-tab"
                                        className={detailWorkspaceStyles.tab}
                                        value="access"
                                    >
                                        <span>数据范围</span>
                                        {businessDraft?.dirty ? (
                                            <span
                                                aria-label="有未保存的数据范围草稿"
                                                className="size-1.5 rounded-full bg-warning"
                                            />
                                        ) : scopeSummary &&
                                          scopeSummary.unconfiguredCount > 0 ? (
                                            <span className="num rounded-full bg-warning-soft px-1.5 py-0 text-tiny font-medium text-warning-soft-foreground">
                                                {scopeSummary.unconfiguredCount}{" "}
                                                待配
                                            </span>
                                        ) : scopeSummary &&
                                          scopeSummary.configurable.length >
                                              0 ? (
                                            <span className="num rounded-full bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                                                {
                                                    scopeSummary.configurable
                                                        .length
                                                }
                                            </span>
                                        ) : null}
                                    </TabsTrigger>
                                ) : null}
                                {canReadScopes ? (
                                    <TabsTrigger
                                        id="account-detail-check-tab"
                                        className={detailWorkspaceStyles.tab}
                                        value="check"
                                    >
                                        访问检查
                                    </TabsTrigger>
                                ) : null}
                            </TabsList>
                        ) : undefined
                    }
                />

                <DetailWorkspaceBody alerts={hasAlerts ? overviewAlerts : null}>
                    <TabsContent
                        value="basic"
                        keepMounted
                        className="data-[hidden]:hidden"
                    >
                        <div
                            data-slot="detail-workspace-split"
                            className="grid items-start gap-x-6 gap-y-0 lg:grid-cols-[minmax(0,1fr)_280px] xl:grid-cols-[minmax(0,1fr)_320px] 2xl:grid-cols-[minmax(0,1fr)_360px]"
                        >
                            <div className="min-w-0">
                                <AccountProfileOverview
                                    account={account}
                                    roles={roles}
                                    orgData={org.data}
                                    scopes={scopes}
                                    canEdit={canUpdateAdmin}
                                    editDisabledReason={editDisabledReason}
                                    canReadRoles={canReadRoles}
                                    canEditRoles={can("role:update")}
                                    canReadScopes={canReadScopes}
                                    onEdit={openEditProfile}
                                    onOpenPermissions={() =>
                                        setTab("permissions")
                                    }
                                    onOpenScopes={openScopesTab}
                                />
                            </div>
                            <aside
                                aria-label="组织归属与账号安全"
                                className="min-w-0 border-t border-border/60 py-5 lg:self-stretch lg:border-t-0 lg:border-l lg:border-border/60 lg:pl-6"
                            >
                                <AccountDetailSidebar
                                    account={account}
                                    isCurrentUser={isCurrentUser}
                                    hasSystemRole={hasSystemRole}
                                    canChangePassword={canUpdateAdmin}
                                    canToggleStatus={canToggleStatus}
                                    canEditProfile={canUpdateAdmin}
                                    canReadOrg={canReadOrg}
                                    canManageOrg={canManageOrg}
                                    orgData={org.data}
                                    orgPending={org.isPending}
                                    orgError={org.isError}
                                    orgFailure={org.error}
                                    onRetryOrg={() => void org.refetch()}
                                    onPasswordChange={() =>
                                        setEditing("password")
                                    }
                                    onEditProfile={openEditProfile}
                                />
                            </aside>
                        </div>
                    </TabsContent>

                    {canReadRoles ? (
                        <TabsContent
                            value="permissions"
                            keepMounted
                            className="data-[hidden]:hidden"
                        >
                            <AccountOperationPermissions
                                key={accountId}
                                account={account}
                                roles={roles}
                                canRead={canReadRoles}
                                canEditRoles={can("role:update")}
                            />
                        </TabsContent>
                    ) : null}

                    {canReadScopes ? (
                        <TabsContent
                            value="access"
                            keepMounted
                            className="data-[hidden]:hidden"
                        >
                            <PersonalBusinessPermissions
                                key={accountId}
                                userId={accountId}
                                draft={businessDraft}
                                onDraftChange={setBusinessDraft}
                                name={account.name}
                                units={org.data?.units ?? []}
                                ready={org.isSuccess}
                                canRead={canReadScopes}
                                canCreate={canCreateScopes}
                                focusedResource={focusedScopeResource}
                                onFocusedResourceHandled={() =>
                                    setFocusedScopeResource(null)
                                }
                            />
                            {org.isError ? (
                                <DetailWorkspaceSection width="form">
                                    <BusinessFailureState
                                        id="account-detail-scope-org-retry"
                                        error={org.error}
                                        onRetry={() => void org.refetch()}
                                    />
                                </DetailWorkspaceSection>
                            ) : null}
                        </TabsContent>
                    ) : null}

                    {canReadScopes ? (
                        <TabsContent
                            value="check"
                            keepMounted
                            className="data-[hidden]:hidden"
                        >
                            <AccessCheckPanel accountId={accountId} />
                        </TabsContent>
                    ) : null}
                </DetailWorkspaceBody>
            </Tabs>

            {profileSnapshot ? (
                <AccountProfileEditor
                    key={accountId}
                    snapshot={profileSnapshot}
                    currentVersion={org.data?.organizationVersion}
                    canName={canUpdateAdmin}
                    canEditRoles={
                        canEditRoles && !isPermissionDeniedError(options.error)
                    }
                    assignableRoles={options.data ?? []}
                    roleLabels={roles.data ?? []}
                    rolesReady={options.isSuccess}
                    roleOptionsError={
                        options.isError ? options.error : undefined
                    }
                    onRetryRoleOptions={() => void options.refetch()}
                    canOrganization={
                        canUpdateAdmin &&
                        canManageOrg &&
                        Boolean(
                            profileSnapshot.view?.people.some(
                                (item) => item.id === accountId,
                            ),
                        )
                    }
                    onDone={(saved) => {
                        setProfileSnapshot(null)
                        if (saved) notifySuccess("账号资料已统一保存。")
                    }}
                />
            ) : null}

            {editing && canUpdateAdmin ? (
                <AccountFormDialog
                    mode="edit"
                    account={account}
                    roleOptions={options.data ?? []}
                    section={editing}
                    onOpenChange={(open) => {
                        if (!open) setEditing(null)
                    }}
                    onSaved={() => notifySuccess("登录密码已修改。")}
                />
            ) : null}

            {togglingStatus && canToggleStatus ? (
                <ToggleAdminStatusDialog
                    account={account}
                    onOpenChange={(open) => {
                        if (!open) setTogglingStatus(false)
                    }}
                    onChanged={(status) =>
                        notifySuccess(
                            status === "suspended"
                                ? "账号已停用。"
                                : "账号已启用。",
                        )
                    }
                />
            ) : null}

            {deleting && canDelete ? (
                <DeleteAdminDialog
                    account={account}
                    onOpenChange={(open) => {
                        if (!open) setDeleting(false)
                    }}
                    onDeleted={() => {
                        notifySuccess("账号已删除。")
                        router.push("/system/accounts")
                    }}
                />
            ) : null}
        </DetailWorkspacePage>
    )
}
