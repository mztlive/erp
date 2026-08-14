"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon, TriangleAlertIcon } from "lucide-react"
import { z } from "zod"

import {
    BackgroundJobProgress,
    BatchOperationResult,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    FormalActionConfirmDialog,
    FormalActionResult,
    ImportIssueTable,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Fact } from "@/features/import-opening/components/batch-facts"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useImportConfirmationOperations,
    useImportExecutionOperations,
    useImportIssuesQuery,
} from "@/features/import-opening/hooks/queries"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type {
    BatchSection,
    ImportBatchView,
    ImportConfirmationView,
    ImportExecutionAction,
    ImportIssueCode,
    ImportObjectCode,
    IssueRowStatus,
} from "@/features/import-opening/types"
import {
    CONFIRMATION_SCOPE_LABEL,
    ISSUE_CODE_LABEL,
    OBJECT_CODE_LABEL,
    RETENTION_LABEL,
    ROW_STATUS_LABEL,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
import { versionText } from "@/lib/ui-text"

function formatBytes(n: number) {
    if (n < 1024) return `${n} B`
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`
    return `${(n / (1024 * 1024)).toFixed(2)} MB`
}

export function OverviewSection({
    batch,
    onGoSection,
}: {
    batch: ImportBatchView
    onGoSection: (s: BatchSection) => void
}) {
    return (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,1.4fr)_minmax(0,1fr)]">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>试算摘要</CardTitle>
                    <CardDescription>
                        试算统计由系统统一计算，与问题明细可能因筛选存在差异。
                    </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                    <MetricStrip columns={4} aria-label="试算指标">
                        <MetricItem
                            label="总行数"
                            value={batch.metrics.total}
                        />
                        <MetricItem
                            label="可应用"
                            value={batch.metrics.valid}
                        />
                        <MetricItem
                            label="冲突"
                            value={batch.metrics.conflict}
                        />
                        <MetricItem label="问题" value={batch.metrics.failed} />
                    </MetricStrip>
                    <div className="mt-4 flex flex-wrap gap-2">
                        <Button
                            type="button"
                            size="sm"
                            variant="secondary"
                            onClick={() => onGoSection("trial")}
                        >
                            查看问题明细
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            onClick={() => onGoSection("confirm")}
                        >
                            责任确认
                        </Button>
                    </div>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>期初口径</CardTitle>
                    <CardDescription>
                        提示按本批对象固定生成，不可修改。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.openingPolicyHints.map((hint) => (
                        <div
                            key={hint.objectCode}
                            className="space-y-1 text-sm"
                        >
                            <div className="font-medium">
                                {OBJECT_CODE_LABEL[hint.objectCode]}
                            </div>
                            <p className="text-muted-foreground">
                                {hint.message}
                            </p>
                        </div>
                    ))}
                    {batch.sourceObjectSet.includes("CARD_OPENING_AR") ||
                    batch.sourceObjectSet.includes("CARD_SALES_ORDER") ? (
                        <Button
                            size="sm"
                            variant="outline"
                            render={<Link href="/finance/card-funds-review" />}
                        >
                            前往卡券票款复核
                            <ExternalLinkIcon className="size-4" />
                        </Button>
                    ) : null}
                    {batch.sourceObjectSet.includes("OPENING_STOCK") ? (
                        <Button
                            size="sm"
                            variant="outline"
                            render={<Link href="/inventory?view=balance" />}
                        >
                            查看库存台账
                            <ExternalLinkIcon className="size-4" />
                        </Button>
                    ) : null}
                </CardContent>
            </Card>

            {batch.invalidation ? (
                <Alert variant="warning" className="lg:col-span-2">
                    <TriangleAlertIcon />
                    <AlertTitle>旧确认已失效</AlertTitle>
                    <AlertDescription>
                        {batch.invalidation.reason}（
                        {formatDateTime(
                            batch.invalidation.invalidatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                        ）。禁止按旧试算版本{" "}
                        <span className="num font-mono">
                            {batch.invalidation.previousTrialVersion}
                        </span>{" "}
                        提交应用。
                    </AlertDescription>
                </Alert>
            ) : null}
        </div>
    )
}

export function FilesSection({ batch }: { batch: ImportBatchView }) {
    const [previewAsset, setPreviewAsset] = React.useState<string | null>(null)
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>合规输入包</CardTitle>
                    <CardDescription>
                        仅展示白名单包元数据；不展示原始存储键、签名 URL
                        或文件正文。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4 text-sm">
                    {batch.inputAsset ? (
                        <>
                            <Fact
                                label="文件名"
                                value={batch.inputAsset.fileName}
                            />
                            <Fact
                                label="大小"
                                value={formatBytes(batch.inputAsset.byteSize)}
                                mono
                            />
                            <Fact
                                label="安全检查"
                                value={
                                    batch.inputAsset.securityScanStatus ===
                                    "PASSED"
                                        ? "通过"
                                        : batch.inputAsset
                                                .securityScanStatus ===
                                            "PENDING"
                                          ? "待扫描"
                                          : batch.inputAsset
                                                  .securityScanStatus ===
                                              "REJECTED"
                                            ? "拒绝"
                                            : "隔离"
                                }
                            />
                            {batch.inputAsset.contentHmacShort ? (
                                <Fact
                                    label={versionText.checksumShort}
                                    value={batch.inputAsset.contentHmacShort}
                                    mono
                                />
                            ) : null}
                            <Fact
                                label="保留策略"
                                value={
                                    RETENTION_LABEL[
                                        batch.inputAsset.retentionClass
                                    ]
                                }
                            />
                        </>
                    ) : (
                        <p className="text-muted-foreground">
                            尚未接收合规包。
                        </p>
                    )}
                    <Separator />
                    <p className="text-xs text-muted-foreground">
                        禁止内容：原始
                        SQL、数据库连接头、商城禁止字段导出。此类文件不得长期留存，也不能在本页展示。
                    </p>
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>结果与诊断资产保留</CardTitle>
                    <CardDescription>
                        成功审计长期 · 失败诊断 30 天 · 导出 7
                        天；下载前重鉴权。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-3 pt-4">
                    {batch.resultAssets.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            尚无结果资产。
                        </p>
                    ) : (
                        batch.resultAssets.map((a) => (
                            <div
                                key={a.assetId}
                                className="rounded-lg border px-3 py-2 text-sm"
                            >
                                <div className="font-medium">{a.fileName}</div>
                                <div className="mt-1 text-xs text-muted-foreground">
                                    {RETENTION_LABEL[a.retentionClass]}
                                    {a.expiresAt
                                        ? ` · 到期 ${formatDateTime(a.expiresAt, "dateStyle", "passthrough")}`
                                        : " · 无到期"}
                                    {" · "}
                                    {formatBytes(a.byteSize)}
                                </div>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    className="mt-2 h-7 text-xs"
                                    onClick={() => setPreviewAsset(a.fileName)}
                                >
                                    查看（示例）
                                </Button>
                                {previewAsset === a.fileName ? (
                                    <p className="mt-2 text-xs text-muted-foreground">
                                        示例：文件正文不在此处展示，仅保留元数据与保留策略。
                                    </p>
                                ) : null}
                            </div>
                        ))
                    )}
                </CardContent>
            </Card>
        </div>
    )
}

export function TrialSection({
    batch,
    urlState,
    patchUrl,
    issueQuery,
}: {
    batch: ImportBatchView
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    issueQuery: ReturnType<typeof useImportIssuesQuery>
}) {
    const issues = issueQuery.data?.rows ?? []

    return (
        <div className="space-y-4">
            <Alert>
                <AlertTitle>问题表范围</AlertTitle>
                <AlertDescription>
                    仅展示失败、冲突、跳过与待映射行；不混入成功长表。筛选写入
                    URL，刷新可恢复。
                </AlertDescription>
            </Alert>

            <div className="flex flex-wrap items-end gap-2">
                <div className="space-y-1">
                    <Label className="text-xs">错误码</Label>
                    <OptionCombobox
                        value={urlState.issueCode ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                issueCode:
                                    v === "all"
                                        ? undefined
                                        : (v as ImportIssueCode),
                                section: "trial",
                            })
                        }}
                        options={[
                            { value: "all", label: "全部错误码" },
                            ...(
                                Object.keys(
                                    ISSUE_CODE_LABEL,
                                ) as ImportIssueCode[]
                            ).map((code) => ({
                                value: code,
                                label: ISSUE_CODE_LABEL[code],
                            })),
                        ]}
                        className="w-[12rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <div className="space-y-1">
                    <Label className="text-xs">对象</Label>
                    <OptionCombobox
                        value={urlState.issueObjectType ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                issueObjectType:
                                    v === "all"
                                        ? undefined
                                        : (v as ImportObjectCode),
                                section: "trial",
                            })
                        }}
                        options={[
                            { value: "all", label: "全部对象" },
                            ...batch.sourceObjectSet.map((code) => ({
                                value: code,
                                label: OBJECT_CODE_LABEL[code],
                            })),
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <div className="space-y-1">
                    <Label className="text-xs">处理状态</Label>
                    <OptionCombobox
                        value={urlState.rowStatus ?? "all"}
                        onValueChange={(v) => {
                            if (v == null) return
                            patchUrl({
                                rowStatus:
                                    v === "all"
                                        ? undefined
                                        : (v as IssueRowStatus),
                                section: "trial",
                            })
                        }}
                        options={[
                            { value: "all", label: "全部状态" },
                            ...(
                                Object.keys(
                                    ROW_STATUS_LABEL,
                                ) as IssueRowStatus[]
                            ).map((s) => ({
                                value: s,
                                label: ROW_STATUS_LABEL[s],
                            })),
                        ]}
                        className="w-[10rem]"
                        size="sm"
                        allowClear={false}
                    />
                </div>
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    onClick={() =>
                        patchUrl({
                            issueCode: undefined,
                            issueObjectType: undefined,
                            rowStatus: undefined,
                            section: "trial",
                        })
                    }
                >
                    清除筛选
                </Button>
            </div>

            <ImportIssueTable
                caption="导入问题明细（不含成功行）"
                emptyMessage={
                    issueQuery.isPending
                        ? "问题明细加载中…"
                        : "当前筛选下没有问题行"
                }
                repairableLabel="可在修复后重试"
                externalLabel="需外部处理后再试"
                issues={issues.map((issue) => ({
                    id: issue.issueId,
                    rowNumber: issue.sourceRowNo,
                    field: `${OBJECT_CODE_LABEL[issue.objectType]} · ${issue.sourceColumnName}`,
                    errorCode: issue.issueCode,
                    message: (
                        <span>
                            <span className="text-muted-foreground">
                                [{ROW_STATUS_LABEL[issue.rowStatus]}]{" "}
                            </span>
                            {issue.errorDetail}
                        </span>
                    ),
                    repairable: issue.repairable,
                }))}
            />
            <p className="text-xs text-muted-foreground">
                共 {issueQuery.data?.totalCount ?? 0} 条问题
            </p>
            <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={urlState.page <= 1 || issueQuery.isFetching}
                    onClick={() =>
                        patchUrl({ page: urlState.page - 1, section: "trial" })
                    }
                >
                    上一页
                </Button>
                <span>
                    第 {urlState.page} /{" "}
                    {Math.max(
                        1,
                        Math.ceil((issueQuery.data?.totalCount ?? 0) / 20),
                    )}{" "}
                    页
                </span>
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={
                        issueQuery.isFetching ||
                        urlState.page * 20 >= (issueQuery.data?.totalCount ?? 0)
                    }
                    onClick={() =>
                        patchUrl({ page: urlState.page + 1, section: "trial" })
                    }
                >
                    下一页
                </Button>
            </div>
        </div>
    )
}

const RETURN_REASON_OPTIONS = [
    { value: "DATA_MISMATCH", label: "试算数据与业务事实不一致" },
    { value: "RULE_MISMATCH", label: "导入口径或规则不一致" },
    { value: "MISSING_EVIDENCE", label: "缺少必要核对依据" },
    { value: "OTHER", label: "其它需修复问题" },
] as const

const returnForFixSchema = z.object({
    reasonCode: z.string().trim().min(1, "请选择退回原因"),
    comment: z.string().trim().min(3, "请填写至少 3 个字的修复说明"),
})

/** 采集退回原因；提交失败时保留输入并保持对话框打开。 */
function ReturnForFixDialog({
    confirmation,
    pending,
    onSubmit,
    onCancel,
}: {
    confirmation: ImportConfirmationView
    pending: boolean
    onSubmit: (value: { reasonCode: string; comment: string }) => Promise<void>
    onCancel: () => void
}) {
    const form = useAppForm({
        defaultValues: { reasonCode: "DATA_MISMATCH", comment: "" },
        validators: { onChange: returnForFixSchema },
        onSubmit: async ({ value }) => onSubmit(value),
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !pending) onCancel()
            }}
        >
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>
                        退回{CONFIRMATION_SCOPE_LABEL[confirmation.scope]}修复
                    </DialogTitle>
                    <DialogDescription>
                        本次试算会形成已退回结论并完成当前任务；修复并重新试算后，系统才会创建新任务。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reasonCode"
                        children={(field) => (
                            <field.SelectField
                                label="退回原因"
                                options={RETURN_REASON_OPTIONS}
                                allowClear={false}
                            />
                        )}
                    />
                    <form.AppField
                        name="comment"
                        children={(field) => (
                            <field.TextareaField
                                label="修复说明"
                                rows={4}
                                placeholder="说明需要修复的数据、口径或依据"
                            />
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            render={<Button type="button" variant="outline" />}
                        >
                            返回核对
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认退回修复"
                                pendingLabel="正在提交"
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

const CANCEL_PENDING_REASON_OPTIONS = [
    { value: "OPERATOR_CANCELLED", label: "操作人终止本批未应用项" },
    { value: "DATA_SCOPE_CHANGED", label: "导入数据范围已变化" },
    { value: "BUSINESS_WINDOW_CLOSED", label: "业务执行窗口已关闭" },
    { value: "OTHER", label: "其它终止原因" },
] as const

const cancelPendingSchema = z.object({
    reasonCode: z.string().trim().min(1, "请选择取消原因"),
    comment: z.string().trim().max(1024, "操作说明不能超过 1024 个字符"),
})

/** 采集取消未应用项原因；已形成的业务事实不会被本动作回滚。 */
function CancelPendingDialog({
    pending,
    onSubmit,
    onCancel,
}: {
    pending: boolean
    onSubmit: (value: { reasonCode: string; comment: string }) => Promise<void>
    onCancel: () => void
}) {
    const form = useAppForm({
        defaultValues: { reasonCode: "OPERATOR_CANCELLED", comment: "" },
        validators: { onChange: cancelPendingSchema },
        onSubmit: async ({ value }) => onSubmit(value),
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !pending) onCancel()
            }}
        >
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>取消尚未应用项</DialogTitle>
                    <DialogDescription>
                        系统只停止本批尚未应用的项；已成功、已跳过及已形成的业务事实保持不变。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reasonCode"
                        children={(field) => (
                            <field.SelectField
                                label="取消原因"
                                options={CANCEL_PENDING_REASON_OPTIONS}
                                allowClear={false}
                            />
                        )}
                    />
                    <form.AppField
                        name="comment"
                        children={(field) => (
                            <field.TextareaField
                                label="操作说明（可选）"
                                rows={4}
                                placeholder="补充取消范围或业务窗口信息"
                            />
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    type="button"
                                    variant="outline"
                                    disabled={pending}
                                />
                            }
                        >
                            返回
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认取消未应用项"
                                pendingLabel="正在取消"
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

/** 返回同一命令载荷在当前页面生命周期内复用的幂等键。 */
function commandIdempotencyKey(
    keys: Map<string, string>,
    identity: string,
): string {
    const existing = keys.get(identity)
    if (existing) return existing
    const key = `w18:${crypto.randomUUID()}`
    keys.set(identity, key)
    return key
}

export function ConfirmSection({
    batch,
    workItemTypeMissing,
    confirmBlocked,
}: {
    batch: ImportBatchView
    workItemTypeMissing: boolean
    confirmBlocked: ImportBatchView["actionBlockers"]
}) {
    const operations = useImportConfirmationOperations()
    const [confirming, setConfirming] = React.useState<ImportConfirmationView>()
    const [returning, setReturning] = React.useState<ImportConfirmationView>()
    const idempotencyKeys = React.useRef(new Map<string, string>())

    const startProcessing = React.useCallback(
        async (confirmation: ImportConfirmationView) => {
            const task = confirmation.workItem
            if (!task) return
            operations.resetError()
            await operations.startProcessing({
                workItemId: task.workItemId,
                expectedTaskVersion: task.taskVersion,
                idempotencyKey: commandIdempotencyKey(
                    idempotencyKeys.current,
                    `${task.workItemId}:START_PROCESSING:${task.taskVersion}`,
                ),
            })
        },
        [operations],
    )

    const complete = React.useCallback(
        async (
            confirmation: ImportConfirmationView,
            action: "CONFIRM_SCOPE" | "RETURN_FOR_FIX",
            reasonCode?: string,
            comment?: string,
        ) => {
            const task = confirmation.workItem
            if (!task) return
            operations.resetError()
            const payloadIdentity = [
                task.workItemId,
                action,
                task.taskVersion,
                batch.version,
                confirmation.trialVersion,
                reasonCode ?? "",
                comment ?? "",
            ].join(":")
            await operations.completeConfirmation({
                batchId: batch.batchId,
                batchVersion: batch.version,
                trialVersion: confirmation.trialVersion,
                confirmationScope: confirmation.scope,
                workItemId: task.workItemId,
                taskVersion: task.taskVersion,
                subjectVersion: task.subjectVersion,
                action,
                reasonCode,
                comment,
                idempotencyKey: commandIdempotencyKey(
                    idempotencyKeys.current,
                    payloadIdentity,
                ),
            })
        },
        [batch.batchId, batch.version, operations],
    )

    return (
        <div className="space-y-4">
            {workItemTypeMissing ? (
                <FormalActionResult
                    status="blocked"
                    title="责任确认任务不完整"
                    description="当前试算缺少已登记的责任确认任务，不能提交确认或退回。请联系管理员重新生成确认任务。"
                />
            ) : null}

            {operations.error ? (
                <BusinessFailureState
                    title="责任确认未完成"
                    error={operations.error}
                />
            ) : null}

            <div className="grid gap-3 md:grid-cols-2">
                {batch.confirmations.map((confirmation) => {
                    const task = confirmation.workItem
                    const actions = task?.allowedActions ?? []
                    const canStart =
                        !workItemTypeMissing &&
                        confirmation.result === "PENDING" &&
                        actions.includes("START_PROCESSING")
                    const canConfirm =
                        !workItemTypeMissing &&
                        confirmation.result === "PENDING" &&
                        actions.includes("CONFIRM_SCOPE") &&
                        actions.includes("RETURN_FOR_FIX")
                    return (
                        <Card
                            key={confirmation.confirmationId}
                            size="sm"
                            className={`${surfacePanelClassName} ${confirmation.focused ? "ring-2 ring-primary/40" : ""}`}
                        >
                            <CardHeader className="border-b border-border/30">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <CardTitle className="text-base">
                                        {
                                            CONFIRMATION_SCOPE_LABEL[
                                                confirmation.scope
                                            ]
                                        }
                                    </CardTitle>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={
                                            confirmation.result === "CONFIRMED"
                                                ? "已确认"
                                                : confirmation.result ===
                                                    "REJECTED"
                                                  ? "已退回"
                                                  : confirmation.result ===
                                                      "INVALIDATED"
                                                    ? "已失效"
                                                    : canStart
                                                      ? "团队待处理"
                                                      : "待确认"
                                        }
                                        tone={
                                            confirmation.result === "CONFIRMED"
                                                ? "success"
                                                : confirmation.result ===
                                                        "REJECTED" ||
                                                    confirmation.result ===
                                                        "INVALIDATED"
                                                  ? "destructive"
                                                  : "warning"
                                        }
                                    />
                                </div>
                                <CardDescription>
                                    试算版本{" "}
                                    <span className="num font-mono">
                                        {confirmation.trialVersion}
                                    </span>
                                    {confirmation.focused
                                        ? " · 当前待处理入口"
                                        : confirmation.inViewerResponsibility
                                          ? " · 由本人负责"
                                          : " · 只读"}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 pt-4 text-sm">
                                {confirmation.confirmedByLabel ? (
                                    <p>
                                        确认人 {confirmation.confirmedByLabel}
                                        {confirmation.confirmedAt
                                            ? ` · ${formatDateTime(confirmation.confirmedAt, "dateStyle", "passthrough")}`
                                            : ""}
                                    </p>
                                ) : null}
                                {confirmation.comment ? (
                                    <p className="text-muted-foreground">
                                        {confirmation.comment}
                                    </p>
                                ) : null}
                                <div className="flex flex-wrap gap-2">
                                    {canStart ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            disabled={operations.isStarting}
                                            onClick={() =>
                                                void startProcessing(
                                                    confirmation,
                                                )
                                            }
                                        >
                                            {operations.isStarting
                                                ? "正在开始"
                                                : "开始处理"}
                                        </Button>
                                    ) : null}
                                    {canConfirm ? (
                                        <>
                                            <Button
                                                type="button"
                                                size="sm"
                                                disabled={
                                                    operations.isCompleting
                                                }
                                                onClick={() =>
                                                    setConfirming(confirmation)
                                                }
                                            >
                                                确认本范围
                                            </Button>
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={
                                                    operations.isCompleting
                                                }
                                                onClick={() =>
                                                    setReturning(confirmation)
                                                }
                                            >
                                                退回修复
                                            </Button>
                                        </>
                                    ) : null}
                                </div>
                                {!canStart && !canConfirm ? (
                                    <p className="text-xs text-muted-foreground">
                                        {workItemTypeMissing
                                            ? "当前确认任务不完整，入口已阻断"
                                            : confirmation.result !== "PENDING"
                                              ? "本范围已有正式结论或已失效"
                                              : (task?.actionBlockers[0] ??
                                                "当前范围不由本人处理")}
                                    </p>
                                ) : null}
                            </CardContent>
                        </Card>
                    )
                })}
            </div>

            {confirmBlocked.length > 0 ? (
                <ul className="space-y-1 text-sm text-muted-foreground">
                    {confirmBlocked.map((blocker) => (
                        <li key={`${blocker.action}-${blocker.code}`}>
                            {blocker.message}
                        </li>
                    ))}
                </ul>
            ) : null}

            {confirming ? (
                <FormalActionConfirmDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title={`确认${CONFIRMATION_SCOPE_LABEL[confirming.scope]}`}
                    actionLabel="确认本范围"
                    description="系统将记录本范围正式确认事实，并在同一操作中完成当前任务。"
                    fromStatus={{ label: "待确认", tone: "warning" }}
                    toStatus={{ label: "已确认", tone: "success" }}
                    effects={["记录责任范围确认结论", "完成当前处理任务"]}
                    irreversibleEffects={[
                        "结论写入审计，试算变化后由新任务重新确认",
                    ]}
                    pending={operations.isCompleting}
                    onConfirm={() => complete(confirming, "CONFIRM_SCOPE")}
                />
            ) : null}

            {returning ? (
                <ReturnForFixDialog
                    confirmation={returning}
                    pending={operations.isCompleting}
                    onCancel={() => setReturning(undefined)}
                    onSubmit={async ({ reasonCode, comment }) => {
                        await complete(
                            returning,
                            "RETURN_FOR_FIX",
                            reasonCode,
                            comment,
                        )
                        setReturning(undefined)
                    }}
                />
            ) : null}
        </div>
    )
}

/** 独立提交应用、取消未应用项与重新准备失败项。 */
export function ImportExecutionActions({
    batch,
    onGoSection,
}: {
    batch: ImportBatchView
    onGoSection: (section: BatchSection) => void
}) {
    const operations = useImportExecutionOperations()
    const profileQuery = useAccountProfileQuery()
    const [confirming, setConfirming] = React.useState<
        "START_APPLY" | "RETRY_FAILED"
    >()
    const [cancelling, setCancelling] = React.useState(false)
    const idempotencyKeys = React.useRef(new Map<string, string>())
    const canExecute = hasPermission(
        profileQuery.data?.permissions,
        "legacy_import_batch:execute",
    )
    const canStart = canExecute && batch.allowedActions.includes("START_APPLY")
    const canCancel =
        canExecute && batch.allowedActions.includes("CANCEL_PENDING")
    const canRetry = canExecute && batch.allowedActions.includes("RETRY_FAILED")
    const visible = canStart || canCancel || canRetry

    const execute = React.useCallback(
        async (
            action: ImportExecutionAction,
            reasonCode?: string,
            comment?: string,
        ) => {
            operations.resetError()
            const identity = [
                batch.batchId,
                action,
                batch.version,
                batch.trialVersion,
                reasonCode ?? "",
                comment ?? "",
            ].join(":")
            const result = await operations.execute({
                batchId: batch.batchId,
                expectedBatchVersion: batch.version,
                expectedTrialVersion:
                    batch.trialVersion === "0" ? undefined : batch.trialVersion,
                action,
                reasonCode,
                comment: comment?.trim() || undefined,
                requestId: commandIdempotencyKey(
                    idempotencyKeys.current,
                    identity,
                ),
            })
            setConfirming(undefined)
            setCancelling(false)
            if (result.nextStep === "MONITOR_PROGRESS") {
                onGoSection("progress")
            } else if (result.nextStep === "REVIEW_RESULT") {
                onGoSection("result")
            } else {
                onGoSection("confirm")
            }
        },
        [batch, onGoSection, operations],
    )

    if (!visible) return null

    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>导入执行</CardTitle>
                <CardDescription>
                    责任确认只形成待应用状态；只有“提交应用”会启动后台任务。取消和失败项重试均保留已形成的业务事实。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {operations.error ? (
                    <BusinessFailureState
                        title="导入执行命令未完成"
                        error={operations.error}
                    />
                ) : null}
                <div className="flex flex-wrap gap-2">
                    {canStart ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={operations.isExecuting}
                            onClick={() => setConfirming("START_APPLY")}
                        >
                            提交应用
                        </Button>
                    ) : null}
                    {canCancel ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={operations.isExecuting}
                            onClick={() => setCancelling(true)}
                        >
                            取消未应用项
                        </Button>
                    ) : null}
                    {canRetry ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={operations.isExecuting}
                            onClick={() => setConfirming("RETRY_FAILED")}
                        >
                            重新准备失败项
                        </Button>
                    ) : null}
                </div>
            </CardContent>

            {confirming === "START_APPLY" ? (
                <FormalActionConfirmDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title="提交导入应用"
                    actionLabel="确认提交应用"
                    description="系统将再次核验批次和试算版本，随后把批次推进为导入中并启动后台任务。"
                    fromStatus={{ label: "待应用", tone: "success" }}
                    toStatus={{ label: "导入中", tone: "info" }}
                    effects={["启动关联后台任务", "只处理当前仍待应用的项"]}
                    irreversibleEffects={["已形成的业务对象不会由本批自动回滚"]}
                    pending={operations.isExecuting}
                    onConfirm={() => execute("START_APPLY")}
                />
            ) : null}

            {confirming === "RETRY_FAILED" ? (
                <FormalActionConfirmDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title="重新准备失败项"
                    actionLabel="确认重新准备"
                    description="系统只把上一轮失败行重新准备为待应用，不会在本动作中启动后台任务。"
                    fromStatus={{ label: "失败结果", tone: "destructive" }}
                    toStatus={{ label: "待应用", tone: "success" }}
                    effects={[
                        "保留已成功与已跳过结果",
                        "仅清理失败行的上次失败诊断",
                    ]}
                    irreversibleEffects={["准备完成后仍需再次点击“提交应用”"]}
                    pending={operations.isExecuting}
                    onConfirm={() => execute("RETRY_FAILED")}
                />
            ) : null}

            {cancelling ? (
                <CancelPendingDialog
                    pending={operations.isExecuting}
                    onCancel={() => setCancelling(false)}
                    onSubmit={async ({ reasonCode, comment }) => {
                        await execute("CANCEL_PENDING", reasonCode, comment)
                    }}
                />
            ) : null}
        </Card>
    )
}

export function ProgressSection({ batch }: { batch: ImportBatchView }) {
    const job = batch.backgroundJob
    if (!job) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="暂无导入任务"
                description="提交应用后将在此展示任务号、成功/跳过/失败计数与最近进度时间；刷新可恢复进度。"
            />
        )
    }

    return (
        <div className="space-y-4">
            <BackgroundJobProgress
                mode="partialAllowed"
                status={job.status}
                total={job.total}
                completed={job.processed}
                succeeded={job.succeeded}
                skipped={job.skipped}
                failed={job.failed}
                label={`导入执行进度 · ${batch.batchNo}`}
                description={
                    <span>
                        最近进度{" "}
                        {formatDateTime(
                            job.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}{" "}
                        ·
                        允许部分成功；已形成的处理结果不会因同批其它失败而回退。
                    </span>
                }
            />
            <Card size="sm" className={surfacePanelClassName}>
                <CardContent className="grid gap-3 pt-4 sm:grid-cols-4">
                    <Fact
                        label="已处理"
                        value={`${job.processed}/${job.total}`}
                        mono
                    />
                    <Fact label="成功" value={job.succeeded} mono />
                    <Fact label="跳过" value={job.skipped} mono />
                    <Fact label="失败" value={job.failed} mono />
                </CardContent>
            </Card>
        </div>
    )
}

export function ResultSection({
    batch,
    onOpenRepair,
}: {
    batch: ImportBatchView
    onOpenRepair: (batchId: string) => void
}) {
    const partitions = batch.applyPartitions

    if (!partitions && batch.stage !== "RESULT") {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="结果尚未形成"
                description="导入完成后，此处将列出成功、跳过与失败项，失败的记录可在此重新处理。"
            />
        )
    }

    return (
        <div className="space-y-4">
            {partitions ? (
                <BatchOperationResult
                    title="应用结果分区"
                    succeeded={partitions.succeeded.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    skipped={partitions.skipped.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    failed={partitions.failed.map((i) => ({
                        id: i.id,
                        label: i.label,
                        detail: i.detail,
                        code: i.code,
                    }))}
                    retryAction={
                        batch.repairBatchId ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() =>
                                    onOpenRepair(batch.repairBatchId!)
                                }
                            >
                                打开修复批次
                                {batch.repairBatchNo
                                    ? ` ${batch.repairBatchNo}`
                                    : ""}
                            </Button>
                        ) : undefined
                    }
                />
            ) : null}

            {batch.backgroundJob ? (
                <BackgroundJobProgress
                    mode="partialAllowed"
                    status={batch.backgroundJob.status}
                    total={batch.backgroundJob.total}
                    completed={batch.backgroundJob.processed}
                    succeeded={batch.backgroundJob.succeeded}
                    skipped={batch.backgroundJob.skipped}
                    failed={batch.backgroundJob.failed}
                    label="最终应用进度"
                />
            ) : null}

            <Alert>
                <AlertTitle>防重复与不可覆盖</AlertTitle>
                <AlertDescription>
                    已导入成功的数据不会因取消、重试或上传新文件而被覆盖或删除；重新处理仅针对失败项。
                </AlertDescription>
            </Alert>
        </div>
    )
}

export function AuditSection({ batch }: { batch: ImportBatchView }) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>可追溯谱系</CardTitle>
                <CardDescription>
                    来源身份、规则版本、manifest、成功结果与映射谱系可审计；详细事件在权限与审计中。
                </CardDescription>
            </CardHeader>
            <CardContent className="grid gap-3 pt-4 sm:grid-cols-2">
                <Fact label="批次号" value={batch.batchNo} mono />
                <Fact label="规则版本" value={batch.importRuleVersion} mono />
                <Fact label="试算版本" value={batch.trialVersion} mono />
                <Fact label="批次版本" value={batch.version} mono />
                {batch.inputAsset?.contentHmacShort ? (
                    <Fact
                        label={versionText.packageChecksum}
                        value={batch.inputAsset.contentHmacShort}
                        mono
                    />
                ) : null}
                {batch.linkedValidationBatchNo ? (
                    <Fact
                        label="关联验证/源批次"
                        value={batch.linkedValidationBatchNo}
                    />
                ) : null}
                <div className="sm:col-span-2">
                    <Button
                        size="sm"
                        variant="outline"
                        render={
                            <Link
                                href={`/system/access-audit?objectType=legacy_import_batch&objectId=${encodeURIComponent(batch.batchId)}`}
                            />
                        }
                    >
                        在权限与审计中查看
                        <ExternalLinkIcon className="size-4" />
                    </Button>
                </div>
            </CardContent>
        </Card>
    )
}
