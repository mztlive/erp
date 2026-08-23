"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { ColumnDef } from "@tanstack/react-table"
import {
    MoreHorizontalIcon,
    PlusIcon,
    SearchIcon,
    ShieldCheckIcon,
    Trash2Icon,
} from "lucide-react"

import {
    BusinessFailureState,
    DataTable,
    ListToolbar,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Separator } from "@/components/ui/separator"
import { AccountFormDialog } from "@/features/admin/components/accounts/account-form-dialog"
import type { AccountDraft } from "@/features/admin/components/accounts/account-form-dialog"
import { DeleteAdminDialog } from "@/features/admin/components/accounts/delete-admin-dialog"
import {
    useAdminsQuery,
    useAssignableRolesQuery,
    useRolesQuery,
} from "@/features/admin/hooks/queries"
import type { AdminAccount } from "@/features/admin/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

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
    const adminsQuery = useAdminsQuery()
    const rolesQuery = useRolesQuery()
    const assignableRolesQuery = useAssignableRolesQuery()

    const [keyword, setKeyword] = React.useState("")
    const [accountForm, setAccountForm] =
        React.useState<AccountFormState | null>(null)
    const [deletingAccount, setDeletingAccount] = React.useState<{
        id: string
        account: string
    } | null>(null)

    const roleNameById = React.useMemo(
        () => new Map((rolesQuery.data ?? []).map((role) => [role.id, role.name])),
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
                header: "账号",
                cell: ({ row }) => (
                    <div className="min-w-[9rem]">
                        <div className="font-medium">
                            {row.original.name || row.original.account}
                        </div>
                        <div className="font-mono text-xs text-muted-foreground">
                            {row.original.account}
                        </div>
                    </div>
                ),
            },
            {
                id: "roles",
                header: "角色",
                cell: ({ row }) =>
                    row.original.role_ids
                        .map((id) => roleNameById.get(id) ?? id)
                        .join("、") || "—",
            },
            {
                id: "createdAt",
                header: "创建时间",
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {formatDateTime(
                            new Date(row.original.created_at * 1000).toISOString(),
                            "full",
                        )}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                cell: ({ row }) => {
                    const account = row.original
                    return (
                        <div className="flex items-center justify-end gap-1">
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
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
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    render={
                                        <Button
                                            type="button"
                                            size="icon-xs"
                                            variant="ghost"
                                            aria-label={`${account.account} 更多操作`}
                                        />
                                    }
                                >
                                    <MoreHorizontalIcon aria-hidden="true" />
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                    align="end"
                                    className="min-w-40"
                                >
                                    <DropdownMenuItem
                                        onClick={() =>
                                            router.push(
                                                `/system/access-audit?view=users&subjectType=USER&subjectId=${account.id}`,
                                            )
                                        }
                                    >
                                        <ShieldCheckIcon aria-hidden="true" />
                                        查看有效权限
                                    </DropdownMenuItem>
                                    <DropdownMenuItem
                                        variant="destructive"
                                        onClick={() =>
                                            setDeletingAccount({
                                                id: account.id,
                                                account: account.account,
                                            })
                                        }
                                    >
                                        <Trash2Icon aria-hidden="true" />
                                        删除
                                    </DropdownMenuItem>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        </div>
                    )
                },
            },
        ],
        [roleNameById, router],
    )

    return (
        <PageScaffold density="compact">
            <PageHeader
                title="账号管理"
                description="维护登录账号与初始角色绑定；授权口径与有效权限在权限配置页查看。"
                actions={
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => router.push("/system/access-audit")}
                        >
                            <ShieldCheckIcon
                                className="size-3.5"
                                aria-hidden="true"
                            />
                            权限配置
                        </Button>
                        <Button
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
                }
            />

            <div className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}>
                <div className="flex flex-wrap items-center gap-2 px-3 py-2.5">
                    <div className="min-w-[16rem] flex-1">
                        <ListToolbar
                            search={
                                <InputGroup>
                                    <InputGroupAddon>
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        value={keyword}
                                        onChange={(event) =>
                                            setKeyword(event.target.value)
                                        }
                                        placeholder="账号、姓名或角色"
                                        aria-label="搜索账号"
                                    />
                                </InputGroup>
                            }
                        />
                    </div>
                    <span
                        className="shrink-0 text-xs text-muted-foreground"
                        aria-live="polite"
                    >
                        共 {rows.length} 条
                    </span>
                </div>
                <Separator />
                <div data-slot="business-table-frame-table">
                    {adminsQuery.isError ? (
                        <BusinessFailureState
                            error={adminsQuery.error}
                            title="账号列表加载失败"
                            action={
                                <Button
                                    type="button"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() => void adminsQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : (
                        <DataTable
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
                        />
                    )}
                </div>
            </div>

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
