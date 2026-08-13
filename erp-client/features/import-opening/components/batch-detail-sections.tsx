"use client"

import * as React from "react"
import Link from "next/link"
import { ExternalLinkIcon, TriangleAlertIcon } from "lucide-react"

import {
    BackgroundJobProgress,
    BatchOperationResult,
    BusinessEmptyState,
    BusinessStatusBadge,
    FormalActionResult,
    ImportIssueTable,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Label } from "@/components/ui/label"
import { Separator } from "@/components/ui/separator"
import { Fact } from "@/features/import-opening/components/batch-facts"
import { useImportIssuesQuery } from "@/features/import-opening/hooks/queries"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type {
    BatchSection,
    ImportBatchView,
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
    WORK_ITEM_TYPE_BLOCKER,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"
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

export function ConfirmSection({
    batch,
    workItemTypeMissing,
    confirmBlocked,
}: {
    batch: ImportBatchView
    workItemTypeMissing: boolean
    confirmBlocked: ImportBatchView["actionBlockers"]
}) {
    return (
        <div className="space-y-4">
            {workItemTypeMissing ? (
                <FormalActionResult
                    status="blocked"
                    title="业务确认入口暂不可用"
                    description={WORK_ITEM_TYPE_BLOCKER.message}
                    facts={[
                        {
                            label: "待配置项",
                            value: WORK_ITEM_TYPE_BLOCKER.requiredRegistration.join(
                                "、",
                            ),
                        },
                        {
                            label: "禁止项",
                            value: "不得借用异常通道充当必经确认；不得上线页面私有任务类型",
                        },
                    ]}
                />
            ) : null}

            <div className="grid gap-3 md:grid-cols-2">
                {batch.confirmations.map((c) => {
                    const canAttempt =
                        c.inViewerResponsibility &&
                        !workItemTypeMissing &&
                        c.result === "PENDING"
                    return (
                        <Card
                            key={c.scope}
                            size="sm"
                            className={surfacePanelClassName}
                        >
                            <CardHeader className="border-b border-border/30">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <CardTitle className="text-base">
                                        {CONFIRMATION_SCOPE_LABEL[c.scope]}
                                    </CardTitle>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={
                                            c.result === "CONFIRMED"
                                                ? "已确认"
                                                : c.result === "REJECTED"
                                                  ? "已退回"
                                                  : c.result === "INVALIDATED"
                                                    ? "已失效"
                                                    : "待确认"
                                        }
                                        tone={
                                            c.result === "CONFIRMED"
                                                ? "success"
                                                : c.result === "REJECTED" ||
                                                    c.result === "INVALIDATED"
                                                  ? "destructive"
                                                  : "warning"
                                        }
                                    />
                                </div>
                                <CardDescription>
                                    试算版本{" "}
                                    <span className="num font-mono">
                                        {c.trialVersion}
                                    </span>
                                    {c.inViewerResponsibility
                                        ? " · 属于当前角色责任范围"
                                        : " · 非当前角色责任范围"}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 pt-4 text-sm">
                                {c.confirmedByLabel ? (
                                    <p>
                                        确认人 {c.confirmedByLabel}
                                        {c.confirmedAt
                                            ? ` · ${formatDateTime(c.confirmedAt, "dateStyle", "passthrough")}`
                                            : ""}
                                    </p>
                                ) : null}
                                {c.comment ? (
                                    <p className="text-muted-foreground">
                                        {c.comment}
                                    </p>
                                ) : null}
                                <div className="flex flex-wrap gap-2">
                                    {workItemTypeMissing ? (
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            render={
                                                <Link href="/workspace/tasks" />
                                            }
                                        >
                                            去待办队列处理
                                            <ExternalLinkIcon className="size-4" />
                                        </Button>
                                    ) : (
                                        <>
                                            <Button
                                                type="button"
                                                size="sm"
                                                disabled={!canAttempt}
                                            >
                                                确认本范围
                                            </Button>
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={!canAttempt}
                                            >
                                                退回修复
                                            </Button>
                                        </>
                                    )}
                                </div>
                                {!canAttempt ? (
                                    <p className="text-xs text-muted-foreground">
                                        {workItemTypeMissing
                                            ? "确认与退回任务尚未配置，入口暂不可用"
                                            : !c.inViewerResponsibility
                                              ? "非本人责任范围，只读"
                                              : c.result !== "PENDING"
                                                ? "本范围已有结论或已失效"
                                                : "当前不可操作"}
                                    </p>
                                ) : null}
                            </CardContent>
                        </Card>
                    )
                })}
            </div>

            {confirmBlocked.length > 0 ? (
                <ul className="space-y-1 text-sm text-muted-foreground">
                    {confirmBlocked.map((b) => (
                        <li key={`${b.action}-${b.code}`}>{b.message}</li>
                    ))}
                </ul>
            ) : null}
        </div>
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
