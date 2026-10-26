"use client"

import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import type { BackgroundJobItemView, BackgroundJobView } from "../api"
import { SupplierImportFailures } from "./supplier-import-failures"

type DirectImportResultProps = {
    job: BackgroundJobView
    currentUserId?: string
}

/** 直接写入任务按真实行计数展示，待确认结果不被解释为确定失败。 */
export function DirectImportResult({
    job,
    currentUserId,
}: DirectImportResultProps) {
    const account = useAccountProfileQuery()
    const supplier = job.domain_job_type === "SUPPLIER_IMPORT"
    const resource = supplier ? "supplier" : "product"
    const permissions = account.data?.permissions
    const canList = hasPermission(permissions, `${resource}:list`)
    const canImport = hasPermission(permissions, `${resource}:create`)
    const finished = job.finished_at !== null
    const unprocessed = Math.max(0, job.total_count - job.processed_count)
    const needsAttention = job.failed_count > 0 || unprocessed > 0
    const id = `background-job-direct-import-${toAutomationIdSegment(job.id)}`
    const counts = [
        { label: "已写入", count: job.success_count },
        { label: "跳过", count: job.skipped_count },
        { label: "未成功", count: job.failed_count },
        { label: "未处理", count: unprocessed },
    ]

    return (
        <section className="space-y-4" aria-label="导入结果与后续处理">
            <div className="space-y-2">
                <h3 className="font-medium">
                    {finished ? "导入结果" : "导入进度"}
                </h3>
                <Progress
                    value={job.processed_count}
                    max={job.total_count || 1}
                    aria-label="已处理导入行数"
                >
                    <ProgressLabel>
                        已处理 {job.processed_count} / {job.total_count} 行
                    </ProgressLabel>
                </Progress>
            </div>
            <dl className="grid grid-cols-2 gap-3">
                {counts.map(({ label, count }) => (
                    <div key={label} className="rounded-lg border px-3 py-2">
                        <dt className="text-xs text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="num mt-1 font-medium">{count}</dd>
                    </div>
                ))}
            </dl>
            <div className="space-y-2 text-muted-foreground">
                <p>
                    {supplier
                        ? "成功行已登记供应商资料。重复供应商按逐行说明跳过，已有资料不会重复创建。"
                        : "商品新建、规格追加、图片补齐及供给处理的具体结果见逐行说明，成功行已经保存。"}
                </p>
                {job.failed_count > 0 && (
                    <p>
                        未成功行包括执行失败和结果待核对。请先核对各行原因；结果待核对时先查看已有资料，避免重复办理。
                    </p>
                )}
                {unprocessed > 0 && (
                    <p>
                        {finished
                            ? `还有 ${unprocessed} 行未处理，任务已结束，这些行不会继续执行。`
                            : `还有 ${unprocessed} 行未处理，任务将继续执行，可稍后回来查看。`}
                    </p>
                )}
                {finished && needsAttention && (
                    <p>
                        {supplier
                            ? "下载待处理行，核对结果后修正文件，再到供应商列表导入。下载文件仅限原提交人，并重新检查当前权限。"
                            : "请按逐行结果中的 Excel 行号核对原文件，只保留需要处理的行，修正后到商品列表重新导入。"}
                    </p>
                )}
            </div>
            {supplier &&
                finished &&
                needsAttention &&
                job.requested_by === currentUserId &&
                canImport && <SupplierImportFailures jobId={job.id} />}
            {canList && (
                <Button
                    id={`${id}-open-list`}
                    variant="outline"
                    render={
                        <Link
                            href={
                                supplier
                                    ? "/master-data/suppliers"
                                    : "/master-data/products"
                            }
                        />
                    }
                >
                    {supplier ? "前往供应商列表" : "前往商品列表"}
                    <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                </Button>
            )}
        </section>
    )
}

/** 只把后端明确保存为人类名称的导入对象字段显示为名称。 */
export function DirectImportItemIdentity({
    item,
}: {
    item: BackgroundJobItemView
}) {
    const account = useAccountProfileQuery()
    const name =
        item.object_type === "product_import_row" ||
        item.object_type === "supplier_import_row"
            ? item.object_id
            : null
    const type = item.result_object_type
    const resource = type === "product" || type === "supplier" ? type : null
    const canRead =
        resource &&
        hasPermission(account.data?.permissions, `${resource}:detail`)
    const href =
        resource && item.result_object_id
            ? `/master-data/${resource === "product" ? "products" : "suppliers"}/${encodeURIComponent(item.result_object_id)}?section=overview`
            : null

    return (
        <>
            {name && <p className="mt-1 font-medium">{name}</p>}
            {canRead && href && (
                <Button
                    id={`background-job-import-item-${toAutomationIdSegment(item.id)}-open-object`}
                    size="sm"
                    variant="link"
                    className="mt-1 h-auto px-0"
                    render={<Link href={href} />}
                >
                    {resource === "product" ? "查看商品资料" : "查看供应商资料"}
                    <ArrowUpRightIcon data-icon="inline-end" aria-hidden />
                </Button>
            )}
        </>
    )
}

/** 记录缺少结果时显示未处理，不能按任务终态推断本行已执行。 */
export function directImportItemResultLabel(item: BackgroundJobItemView) {
    if (item.result_code === "outcome_unknown") return "结果待核对"
    switch (item.status) {
        case "success":
            return "已写入"
        case "skipped":
            return "已跳过"
        case "failed":
            return "执行失败"
        default:
            return "未处理"
    }
}
