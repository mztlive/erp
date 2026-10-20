"use client"

import {
    BusinessFailureState,
    workspaceTaskSurfacePadClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { displayActorName } from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceMoney } from "./workspace-money"

const sectionClass = cn(
    workspaceTaskSurfacePadClassName,
    "space-y-3 border-b border-grid py-4",
)
const primaryFields = new Set([
    "回款金额",
    "到账日",
    "银行流水",
    "待核销",
    "往来主体",
    "提交人",
])

/** 回款审批只读作业面；仅消费此次审批资料，不以当前回款或销售单替换提交版本。 */
export function WorkspaceReceiptApproval({
    item,
    instanceId,
}: {
    item: WorkspaceWorkItem
    instanceId: string
}) {
    const materials = useApprovalMaterials(instanceId, true)
    const prefix = `workspace-receipt-approval-${toAutomationIdSegment(item.workItemId)}`
    const source = materials.data?.display.source
    const sections = source
        ? source.extra_sections.map((field) => ({
              ...field,
              value:
                  field.object_id?.trim() === field.value.trim()
                      ? "未记录名称"
                      : field.value,
          }))
        : (item.summarySections ?? [])
    const field = (label: string) =>
        sections.find((entry) => entry.label === label)?.value
    const lines = source
        ? source.lines.map((line) => ({
              title: line.title,
              quantity: line.quantity,
              dueLabel: line.due_label,
          }))
        : (item.briefLines ?? [])
    const moreCount = source?.more_count ?? item.briefMoreCount ?? 0
    const amount = field("回款金额") ?? item.amountSummary?.value
    const submitter = displayActorName(
        source?.submitter_name ?? field("提交人"),
    )
    const bankReference = field("银行流水")
    const additionalFields = sections.filter(
        (entry) => !primaryFields.has(entry.label),
    )

    return (
        <div aria-label="回款审批资料">
            <section className={sectionClass} aria-label="回款概览">
                <dl className="grid grid-cols-2 items-end gap-x-6 gap-y-4 @min-[540px]/document:grid-cols-[1.4fr_1fr_1fr]">
                    <div className="col-span-2 @min-[540px]/document:col-span-1">
                        <dt className="text-xs text-muted-foreground">
                            本次登记回款
                        </dt>
                        <dd className="num mt-1 text-3xl font-semibold tracking-tight">
                            {amount ? (
                                <WorkspaceMoney value={amount} />
                            ) : (
                                "金额未记录"
                            )}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            到账日期
                        </dt>
                        <dd className="num mt-1 text-base font-medium">
                            {field("到账日") || "未记录"}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            本次拟核销
                        </dt>
                        <dd className="mt-1 text-base font-medium">
                            {field("待核销") ||
                                (lines.length
                                    ? `${lines.length + moreCount} 笔`
                                    : "未记录分配")}
                        </dd>
                    </div>
                </dl>
                <p className="text-xs text-muted-foreground">
                    提交人：{submitter || "未记录姓名"}
                    <span className="mx-2" aria-hidden="true">
                        ·
                    </span>
                    末节点审批通过并入账后计入已收
                </p>
            </section>

            <section className={sectionClass} aria-label="到账依据">
                <h3 className="text-sm font-semibold">到账依据</h3>
                <dl className="grid gap-3 @min-[540px]/document:grid-cols-2">
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            付款主体
                        </dt>
                        <dd className="mt-1 break-words text-sm">
                            {field("往来主体") ||
                                materials.data?.display.counterparty_label ||
                                item.counterpartyName ||
                                "未记录"}
                        </dd>
                    </div>
                    <div>
                        <dt className="text-xs text-muted-foreground">
                            银行流水／凭证引用
                        </dt>
                        <dd className="num mt-1 break-all text-sm">
                            {bankReference || "本次提交未记录"}
                        </dd>
                    </div>
                </dl>
                <div className="space-y-2 border-t border-grid pt-3">
                    <p className="text-xs text-muted-foreground">
                        提交凭证
                        {materials.data
                            ? ` · ${materials.data.attachments.length} 份`
                            : ""}
                    </p>
                    {materials.isPending ? (
                        <p
                            role="status"
                            className="text-sm text-muted-foreground"
                        >
                            正在读取此次提交的凭证…
                        </p>
                    ) : materials.isError ? (
                        <BusinessFailureState
                            id={`${prefix}-materials-retry`}
                            title="提交资料读取失败"
                            error={materials.error}
                            onRetry={() => void materials.refetch()}
                        />
                    ) : (
                        <ApprovalMaterialFiles
                            instanceId={instanceId}
                            attachments={materials.data.attachments}
                            idPrefix={`${prefix}-file`}
                            emptyMessage="暂无可查看的到账凭证，请联系提交人核对。"
                        />
                    )}
                </div>
            </section>

            <section className={sectionClass} aria-label="本次拟核销明细">
                <h3 className="text-sm font-semibold">本次拟核销明细</h3>
                {lines.length ? (
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead scope="col">关联销售单</TableHead>
                                <TableHead scope="col" className="text-right">
                                    本次拟核销金额
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {lines.map((line, index) => (
                                <TableRow key={`${index}:${line.title}`}>
                                    <TableCell className="whitespace-normal break-words">
                                        {line.title}
                                        {line.dueLabel ? (
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                {line.dueLabel}
                                            </p>
                                        ) : null}
                                    </TableCell>
                                    <TableCell className="num text-right font-semibold">
                                        {line.quantity || "未记录"}
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                ) : (
                    <p className="text-sm text-muted-foreground">
                        暂无可查看的核销分配明细，请联系提交人核对。
                    </p>
                )}
                {moreCount > 0 ? (
                    <Alert variant="warning">
                        <AlertTitle>核销摘要未完整展示</AlertTitle>
                        <AlertDescription>
                            当前展示 {lines.length} 笔，另有 {moreCount}{" "}
                            笔未包含在摘要中。请联系提交人核对完整分配明细。
                        </AlertDescription>
                    </Alert>
                ) : null}
                <p className="text-xs leading-5 text-muted-foreground">
                    以上为本次提交的分配方案，不代表已核销金额。
                </p>
            </section>

            {additionalFields.length ? (
                <section className={sectionClass} aria-label="补充资料">
                    <h3 className="text-sm font-semibold">补充资料</h3>
                    <dl className="grid gap-3 @min-[540px]/document:grid-cols-2">
                        {additionalFields.map((entry) => (
                            <div key={entry.label}>
                                <dt className="text-xs text-muted-foreground">
                                    {entry.label}
                                </dt>
                                <dd className="mt-1 break-words text-sm">
                                    {entry.value}
                                </dd>
                            </div>
                        ))}
                    </dl>
                </section>
            ) : null}
        </div>
    )
}
