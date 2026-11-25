"use client"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"

import { DeliveryPackingDetails } from "@/features/fulfillment-operations/components/delivery-packing-details"

import Link from "next/link"
import { CircleAlertIcon } from "lucide-react"
import {
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
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
import { useFulfillmentQueueQuery } from "@/features/fulfillment-operations/hooks/queries"
import { compareDecimal, subtractFixed } from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"
import { WorkspacePurchasePaymentSummary } from "./workspace-purchase-payment"

const sectionClass = "min-w-0 pb-3"

export function WorkspaceDeliveryBrief({ item }: { item: WorkspaceWorkItem }) {
    const profile = useAccountProfileQuery()
    const canViewSales = hasPermission(
        profile.data?.permissions,
        "sales_order:detail",
    )
    const canViewPurchase = hasPermission(
        profile.data?.permissions,
        "purchase_order:detail",
    )
    const fields = item.summarySections ?? []
    const value = (label: string) =>
        fields.find((field) => field.label === label)?.value
    const lines = item.deliveryLines ?? []
    const { names } = useProductUnitNames([], lines.length > 0)
    const trackingLines = lines.filter((line) => line.tracking.length > 0)
    const shippedAt = value("发货时间")
    const hasDeliveryRecords =
        trackingLines.length > 0 ||
        Boolean(shippedAt) ||
        lines.some((line) => compareDecimal(line.shipped_quantity, "0", 4) > 0)
    const pending = lines.filter(
        (line) => compareDecimal(line.pending_quantity, "0", 4) > 0,
    ).length
    const mode = value("发货方式") ?? "发货"
    const links = fields.filter((field) =>
        ["来源销售单", "关联采购单"].includes(field.label),
    )
    const identity = toAutomationIdSegment(item.workItemId)
    return (
        <>
            <section className={sectionClass} aria-label="本次发货概况">
                <div className="flex flex-wrap items-center gap-2">
                    <StatusBadge
                        label={value("履约状态") ?? "状态待核对"}
                        tone={item.statusTone}
                    />
                    <span className="text-body-compact font-medium">
                        {mode}
                    </span>
                    <span className="num text-body-compact text-muted-foreground">
                        {lines.length} 项商品 · {pending} 项待发
                        {mode === "供应商直发" && value("履约批次")
                            ? ` · 批次 ${value("履约批次")}`
                            : ""}
                    </span>
                    {workspaceFulfillmentDescriptor(item)?.role ===
                    "procurement" ? (
                        <WorkspacePurchasePaymentSummary
                            workItemId={item.workItemId}
                        />
                    ) : null}
                </div>
                <WorkspaceFactGrid columns={3} className="mt-3">
                    <WorkspaceFactItem
                        label={mode === "供应商直发" ? "供应商" : "发货仓库"}
                        value={value(
                            mode === "供应商直发" ? "供应商" : "发货仓库",
                        )}
                        emptyText="未提供"
                    />
                    {mode !== "供应商直发" ? (
                        <WorkspaceFactItem
                            label="发货批次"
                            value={value("履约批次")}
                            numeric
                            selectable
                            emptyText="未提供"
                        />
                    ) : null}
                    <WorkspaceFactItem
                        label="履约责任人"
                        value={item.ownerUserLabel}
                        emptyText="未提供"
                    />
                    <WorkspaceFactItem
                        label="销售负责人"
                        value={value("销售负责人")}
                        emptyText="未提供"
                    />
                    <WorkspaceFactItem
                        label="客户最早交付日"
                        value={deadlineLabel(value("最早客户交付日"))}
                        numeric
                        emptyText="未提供"
                    />
                    {mode === "供应商直发" ? (
                        <WorkspaceFactItem
                            label="采购最早交付日"
                            value={deadlineLabel(value("最早预计交付日"))}
                            numeric
                            emptyText="未提供"
                        />
                    ) : null}
                    <WorkspaceFactItem
                        label="任务截止时间"
                        value={item.dueAt ? item.dueAtLabel : undefined}
                        numeric={Boolean(item.dueAt)}
                        emptyText="未设置"
                    />
                </WorkspaceFactGrid>
            </section>
            {item.businessObjectType === "delivery" && (
                <div className={sectionClass}>
                    <DeliveryPackingDetails
                        deliveryId={item.businessObjectId}
                        idPrefix={`workspace-${identity}`}
                    />
                </div>
            )}
            <section className={sectionClass} aria-label="收货信息">
                <h4 className="text-body-compact font-semibold">收货信息</h4>
                <p
                    className={
                        value("收货信息")
                            ? "mt-2 text-body-compact font-medium text-foreground wrap-anywhere whitespace-pre-wrap"
                            : "mt-2 text-xs text-muted-foreground"
                    }
                >
                    {value("收货信息") ??
                        "收货信息暂不可用，请联系销售负责人核对"}
                </p>
            </section>
            <section className={sectionClass} aria-label="本批次发货明细">
                <div className="mb-3 flex flex-wrap items-baseline gap-2">
                    <h4 className="text-body-compact font-semibold">
                        本批次发货明细
                    </h4>
                    <span className="num text-xs text-muted-foreground">
                        {lines.length} 项 · 数量仅统计当前批次
                    </span>
                </div>
                <Table className="table-fixed [&_th]:whitespace-normal [&_th]:bg-surface-sunken [&_th]:px-2 [&_td]:px-2 [&_td]:whitespace-normal">
                    <TableHeader className="bg-transparent [&_tr]:border-0">
                        <TableRow>
                            <TableHead className="w-[34%] rounded-l-md">
                                商品／规格
                            </TableHead>
                            <TableHead className="text-right">本批次</TableHead>
                            <TableHead className="text-right">已发</TableHead>
                            <TableHead className="text-right">待发</TableHead>
                            <TableHead className="w-[25%] rounded-r-md">
                                交付期限
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines.map((line) => (
                            <TableRow key={line.id}>
                                <TableCell className="wrap-anywhere">
                                    <p className="break-words font-medium">
                                        {line.title}
                                    </p>
                                    {line.specification &&
                                    line.specification !== "无规格" ? (
                                        <p className="mt-1 text-xs text-muted-foreground">
                                            {line.specification}
                                        </p>
                                    ) : null}
                                </TableCell>
                                <TableCell className="num wrap-anywhere text-right">
                                    {line.quantity}{" "}
                                    {names.get(line.unit?.trim() ?? "")}
                                </TableCell>
                                <TableCell className="num wrap-anywhere text-right">
                                    {line.shipped_quantity}{" "}
                                    {names.get(line.unit?.trim() ?? "")}
                                </TableCell>
                                <TableCell className="num wrap-anywhere text-right font-semibold">
                                    {line.pending_quantity}{" "}
                                    {names.get(line.unit?.trim() ?? "")}
                                </TableCell>
                                <TableCell>
                                    <p className="num wrap-anywhere text-xs">
                                        客户：
                                        {line.customer_due_date ?? "未提供"}
                                    </p>
                                    {mode === "供应商直发" ? (
                                        <p className="num mt-1 text-xs text-muted-foreground">
                                            采购：
                                            {line.expected_delivery_date ??
                                                "未提供"}
                                        </p>
                                    ) : null}
                                </TableCell>
                            </TableRow>
                        ))}
                    </TableBody>
                </Table>
                {!lines.length ? (
                    <p className="py-3 text-xs text-muted-foreground">
                        本批次尚无发货明细
                    </p>
                ) : null}
            </section>
            {hasDeliveryRecords ? (
                <section className={sectionClass} aria-label="物流与发货记录">
                    <h4 className="text-body-compact font-semibold">
                        物流与发货记录
                    </h4>
                    {trackingLines.length > 0 ? (
                        <ul className="mt-3 space-y-3">
                            {trackingLines.map((line) => (
                                <li key={line.id} className="text-body-compact">
                                    <p className="font-medium">{line.title}</p>
                                    {line.tracking.map((tracking) => (
                                        <p
                                            key={tracking}
                                            className="num mt-1 break-all text-muted-foreground"
                                        >
                                            {tracking}
                                        </p>
                                    ))}
                                </li>
                            ))}
                        </ul>
                    ) : (
                        <p className="mt-2 text-xs text-muted-foreground">
                            尚未登记物流
                        </p>
                    )}
                    <p className="num mt-2 text-xs text-muted-foreground">
                        发货时间：{shippedAt ?? "未登记"}
                    </p>
                </section>
            ) : null}
            <section className={sectionClass} aria-label="关联单据">
                <h4 className="text-body-compact font-semibold">关联单据</h4>
                {links.length > 0 ? (
                    <ul className="mt-2 divide-y divide-border/50">
                        {links.map((field) => {
                            const isSales = field.label === "来源销售单"
                            const canLink =
                                Boolean(field.objectId) &&
                                (isSales ? canViewSales : canViewPurchase)
                            const salesAmount = isSales
                                ? value("来源销售单金额")
                                : undefined
                            return (
                                <li
                                    key={field.label}
                                    className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1 py-2 first:pt-0 last:pb-0"
                                >
                                    <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
                                        <span className="text-xs font-medium text-muted-foreground">
                                            {field.label}
                                        </span>
                                        <span className="num select-all text-body-compact font-medium text-foreground">
                                            {field.value || "未提供"}
                                        </span>
                                        {salesAmount ? (
                                            <span className="num text-xs text-muted-foreground">
                                                · 含税 {salesAmount}
                                            </span>
                                        ) : null}
                                    </div>
                                    {canLink && field.objectId ? (
                                        <Link
                                            id={`workspace-delivery-${identity}-${isSales ? "sales" : "purchase"}`}
                                            href={`/${isSales ? "sales" : "procurement"}/orders/${encodeURIComponent(field.objectId)}`}
                                            className={cn(
                                                "inline-flex shrink-0 items-center rounded-md px-2 py-1 text-xs font-medium",
                                                detailWorkspaceStyles.inlineLinkButton,
                                            )}
                                        >
                                            查看单据
                                        </Link>
                                    ) : null}
                                </li>
                            )
                        })}
                    </ul>
                ) : (
                    <WorkspaceFactGrid columns={2} className="mt-2">
                        <WorkspaceFactItem
                            label="来源销售单金额"
                            value={value("来源销售单金额")}
                            numeric
                            emptyText="未提供"
                        />
                    </WorkspaceFactGrid>
                )}
            </section>
        </>
    )
}

