"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    ListDataTable,
    ListWorkspaceFilterBar,
    ListWorkspacePage,
    ListWorkspaceViews,
    MoneyValue,
    TableIdentityCell,
    TableRowActions,
    listWorkspaceStyles,
    type ColumnDef,
    type ListWorkspaceFilterChip,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    SettlementPartySearchCombobox,
    SupplierSearchCombobox,
} from "@/features/entity-selectors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"
import { listFilterText } from "@/lib/ui-text"

import { useDepositAccounts } from "../hooks/queries"
import type {
    AccountRow,
    DepositAccountListFilters,
    DepositAccountStatusFilter,
} from "../types"
import { DepositCommandDialog } from "./command-dialog"

const PREFIX = "supplier-deposits"
const ACCOUNTS_LABEL = "预存款账户"
const CREATE_LABEL = "新增账户"
const AMOUNT_COLUMN = { align: "end", numeric: true, width: "amount" } as const
const SCALE = { maxScale: 2, outputScale: 2 } as const

const STATUS_VIEWS: ReadonlyArray<{
    value: DepositAccountStatusFilter | "all"
    label: string
}> = [
    { value: "all", label: "全部账户" },
    { value: "reconciliation_required", label: "待核对" },
    { value: "available", label: "可使用" },
    { value: "disabled", label: "已停用" },
]

function parseStatus(
    value: string | null,
): DepositAccountStatusFilter | undefined {
    if (
        value === "available" ||
        value === "reconciliation_required" ||
        value === "disabled"
    ) {
        return value
    }
    return undefined
}

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
        meta: { label: "状态", width: "status" },
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
        id: "available-balance",
        header: "可用余额",
        meta: { ...AMOUNT_COLUMN, label: "可用余额" },
        cell: ({
            row: {
                original: { account },
            },
        }) => {
            const available = subtractFixed(
                subtractFixed(
                    account.book_balance,
                    account.reserved_amount,
                    SCALE,
                ),
                account.unavailable_amount,
                SCALE,
            )
            return (
                <MoneyValue
                    value={available}
                    display="exact"
                    className="font-semibold"
                />
            )
        },
    },
    {
        id: "book-balance",
        header: "账面余额",
        meta: { ...AMOUNT_COLUMN, label: "账面余额" },
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
        meta: { ...AMOUNT_COLUMN, label: "已占用" },
        cell: ({
            row: {
                original: { account },
            },
        }) => {
            const hasUnavailable =
                compareDecimal(account.unavailable_amount, "0", 2) > 0
            return (
                <div className="flex flex-col items-end">
                    <MoneyValue
                        value={account.reserved_amount}
                        display="exact"
                    />
                    {hasUnavailable ? (
                        <span className="text-xs text-muted-foreground">
                            含不可用{" "}
                            <MoneyValue
                                value={account.unavailable_amount}
                                display="exact"
                            />
                        </span>
                    ) : null}
                </div>
            )
        },
    },
    {
        id: "actions",
        header: "操作",
        enableHiding: false,
        meta: { label: "操作", align: "end", width: "status" },
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
    "available-balance",
    "book-balance",
    "reserved-amount",
    "actions",
]

