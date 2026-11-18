"use client"

import { useState } from "react"
import Link from "next/link"
import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessTableFrame,
    MoneyValue,
    PageScaffold,
} from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Badge } from "@/components/ui/badge"
import {
    Table,
    TableHeader,
    TableHead,
    TableBody,
    TableRow,
    TableCell,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { subtractFixed } from "@/lib/fixed-decimal"
import { useDepositAccounts } from "../hooks/queries"
import { DepositCommandDialog } from "./command-dialog"

export function SupplierDepositsPage() {
    const profile = useAccountProfileQuery()
    const canList = hasPermission(
        profile.data?.permissions,
        "supplier_deposit:list",
    )
    const canCreate = hasPermission(
        profile.data?.permissions,
        "supplier_deposit:create",
    )
    const [page, setPage] = useState(1)
    const [createOpen, setCreateOpen] = useState(false)
    const query = useDepositAccounts(page, canList)
    const rows = query.data?.items ?? []
    return (
        <PageScaffold density="compact">
            <ListWorkspaceHeader
                eyebrow="财务"
                title="供应商预存款"
                description="按付款主体、供应商及资金账号管理已核实到账的余额。"
            >
                {canCreate && (
                    <Button
                        id="supplier-deposits-create"
                        onClick={() => setCreateOpen(true)}
                    >
                        新增账户
                    </Button>
                )}
            </ListWorkspaceHeader>
            {profile.isPending ? (
                <div aria-busy className="h-36 animate-pulse bg-muted" />
            ) : profile.isError ? (
                <BusinessFailureState
                    idPrefix="supplier-deposits-profile"
                    error={profile.error}
                    onRetry={() => void profile.refetch()}
                />
            ) : !canList ? (
                <BusinessFailureState
                    kind="permission"
                    description="当前账号不能查看供应商预存款。"
                />
            ) : query.isError ? (
                <BusinessFailureState
                    idPrefix="supplier-deposits-list"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isPending ? (
                <div aria-busy className="h-36 animate-pulse bg-muted" />
            ) : rows.length === 0 ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无预存款账户"
                    description="新增账户后，按充值申请、审批、银行付款和到账核对逐步登记资金。"
                />
            ) : (
                <BusinessTableFrame
                    title="预存款账户"
                    table={
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>供应商 / 付款主体</TableHead>
                                    <TableHead>状态</TableHead>
                                    <TableHead>账面余额</TableHead>
                                    <TableHead>已占用</TableHead>
                                    <TableHead>可用余额</TableHead>
                                    <TableHead>操作</TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {rows.map((row) => {
                                    const account = row.account
                                    const available = subtractFixed(
                                        subtractFixed(
                                            account.book_balance,
                                            account.reserved_amount,
                                            { maxScale: 2, outputScale: 2 },
                                        ),
                                        account.unavailable_amount,
                                        { maxScale: 2, outputScale: 2 },
                                    )
                                    return (
                                        <TableRow key={account.id}>
                                            <TableCell>
                                                <p className="font-medium">
                                                    {row.supplier_name}
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    {row.payer_party_name} ·
                                                    人民币
                                                    {row.external_account_label
                                                        ? ` · ${row.external_account_label}`
                                                        : ""}
                                                </p>
                                            </TableCell>
                                            <TableCell>
                                                <Badge
                                                    variant={
                                                        account.reconciliation_required
                                                            ? "destructive"
                                                            : "secondary"
                                                    }
                                                >
                                                    {!account.enabled
                                                        ? "已停用"
                                                        : account.reconciliation_required
                                                          ? "待核对"
                                                          : "可使用"}
                                                </Badge>
                                            </TableCell>
                                            <TableCell>
                                                <MoneyValue
                                                    value={account.book_balance}
                                                    display="exact"
                                                />
                                            </TableCell>
                                            <TableCell>
                                                <MoneyValue
                                                    value={
                                                        account.reserved_amount
                                                    }
                                                    display="exact"
                                                />
                                            </TableCell>
                                            <TableCell>
                                                <MoneyValue
                                                    value={available}
                                                    display="exact"
                                                />
                                            </TableCell>
                                            <TableCell>
                                                <Button
                                                    id={`supplier-deposits-open-${toAutomationIdSegment(account.id)}`}
                                                    variant="link"
                                                    render={
                                                        <Link
                                                            href={`/finance/supplier-deposits/${encodeURIComponent(account.id)}`}
                                                        />
                                                    }
                                                >
                                                    查看账户
                                                </Button>
                                            </TableCell>
                                        </TableRow>
                                    )
                                })}
                            </TableBody>
                        </Table>
                    }
                    footer={
                        <div className="flex flex-wrap items-center justify-end gap-3">
                            <Button
                                id="supplier-deposits-previous"
                                variant="outline"
                                disabled={page <= 1 || query.isFetching}
                                onClick={() => setPage((value) => value - 1)}
                            >
                                上一页
                            </Button>
                            <span className="text-sm">第 {page} 页</span>
                            <Button
                                id="supplier-deposits-next"
                                variant="outline"
                                disabled={
                                    !query.data?.has_more || query.isFetching
                                }
                                onClick={() => setPage((value) => value + 1)}
                            >
                                下一页
                            </Button>
                        </div>
                    }
                />
            )}
            {canCreate && createOpen && (
                <DepositCommandDialog
                    action="create"
                    open
                    onOpenChange={setCreateOpen}
                />
            )}
        </PageScaffold>
    )
}
