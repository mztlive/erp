"use client"

import * as React from "react"

import {
    BusinessFailureState,
    DocumentSection,
    MoneyValue,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { downloadFileAsset } from "@/features/file-assets/api/file-assets"
import type { SalesExtraCost } from "@/features/sales-orders/api/sales-order-extra-costs"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    ReduceExtraCostDialog,
    RegisterExtraCostDialog,
} from "@/features/sales-orders/components/sales-order-extra-cost-dialogs"
import { useSalesExtraCostsQuery } from "@/features/sales-orders/hooks/queries"
import {
    EXTRA_COST_CATEGORY_LABEL,
    formatOccurredDate,
    formatTaxRate,
} from "@/features/sales-orders/lib/extra-cost"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

/**
 * 销售单「附加费用」分区：本单由我方承担、不对应销售明细供给的费用。
 * 需 `cost_entry:list` 才读取；`cost_entry:create` 才能登记与冲减。
 */
export function ExtraCostPanel({ order }: { order: SalesOrderDetailView }) {
    const account = useAccountProfileQuery()
    const granted = account.data?.permissions
    const canCreate = hasPermission(granted, "cost_entry:create")
    const canDownload = hasPermission(granted, "file_asset:preview")
    const query = useSalesExtraCostsQuery(order.id, true)
    const [registerOpen, setRegisterOpen] = React.useState(false)
    const [reducing, setReducing] = React.useState<SalesExtraCost | null>(null)
    const lineNames = new Map(
        order.lineItems.map((line) => [line.id, line.name]),
    )
    const effective = order.currentRevisionNo != null

    if (query.isPending) {
        return (
            <div
                className="h-48 animate-pulse rounded-lg bg-muted"
                aria-busy="true"
                aria-label="加载本单附加费用"
            />
        )
    }
    if (query.isError) {
        return (
            <BusinessFailureState
                id="sales-orders-extra-cost-retry"
                title="附加费用加载失败"
                error={query.error}
                onRetry={() => {
                    void query.refetch()
                }}
            />
        )
    }
    const list = query.data
    return (
        <div className="space-y-6">
            <div className="grid grid-cols-2 gap-4 rounded-lg border bg-muted/30 px-4 py-3 sm:max-w-md">
                <div>
                    <p className="text-xs text-muted-foreground">
                        已发生（扣除冲减）
                    </p>
                    <MoneyValue
                        value={list.occurredGrossTotal}
                        size="summary"
                    />
                </div>
                <div>
                    <p className="text-xs text-muted-foreground">待发生</p>
                    <MoneyValue value={list.pendingGrossTotal} size="summary" />
                </div>
            </div>
            <DocumentSection
                title="费用明细"
                className="border-0 py-0 [&_h2]:text-sm [&>div:last-child]:pt-3"
                description="已发生的费用计入本单实际经营盈亏；待发生的采购费用确认发生后才计入。"
                action={
                    canCreate && effective ? (
                        <Button
                            id="sales-orders-extra-cost-register"
                            type="button"
                            size="sm"
                            onClick={() => setRegisterOpen(true)}
                        >
                            登记费用
                        </Button>
                    ) : null
                }
            >
                {list.items.length === 0 ? (
                    <p className="py-6 text-sm text-muted-foreground">
                        本单还没有附加费用。
                    </p>
                ) : (
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>费用</TableHead>
                                <TableHead>归属</TableHead>
                                <TableHead className="text-right">
                                    含税金额
                                </TableHead>
                                <TableHead>状态</TableHead>
                                <TableHead className="text-right">
                                    操作
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {list.items.map((item) => (
                                <ExtraCostRow
                                    key={`${item.source}-${item.id}`}
                                    item={item}
                                    lineName={
                                        item.salesOrderLineId
                                            ? (lineNames.get(
                                                  item.salesOrderLineId,
                                              ) ?? "明细已移除")
                                            : "整张销售单"
                                    }
                                    canReduce={canCreate}
                                    canDownload={canDownload}
                                    onReduce={() => setReducing(item)}
                                />
                            ))}
                        </TableBody>
                    </Table>
                )}
            </DocumentSection>
            <RegisterExtraCostDialog
                order={order}
                open={registerOpen}
                onOpenChange={setRegisterOpen}
            />
            <ReduceExtraCostDialog
                order={order}
                cost={reducing}
                onOpenChange={(open) => {
                    if (!open) setReducing(null)
                }}
            />
        </div>
    )
}

