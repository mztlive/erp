"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import type { ColumnDef } from "@tanstack/react-table"
import {
    BanIcon,
    CircleCheckIcon,
    PlusIcon,
    ShieldCheckIcon,
    Trash2Icon,
} from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    TableAsyncCell,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    listWorkspaceFilterStatusText,
    type TableRowAction,
} from "@/components/business"
import { AccountFormDialog } from "@/features/admin/components/accounts/account-form-dialog"
import { DeleteAdminDialog } from "@/features/admin/components/accounts/delete-admin-dialog"
import { ToggleAdminStatusDialog } from "@/features/admin/components/accounts/toggle-admin-status-dialog"
import {
    useAdminsQuery,
    useAssignableRolesQuery,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import type { AdminAccount } from "@/features/admin/types"
import { adminAccountStatusBadge } from "@/features/admin/lib/account-status"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { isPermissionDeniedError } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { PeopleNavigation } from "@/features/organization/components/people-navigation"
import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"
import { unitLabel } from "@/features/organization/lib/tree"
import { formatDateTime } from "@/lib/datetime"
import { restoreListRowFocus } from "@/lib/list-row-focus"

/** 人员列表负责查找、创建与常用账号操作；完整设置进入人员资料。 */
export function AccountsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const profileQuery = useAccountProfileQuery()
    const permissions = profileQuery.data?.permissions
    const canReadRoles = hasPermission(permissions, "role:list")
    const canCreateAdmin =
        hasPermission(permissions, "admin:create") &&
        hasPermission(permissions, "role:assignable_list")
    const canReadOrganization = hasPermission(
        profileQuery.data?.permissions,
        "org_unit:list",
    )
    const canUpdateAdmin = hasPermission(permissions, "admin:update")
    const canDeleteAdmin = hasPermission(
        profileQuery.data?.permissions,
        "admin:delete",
    )
    const currentUserId = profileQuery.data?.userid
    const organizationQuery = useOrganizationStateQuery(canReadOrganization)
    const adminsQuery = useAdminsQuery()
    const rolesQuery = useRolesQuery(canReadRoles)
    const assignableRolesQuery = useAssignableRolesQuery(canCreateAdmin)
    const showCreateAdmin =
        canCreateAdmin && !isPermissionDeniedError(assignableRolesQuery.error)
    const canOpenCreateDialog =
        showCreateAdmin && assignableRolesQuery.isSuccess

    /** 权限配置页按角色跳转过来时（?q=角色名），首屏直接带上该筛选。 */
    const keywordFromUrl = searchParams.get("q") ?? ""
    const [keyword, setKeyword] = React.useState(keywordFromUrl)
    const { searchDraft, setSearchDraft, searchInputRef } =
        useSearchDraft(keywordFromUrl)
    useSearchHotkey(searchInputRef, {
        overlaySelector:
            '[role="dialog"], [role="alertdialog"], [role="menu"], [data-slot="sheet"]',
    })
    React.useEffect(() => {
        setKeyword(searchParams.get("q") ?? "")
    }, [searchParams])
    const [creating, setCreating] = React.useState(false)
    const [togglingAccount, setTogglingAccount] =
        React.useState<AdminAccount | null>(null)
    const [deletingAccount, setDeletingAccount] =
        React.useState<AdminAccount | null>(null)
    const [notice, setNotice] = React.useState<string | null>(null)

    const roleNameById = React.useMemo(
        () =>
            new Map(
                (rolesQuery.data ?? []).map((role) => [role.id, role.name]),
            ),
        [rolesQuery.data],
    )
    const systemRoleIds = React.useMemo(
        () =>
            new Set(
                (rolesQuery.data ?? [])
                    .filter((role) => role.system)
                    .map((role) => role.id),
            ),
        [rolesQuery.data],
    )

    const rows = React.useMemo(() => {
        const q = keyword.trim().toLowerCase()
        const all = adminsQuery.data ?? []
        if (!q) return all
        return all.filter((account) =>
            [
                account.account,
                account.name,
                ...(canReadRoles
                    ? account.role_ids.map((id) => roleNameById.get(id) ?? "")
                    : []),
            ]
                .join(" ")
                .toLowerCase()
                .includes(q),
        )
    }, [adminsQuery.data, canReadRoles, keyword, roleNameById])

    const columns = React.useMemo<ColumnDef<AdminAccount>[]>(() => {
        const actionsColumn: ColumnDef<AdminAccount> = {
            id: "actions",
            header: "操作",
            meta: { label: "操作", width: "status", align: "end" },
            enableSorting: false,
            enableHiding: false,
            cell: ({ row }) => {
                const account = row.original
                const segment = toAutomationIdSegment(account.id)
                const hasSystemRole = account.role_ids.some((id) =>
                    systemRoleIds.has(id),
                )
                const statusDisabledReason = hasSystemRole
                    ? "系统角色账号不可停用或启用"
                    : !rolesQuery.isSuccess
                      ? "角色尚未读取，暂不能变更账号状态"
                      : account.status === "active" &&
                          currentUserId === account.id
                        ? "不能停用当前登录账号"
                        : undefined
                const deleteDisabledReason = hasSystemRole
                    ? "系统角色账号不可删除"
                    : !rolesQuery.isSuccess
                      ? "角色尚未读取，暂不能删除"
                      : undefined
                const actions: TableRowAction[] = []
                if (canUpdateAdmin && canReadRoles) {
                    actions.push({
                        id: `governance-admin-accounts-row-${segment}-toggle-status`,
                        label: account.status === "active" ? "停用" : "启用",
                        icon:
                            account.status === "active"
                                ? BanIcon
                                : CircleCheckIcon,
                        disabled: Boolean(statusDisabledReason),
                        disabledReason: statusDisabledReason,
                        onClick: () => setTogglingAccount(account),
                    })
                }
                if (canDeleteAdmin && canReadRoles) {
                    actions.push({
                        id: `governance-admin-accounts-row-${segment}-delete`,
                        label: "删除",
                        icon: Trash2Icon,
                        destructive: true,
                        placement: "menu",
                        disabled: Boolean(deleteDisabledReason),
                        disabledReason: deleteDisabledReason,
                        onClick: () => setDeletingAccount(account),
                    })
                }
                return (
                    <TableRowActions
                        moreId={`governance-admin-accounts-row-${segment}-more`}
                        moreLabel={`${account.name || account.account} 更多操作`}
                        actions={actions}
                    />
                )
            },
        }
        const departmentColumn: ColumnDef<AdminAccount> = {
            id: "department",
            header: "所属部门",
            meta: { width: "default" },
            cell: ({ row }) => {
                const organization = organizationQuery.data
                const person = organization?.people.find(
                    (item) => item.id === row.original.id,
                )
                return (
                    <TableAsyncCell
                        pending={!organization && !organizationQuery.isError}
                        failed={organizationQuery.isError}
                        pendingLabel="正在加载…"
                        failedLabel="部门加载失败"
                    >
                        {!organization || !person ? (
                            <TableEmptyCell
                                variant="label"
                                label="不在可查看范围"
                            />
                        ) : person.own_org_unit_id ? (
                            <TableTextCell
                                value={unitLabel(
                                    organization.units,
                                    person.own_org_unit_id,
                                )}
                                clamp={1}
                            />
                        ) : (
                            <TableEmptyCell
                                variant="label"
                                label="未分配部门"
                                className="text-warning"
                            />
                        )}
                    </TableAsyncCell>
                )
            },
        }
        const rolesColumn: ColumnDef<AdminAccount> = {
            id: "roles",
            header: "角色",
            meta: { width: "reference" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.role_ids
                        .map((id) => roleNameById.get(id) ?? "角色信息待确认")
                        .join("、")}
                    clamp={2}
                    className="whitespace-normal"
                />
            ),
        }
        return [
            {
                id: "identity",
                enableHiding: false,
                header: "人员 / 登录账号",
                meta: { width: "flex" },
                cell: ({ row: { original: account } }) => (
                    <TableIdentityCell
                        primary={
                            <Link
                                id={`account-open-${toAutomationIdSegment(account.id)}`}
                                className="text-primary hover:underline"
                                href={`/system/accounts/${encodeURIComponent(account.id)}`}
                            >
                                {account.name || account.account}
                            </Link>
                        }
                        primaryTitle={account.name || account.account}
                        secondary={account.account}
                        secondaryTitle={account.account}
                        numericSecondary
                    />
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        appearance={
                            row.original.status === "active" ? "quiet" : "badge"
                        }
                        {...adminAccountStatusBadge(row.original.status)}
                    />
                ),
            },
            ...(canReadOrganization ? [departmentColumn] : []),
            ...(canReadRoles ? [rolesColumn] : []),
            {
                id: "createdAt",
                header: "创建时间",
                meta: { width: "default" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={formatDateTime(
                            new Date(
                                row.original.created_at * 1000,
                            ).toISOString(),
                            "full",
                        )}
                        numeric
                        clamp={1}
                        className="text-muted-foreground"
                    />
                ),
            },
            ...(canReadRoles && (canUpdateAdmin || canDeleteAdmin)
                ? [actionsColumn]
                : []),
        ]
    }, [
        roleNameById,
        canReadOrganization,
        canReadRoles,
        organizationQuery.data,
        organizationQuery.isError,
        systemRoleIds,
        rolesQuery.isSuccess,
        currentUserId,
        canUpdateAdmin,
        canDeleteAdmin,
    ])

    const hasSearch = keyword.trim().length > 0
    const hasPendingChanges = searchDraft.trim() !== keyword.trim()
    const appliedChips = hasSearch
        ? [{ key: "q", label: `搜索：${keyword.trim()}` }]
        : []

    const applyFilters = React.useCallback(() => {
        const next = searchDraft.trim()
        setSearchDraft(next)
        setKeyword(next)
        router.replace(
            `/system/accounts${next ? `?q=${encodeURIComponent(next)}` : ""}`,
            { scroll: false },
        )
    }, [searchDraft, router, setSearchDraft])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setKeyword("")
        router.replace("/system/accounts", { scroll: false })
    }, [router, setSearchDraft])

    return (
        <ListWorkspacePage
            title="组织与人员"
            views={
                canReadOrganization ||
                hasPermission(permissions, "admin:list") ? (
                    <div className="min-w-0 [&_[data-slot=tabs-list]]:border-b-0 md:[&_[data-slot=tabs-trigger]]:min-h-12">
                        <PeopleNavigation current="accounts" />
                    </div>
                ) : undefined
            }
            actions={[
                {
                    actionKey: "permission-config",
                    id: "governance-admin-accounts-permission-config",
                    type: "button",
                    label: "角色与权限",
                    icon: ShieldCheckIcon,
                    tier: "quiet",
                    hidden: !canReadRoles,
                    onClick: () => router.push("/system/access-audit"),
                },
                {
                    actionKey: "create",
                    id: "governance-admin-accounts-create",
                    type: "button",
                    label: "新建账号",
                    icon: PlusIcon,
                    tier: "primary",
                    hidden: !showCreateAdmin,
                    disabled: !canOpenCreateDialog,
                    onClick: () => setCreating(true),
                },
            ]}
            notices={
                notice ? (
                    <p
                        role="status"
                        className="rounded-lg bg-muted p-3 text-sm"
                    >
                        {notice}
                    </p>
                ) : null
            }
            alerts={
                <>
                    {canReadOrganization && organizationQuery.isError ? (
                        <BusinessFailureState
                            id="accounts-organization-retry"
                            title="部门信息加载失败"
                            error={organizationQuery.error}
                            onRetry={() => void organizationQuery.refetch()}
                        />
                    ) : null}
                    {assignableRolesQuery.isError && showCreateAdmin ? (
                        <BusinessFailureState
                            id="accounts-role-options-retry"
                            title="新建账号所需角色加载失败"
                            error={assignableRolesQuery.error}
                            onRetry={() => void assignableRolesQuery.refetch()}
                        />
                    ) : null}
                </>
            }
            surface={{
                ariaLabel: "账号列表",
                toolbar: (
                    <ListWorkspaceFilterBar
                        morePresentation="popover"
                        idPrefix="governance-admin-accounts"
                        formAriaLabel="账号查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id="governance-admin-accounts-search"
                                searchInputRef={searchInputRef}
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder={
                                    canReadRoles
                                        ? "账号、姓名或角色"
                                        : "账号或姓名"
                                }
                                aria-label="搜索账号"
                            />
                        }
                        resultStatus={
                            adminsQuery.isPending || adminsQuery.isError
                                ? listWorkspaceFilterStatusText({
                                      loading: adminsQuery.isPending,
                                      failed: adminsQuery.isError,
                                      noun: "个账号",
                                  })
                                : undefined
                        }
                        chips={appliedChips}
                        onClearChip={clearAllFilters}
                        onClearAll={clearAllFilters}
                        hasPendingChanges={hasPendingChanges}
                    />
                ),
                table: (
                    <ListDataTable
                        id="governance-admin-accounts-table"
                        entityName="登录账号"
                        columns={columns}
                        data={rows}
                        getRowId={(row) => row.id}
                        rowLabel={(row) =>
                            `打开${row.name || row.account}的人员资料`
                        }
                        onRowOpen={(row) =>
                            router.push(
                                `/system/accounts/${encodeURIComponent(row.id)}`,
                            )
                        }
                        rowCount={rows.length}
                        loading={
                            adminsQuery.isPending || adminsQuery.isFetching
                        }
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                        isError={adminsQuery.isError}
                        error={adminsQuery.error}
                        retryButtonId="governance-admin-accounts-retry"
                        onRetry={() => void adminsQuery.refetch()}
                        hasActiveFilters={hasSearch}
                        onClearFilters={clearAllFilters}
                        canCreate={canOpenCreateDialog}
                        createLabel="新建账号"
                        onCreate={() => setCreating(true)}
                        emptyCopy={{
                            filterDescription: "没有账号符合当前搜索条件。",
                            emptyDescription: canOpenCreateDialog
                                ? "点击「新建账号」创建第一条账号记录。"
                                : undefined,
                        }}
                    />
                ),
            }}
        >
            {creating && showCreateAdmin ? (
                <AccountFormDialog
                    onCreated={(accountName) => {
                        setNotice("账号已创建，正在打开人员资料继续配置。")
                        void adminsQuery
                            .refetch()
                            .then((result) => {
                                const created = result.data?.find(
                                    (row) => row.account === accountName,
                                )
                                if (created)
                                    router.push(
                                        `/system/accounts/${encodeURIComponent(created.id)}`,
                                    )
                                else
                                    setNotice(
                                        "账号已创建。请刷新列表并点击姓名继续配置，无需重复创建账号。",
                                    )
                            })
                            .catch(() =>
                                setNotice(
                                    "账号已创建，请刷新列表后点击姓名继续配置。",
                                ),
                            )
                    }}
                    mode="create"
                    account={null}
                    roleOptions={assignableRolesQuery.data ?? []}
                    onOpenChange={(open) => {
                        if (!open) setCreating(false)
                    }}
                />
            ) : null}
            {togglingAccount ? (
                <ToggleAdminStatusDialog
                    account={togglingAccount}
                    onOpenChange={(open) => {
                        if (!open) {
                            setTogglingAccount(null)
                            requestAnimationFrame(() =>
                                restoreListRowFocus(togglingAccount.id),
                            )
                        }
                    }}
                    onChanged={(status) =>
                        setNotice(
                            status === "suspended"
                                ? "账号已停用。"
                                : "账号已启用。",
                        )
                    }
                />
            ) : null}
            {deletingAccount ? (
                <DeleteAdminDialog
                    account={deletingAccount}
                    onOpenChange={(open) => {
                        if (!open) {
                            setDeletingAccount(null)
                            requestAnimationFrame(() =>
                                restoreListRowFocus(deletingAccount.id),
                            )
                        }
                    }}
                    onDeleted={() => setNotice("账号已删除。")}
                />
            ) : null}
        </ListWorkspacePage>
    )
}
