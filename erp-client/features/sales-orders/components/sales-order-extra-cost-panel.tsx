"use client"

import * as React from "react"

import {
    BusinessFailureState,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
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
            <DetailWorkspaceSection width="form">
                <div
                    className="h-36 animate-pulse rounded-md bg-muted/70"
                    aria-busy="true"
                    aria-label="加载本单附加费用"
                />
            </DetailWorkspaceSection>
        )
    }
    if (query.isError) {
        return (
            <DetailWorkspaceSection width="form">
                <BusinessFailureState
                    id="sales-orders-extra-cost-retry"
                    title="附加费用加载失败"
                    error={query.error}
                    onRetry={() => {
                        void query.refetch()
                    }}
                />
            </DetailWorkspaceSection>
        )
    }
    const list = query.data
    return (
        <>
            <DetailWorkspaceSection width="full">
                <DetailWorkspaceMetrics
                    columns={2}
                    variant="inline"
                    ariaLabel="附加费用摘要"
                    className="max-w-xl"
                >
                    <DetailWorkspaceMetricItem
                        emphasis="primary"
                        label="已发生（扣除冲减）"
                        value={<MoneyValue value={list.occurredGrossTotal} />}
                    />
                    <DetailWorkspaceMetricItem
                        label="待发生"
                        value={<MoneyValue value={list.pendingGrossTotal} />}
                    />
                </DetailWorkspaceMetrics>
            </DetailWorkspaceSection>

            <DetailWorkspaceSection
                width="full"
                title="费用明细"
                subtitle="已发生的费用计入本单实际经营盈亏；待发生的采购费用确认发生后才计入。"
                actions={
                    canCreate && effective ? (
                        <Button
                            id="sales-orders-extra-cost-register"
                            type="button"
                            size="sm"
                            className="px-3.5"
                            onClick={() => setRegisterOpen(true)}
                        >
                            登记费用
                        </Button>
                    ) : null
                }
            >
                {list.items.length === 0 ? (
                    <p className="py-4 text-xs text-muted-foreground">
                        本单还没有附加费用。
                    </p>
                ) : (
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.dataTableWrapper}
                    >
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHead>费用</TableHead>
                                    <TableHead>归属</TableHead>
                                    <TableHead data-align="end">
                                        含税金额
                                    </TableHead>
                                    <TableHead>状态</TableHead>
                                    <TableHead data-align="end">操作</TableHead>
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
                    </div>
                )}
            </DetailWorkspaceSection>

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
        </>
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
                    <div className="num text-xs break-all text-muted-foreground">
                        {item.source === "registration"
                            ? "财务登记"
                            : `采购单 ${item.purchaseNo ?? "—"}`}
                    </div>
                </TableCell>
                <TableCell className="whitespace-nowrap">{lineName}</TableCell>
                <TableCell data-align="end" className="whitespace-nowrap">
                    <MoneyValue value={item.grossAmount} />
                    <p className="num text-xs text-muted-foreground">
                        税率 {formatTaxRate(item.inputTaxRate)}
                        {item.reductions.length > 0
                            ? ` · 已冲减 ${item.reducedGrossAmount}`
                            : ""}
                    </p>
                </TableCell>
                <TableCell className="whitespace-nowrap">
                    {item.status === "occurred" ? (
                        <StatusBadge
                            tone="success"
                            label={`已发生 · ${formatOccurredDate(item.occurredAt)}`}
                        />
                    ) : (
                        <StatusBadge tone="warning" label="待发生" />
                    )}
                </TableCell>
                <TableCell data-align="end">
                    <div className="flex justify-end gap-1">
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
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
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
                    className="bg-muted/25 text-muted-foreground"
                >
                    <TableCell
                        className="min-w-56 whitespace-normal"
                        colSpan={2}
                    >
                        冲减：{reduction.description}
                    </TableCell>
                    <TableCell data-align="end" className="whitespace-nowrap">
                        −<MoneyValue value={reduction.grossAmount} />
                    </TableCell>
                    <TableCell className="num whitespace-nowrap">
                        {formatOccurredDate(reduction.occurredAt)}
                    </TableCell>
                    <TableCell data-align="end">
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
            className={detailWorkspaceStyles.inlineLinkButton}
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
