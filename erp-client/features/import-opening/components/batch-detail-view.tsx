"use client"

import { ArrowLeftIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DocumentHeader,
    FormalActionResult,
    ImportStageIndicator,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
    type ImportStageStates,
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
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    AuditSection,
    ConfirmSection,
    FilesSection,
    OverviewSection,
    ProgressSection,
    ResultSection,
    TrialSection,
} from "@/features/import-opening/components/batch-detail-sections"
import { Fact, GateRow } from "@/features/import-opening/components/batch-facts"
import {
    useImportBatchDetailQuery,
    useImportIssuesQuery,
} from "@/features/import-opening/hooks/queries"
import { formatObjectSet } from "@/features/import-opening/lib/labels"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type {
    BatchSection,
    ImportPipelineStage,
} from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    BATCH_STATUS_TONE,
    ENVIRONMENT_LABEL,
    PIPELINE_STAGE_LABEL,
    PIPELINE_TO_INDICATOR,
} from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

const SECTION_TABS: { id: BatchSection; label: string }[] = [
    { id: "overview", label: "概览" },
    { id: "files", label: "文件与规则" },
    { id: "trial", label: "试算与问题" },
    { id: "confirm", label: "责任确认" },
    { id: "progress", label: "执行进度" },
    { id: "result", label: "结果" },
    { id: "audit", label: "审计" },
]

const PIPELINE_ORDER: ImportPipelineStage[] = [
    "RECEIVE",
    "VALIDATE",
    "TRIAL",
    "CONFIRM",
    "APPLY",
    "RESULT",
]

function buildStageStates(current: ImportPipelineStage): ImportStageStates {
    const currentIdx = PIPELINE_ORDER.indexOf(current)
    const states: {
        [K in import("@/components/business").ImportStageKey]: {
            status: "pending" | "current" | "complete" | "failed"
            description?: string
        }
    } = {
        upload: { status: "pending" },
        mapping: { status: "pending" },
        validation: { status: "pending" },
        preview: { status: "pending" },
        submission: { status: "pending" },
        result: { status: "pending" },
    }
    for (let i = 0; i < PIPELINE_ORDER.length; i += 1) {
        const stage = PIPELINE_ORDER[i]!
        const key = PIPELINE_TO_INDICATOR[stage]
        let status: "pending" | "current" | "complete" | "failed" = "pending"
        if (i < currentIdx) status = "complete"
        else if (i === currentIdx) status = "current"
        states[key] = {
            status,
            description: PIPELINE_STAGE_LABEL[stage],
        }
    }
    return states
}

