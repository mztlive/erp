"use client"

import { ArrowUpRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useRouter } from "@/lib/navigation"
import type { BackgroundJobView } from "../api"

type DomainJobPresentation = {
    title: string
    description: string
    href?: string
    actionLabel?: string
    showRowCounts?: boolean
}

const historicExportRoutes: Record<
    string,
    { href: string; actionLabel: string }
> = {
    SALES_ORDER_EXPORT: {
        href: "/sales/orders",
        actionLabel: "打开销售单列表",
    },
    INVENTORY_LEDGER_EXPORT: {
        href: "/inventory",
        actionLabel: "打开库存台账",
    },
    CONTRACT_EXPORT: {
        href: "/sales/contracts",
        actionLabel: "打开合同列表",
    },
    supplier_fulfillment_order_export: {
        href: "/supplier-api/orders",
        actionLabel: "打开供应商订单",
    },
}

function connectionPresentation(
    job: BackgroundJobView,
    catalog: boolean,
): DomainJobPresentation {
    const operation = catalog ? "目录同步" : "接口检测"
    const completed = job.status === "succeeded"
    const cancelled = job.status === "cancelled"
    const failed = job.status === "failed"
    const href = job.domain_job_id
        ? `/supplier-api/connections/${encodeURIComponent(job.domain_job_id)}?section=${catalog ? "catalog" : "health"}`
        : "/supplier-api/connections"
    return {
        title: completed
            ? `${operation}已完成`
            : cancelled
              ? `${operation}已取消`
              : failed
                ? `${operation}未完成`
                : `${operation}进度`,
        description: catalog
            ? "请在连接资料中核对同步记录、目录更新时间和当前商品可用情况。此处的任务计数按一次同步操作统计。"
            : "请在连接资料中查看本次检测记录和失败原因；是否可启用连接，以当前配置和检测结果为准。此处的任务计数按一次检测操作统计。",
        href,
        actionLabel: catalog ? "查看目录同步记录" : "查看接口检测记录",
    }
}

function domainJobPresentation(job: BackgroundJobView): DomainJobPresentation {
    switch (job.domain_job_type) {
        case "SUPPLIER_HEALTH_CHECK":
            return connectionPresentation(job, false)
        case "SUPPLIER_CATALOG_SYNC":
            return connectionPresentation(job, true)
        case "LEGACY_IMPORT":
            return {
                title: "历史期初导入结果",
                description:
                    "请在导入批次中核对试算、责任确认及已生成的业务对象。后续应用和失败项处理，以批次页面当前允许的操作为准。",
                href: job.domain_job_id
                    ? `/governance/imports?batchId=${encodeURIComponent(job.domain_job_id)}&section=result`
                    : "/governance/imports",
                actionLabel: job.domain_job_id
                    ? "查看导入批次结果"
                    : "打开历史期初导入",
                showRowCounts: true,
            }
        default: {
            const historic = job.domain_job_type
                ? historicExportRoutes[job.domain_job_type]
                : undefined
            if (historic) {
                return {
                    title: "历史导出记录",
                    description:
                        "此记录使用旧导出流程，当前任务页无法恢复其下载。请返回对应业务列表核对范围并重新导出。",
                    ...historic,
                }
            }
            return {
                title: "执行记录",
                description:
                    "此任务尚未提供可识别的业务结果入口。请凭任务号联系管理员核对原业务及执行结果，再决定后续处理。",
            }
        }
    }
}

/** 仅处理实际登记到后台任务集合的业务类型；未知类型保留可核对的执行记录。 */
export function DomainJobResult({ job }: { job: BackgroundJobView }) {
    const router = useRouter()
    const presentation = domainJobPresentation(job)
    const href = presentation.href
    return (
        <section className="space-y-3" aria-label="业务执行结果">
            <h3 className="font-medium">{presentation.title}</h3>
            <p className="text-muted-foreground">{presentation.description}</p>
            {presentation.showRowCounts && (
                <dl className="grid grid-cols-2 gap-3">
                    {[
                        ["成功行数", job.success_count],
                        ["跳过行数", job.skipped_count],
                        ["失败行数", job.failed_count],
                        [
                            "未处理行数",
                            Math.max(0, job.total_count - job.processed_count),
                        ],
                    ].map(([label, count]) => (
                        <div key={label}>
                            <dt className="text-xs text-muted-foreground">
                                {label}
                            </dt>
                            <dd className="num mt-1 font-medium">{count}</dd>
                        </div>
                    ))}
                </dl>
            )}
            {href && (
                <Button
                    id={`background-jobs-domain-${toAutomationIdSegment(job.id)}-open`}
                    type="button"
                    variant="outline"
                    onClick={() => router.push(href)}
                >
                    {presentation.actionLabel}
                    <ArrowUpRightIcon data-icon="inline-end" />
                </Button>
            )}
        </section>
    )
}
