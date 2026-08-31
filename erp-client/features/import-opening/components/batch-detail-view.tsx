"use client"

import { ArrowLeftIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DocumentHeader,
    ImportStageIndicator,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
    workspaceEmbeddedScaffoldClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    AuditSection,
    ConfirmSection,
    FilesSection,
    ImportExecutionActions,
    OverviewSection,
    ProgressSection,
    ResultSection,
    TrialSection,
} from "@/features/import-opening/components/batch-detail-sections"
import { Fact } from "@/features/import-opening/components/batch-facts"
import { ProductionGateCard } from "@/features/import-opening/components/production-gate-card"
import {
    useImportBatchDetailQuery,
    useImportIssuesQuery,
} from "@/features/import-opening/hooks/queries"
import { formatObjectSet } from "@/features/import-opening/lib/labels"
import {
    buildStageStates,
    importStageLabels,
} from "@/features/import-opening/lib/pipeline"
import type { ImportOpeningUrlState } from "@/features/import-opening/lib/url-state"
import type { BatchSection } from "@/features/import-opening/types"
import {
    BATCH_STATUS_LABEL,
    BATCH_STATUS_TONE,
    ENVIRONMENT_LABEL,
    PIPELINE_STAGE_LABEL,
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

export function BatchDetailView({
    batchId,
    urlState,
    patchUrl,
    replaceUrl,
    embedded = false,
    onTaskCompleted,
}: {
    batchId: string
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    replaceUrl: (next: ImportOpeningUrlState) => void
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
}) {
    const detailQuery = useImportBatchDetailQuery({
        batchId,
        workItemId: urlState.workItemId,
        confirmationScope: urlState.confirmationScope,
        queueContextId: urlState.queueContextId,
    })
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
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                <BusinessFailureState
                    title="批次加载失败"
                    error={detailQuery.error}
                    action={
                        <Button
                            id="operations-import-batch-detail-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void detailQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    if (!batch) {
        return (
            <PageScaffold
                density={embedded ? "compact" : "default"}
                className={
                    embedded ? workspaceEmbeddedScaffoldClassName : undefined
                }
            >
                <BusinessEmptyState
                    kind="no-data"
                    title="批次不存在"
                    description="请返回列表或检查批次身份。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <Button
                            id="operations-import-batch-detail-empty-back"
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            onClick={() =>
                                replaceUrl({
                                    ...urlState,
                                    batchId: undefined,
                                    section: "overview",
                                    workItemId: undefined,
                                    confirmationScope: undefined,
                                    queueContextId: undefined,
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
    const confirmBlocked = batch.actionBlockers.filter(
        (b) => b.action === "CONFIRM_SCOPE",
    )
    const workItemTypeMissing = !batch.productionGates.workItemTypeRegistered

    return (
        <PageScaffold
            density={embedded ? "compact" : "default"}
            className={
                embedded ? workspaceEmbeddedScaffoldClassName : undefined
            }
        >
            {!embedded ? (
                <PageHeader
                    variant="object-chrome"
                    actions={
                        <Button
                            id="operations-import-batch-detail-back"
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
                                    workItemId: undefined,
                                    confirmationScope: undefined,
                                    queueContextId: undefined,
                                })
                            }
                        >
                            <ArrowLeftIcon className="size-4" />
                            返回批次列表
                        </Button>
                    }
                />
            ) : null}

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

            {!embedded ? (
                <Tabs
                    value={section}
                    onValueChange={(v) => {
                        if (v == null) return
                        patchUrl({ section: v as BatchSection })
                    }}
                >
                    <TabsList className="flex h-auto flex-wrap">
                        {SECTION_TABS.map((tab) => (
                            <TabsTrigger
                                key={tab.id}
                                id={`operations-import-batch-detail-section-${tab.id}-trigger`}
                                value={tab.id}
                            >
                                {tab.label}
                            </TabsTrigger>
                        ))}
                    </TabsList>
                </Tabs>
            ) : null}

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
                    onTaskCompleted={onTaskCompleted}
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

            {!embedded ? (
                <ImportExecutionActions
                    batch={batch}
                    onGoSection={(nextSection) =>
                        patchUrl({ section: nextSection })
                    }
                />
            ) : null}

            {/* 生产应用门禁：仅提交应用前阶段展示 */}
            {!embedded &&
            batch.stage !== "RESULT" &&
            batch.stage !== "APPLY" ? (
                <ProductionGateCard batch={batch} />
            ) : null}
        </PageScaffold>
    )
}
