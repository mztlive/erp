"use client"

import * as React from "react"
import { useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef } from "@tanstack/react-table"
import { PlusIcon, ShieldCheckIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { AccountPermissionsSheet } from "@/features/admin/components/accounts/account-permissions-sheet"
import { AccountFormDialog } from "@/features/admin/components/accounts/account-form-dialog"
import type { AccountDraft } from "@/features/admin/components/accounts/account-form-dialog"
import { DeleteAdminDialog } from "@/features/admin/components/accounts/delete-admin-dialog"
import {
    useAdminsQuery,
    useAssignableRolesQuery,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import type { AdminAccount } from "@/features/admin/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"

type AccountFormState = {
    mode: "create" | "edit"
    account: AccountDraft | null
}

/**
 * 账号管理：登录账号的新建、改资料与删除。
 *
 * 与「权限配置」分工：这里管账号本身（账号、姓名、密码），
 * 角色只做初始绑定；授权口径与有效权限解释在权限配置页。
 */
export function AccountsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()
    const adminsQuery = useAdminsQuery()
    const rolesQuery = useRolesQuery()
    const assignableRolesQuery = useAssignableRolesQuery()

    /** 权限配置页按角色跳转过来时（?q=角色名），首屏直接带上该筛选。 */
    const [keyword, setKeyword] = React.useState(
        () => searchParams.get("q") ?? "",
    )
    const [searchDraft, setSearchDraft] = React.useState(keyword)
    const [accountForm, setAccountForm] =
        React.useState<AccountFormState | null>(null)
    const [permissionAccount, setPermissionAccount] =
        React.useState<AdminAccount | null>(null)
    const [permissionsOpen, setPermissionsOpen] = React.useState(false)
    const permissionReturnId = React.useRef<string | null>(null)
    const editAfterPermissionsClose = React.useRef(false)

    const [deletingAccount, setDeletingAccount] = React.useState<{
        id: string
        account: string
    } | null>(null)

    const roleNameById = React.useMemo(
        () =>
            new Map(
                (rolesQuery.data ?? []).map((role) => [role.id, role.name]),
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
                ...account.role_ids.map((id) => roleNameById.get(id) ?? ""),
            ]
                .join(" ")
                .toLowerCase()
                .includes(q),
        )
    }, [adminsQuery.data, keyword, roleNameById])

    const columns = React.useMemo<ColumnDef<AdminAccount>[]>(
        () => [
            {
                id: "identity",
                size: 240,
                header: "账号",
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <div className="font-medium">
                            {row.original.name || row.original.account}
                        </div>
                        <div className="mt-1 text-xs text-muted-foreground">
                            {row.original.account}
                        </div>
                    </div>
                ),
            },
            {
                id: "roles",
                size: 300,
                header: "角色",
                cell: ({ row }) =>
                    row.original.role_ids
                        .map((id) => roleNameById.get(id) ?? "角色信息待确认")
                        .join("、") || "—",
            },
            {
                id: "createdAt",
                size: 190,
                header: "创建时间",
                cell: ({ row }) => (
                    <span className="num text-[13px] text-muted-foreground">
                        {formatDateTime(
                            new Date(
                                row.original.created_at * 1000,
                            ).toISOString(),
                            "full",
                        )}
                    </span>
                ),
            },
            {
                id: "actions",
                size: 220,
                minSize: 220,
                header: () => <span className="block text-right">操作</span>,
                cell: ({ row }) => {
                    const account = row.original
                    const protectedAccount = account.role_ids.some((id) =>
                        rolesQuery.data?.some(
                            (role) => role.id === id && role.system,
                        ),
                    )
                    const segment = toAutomationIdSegment(account.id)
                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                id={`governance-admin-accounts-row-${segment}-edit`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() =>
                                    setAccountForm({
                                        mode: "edit",
                                        account: {
                                            id: account.id,
                                            account: account.account,
                                            name: account.name,
                                            role_ids: [...account.role_ids],
                                        },
                                    })
                                }
                            >
                                编辑
                            </Button>
                            <Button
                                id={`governance-admin-accounts-row-${segment}-permissions`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                onClick={() => {
                                    permissionReturnId.current = `governance-admin-accounts-row-${segment}-permissions`
                                    setPermissionAccount(account)
                                    setPermissionsOpen(true)
                                }}
                            >
                                查看权限
                            </Button>
                            <Button
                                id={`governance-admin-accounts-row-${segment}-delete`}
                                disabled={protectedAccount}
                                title={
                                    protectedAccount
                                        ? "绑定系统角色的账号不可删除"
                                        : undefined
                                }
                                type="button"
                                size="xs"
                                variant="ghost"
                                className="text-destructive hover:bg-destructive/10 hover:text-destructive"
                                onClick={() =>
                                    setDeletingAccount({
                                        id: account.id,
                                        account: account.account,
                                    })
                                }
                            >
                                删除
                            </Button>
                        </div>
                    )
                },
            },
        ],
        [roleNameById, rolesQuery.data],
    )

    const hasSearch = keyword.trim().length > 0
    const hasPendingChanges = searchDraft.trim() !== keyword.trim()
    const appliedChips = hasSearch
        ? [{ key: "q", label: `搜索：${keyword.trim()}` }]
        : []

    const applyFilters = React.useCallback(() => {
        const next = searchDraft.trim()
        setSearchDraft(next)
        setKeyword(next)
    }, [searchDraft])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setKeyword("")
    }, [])

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="系统"
                title="账号管理"
                description="管理成员登录账号与角色分配。"
            >
                <div className="flex flex-wrap items-center gap-2">
                    <Button
                        id="governance-admin-accounts-permission-config"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => router.push("/system/access-audit")}
                    >
                        <ShieldCheckIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        权限配置
                    </Button>
                    <Button
                        id="governance-admin-accounts-create"
                        type="button"
                        size="sm"
                        onClick={() =>
                            setAccountForm({
                                mode: "create",
                                account: null,
                            })
                        }
                    >
                        <PlusIcon className="size-3.5" aria-hidden="true" />
                        新建账号
                    </Button>
                </div>
            </ListWorkspaceHeader>

            <ListWorkSurface
                ariaLabel="账号列表"
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix="governance-admin-accounts"
                        formAriaLabel="账号查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id="governance-admin-accounts-search"
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="账号、姓名或角色"
                                aria-label="搜索账号"
                            />
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: adminsQuery.isPending,
                            failed: adminsQuery.isError,
                            resultCount: adminsQuery.data
                                ? rows.length
                                : undefined,
                            noun: "个账号",
                            loadingLabel: "正在加载账号…",
                        })}
                        chips={appliedChips}
                        onClearChip={clearAllFilters}
                        onClearAll={clearAllFilters}
                        hasPendingChanges={hasPendingChanges}
                    />
                }
                table={
                    <DataTable
                        id="governance-admin-accounts-table"
                        columns={columns}
                        data={rows}
                        getRowId={(row) => row.id}
                        rowCount={rows.length}
                        layout="flush"
                        loading={adminsQuery.isPending}
                        defaultColumnPinning={{
                            left: ["identity"],
                            right: ["actions"],
                        }}
                        errorState={
                            adminsQuery.isError ? (
                                <BusinessFailureState
                                    error={adminsQuery.error}
                                    title="账号列表加载失败"
                                    action={
                                        <Button
                                            id="governance-admin-accounts-retry"
                                            type="button"
                                            variant="secondary"
                                            className="rounded-lg shadow-none"
                                            onClick={() =>
                                                void adminsQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !adminsQuery.isError && rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={hasSearch ? "filter" : "no-data"}
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        hasSearch
                                            ? "当前筛选无结果"
                                            : "还没有登录账号"
                                    }
                                    description={
                                        hasSearch
                                            ? "没有账号符合当前搜索条件。"
                                            : "点击「新建账号」创建第一条账号记录。"
                                    }
                                />
                            ) : undefined
                        }
                    />
                }
            />

            <AccountPermissionsSheet
                account={permissionAccount}
                open={permissionsOpen}
                onOpenChange={setPermissionsOpen}
                onAdjustRoles={() => {
                    editAfterPermissionsClose.current = true
                    setPermissionsOpen(false)
                }}
                onClosed={() => {
                    if (
                        editAfterPermissionsClose.current &&
                        permissionAccount
                    ) {
                        editAfterPermissionsClose.current = false
                        setAccountForm({
                            mode: "edit",
                            account: {
                                ...permissionAccount,
                                role_ids: [...permissionAccount.role_ids],
                            },
                        })
                    } else if (permissionReturnId.current) {
                        document
                            .getElementById(permissionReturnId.current)
                            ?.focus({ preventScroll: true })
                    }
                }}
            />

            {accountForm ? (
                <AccountFormDialog
                    key={
                        accountForm.mode === "edit"
                            ? (accountForm.account?.id ?? "edit")
                            : "create"
                    }
                    mode={accountForm.mode}
                    account={accountForm.account}
                    roleOptions={assignableRolesQuery.data ?? []}
                    onOpenChange={(open) => {
                        if (!open) setAccountForm(null)
                    }}
                />
            ) : null}

            {deletingAccount ? (
                <DeleteAdminDialog
                    key={deletingAccount.id}
                    account={deletingAccount}
                    onOpenChange={(open) => {
                        if (!open) setDeletingAccount(null)
                    }}
                />
            ) : null}
        </PageScaffold>
    )
}