export function SupplierDepositsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const params = useSearchParams()
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const canList = hasPermission(permissions, "supplier_deposit:list")
    const canCreate = hasPermission(permissions, "supplier_deposit:create")

    const page = Math.max(
        1,
        Number.parseInt(params.get("page") ?? "1", 10) || 1,
    )
    const supplierId = params.get("supplierId") ?? ""
    const payerPartyId = params.get("payerPartyId") ?? ""
    const status = parseStatus(params.get("status"))

    const [supplierDraft, setSupplierDraft] = React.useState(supplierId)
    const [payerDraft, setPayerDraft] = React.useState(payerPartyId)
    const [supplierLabel, setSupplierLabel] = React.useState<string>()
    const [payerLabel, setPayerLabel] = React.useState<string>()
    const [createOpen, setCreateOpen] = React.useState(false)

    React.useEffect(() => {
        setSupplierDraft(supplierId)
        setPayerDraft(payerPartyId)
    }, [payerPartyId, supplierId])

    const patchUrl = React.useCallback(
        (next: {
            supplierId?: string | null
            payerPartyId?: string | null
            status?: DepositAccountStatusFilter | null
            page?: number | null
        }) => {
            const url = new URLSearchParams(params.toString())
            const apply = (key: string, value: string | null | undefined) => {
                if (value === undefined) return
                if (value) url.set(key, value)
                else url.delete(key)
            }
            apply("supplierId", next.supplierId)
            apply("payerPartyId", next.payerPartyId)
            apply("status", next.status)
            if (next.page !== undefined) {
                if (next.page && next.page > 1)
                    url.set("page", String(next.page))
                else url.delete("page")
            }
            const qs = url.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [params, pathname, router],
    )

    const filters: DepositAccountListFilters = {
        page,
        supplierId: supplierId || undefined,
        payerPartyId: payerPartyId || undefined,
        status,
    }
    const query = useDepositAccounts(filters, canList)
    const rows = query.data?.items ?? []
    const openCreate = () => setCreateOpen(true)
    const showList = !profile.isPending && !profile.isError && canList

    const hasActiveFilters = Boolean(supplierId || payerPartyId || status)
    const hasPendingChanges =
        supplierDraft !== supplierId || payerDraft !== payerPartyId

    const applyFilters = () => {
        patchUrl({
            supplierId: supplierDraft || null,
            payerPartyId: payerDraft || null,
            page: 1,
        })
    }

    const clearFilters = () => {
        setSupplierDraft("")
        setPayerDraft("")
        patchUrl({
            supplierId: null,
            payerPartyId: null,
            status: null,
            page: 1,
        })
    }

    const chips: ListWorkspaceFilterChip[] = []
    if (supplierId && supplierDraft !== supplierId) {
        chips.push({
            key: "supplierId",
            label: `供应商：${supplierLabel ?? "已选供应商"}`,
            onClear: () => {
                setSupplierDraft("")
                patchUrl({ supplierId: null, page: 1 })
            },
        })
    }
    if (payerPartyId && payerDraft !== payerPartyId) {
        chips.push({
            key: "payerPartyId",
            label: `付款主体：${payerLabel ?? "已选主体"}`,
            onClear: () => {
                setPayerDraft("")
                patchUrl({ payerPartyId: null, page: 1 })
            },
        })
    }

    const views = showList ? (
        <ListWorkspaceViews
            variant="inline"
            ariaLabel="预存款账户状态视图"
            items={STATUS_VIEWS.map((item) => ({
                id: `${PREFIX}-view-${toAutomationIdSegment(item.value)}`,
                label: item.label,
                active: (status ?? "all") === item.value,
                onClick: () =>
                    patchUrl({
                        status: item.value === "all" ? null : item.value,
                        page: 1,
                    }),
            }))}
        />
    ) : undefined

    const toolbar = (
        <ListWorkspaceFilterBar
            idPrefix={`${PREFIX}-toolbar`}
            formAriaLabel="供应商预存款筛选"
            onSubmit={applyFilters}
            queryButtonId={`${PREFIX}-toolbar-apply`}
            clearButtonId={`${PREFIX}-toolbar-clear`}
            search={null}
            className="[&_[data-slot=list-toolbar-filters]]:min-w-0 [&_[data-slot=list-toolbar-filters]]:shrink [&_[data-slot=list-toolbar-filters]]:self-center"
            primaryFilters={
                <>
                    <div className="w-56 max-w-full min-w-0">
                        <SupplierSearchCombobox
                            id={`${PREFIX}-filter-supplier`}
                            className="w-full min-w-0"
                            filterLabel="供应商"
                            purpose="filter"
                            value={supplierDraft || undefined}
                            onValueChange={(id) => setSupplierDraft(id ?? "")}
                            onItemChange={(item) =>
                                setSupplierLabel(item?.supplierName)
                            }
                            placeholder="全部"
                            aria-label="筛选供应商"
                        />
                    </div>
                    <div className="w-56 max-w-full min-w-0">
                        <SettlementPartySearchCombobox
                            id={`${PREFIX}-filter-payer`}
                            className="w-full min-w-0"
                            filterLabel="付款主体"
                            purpose="filter"
                            value={payerDraft || undefined}
                            onValueChange={(id) => setPayerDraft(id ?? "")}
                            onItemChange={(item) =>
                                setPayerLabel(item?.displayName)
                            }
                            placeholder="全部"
                            aria-label="筛选付款主体"
                        />
                    </div>
                </>
            }
            extraPrimary={
                hasActiveFilters && chips.length === 0 ? (
                    <Button
                        id={`${PREFIX}-toolbar-clear-all`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={listWorkspaceStyles.quietButton}
                        onClick={clearFilters}
                    >
                        {listFilterText.clear}
                    </Button>
                ) : undefined
            }
            chips={chips}
            onClearAll={clearFilters}
            hasPendingChanges={hasPendingChanges}
            resultStatus={
                query.isError
                    ? "查询未完成"
                    : query.isFetching
                      ? "查询中…"
                      : null
            }
        />
    )

    // 接口仅返回是否有下一页，不暴露未授权账户总数。
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
                hasActiveFilters={hasActiveFilters}
                onClearFilters={clearFilters}
                clearFiltersButtonId={`${PREFIX}-empty-clear-filters`}
                emptyCopy={{
                    filterTitle: "无匹配预存款账户",
                    filterDescription:
                        "请更换供应商、付款主体或状态视图后重试。",
                    emptyTitle: "暂无预存款账户",
                    emptyDescription:
                        "新增账户后，按充值申请、审批、银行付款和到账核对逐步登记资金。",
                }}
                canCreate={canCreate}
                onCreate={openCreate}
                createLabel={CREATE_LABEL}
                onRowOpen={(row) =>
                    router.push(
                        `/finance/supplier-deposits/${encodeURIComponent(row.account.id)}`,
                    )
                }
            />
            {!query.isError && (rows.length > 0 || page > 1) ? (
                <div
                    data-slot="data-table-pagination"
                    className="mt-auto flex shrink-0 flex-wrap items-center justify-between gap-2 border-t border-border/60 bg-card px-(--scaffold-px) py-2.5 text-xs text-muted-foreground"
                >
                    <span className="num">
                        本页 {rows.length} 条
                        {!query.data?.has_more && page === 1
                            ? " · 已全部显示"
                            : null}
                    </span>
                    {(page > 1 || query.data?.has_more) && (
                        <div
                            role="group"
                            aria-label="翻页"
                            className="ml-auto flex flex-wrap items-center gap-2"
                        >
                            <span className="num">第 {page} 页</span>
                            <Button
                                id={`${PREFIX}-previous`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={page <= 1 || query.isFetching}
                                onClick={() => patchUrl({ page: page - 1 })}
                            >
                                上一页
                            </Button>
                            <Button
                                id={`${PREFIX}-next`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={
                                    !query.data?.has_more || query.isFetching
                                }
                                onClick={() => patchUrl({ page: page + 1 })}
                            >
                                下一页
                            </Button>
                        </div>
                    )}
                </div>
            ) : null}
        </>
    )

    return (
        <ListWorkspacePage
            title="供应商预存款"
            views={views}
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
                          toolbar,
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
