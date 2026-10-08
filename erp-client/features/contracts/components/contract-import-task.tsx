"use client"

import {
    CheckCircle2Icon,
    ChevronRightIcon,
    FileTextIcon,
    RefreshCwIcon,
} from "lucide-react"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Spinner } from "@/components/ui/spinner"
import type { ContractImportTask as ImportTask } from "@/features/contracts/api/upload"
import { toAutomationIdSegment } from "@/lib/automation-id"

import {
    ContractImportProgress,
    importStageLabel,
} from "./contract-import-progress"

const FIELDS: Record<string, string> = {
    contract_no: "合同编号",
    customer_name: "对方签约名称",
    customer_credit_code: "对方信用代码",
    company_name: "我方签约名称",
    company_credit_code: "我方信用代码",
    settlement_name: "结算主体",
    settlement_credit_code: "结算主体信用代码",
    payment_terms: "付款条件",
    invoice_type: "开票要求",
    tax_point: "税率",
    signed_at: "签订日期",
    valid_from: "有效期起",
    valid_to: "有效期止",
    business_scope: "业务范围",
}

export function ContractImportTask({
    task,
    showExtraction = true,
    busy,
    recoveryAvailable,
    contextError,
    previewing,
    refreshing,
    onPreview,
    onRetry,
    onRefresh,
    onNew,
}: {
    task: ImportTask
    showExtraction?: boolean
    busy: boolean
    recoveryAvailable: boolean
    contextError?: string
    previewing: boolean
    refreshing: boolean
    onPreview: () => void
    onRetry: () => void
    onRefresh: () => void
    onNew: () => void
}) {
    const processing = task.status === "processing"
    const failed = task.status === "failed"
    const stageLabel = importStageLabel(task.stage)
    return (
        <section className="space-y-5" aria-label="合同识别结果">
            <div className="flex min-w-0 flex-wrap items-center gap-3 rounded-lg border border-border p-4">
                <div className="flex size-11 shrink-0 items-center justify-center rounded-lg bg-muted">
                    <FileTextIcon className="size-5" aria-hidden="true" />
                </div>
                <div className="min-w-0 flex-1 basis-40">
                    <p
                        className="truncate text-sm font-medium"
                        title={task.file_name}
                    >
                        {task.file_name}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        PDF · {task.page_count} 页
                    </p>
                </div>
                <Button
                    id="contract-import-preview"
                    type="button"
                    variant="ghost"
                    size="sm"
                    disabled={previewing}
                    onClick={onPreview}
                >
                    {previewing ? <Spinner /> : null}查看原文
                </Button>
            </div>
            {contextError ? (
                <Alert variant="destructive">
                    <AlertTitle>此合同不适用于当前操作</AlertTitle>
                    <AlertDescription>{contextError}</AlertDescription>
                </Alert>
            ) : null}
            {processing && !recoveryAvailable ? (
                <ContractImportProgress stage={task.stage} />
            ) : null}
            {task.status === "review" ? (
                <Alert>
                    <AlertTitle>识别完成，请核对并补充</AlertTitle>
                    <AlertDescription>
                        未识别或存在冲突的字段已留空，其余信息已预填。确认后保存合同。
                    </AlertDescription>
                </Alert>
            ) : null}
            {task.status === "succeeded" ? (
                <div
                    className="flex items-start gap-3 rounded-lg bg-muted/50 p-5"
                    role="status"
                >
                    <CheckCircle2Icon
                        className="mt-0.5 size-5 shrink-0 text-success"
                        aria-hidden="true"
                    />
                    <div>
                        <h3 className="font-medium">合同已归档</h3>
                        <p className="mt-1 text-sm text-muted-foreground">
                            合同信息已确认并归档，下方保留原始识别依据。
                        </p>
                    </div>
                </div>
            ) : null}
            {failed || recoveryAvailable || task.status === "ready" ? (
                <div className="space-y-4 rounded-lg border border-border p-5">
                    <div className="space-y-2" role="status">
                        <h3 className="font-medium">
                            {failed
                                ? "合同未归档"
                                : recoveryAvailable
                                  ? "识别尚未完成"
                                  : "合同已上传，等待识别"}
                        </h3>
                        <p className="break-words text-sm leading-relaxed text-muted-foreground">
                            {failed ? (
                                <>
                                    {stageLabel
                                        ? `失败阶段：${stageLabel}。`
                                        : ""}
                                    {task.failure?.field
                                        ? `${FIELDS[task.failure.field] ?? "合同字段"}：`
                                        : ""}
                                    {task.failure?.page
                                        ? `第 ${task.failure.page} 页，`
                                        : ""}
                                    {task.failure?.message ||
                                        "识别未完成，请重试或更换文件。"}
                                </>
                            ) : recoveryAvailable ? (
                                `本次识别长时间未完成${stageLabel ? `，停留在${stageLabel}` : ""}，可以恢复识别或刷新结果。`
                            ) : (
                                "开始识别后，系统将识别文字并提取合同信息。"
                            )}
                        </p>
                    </div>
                    <div className="flex flex-wrap items-center gap-2">
                        <Button
                            id="contract-import-retry"
                            type="button"
                            disabled={busy || Boolean(contextError)}
                            onClick={onRetry}
                        >
                            {busy ? <Spinner /> : null}
                            {recoveryAvailable
                                ? "恢复识别"
                                : failed
                                  ? "重新识别"
                                  : "开始识别"}
                        </Button>
                        {failed ? (
                            <Button
                                id="contract-import-replace-file"
                                type="button"
                                variant="outline"
                                disabled={busy}
                                onClick={onNew}
                            >
                                更换文件
                            </Button>
                        ) : null}
                        <Button
                            id="contract-import-refresh"
                            type="button"
                            variant="ghost"
                            size="icon-sm"
                            aria-label="刷新识别结果"
                            title="刷新识别结果"
                            disabled={refreshing}
                            onClick={onRefresh}
                        >
                            <RefreshCwIcon
                                className={
                                    refreshing
                                        ? "animate-spin motion-reduce:animate-none"
                                        : undefined
                                }
                            />
                        </Button>
                    </div>
                </div>
            ) : null}
            {showExtraction && task.extraction ? (
                <div className="space-y-3">
                    <h3 className="text-sm font-medium">识别信息</h3>
                    <dl className="grid gap-x-6 gap-y-4 sm:grid-cols-2">
                        {Object.entries(task.extraction.fields).map(
                            ([key, field]) => (
                                <div
                                    key={key}
                                    className="min-w-0 border-b border-border pb-3"
                                >
                                    <dt className="text-xs text-muted-foreground">
                                        {FIELDS[key] ?? "合同字段"}
                                    </dt>
                                    <dd className="mt-1 break-words text-sm">
                                        {field.value}
                                    </dd>
                                    <dd className="mt-2">
                                        <Collapsible>
                                            <CollapsibleTrigger
                                                id={`contract-import-evidence-${toAutomationIdSegment(key)}`}
                                                className="group flex items-center gap-1 text-xs text-muted-foreground outline-none hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                                            >
                                                <ChevronRightIcon
                                                    className="size-3 transition-transform group-data-panel-open:rotate-90"
                                                    aria-hidden="true"
                                                />
                                                查看依据 · 第 {field.page} 页
                                            </CollapsibleTrigger>
                                            <CollapsibleContent>
                                                <p className="mt-2 break-words rounded-md bg-muted/50 p-2 text-xs leading-relaxed text-muted-foreground">
                                                    {field.quote}
                                                </p>
                                            </CollapsibleContent>
                                        </Collapsible>
                                    </dd>
                                </div>
                            ),
                        )}
                    </dl>
                </div>
            ) : null}
        </section>
    )
}
