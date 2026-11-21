"use client"

import { useState } from "react"
import { PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListWorkspacePage,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    type ColumnDef,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { subtractFixed } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

import { useDepositAccounts } from "../hooks/queries"
import type { AccountRow } from "../types"
import { DepositCommandDialog } from "./command-dialog"

const PREFIX = "supplier-deposits"
const ACCOUNTS_LABEL = "预存款账户"
const CREATE_LABEL = "新增账户"
const AMOUNT_COLUMN = { align: "end", numeric: true, width: "amount" } as const

const accountColumns: ColumnDef<AccountRow>[] = [
    {
        id: "name",
        header: "供应商 / 付款主体",
        enableHiding: false,
        meta: { label: "供应商 / 付款主体", width: "flex" },
        cell: ({ row: { original } }) => {
            const { supplier_name, payer_party_name, external_account_label } =
                original
            const secondary = [
                payer_party_name,
                "人民币",
                external_account_label,
            ]
                .filter(Boolean)
                .join(" · ")
            return (
                <TableIdentityCell
                    primary={supplier_name}
                    primaryTitle={supplier_name}
                    secondary={secondary}
                    secondaryTitle={secondary}
                />
            )
        },
    },
    {
        id: "status",
        header: "状态",
        meta: { width: "status" },
        cell: ({
            row: {
                original: { account },
            },
        }) => (
            <BusinessStatusBadge
                context="list"
                label={
                    !account.enabled
                        ? "已停用"
                        : account.reconciliation_required
                          ? "待核对"
                          : "可使用"
                }
                tone={
                    account.reconciliation_required
                        ? "destructive"
                        : account.enabled
                          ? "success"
                          : "neutral"
                }
                appearance={
                    account.enabled && !account.reconciliation_required
                        ? "quiet"
                        : "badge"
                }
            />
        ),
    },
    {
        id: "book-balance",
        header: "账面余额",
        meta: AMOUNT_COLUMN,
        cell: ({ row }) => (
            <MoneyValue
                value={row.original.account.book_balance}
                display="exact"
            />
        ),
    },
    {
        id: "reserved-amount",
        header: "已占用",
        meta: AMOUNT_COLUMN,
        cell: ({ row }) => (
            <MoneyValue
                value={row.original.account.reserved_amount}
                display="exact"
            />
        ),
    },
    {
        id: "available-balance",
        header: "可用余额",
        meta: AMOUNT_COLUMN,
        cell: ({
            row: {
                original: { account },
            },
        }) => {
            const scale = { maxScale: 2, outputScale: 2 }
            const available = subtractFixed(
                subtractFixed(
                    account.book_balance,
                    account.reserved_amount,
                    scale,
                ),
                account.unavailable_amount,
                scale,
            )
            return <MoneyValue value={available} display="exact" />
        },
    },
    {
        id: "actions",
        header: "操作",
        enableHiding: false,
        meta: { align: "end", width: "status" },
        cell: ({ row }) => {
            const accountId = row.original.account.id
            const openId = `${PREFIX}-open-${toAutomationIdSegment(accountId)}`
            return (
                <TableRowActions
                    moreId={`${openId}-more`}
                    moreLabel="账户更多操作"
                    actions={[
                        {
                            id: openId,
                            href: `/finance/supplier-deposits/${encodeURIComponent(accountId)}`,
                            label: "查看账户",
                        },
                    ]}
                />
            )
        },
    },
]
const accountColumnOrder = [
    "name",
    "status",
    "book-balance",
    "reserved-amount",
    "available-balance",
    "actions",
]

export function SupplierDepositsPage() {
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canList = hasPermission(permissions, "supplier_deposit:list")
    const canCreate = hasPermission(permissions, "supplier_deposit:create")
    const [page, setPage] = useState(1)
    const [createOpen, setCreateOpen] = useState(false)
    const query = useDepositAccounts(page, canList)
    const rows = query.data?.items ?? []
    const openCreate = () => setCreateOpen(true)
    const showList = !profile.isPending && !profile.isError && canList

    // 接口只返回是否有下一页，不提供总数。
    const table = (
        <>
            <ListDataTable
                id={`${PREFIX}-accounts-table`}
                caption={ACCOUNTS_LABEL}
                data={rows}
                columns={accountColumns}
                rowCount={rows.length}
                getRowId={(row) => row.account.id}
                rowLabel={(row) =>
                    `${row.supplier_name}（${row.payer_party_name}）`
                }
                defaultColumnOrder={accountColumnOrder}
                defaultColumnPinning={{ left: ["name"], right: ["actions"] }}
                scrollable
                showPagination={false}
                loading={query.isPending || query.isFetching}
                errorState={
                    query.isError ? (
                        <BusinessFailureState
                            idPrefix={`${PREFIX}-list`}
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    ) : undefined
                }
                emptyCopy={{
                    emptyTitle: "暂无预存款账户",
                    emptyDescription:
                        "新增账户后，按充值申请、审批、银行付款和到账核对逐步登记资金。",
                }}
                canCreate={canCreate}
                onCreate={openCreate}
                createLabel={CREATE_LABEL}
            />
            {!query.isError && rows.length > 0 ? (
                <div
                    data-slot="data-table-pagination"
                    className="flex flex-wrap items-center justify-between gap-2 text-muted-foreground"
                >
                    <span className="num">本页 {rows.length} 条</span>
                    <div
                        role="group"
                        aria-label="翻页"
                        className="ml-auto flex flex-wrap items-center gap-2"
                    >
                        <Button
                            id={`${PREFIX}-previous`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={page <= 1 || query.isFetching}
                            onClick={() => setPage((value) => value - 1)}
                        >
                            上一页
                        </Button>
                        <span className="num">第 {page} 页</span>
                        <Button
                            id={`${PREFIX}-next`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={!query.data?.has_more || query.isFetching}
                            onClick={() => setPage((value) => value + 1)}
                        >
                            下一页
                        </Button>
                    </div>
                </div>
            ) : null}
        </>
    )

    return (
        <ListWorkspacePage
            title="供应商预存款"
            loading={profile.isPending}
            actions={
                canCreate
                    ? [
                          {
                              id: `${PREFIX}-create`,
                              actionKey: "create",
                              label: CREATE_LABEL,
                              icon: PlusIcon,
                              tier: "primary",
                              onClick: openCreate,
                          },
                      ]
                    : undefined
            }
            alerts={
                profile.isError ? (
                    <BusinessFailureState
                        idPrefix={`${PREFIX}-profile`}
                        error={profile.error}
                        onRetry={() => void profile.refetch()}
                    />
                ) : !canList ? (
                    <BusinessFailureState
                        kind="permission"
                        description="当前账号不能查看供应商预存款。"
                    />
                ) : null
            }
            surface={
                showList
                    ? {
                          ariaLabel: ACCOUNTS_LABEL,
                          className: "@container/deposit-accounts",
                          tableClassName:
                              "flex min-w-0 flex-col @max-xl/deposit-accounts:[&_[data-column-pinned]]:inset-x-auto! @max-xl/deposit-accounts:[&_[data-pinned-edge]]:shadow-none!",
                          table,
                      }
                    : undefined
            }
        >
            {canCreate && createOpen && (
                <DepositCommandDialog
                    action="create"
                    open
                    onOpenChange={setCreateOpen}
                />
            )}
        </ListWorkspacePage>
    )
}