/** 展示付款提醒与实际操作限制；付款不足不阻止确认发货。 */
export function WorkspaceDeliveryGate({ item }: { item: WorkspaceWorkItem }) {
    const descriptor = workspaceFulfillmentDescriptor(item)
    const query = useFulfillmentQueueQuery({
        role: descriptor?.role ?? "sales_order",
        operationTypes: descriptor ? [...descriptor.operationTypes] : [],
        operationId: item.businessObjectId,
        currentOperationId: item.businessObjectId,
    })
    const operation = query.data?.current
    const blocker = operation?.actionBlockers.find(
        (entry) => entry.action === "POST",
    )
    const blocked = operation?.gate.state === "BLOCKED"
    const required = operation?.gate.requiredAmount
    const paid = operation?.gate.effectivePaidAmount
    const gap =
        blocked &&
        required != null &&
        paid != null &&
        compareDecimal(required, paid, 2) > 0
            ? subtractFixed(required, paid, { maxScale: 2, outputScale: 2 })
            : null
    const message = query.isPending ? (
        "正在读取付款情况…"
    ) : query.isError || !operation ? (
        "付款情况暂不可用，请刷新后核对"
    ) : blocker ? (
        blocker.message
    ) : blocked && gap ? (
        <span>
            尚需预付 <MoneyValue value={gap} />
            ，不影响确认实际发货
        </span>
    ) : blocked ? (
        "预付待完成，不影响确认实际发货"
    ) : operation.gate.state === "NOT_APPLICABLE" ? (
        "当前无预付要求，请按实际发货情况登记"
    ) : (
        "预付条件已满足，请按实际发货情况登记"
    )
    return (
        <div
            className={cn(
                "flex min-w-0 flex-1 flex-wrap items-center gap-2 text-xs text-muted-foreground",
                blocked &&
                    !query.isError &&
                    "rounded-r-md border-l-2 border-l-warning bg-warning-soft px-3 py-2 text-warning-soft-foreground",
            )}
            role="status"
        >
            {blocked && !query.isError ? (
                <CircleAlertIcon
                    className="size-4 shrink-0"
                    aria-hidden="true"
                />
            ) : null}
            <span>{message}</span>
            {query.isError ? (
                <Button
                    id={`workspace-delivery-gate-retry-${toAutomationIdSegment(item.workItemId)}`}
                    size="sm"
                    variant="ghost"
                    onClick={() => void query.refetch()}
                >
                    重试
                </Button>
            ) : null}
        </div>
    )
}

/** 交期按上海业务日比较，避免午夜时区偏移；不使用任务截止代替交期。 */
function deadlineLabel(date?: string): string | undefined {
    if (!date) return undefined
    const today = new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
    }).format(new Date())
    const days = Math.round(
        (Date.parse(`${date}T00:00:00Z`) - Date.parse(`${today}T00:00:00Z`)) /
            86400000,
    )
    if (!Number.isFinite(days)) return date
    return `${date} · ${days < 0 ? `逾期 ${-days} 天` : days === 0 ? "今日到期" : `剩余 ${days} 天`}`
}
