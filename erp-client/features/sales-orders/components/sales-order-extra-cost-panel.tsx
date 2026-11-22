"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"

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
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { downloadFileAsset } from "@/features/file-assets/api/file-assets"
import {
    ExpensePurchaseCreateDialog,
    type ExpensePurchasePrefill,
} from "@/features/purchase-orders/components/expense-purchase-create-dialog"
import type { SalesExtraCost } from "@/features/sales-orders/api/sales-order-extra-costs"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
    ReduceExtraCostDialog,
    RegisterExtraCostDialog,
} from "@/features/sales-orders/components/sales-order-extra-cost-dialogs"
import {
    salesOrderKeys,
    useSalesExtraCostsQuery,
} from "@/features/sales-orders/hooks/queries"
import {
    EXTRA_COST_CATEGORY_LABEL,
    formatOccurredDate,
    formatTaxRate,
} from "@/features/sales-orders/lib/extra-cost"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"

/**
 * 销售单「附加费用」分区：统一承接财务附加费用台账（登记/冲减）与费用采购单（追加运费/额外费用）。
 * 需 `cost_entry:list` 才读取费用台账；`cost_entry:create` 才能登记与冲减；
 * `purchase_order:create` 且单据已生效时可追加费用采购单。
 */
export function ExtraCostPanel({
    order,
    canViewExtraCost: canViewProp,
    canAddExpensePurchase: canAddExpenseProp,
}: {
    order: SalesOrderDetailView
    canViewExtraCost?: boolean
    canAddExpensePurchase?: boolean
}) {
    const queryClient = useQueryClient()
    const account = useAccountProfileQuery()
    const granted = account.data?.permissions
    const effective = order.currentRevisionNo != null
    const canViewExtraCost =
        canViewProp ?? hasPermission(granted, "cost_entry:list")
    const canAddExpensePurchase =
        canAddExpenseProp ??
        (order.nature === "physical_service" &&
            effective &&
            hasPermission(granted, "purchase_order:create"))
    const canCreate =
        canViewExtraCost && hasPermission(granted, "cost_entry:create")
    const canDownload = hasPermission(granted, "file_asset:preview")

    const query = useSalesExtraCostsQuery(order.id, canViewExtraCost)
    const [registerOpen, setRegisterOpen] = React.useState(false)
    const [reducing, setReducing] = React.useState<SalesExtraCost | null>(null)
    const [expenseOpen, setExpenseOpen] = React.useState(false)
    const [expensePrefill, setExpensePrefill] = React.useState<
        ExpensePurchasePrefill | undefined
    >(undefined)
    const [expenseCreated, setExpenseCreated] = React.useState<string | null>(
        null,
    )

    const freightFollowUps = order.related.freightFollowUps ?? []
    const lineNames = new Map(
        order.lineItems.map((line) => [line.id, line.name]),
    )

    const openExpensePurchase = (prefill?: ExpensePurchasePrefill) => {
        setExpensePrefill(prefill)
        setExpenseOpen(true)
    }

    const freightSection =
        canAddExpensePurchase && freightFollowUps.length > 0 ? (
            <DetailWorkspaceSection width="full">
                <section
                    aria-labelledby="sales-orders-detail-freight-title"
                    className="space-y-2 rounded-lg border border-warning-border bg-warning-soft/75 px-4 py-3"
                >
                    <h3
                        id="sales-orders-detail-freight-title"
                        className="font-semibold text-warning-soft-foreground"
                    >
                        运费待追加
                    </h3>
                    <p className="text-xs text-warning-soft-foreground">
                        以下商品为集采运费另计。供应商收取运费时追加物流费用采购；供应商不收运费时在采购单上标记。
                    </p>
                    <ul className="divide-y divide-warning-border/70">
                        {freightFollowUps.map((item) => (
                            <li
                                key={`${item.purchaseOrderId}-${item.salesOrderLineId}`}
                                className="flex flex-wrap items-center justify-between gap-2 py-2 text-body-compact"
                            >
                                <span className="min-w-0">
                                    {item.productName ?? "商品"}
                                    <span className="ml-2 text-xs text-muted-foreground">
                                        {item.supplierName ?? "供应商"} · 采购单{" "}
                                        <span className="num">
                                            {item.purchaseNo || "草稿"}
                                        </span>
                                    </span>
                                </span>
                                <Button
                                    id={`sales-orders-detail-freight-add-${toAutomationIdSegment(item.purchaseOrderId)}-${toAutomationIdSegment(item.salesOrderLineId)}`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    onClick={() =>
                                        openExpensePurchase({
                                            supplierId: item.supplierId,
                                            salesOrderLineId:
                                                item.salesOrderLineId,
                                            description: `${item.productName ?? "商品"} 运费`,
                                        })
                                    }
                                >
                                    追加运费
                                </Button>
                            </li>
                        ))}
                    </ul>
                </section>
            </DetailWorkspaceSection>
        ) : null

    const addExpenseButton = canAddExpensePurchase ? (
        <Button
            id="sales-orders-detail-add-expense-purchase"
            type="button"
            size="sm"
            variant={canCreate && effective ? "ghost" : "default"}
            className={
                canCreate && effective
                    ? detailWorkspaceStyles.subtleButton
                    : "px-3.5"
            }
            onClick={() => openExpensePurchase(undefined)}
        >
            追加费用采购
        </Button>
    ) : null

    const expenseDialog = canAddExpensePurchase ? (
        <ExpensePurchaseCreateDialog
            salesOrderId={order.id}
            salesLines={order.lineItems
                .filter((line) => line.quantity !== "0")
                .map((line) => ({ id: line.id, name: line.name }))}
            open={expenseOpen}
            onOpenChange={setExpenseOpen}
            prefill={expensePrefill}
            onCreated={(created) => {
                setExpenseCreated(created.purchaseNo)
                void queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.extraCosts(order.id),
                })
                void queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(order.id),
                })
                toast.add({
                    title: "费用采购单已创建",
                    description: `采购单 ${created.purchaseNo} 已提交审批。`,
                    type: "success",
                })
            }}
        />
    ) : null

    if (!canViewExtraCost) {
        return (
            <>
                {freightSection}
                {canAddExpensePurchase ? (
                    <DetailWorkspaceSection
                        width="form"
                        title="费用采购"
                        subtitle="本单有我方承担、不对应销售明细的费用（第三方物流、额外包装、上门安装等）时，可发起费用采购单走审批和付款。"
                        actions={addExpenseButton}
                    >
                        {expenseCreated ? (
                            <p className="rounded-lg bg-muted/35 px-4 py-2.5 text-xs text-muted-foreground">
                                已创建费用采购单{" "}
                                <span className="num font-medium text-foreground">
                                    {expenseCreated}
                                </span>
                                ，已提交审批。
                            </p>
                        ) : (
                            <p className="text-xs text-muted-foreground">
                                当前账号无权查阅财务费用明细台账，可直接发起费用采购单。
                            </p>
                        )}
                    </DetailWorkspaceSection>
                ) : null}
                {expenseDialog}
            </>
        )
    }

    if (query.isPending) {
        return (
            <>
                {freightSection}
                <DetailWorkspaceSection width="form">
                    <div
                        className="h-36 animate-pulse rounded-md bg-muted/70"
                        aria-busy="true"
                        aria-label="加载本单附加费用"
                    />
                </DetailWorkspaceSection>
                {expenseDialog}
            </>
        )
    }
    if (query.isError) {
        return (
            <>
                {freightSection}
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
                {expenseDialog}
            </>
        )
    }
    const list = query.data
    return (
        <>
            {freightSection}

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
                    <>
                        {addExpenseButton}
                        {canCreate && effective ? (
                            <Button
                                id="sales-orders-extra-cost-register"
                                type="button"
                                size="sm"
                                className="px-3.5"
                                onClick={() => setRegisterOpen(true)}
                            >
                                登记费用
                            </Button>
                        ) : null}
                    </>
                }
            >
                {expenseCreated ? (
                    <div className="rounded-lg bg-muted/35 px-4 py-2.5 text-xs text-muted-foreground">
                        已创建费用采购单{" "}
                        <span className="num font-medium text-foreground">
                            {expenseCreated}
                        </span>
                        ，已提交审批。
                    </div>
                ) : null}

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
            {expenseDialog}
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
