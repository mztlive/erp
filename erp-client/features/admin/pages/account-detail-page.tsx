"use client"

import * as React from "react"
import {
    BanIcon,
    CircleCheckIcon,
    EllipsisIcon,
    Trash2Icon,
} from "lucide-react"
import {
    AccountProfileEditor,
    type AccountProfileSnapshot,
} from "../components/accounts/account-profile-editor"
import { AccountProfileOverview } from "../components/accounts/account-profile-overview"
import Link from "next/link"
import { useRouter } from "@/lib/navigation"
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs"
import { DeleteAdminDialog } from "../components/accounts/delete-admin-dialog"
import { ToggleAdminStatusDialog } from "../components/accounts/toggle-admin-status-dialog"
import {
    BusinessFailureState,
    DetailPageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuTrigger,
    DropdownMenuContent,
    DropdownMenuItem,
} from "@/components/ui/dropdown-menu"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import {
    useAdminsQuery,
    useRolesQuery,
    useAssignableRolesQuery,
} from "../hooks/queries"
import { AccountFormDialog } from "../components/accounts/account-form-dialog"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { unitLabel } from "@/features/organization/lib/tree"
import { AccessCheckPanel } from "../components/accounts/access-check-panel"
import { AccountOperationPermissions } from "../components/accounts/account-operation-permissions"
import { PersonalBusinessPermissions } from "../components/accounts/personal-business-permissions"
import { usePersonalGrantDraft } from "../hooks/use-personal-grant-draft"
import { adminAccountStatusBadge } from "../lib/account-status"

