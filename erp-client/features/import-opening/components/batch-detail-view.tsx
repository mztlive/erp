"use client"

import type { ReactNode } from "react"
import Link from "next/link"
import { TriangleAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    DetailWorkspaceSkeleton,
    DetailWorkspaceTabs,
    DocumentHeader,
    ImportStageIndicator,
    PageScaffold,
    workspaceEmbeddedScaffoldClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
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
    { id: "trial", label: "预检与问题" },
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
    backHref,
    backAlways = false,
    onOpenBatch,
    embedded = false,
    onTaskCompleted,
}: {
    batchId: string
    urlState: ImportOpeningUrlState
    patchUrl: (patch: Partial<ImportOpeningUrlState>) => void
    replaceUrl: (next: ImportOpeningUrlState) => void
    backHref?: string
    backAlways?: boolean
    onOpenBatch?: (targetBatchId: string, nextSection?: BatchSection) => void
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
}) {
    const section = urlState.section
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
        Boolean(batchId) && section === "trial",
    )

    const batch = detailQuery.data
    const resolvedBackHref =
        backHref ??
        (urlState.environment === "PRODUCTION"
            ? "/governance/imports?environment=PRODUCTION"
            : "/governance/imports")

    const chromeBack = {
        id: "operations-import-batch-detail-back",
        label: "返回批次列表",
        href: resolvedBackHref,
        visibility: backAlways
            ? ("always" as const)
            : ("standalone-only" as const),
    }

    const wrap = (children: ReactNode) =>
        embedded ? (
            <PageScaffold
                density="compact"
                className={workspaceEmbeddedScaffoldClassName}
            >
                {children}
            </PageScaffold>
        ) : (
            <DetailWorkspacePage>{children}</DetailWorkspacePage>
        )

    if (detailQuery.isPending) {
        if (!embedded) {
            return <DetailWorkspaceSkeleton title="导入批次详情" summary />
        }
        return wrap(
            <div
                className="h-40 animate-pulse rounded-lg bg-muted"
                aria-busy="true"
            />,
        )
    }

    if (detailQuery.isError) {
        const failureContent = (
            <DetailWorkspaceSection width="form">
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
            </DetailWorkspaceSection>
        )
        if (embedded) return wrap(failureContent)
        return wrap(
            <>
                <DetailWorkspaceChrome
                    title="导入批次详情"
                    meta="批次加载失败"
                    back={chromeBack}
                />
                <DetailWorkspaceBody>{failureContent}</DetailWorkspaceBody>
            </>,
        )
    }

    if (!batch) {
        const emptyContent = (
            <DetailWorkspaceSection width="form">
                <BusinessEmptyState
                    kind="no-data"
                    title="批次不存在"
                    description="未找到该导入批次，请返回列表重新选择。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        embedded ? (
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
                        ) : (
                            <Button
                                id="operations-import-batch-detail-empty-back"
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                render={<Link href={resolvedBackHref} />}
                            >
                                返回列表
                            </Button>
                        )
                    }
                />
            </DetailWorkspaceSection>
        )
        if (embedded) return wrap(emptyContent)
        return wrap(
            <>
                <DetailWorkspaceChrome
                    title="导入批次详情"
                    meta="批次不存在"
                    back={chromeBack}
                />
                <DetailWorkspaceBody>{emptyContent}</DetailWorkspaceBody>
            </>,
        )
    }

    const stageStates = buildStageStates(batch.stage)
    const confirmBlocked = batch.actionBlockers.filter(
        (b) => b.action === "CONFIRM_SCOPE",
    )
    const workItemTypeMissing = !batch.productionGates.workItemTypeRegistered
    const primaryStatus = {
        label: BATCH_STATUS_LABEL[batch.status],
        tone: BATCH_STATUS_TONE[batch.status],
    }
    const showExecutionSection =
        !embedded &&
        (section === "overview" ||
            section === "confirm" ||
            section === "progress")
    const showProductionGate =
        showExecutionSection &&
        batch.stage !== "RESULT" &&
        batch.stage !== "APPLY"

    return wrap(
        <>
            {embedded ? (
                <DocumentHeader
                    density="compact"
                    title={batch.sourceSystem.name}
                    documentNumber={batch.batchNo}
                    primaryStatus={primaryStatus}
                    version={batch.version}
                    meta={
                        <DetailWorkspaceMeta
                            items={[
                                ENVIRONMENT_LABEL[batch.environment],
                                <span key="baseline">
                                    基准日{" "}
                                    <span className="num">
                                        {batch.baselineDate}
                                    </span>
                                </span>,
                                <span key="rule">
                                    规则版本{" "}
                                    <span className="num">
                                        {batch.importRuleVersion}
                                    </span>
                                </span>,
                                `当前阶段：${PIPELINE_STAGE_LABEL[batch.stage]}`,
                            ]}
                        />
                    }
                />
            ) : (
                <DetailWorkspaceChrome
                    title={batch.sourceSystem.name}
                    primaryStatus={primaryStatus}
                    back={chromeBack}
                    meta={
                        <DetailWorkspaceMeta
                            items={[
                                <span key="batch-no">
                                    批次号：
                                    <span
                                        className="num"
                                        data-workspace-document-number={
                                            batch.batchNo
                                        }
                                    >
                                        {batch.batchNo}
                                    </span>
                                </span>,
                                <span key="version">
                                    版本{" "}
                                    <span className="num">{batch.version}</span>
                                </span>,
                                ENVIRONMENT_LABEL[batch.environment],
                                <span key="baseline">
                                    基准日{" "}
                                    <span className="num">
                                        {batch.baselineDate}
                                    </span>
                                </span>,
                                <span key="rule">
                                    规则版本{" "}
                                    <span className="num">
                                        {batch.importRuleVersion}
                                    </span>
                                </span>,
                                `当前阶段：${PIPELINE_STAGE_LABEL[batch.stage]}`,
                            ]}
                        />
                    }
                    tabs={
                        <DetailWorkspaceTabs
                            value={section}
                            onValueChange={(value) =>
                                patchUrl({
                                    section: value as BatchSection,
                                    page: 1,
                                })
                            }
                            items={SECTION_TABS.map((tab) => ({
                                ...tab,
                                triggerId: `operations-import-batch-detail-section-${tab.id}-trigger`,
                            }))}
                        />
                    }
                />
            )}

            <DetailWorkspaceBody
                summary={
                    <DetailWorkspaceMetrics
                        columns={4}
                        variant="bar"
                        ariaLabel="导入批次摘要"
                    >
                        <DetailWorkspaceMetricItem
                            label="导入数据类型"
                            value={formatObjectSet(batch.sourceObjectSet)}
                        />
                        <DetailWorkspaceMetricItem
                            label="预检版本"
                            value={batch.trialVersion}
                        />
                        <DetailWorkspaceMetricItem
                            label="发起人"
                            value={batch.initiatorLabel}
                        />
                        <DetailWorkspaceMetricItem
                            label="更新时间"
                            value={formatDateTime(
                                batch.updatedAt,
                                "dateStyle",
                                "passthrough",
                            )}
                        />
                    </DetailWorkspaceMetrics>
                }
                alerts={
                    <>
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
                                <AlertTitle>
                                    期初数据已写入系统（部分或全部）
                                </AlertTitle>
                                <AlertDescription>
                                    {batch.notFormalDataMessage}
                                </AlertDescription>
                            </Alert>
                        )}
                        {batch.invalidation ? (
                            <Alert variant="warning">
                                <TriangleAlertIcon />
                                <AlertTitle>旧确认已失效</AlertTitle>
                                <AlertDescription>
                                    {batch.invalidation.reason}（
                                    <span className="num">
                                        {formatDateTime(
                                            batch.invalidation.invalidatedAt,
                                            "dateStyle",
                                            "passthrough",
                                        )}
                                    </span>
                                    ）。数据或规则已更新，需按最新预检结果重新核对后才能正式导入。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </>
                }
            >
                <DetailWorkspaceSection>
                    <ImportStageIndicator
                        stages={stageStates}
                        stageLabels={importStageLabels}
                        aria-label="导入流程进度"
                    />
                </DetailWorkspaceSection>

                {section === "overview" ? (
                    <OverviewSection
                        batch={batch}
                        onGoSection={(s) => patchUrl({ section: s, page: 1 })}
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

                {section === "progress" ? (
                    <ProgressSection batch={batch} />
                ) : null}

                {section === "result" ? (
                    <ResultSection
                        batch={batch}
                        onOpenRepair={(id) => {
                            if (onOpenBatch) {
                                onOpenBatch(id, "progress")
                            } else {
                                patchUrl({
                                    batchId: id,
                                    section: "progress",
                                    page: 1,
                                })
                            }
                        }}
                    />
                ) : null}

                {section === "audit" ? <AuditSection batch={batch} /> : null}

                {showExecutionSection ? (
                    <ImportExecutionActions
                        batch={batch}
                        onGoSection={(nextSection) =>
                            patchUrl({ section: nextSection, page: 1 })
                        }
                    />
                ) : null}

                {/* 生产应用门禁：仅提交应用前阶段展示 */}
                {showProductionGate ? (
                    <ProductionGateCard batch={batch} />
                ) : null}
            </DetailWorkspaceBody>
        </>,
    )
}
