"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DataTransferContext, TransferKind } from "@/lib/data-transfer"
import type { BackgroundJobView } from "../api"
import { formatJobDateTime } from "../labels"
import { TransferResult } from "./transfer-result"

type ExportDestination = { href: string; label: string; format: string }

const destinations: Partial<Record<TransferKind, ExportDestination>> = {
    product_pool_excel: {
        href: "/master-data/sellable-items",
        label: "打开公司商品池",
        format: "Excel 工作簿",
    },
    supplier_quote_archive: {
        href: "/procurement/inquiries",
        label: "打开采购询价",
        format: "ZIP 报价包",
    },
    sales_orders: { href: "/sales/orders", label: "打开销售单", format: "CSV" },
    purchase_orders: {
        href: "/procurement/orders",
        label: "打开采购订单",
        format: "CSV",
    },
    contracts: { href: "/sales/contracts", label: "打开合同", format: "CSV" },
    customers: { href: "/sales/customers", label: "打开客户", format: "CSV" },
    inventory_balance: { href: "/inventory", label: "打开库存", format: "CSV" },
    inventory_movement: {
        href: "/inventory?view=movement",
        label: "打开库存",
        format: "CSV",
    },
    inventory_adjustment: {
        href: "/inventory?view=adjustment",
        label: "打开库存",
        format: "CSV",
    },
    inventory_reservation: {
        href: "/inventory?view=reservation",
        label: "打开库存",
        format: "CSV",
    },
    supplier_orders: {
        href: "/supplier-api/orders",
        label: "打开供应商订单",
        format: "CSV",
    },
    receivables: {
        href: "/finance/customer-accounts",
        label: "打开客户往来",
        format: "CSV",
    },
    supplier_payables: {
        href: "/finance/supplier-accounts",
        label: "打开供应商往来",
        format: "CSV",
    },
    invoice_requests: {
        href: "/finance/customer-accounts/invoice-requests",
        label: "打开开票申请",
        format: "CSV",
    },
    profit_loss: {
        href: "/analytics/profit-loss",
        label: "打开利润分析",
        format: "CSV",
    },
    customer_quality_current: {
        href: "/analytics/customer-quality",
        label: "打开客户经营质量",
        format: "CSV",
    },
    customer_quality_history: {
        href: "/analytics/customer-quality?caliber=history",
        label: "打开客户经营质量",
        format: "CSV",
    },
    master_data: {
        href: "/master-data",
        label: "打开基础资料",
        format: "CSV",
    },
}

function exportDestination(context: DataTransferContext) {
    if (context.kind === "receivables") {
        const views: Record<string, { view: string; label: string }> = {
            receivable: { view: "receivable", label: "打开应收台账" },
            receipt: { view: "receipt", label: "打开客户收款" },
            sales_invoice: { view: "sales_invoice", label: "打开销项发票" },
        }
        const destination = views[context.context.view ?? "receivable"]
        if (destination)
            return {
                ...destinations.receivables!,
                href: `/finance/customer-accounts/scope?view=${destination.view}`,
                label: destination.label,
            }
    }
    if (context.kind === "supplier_payables") {
        const views: Record<string, { view: string; label: string }> = {
            payable: { view: "payable", label: "打开应付台账" },
            payment: { view: "payment", label: "打开供应商付款" },
            purchase_invoice: {
                view: "purchase_invoice",
                label: "打开进项发票分摊",
            },
            allocation: { view: "purchase_invoice", label: "打开进项发票分摊" },
        }
        const destination = views[context.context.view ?? "payable"]
        if (destination)
            return {
                ...destinations.supplier_payables!,
                href: `/finance/supplier-accounts/scope?view=${destination.view}`,
                label: destination.label,
            }
    }
    if (
        context.kind === "supplier_quote_archive" &&
        context.context.inquiry_id
    ) {
        return {
            ...destinations.supplier_quote_archive!,
            href: `/procurement/inquiries/${encodeURIComponent(context.context.inquiry_id)}`,
        }
    }
    if (context.kind === "master_data") {
        const resource = context.context.resource
        const resources: Record<string, { href: string; label: string }> = {
            product: { href: "/master-data/products", label: "打开商品资料" },
            supplier: {
                href: "/master-data/suppliers",
                label: "打开供应商资料",
            },
            sellable: {
                href: "/master-data/sellable-items",
                label: "打开公司商品池",
            },
        }
        return {
            ...destinations.master_data!,
            ...((resource && resources[resource]) || {
                href: "/master-data",
                label: "打开基础资料",
            }),
        }
    }
    return destinations[context.kind]
}

/** 导出按文件生成和下载展示，不把单次生成计数解释为业务行数。 */
export function TransferExportResult({
    job,
    context,
    portal = false,
}: {
    job: BackgroundJobView
    context: DataTransferContext
    portal?: boolean
}) {
    const destination = exportDestination(context)
    const expired =
        job.result_expires_at !== null &&
        job.result_expires_at <= Date.now() / 1000
    const downloadable = Boolean(job.finished_at && job.result_file_asset_id)
    const active =
        !job.finished_at && ["pending", "running"].includes(job.status)
    const id = `background-jobs-export-${toAutomationIdSegment(job.id)}`

    return (
        <section className="space-y-3">
            <h3 className="font-medium">导出文件</h3>
            <p>
                {expired
                    ? "文件已过期，请返回业务页面重新导出。"
                    : downloadable
                      ? "文件已生成，可以下载。"
                      : active
                        ? "正在生成文件，可离开当前页面，稍后回来下载。"
                        : job.status === "cancelled"
                          ? "导出已取消。需要文件时，请返回业务页面重新导出。"
                          : "未生成可下载文件，请核对任务提示后返回业务页面重新导出。"}
            </p>
            {destination && (
                <p className="text-muted-foreground">
                    文件格式：{destination.format}
                </p>
            )}
            {context.context.file_name && (
                <p className="break-all text-muted-foreground">
                    文件名称：{context.context.file_name}
                </p>
            )}
            {downloadable && job.result_expires_at !== null && (
                <p className="text-muted-foreground">
                    下载有效期至 {formatJobDateTime(job.result_expires_at)}
                </p>
            )}
            {downloadable && !expired && (
                <TransferResult
                    jobId={job.id}
                    portal={portal}
                    label="下载导出文件"
                    idPrefix={`${id}-download`}
                />
            )}
            <p className="text-xs text-muted-foreground">
                下载时会重新核对当前权限和数据范围；文件不可用时，请重新查询并导出。
            </p>
            {destination && !portal && (
                <Button
                    id={`${id}-open-source`}
                    variant="outline"
                    render={<Link href={destination.href} />}
                >
                    {destination.label}
                    <ArrowUpRightIcon data-icon="inline-end" />
                </Button>
            )}
        </section>
    )
}