export function BatchDetailView({
    batchId,
    urlState,
    patchUrl,
    replaceUrl,
}: {
    batchId: string
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    replaceUrl: (next: ImportOpeningUrlState) => void
}) {
    const detailQuery = useImportBatchDetailQuery(batchId)
    const issueQuery = useImportIssuesQuery(
        {
            batchId,
            issueCode: urlState.issueCode ?? "all",
            objectType: urlState.issueObjectType ?? "all",
            rowStatus: urlState.rowStatus ?? "all",
            page: Math.max(1, urlState.page),
            pageSize: 20,
        },
        Boolean(batchId),
    )

    const batch = detailQuery.data
    const section = urlState.section

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="批次加载失败"
                    error={detailQuery.error}
                    onRetry={() => void detailQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (!batch) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="批次不存在"
                    description="请返回列表或检查批次身份。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() =>
                                replaceUrl({
                                    ...urlState,
                                    batchId: undefined,
                                    section: "overview",
                                })
                            }
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const stageStates = buildStageStates(batch.stage)
    const importStageLabels = {
        upload: PIPELINE_STAGE_LABEL.RECEIVE,
        mapping: PIPELINE_STAGE_LABEL.VALIDATE,
        validation: PIPELINE_STAGE_LABEL.TRIAL,
        preview: PIPELINE_STAGE_LABEL.CONFIRM,
        submission: PIPELINE_STAGE_LABEL.APPLY,
        result: PIPELINE_STAGE_LABEL.RESULT,
    }
    const confirmBlocked = batch.actionBlockers.filter(
        (b) => b.action === "CONFIRM_SCOPE",
    )
    const applyBlocked = batch.actionBlockers.filter(
        (b) => b.action === "START_APPLY",
    )
    const workItemTypeMissing = !batch.productionGates.workItemTypeRegistered

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    { id: "gov", label: "治理", href: "/governance/imports" },
                    {
                        id: "imp",
                        label: "导入与期初",
                        href: "/governance/imports",
                    },
                    {
                        id: "batch",
                        label: batch.batchNo,
                        current: true,
                    },
                ]}
                actions={
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() =>
                            replaceUrl({
                                ...urlState,
                                batchId: undefined,
                                section: "overview",
                                issueCode: undefined,
                                issueObjectType: undefined,
                                rowStatus: undefined,
                            })
                        }
                    >
                        <ArrowLeftIcon className="size-4" />
                        返回批次列表
                    </Button>
                }
            />

            <DocumentHeader
                density="compact"
                title={batch.sourceSystem.name}
                documentNumber={batch.batchNo}
                primaryStatus={{
                    label: BATCH_STATUS_LABEL[batch.status],
                    tone: BATCH_STATUS_TONE[batch.status],
                }}
                version={batch.version}
                meta={
                    <span className="text-muted-foreground">
                        {ENVIRONMENT_LABEL[batch.environment]} · 基准日{" "}
                        {batch.baselineDate}
                    </span>
                }
                statuses={[
                    {
                        id: "env",
                        label: "环境",
                        status: {
                            label: ENVIRONMENT_LABEL[batch.environment],
                            tone:
                                batch.environment === "PRODUCTION"
                                    ? "destructive"
                                    : "info",
                        },
                    },
                    {
                        id: "baseline",
                        label: "基准日",
                        status: {
                            label: batch.baselineDate,
                            tone: "neutral",
                        },
                    },
                    {
                        id: "rule",
                        label: "规则版本",
                        status: {
                            label: batch.importRuleVersion,
                            tone: "neutral",
                        },
                    },
                    {
                        id: "stage",
                        label: "当前阶段",
                        status: {
                            label: PIPELINE_STAGE_LABEL[batch.stage],
                            tone: "info",
                        },
                    },
                ]}
            />

            {/* 批次身份摘要：对象集、试算版本、来源系统等（环境/基准日/规则版本见页头） */}
            <Card size="sm" className={surfacePanelClassName}>
                <CardContent className="grid gap-3 pt-4 sm:grid-cols-2 lg:grid-cols-4">
                    <Fact
                        label="对象集合"
                        value={formatObjectSet(batch.sourceObjectSet)}
                    />
                    <Fact label="试算版本" value={batch.trialVersion} mono />
                    <Fact label="来源系统" value={batch.sourceSystem.name} />
                    <Fact label="发起人" value={batch.initiatorLabel} />
                    <Fact
                        label="更新时间"
                        value={formatDateTime(
                            batch.updatedAt,
                            "dateStyle",
                            "passthrough",
                        )}
                    />
                </CardContent>
            </Card>

            {!batch.formalDataFormed ? (
                <Alert variant="warning">
                    <TriangleAlertIcon />
                    <AlertTitle>尚未形成业务数据</AlertTitle>
                    <AlertDescription>
                        {batch.notFormalDataMessage}
                    </AlertDescription>
                </Alert>
            ) : (
                <Alert variant="success">
                    <AlertTitle>已形成业务对象（部分或全部）</AlertTitle>
                    <AlertDescription>
                        {batch.notFormalDataMessage}
                    </AlertDescription>
                </Alert>
            )}

            <ImportStageIndicator
                stages={stageStates}
                stageLabels={importStageLabels}
                aria-label="导入六段流水线"
            />

            <Tabs
                value={section}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({ section: v as BatchSection })
                }}
            >
                <TabsList className="flex h-auto flex-wrap">
                    {SECTION_TABS.map((tab) => (
                        <TabsTrigger key={tab.id} value={tab.id}>
                            {tab.label}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            {section === "overview" ? (
                <OverviewSection
                    batch={batch}
                    onGoSection={(s) => patchUrl({ section: s })}
                />
            ) : null}

            {section === "files" ? <FilesSection batch={batch} /> : null}

            {section === "trial" ? (
                <TrialSection
                    batch={batch}
                    urlState={urlState}
                    patchUrl={patchUrl}
                    issueQuery={issueQuery}
                />
            ) : null}

            {section === "confirm" ? (
                <ConfirmSection
                    batch={batch}
                    workItemTypeMissing={workItemTypeMissing}
                    confirmBlocked={confirmBlocked}
                />
            ) : null}

            {section === "progress" ? <ProgressSection batch={batch} /> : null}

            {section === "result" ? (
                <ResultSection
                    batch={batch}
                    onOpenRepair={(id) =>
                        patchUrl({ batchId: id, section: "progress", page: 1 })
                    }
                />
            ) : null}

            {section === "audit" ? <AuditSection batch={batch} /> : null}

            {/* 生产应用门禁：仅提交应用前阶段展示 */}
            {batch.stage !== "RESULT" && batch.stage !== "APPLY" ? (
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>提交前检查</CardTitle>
                        <CardDescription>
                            验证环境校验与业务确认是生产应用前置条件；系统管理员不能代替确认。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3 pt-4">
                        <GateRow
                            ok={batch.productionGates.validationEnvPassed}
                            label="验证环境校验与确认已通过并关联"
                        />
                        <GateRow
                            ok={batch.productionGates.allConfirmationsComplete}
                            label="全部必要责任确认完成"
                        />
                        <GateRow
                            ok={batch.productionGates.noBlockingIssues}
                            label="无阻塞校验问题"
                        />
                        <GateRow
                            ok={batch.productionGates.trialVersionMatches}
                            label="试算版本与确认一致（未因规则变化失效）"
                        />
                        <GateRow
                            ok={batch.productionGates.ruleVersionStable}
                            label="规则版本稳定"
                        />
                        <GateRow
                            ok={batch.productionGates.workItemTypeRegistered}
                            label="导入确认任务类型已登记"
                        />
                        {applyBlocked.length > 0 ? (
                            <FormalActionResult
                                status="blocked"
                                title="提交生产应用已阻断"
                                description={applyBlocked
                                    .map((b) => b.message)
                                    .join(" ")}
                                facts={applyBlocked.map((b) => ({
                                    label: b.code,
                                    value: b.message,
                                }))}
                            />
                        ) : (
                            <FormalActionResult
                                status="succeeded"
                                title={
                                    batch.stage === "CONFIRM"
                                        ? "检查已完成，可提交应用"
                                        : "检查已完成"
                                }
                                description="提交时系统会再次核验权限与数据，确认无误后开始导入。"
                            />
                        )}
                    </CardContent>
                </Card>
            ) : null}
        </PageScaffold>
    )
}