function ExtraCostRow({
    item,
    lineName,
    canReduce,
    canDownload,
    onReduce,
}: {
    item: SalesExtraCost
    lineName: string
    canReduce: boolean
    canDownload: boolean
    onReduce: () => void
}) {
    const key = toAutomationIdSegment(`${item.source}-${item.id}`)
    const reducible =
        canReduce &&
        item.source === "registration" &&
        compareDecimal(item.remainingGrossAmount, "0", 2) > 0
    return (
        <>
            <TableRow>
                <TableCell className="min-w-56 whitespace-normal">
                    <div className="font-medium">
                        {EXTRA_COST_CATEGORY_LABEL[item.category]}：
                        {item.description}
                    </div>
                    <div className="text-xs break-all text-muted-foreground">
                        {item.source === "registration"
                            ? "财务登记"
                            : `采购单 ${item.purchaseNo ?? "—"}`}
                    </div>
                </TableCell>
                <TableCell className="whitespace-nowrap">{lineName}</TableCell>
                <TableCell className="text-right whitespace-nowrap">
                    <MoneyValue value={item.grossAmount} />
                    <p className="text-xs text-muted-foreground">
                        税率 {formatTaxRate(item.inputTaxRate)}
                        {item.reductions.length > 0
                            ? ` · 已冲减 ${item.reducedGrossAmount}`
                            : ""}
                    </p>
                </TableCell>
                <TableCell className="whitespace-nowrap">
                    {item.status === "occurred" ? (
                        <Badge variant="secondary">
                            已发生 · {formatOccurredDate(item.occurredAt)}
                        </Badge>
                    ) : (
                        <Badge variant="warning">待发生</Badge>
                    )}
                </TableCell>
                <TableCell className="text-right">
                    <div className="flex justify-end gap-2">
                        {canDownload && item.evidenceAttachmentId ? (
                            <EvidenceButton
                                id={`sales-orders-extra-cost-evidence-${key}`}
                                assetId={item.evidenceAttachmentId}
                                fileName={`${item.description}-凭证`}
                            />
                        ) : null}
                        {reducible ? (
                            <Button
                                id={`sales-orders-extra-cost-reduce-${key}`}
                                type="button"
                                size="sm"
                                variant="secondary"
                                onClick={onReduce}
                            >
                                冲减
                            </Button>
                        ) : null}
                    </div>
                </TableCell>
            </TableRow>
            {item.reductions.map((reduction) => (
                <TableRow
                    key={reduction.costEntryId}
                    className="bg-muted/30 text-muted-foreground"
                >
                    <TableCell
                        className="min-w-56 whitespace-normal"
                        colSpan={2}
                    >
                        冲减：{reduction.description}
                    </TableCell>
                    <TableCell className="text-right whitespace-nowrap">
                        −<MoneyValue value={reduction.grossAmount} />
                    </TableCell>
                    <TableCell className="whitespace-nowrap">
                        {formatOccurredDate(reduction.occurredAt)}
                    </TableCell>
                    <TableCell className="text-right">
                        {canDownload && reduction.evidenceAttachmentId ? (
                            <EvidenceButton
                                id={`sales-orders-extra-cost-evidence-${toAutomationIdSegment(reduction.costEntryId)}`}
                                assetId={reduction.evidenceAttachmentId}
                                fileName={`${reduction.description}-冲减凭证`}
                            />
                        ) : null}
                    </TableCell>
                </TableRow>
            ))}
        </>
    )
}

function EvidenceButton({
    id,
    assetId,
    fileName,
}: {
    id: string
    assetId: string
    fileName: string
}) {
    const [pending, setPending] = React.useState(false)
    const [failed, setFailed] = React.useState(false)
    return (
        <Button
            id={id}
            type="button"
            size="sm"
            variant="ghost"
            disabled={pending}
            title={
                failed ? "凭证下载失败，请确认有文件查看权限后重试" : undefined
            }
            onClick={() => {
                setPending(true)
                setFailed(false)
                downloadFileAsset(assetId, fileName)
                    .catch(() => setFailed(true))
                    .finally(() => setPending(false))
            }}
        >
            {failed ? "重试凭证" : "凭证"}
        </Button>
    )
}