export function AccountDetailPage({ accountId }: { accountId: string }) {
    const { draft: businessDraft, setDraft: setBusinessDraft } =
        usePersonalGrantDraft(accountId)
    const accounts = useAdminsQuery()
    const roles = useRolesQuery()
    const options = useAssignableRolesQuery()
    const profile = useAccountProfileQuery()
    const [profileDraft, setProfileSnapshot] =
        React.useState<AccountProfileSnapshot | null>(null)
    const profileSnapshot =
        profileDraft?.account.id === accountId ? profileDraft : null
    const account =
        accounts.data?.find((row) => row.id === accountId) ??
        (profileSnapshot?.account.id === accountId
            ? profileSnapshot.account
            : null)
    const can = (permission: string) =>
        hasPermission(profile.data?.permissions, permission)
    const org = useOrganizationStateQuery(can("org_unit:list"))
    const router = useRouter()
    const [editing, setEditing] = React.useState<"password" | null>(null)
    const [deleting, setDeleting] = React.useState(false)
    const [togglingStatus, setTogglingStatus] = React.useState(false)
    const [tab, setTab] = React.useState("basic")
    const [notice, setNotice] = React.useState<string | null>(null)
    if (accounts.isPending && !account)
        return (
            <PageScaffold>
                <p role="status">正在读取人员资料…</p>
            </PageScaffold>
        )
    if (accounts.isError && !profileSnapshot)
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={accounts.error}
                    onRetry={() => void accounts.refetch()}
                    id="account-detail-retry"
                />
            </PageScaffold>
        )
    if (!account)
        return (
            <PageScaffold>
                <p>账号不存在或无权查看。</p>
                <Link id="account-detail-missing-back" href="/system/accounts">
                    返回人员账号
                </Link>
            </PageScaffold>
        )
    const person = org.data?.people.find((row) => row.id === account.id)
    const departmentLabel = org.isError
        ? "部门加载失败"
        : !can("org_unit:list")
          ? "无部门查看权限"
          : !org.data
            ? "正在读取…"
            : !person
              ? "不在可查看范围"
              : person.own_org_unit_id
                ? unitLabel(org.data.units, person.own_org_unit_id)
                : "未分配部门"
    const hasSystemRole = account.role_ids.some((id) =>
        roles.data?.some((role) => role.id === id && role.system),
    )
    const singleRole =
        account.role_ids.length === 1
            ? roles.data?.find((role) => role.id === account.role_ids[0])
            : undefined
    const deleteDisabledReason = hasSystemRole
        ? "系统角色账号不可删除"
        : !roles.isSuccess
          ? "角色尚未读取，暂不能删除"
          : undefined
    const isActive = account.status === "active"
    const statusDisabledReason = hasSystemRole
        ? "系统角色账号不可停用或启用"
        : !roles.isSuccess
          ? "角色尚未读取，暂不能变更账号状态"
          : isActive && profile.data?.userid === account.id
            ? "不能停用当前登录账号"
            : undefined
    return (
        <PageScaffold density="compact" className="gap-3">
            <DetailPageHeader
                className="space-y-2"
                back={{
                    id: "account-detail-back",
                    label: "人员账号",
                    href: "/system/accounts",
                }}
                title={account.name}
                numberLabel="登录账号"
                documentNumber={account.account}
                primaryStatus={adminAccountStatusBadge(account.status)}
                meta={
                    <>
                        <span>所属部门：{departmentLabel}</span>
                        <span>
                            {singleRole
                                ? `角色：${singleRole.name}`
                                : `${account.role_ids.length} 个角色`}
                        </span>
                    </>
                }
                secondaryActions={
                    (can("admin:delete") || can("admin:update")) && (
                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id="account-detail-more"
                                render={<Button variant="ghost" size="sm" />}
                            >
                                <EllipsisIcon
                                    data-icon="inline-start"
                                    aria-hidden
                                />
                                更多
                            </DropdownMenuTrigger>
                            <DropdownMenuContent align="end" className="w-56">
                                {can("admin:update") && (
                                    <DropdownMenuItem
                                        id="account-detail-toggle-status"
                                        disabled={Boolean(statusDisabledReason)}
                                        aria-describedby={
                                            statusDisabledReason
                                                ? "account-detail-status-reason"
                                                : undefined
                                        }
                                        onClick={() => setTogglingStatus(true)}
                                    >
                                        {isActive ? (
                                            <BanIcon aria-hidden />
                                        ) : (
                                            <CircleCheckIcon aria-hidden />
                                        )}
                                        {isActive ? "停用账号" : "启用账号"}
                                    </DropdownMenuItem>
                                )}
                                {can("admin:update") &&
                                    statusDisabledReason && (
                                        <p
                                            id="account-detail-status-reason"
                                            className="px-2 py-1.5 text-xs text-muted-foreground"
                                        >
                                            {statusDisabledReason}
                                        </p>
                                    )}
                                {can("admin:delete") && (
                                    <DropdownMenuItem
                                        id="account-detail-delete"
                                        variant="destructive"
                                        disabled={Boolean(deleteDisabledReason)}
                                        aria-describedby={
                                            deleteDisabledReason
                                                ? "account-detail-delete-reason"
                                                : undefined
                                        }
                                        onClick={() => setDeleting(true)}
                                    >
                                        <Trash2Icon aria-hidden />
                                        删除账号
                                    </DropdownMenuItem>
                                )}
                                {can("admin:delete") &&
                                    deleteDisabledReason && (
                                        <p
                                            id="account-detail-delete-reason"
                                            className="px-2 py-1.5 text-xs text-muted-foreground"
                                        >
                                            {deleteDisabledReason}
                                        </p>
                                    )}
                            </DropdownMenuContent>
                        </DropdownMenu>
                    )
                }
            />
            {notice && (
                <Alert variant="success">
                    <CircleCheckIcon aria-hidden="true" />
                    <AlertTitle>{notice}</AlertTitle>
                </Alert>
            )}
            <div className={surfacePanelClassName}>
                <Tabs
                    value={tab}
                    onValueChange={(next) => setTab(String(next))}
                >
                    <TabsList
                        variant="line"
                        className="w-full justify-start overflow-x-auto border-b border-grid px-4"
                        aria-label="人员设置分区"
                    >
                        <TabsTrigger
                            id="account-detail-basic-tab"
                            value="basic"
                        >
                            基本资料
                        </TabsTrigger>
                        <TabsTrigger
                            id="account-detail-access-tab"
                            value="access"
                        >
                            数据范围
                        </TabsTrigger>
                        {can("data_scope:list") && (
                            <TabsTrigger
                                id="account-detail-check-tab"
                                value="check"
                            >
                                访问检查
                            </TabsTrigger>
                        )}
                    </TabsList>
                    <TabsContent
                        value="basic"
                        keepMounted
                        className="space-y-4 p-4 data-[hidden]:hidden"
                    >
                        <AccountProfileOverview
                            account={account}
                            roles={roles}
                            departmentLabel={departmentLabel}
                            canEdit={
                                can("admin:update") ||
                                (can("org_unit:manage") && Boolean(person))
                            }
                            editDisabledReason={
                                can("org_unit:list") &&
                                can("org_unit:manage") &&
                                (org.isPending || org.isFetching)
                                    ? "正在读取组织信息，请稍后编辑"
                                    : undefined
                            }
                            canChangePassword={can("admin:update")}
                            canReadRoles={can("role:list")}
                            onEdit={() =>
                                setProfileSnapshot({
                                    account,
                                    view: org.data,
                                })
                            }
                            onPasswordChange={() => setEditing("password")}
                        />
                        {org.isError && (
                            <BusinessFailureState
                                id="account-profile-org-retry"
                                error={org.error}
                                onRetry={() => void org.refetch()}
                            />
                        )}
                        <AccountOperationPermissions
                            key={accountId}
                            account={account}
                            roles={roles}
                            canRead={can("role:list")}
                        />
                    </TabsContent>
                    <TabsContent
                        value="access"
                        keepMounted
                        className="space-y-4 p-4 data-[hidden]:hidden"
                    >
                        <PersonalBusinessPermissions
                            key={accountId}
                            userId={accountId}
                            draft={businessDraft}
                            onDraftChange={setBusinessDraft}
                            name={account.name}
                            units={org.data?.units ?? []}
                            ready={org.isSuccess}
                            canRead={can("data_scope:list")}
                            canCreate={
                                can("data_scope:create") &&
                                can("org_unit:manage")
                            }
                        />
                        {org.isError && (
                            <BusinessFailureState
                                id="account-detail-scope-org-retry"
                                error={org.error}
                                onRetry={() => void org.refetch()}
                            />
                        )}
                    </TabsContent>
                    {can("data_scope:list") && (
                        <TabsContent
                            value="check"
                            keepMounted
                            className="p-4 data-[hidden]:hidden"
                        >
                            <AccessCheckPanel accountId={accountId} />
                        </TabsContent>
                    )}
                </Tabs>
            </div>
            {profileSnapshot && (
                <AccountProfileEditor
                    key={accountId}
                    snapshot={profileSnapshot}
                    currentVersion={org.data?.organizationVersion}
                    canName={can("admin:update")}
                    assignableRoles={options.data ?? []}
                    roleLabels={roles.data ?? []}
                    rolesReady={options.isSuccess}
                    roleOptionsError={
                        options.isError ? options.error : undefined
                    }
                    onRetryRoleOptions={() => void options.refetch()}
                    canOrganization={
                        can("org_unit:manage") &&
                        Boolean(
                            profileSnapshot.view?.people.some(
                                (person) => person.id === accountId,
                            ),
                        )
                    }
                    onDone={(saved) => {
                        setProfileSnapshot(null)
                        if (saved) setNotice("账号资料已统一保存。")
                    }}
                />
            )}
            {editing && (
                <AccountFormDialog
                    mode="edit"
                    account={account}
                    roleOptions={options.data ?? []}
                    section={editing}
                    onOpenChange={(open) => {
                        if (!open) setEditing(null)
                    }}
                    onSaved={() => setNotice("密码已修改。")}
                />
            )}
            {togglingStatus && (
                <ToggleAdminStatusDialog
                    account={account}
                    onOpenChange={(open) => {
                        if (!open) setTogglingStatus(false)
                    }}
                    onChanged={(status) =>
                        setNotice(
                            status === "suspended"
                                ? "账号已停用。"
                                : "账号已启用。",
                        )
                    }
                />
            )}
            {deleting && (
                <DeleteAdminDialog
                    account={account}
                    onOpenChange={(open) => {
                        if (!open) setDeleting(false)
                    }}
                    onDeleted={() => router.push("/system/accounts")}
                />
            )}
        </PageScaffold>
    )
}
