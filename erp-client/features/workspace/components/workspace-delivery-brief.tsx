"use client"

import Link from "next/link"
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
import { useFulfillmentQueueQuery } from "@/features/fulfillment-operations/hooks/queries"
import { compareDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { workspaceFulfillmentDescriptor } from "../lib/workspace-fulfillment"
import type { WorkspaceWorkItem } from "../types"

const sectionClass =
    "border-b border-grid px-5 py-5 in-data-[slot=sheet-content]:px-7"

export function WorkspaceDeliveryBrief({ item }: { item: WorkspaceWorkItem }) {
    const fields = item.summarySections ?? []
    const value = (label: string) =>
        fields.find((field) => field.label === label)?.value
    const lines = item.deliveryLines ?? []
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
                    <span className="text-sm font-medium">{mode}</span>
                    <span className="text-sm text-muted-foreground">
                        {lines.length} 项商品 · {pending} 项待发
                    </span>
                </div>
                <p className="num mt-2 text-xs text-muted-foreground">
                    发货批次：{value("履约批次") ?? "未提供"}
                </p>
                <dl className="mt-4 grid grid-cols-1 gap-x-8 gap-y-3 @min-[640px]/delivery-brief:grid-cols-2">
                    <Fact
                        label={mode === "供应商直发" ? "供应商" : "发货仓库"}
                        value={value(
                            mode === "供应商直发" ? "供应商" : "发货仓库",
                        )}
                    />
                    <Fact label="履约责任人" value={item.ownerUserLabel} />
                    <Fact label="销售负责人" value={value("销售负责人")} />
                    <Fact
                        label="客户最早交付日"
                        value={deadlineLabel(value("最早客户交付日"))}
                    />
                    {mode === "供应商直发" ? (
                        <Fact
                            label="采购最早交付日"
                            value={deadlineLabel(value("最早预计交付日"))}
                        />
                    ) : null}
                    <Fact
                        label="任务截止时间"
                        value={item.dueAt ? item.dueAtLabel : "未设置"}
                    />
                </dl>
            </section>
            <section className={sectionClass} aria-label="收货信息">
                <h4 className="text-sm font-semibold">收货信息</h4>
                <p className="mt-2 text-sm text-muted-foreground">
                    {value("收货信息") ??
                        "收货信息暂不可用，请联系销售负责人核对"}
                </p>
            </section>
            <section className={sectionClass} aria-label="本批次发货明细">
                <div className="mb-3 flex flex-wrap items-baseline gap-2">
                    <h4 className="text-sm font-semibold">本批次发货明细</h4>
                    <span className="text-xs text-muted-foreground">
                        {lines.length} 项 · 数量仅统计当前批次
                    </span>
                </div>
                <Table>
                    <TableHeader>
                        <TableRow>
                            <TableHead>商品／规格</TableHead>
                            <TableHead className="text-right">本批次</TableHead>
                            <TableHead className="text-right">已发</TableHead>
                            <TableHead className="text-right">待发</TableHead>
                            <TableHead>交付期限</TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {lines.map((line) => (
                            <TableRow key={line.id}>
                                <TableCell className="min-w-36 max-w-64 whitespace-normal">
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
                                <TableCell className="num text-right">
                                    {line.quantity} {line.unit}
                                </TableCell>
                                <TableCell className="num text-right">
                                    {line.shipped_quantity} {line.unit}
                                </TableCell>
                                <TableCell className="num text-right font-semibold">
                                    {line.pending_quantity} {line.unit}
                                </TableCell>
                                <TableCell>
                                    <p className="num text-xs">
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
                    <p className="py-3 text-sm text-muted-foreground">
                        本批次尚无发货明细
                    </p>
                ) : null}
            </section>
            <section className={sectionClass} aria-label="物流与发货记录">
                <h4 className="text-sm font-semibold">物流与发货记录</h4>
                {lines.some((line) => line.tracking.length) ? (
                    <ul className="mt-3 space-y-3">
                        {lines
                            .filter((line) => line.tracking.length)
                            .map((line) => (
                                <li key={line.id} className="text-sm">
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
                    <p className="mt-2 text-sm text-muted-foreground">
                        尚未登记物流
                    </p>
                )}
                <p className="mt-2 text-xs text-muted-foreground">
                    发货时间：{value("发货时间") ?? "未登记"}
                </p>
            </section>
            <section className={sectionClass} aria-label="关联单据">
                <h4 className="text-sm font-semibold">关联单据</h4>
                <dl className="mt-3 grid grid-cols-1 gap-3 @min-[640px]/delivery-brief:grid-cols-2">
                    {links.map((field) => (
                        <div key={field.label} className="min-w-0 text-sm">
                            <dt className="mb-1 text-xs text-muted-foreground">
                                {field.label}
                            </dt>
                            <dd className="num break-all">
                                {field.objectId ? (
                                    <Link
                                        id={`workspace-delivery-${identity}-${field.label === "来源销售单" ? "sales" : "purchase"}`}
                                        href={`/${field.label === "来源销售单" ? "sales" : "procurement"}/orders/${encodeURIComponent(field.objectId)}`}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="underline underline-offset-4"
                                    >
                                        {field.value}
                                    </Link>
                                ) : (
                                    field.value
                                )}
                            </dd>
                        </div>
                    ))}
                    <Fact
                        label="来源销售单金额"
                        value={value("来源销售单金额")}
                    />
                </dl>
            </section>
        </>
    )
}

function Fact({ label, value }: { label: string; value?: string }) {
    return (
        <div className="grid min-w-0 grid-cols-[7rem_minmax(0,1fr)] items-baseline gap-3 text-sm">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="min-w-0 break-words">{value || "未提供"}</dd>
        </div>
    )
}

/** 只读取当前执行者的实际付款条件，不把任务待处理状态推断成可发货。 */
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
    const message = query.isPending
        ? "正在核对发货条件…"
        : query.isError || !operation
          ? "发货条件暂不可用，请刷新后核对"
          : blocked
            ? "先款条件未满足，暂不能确认发货"
            : (blocker?.message ?? "可登记发货，提交时将再次核对发货条件")
    return (
        <div
            className="flex min-w-0 flex-1 flex-wrap items-center gap-2 text-xs text-muted-foreground"
            role="status"
        >
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
