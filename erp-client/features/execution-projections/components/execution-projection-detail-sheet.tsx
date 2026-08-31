"use client"

import * as React from "react"
import { TriangleAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    DocumentHeader,
    DocumentSection,
    QuickPreviewSheet,
    StatusTrackSummary,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { DeliveryHistoryTable } from "@/features/execution-projections/components/delivery-history-table"
import {
    ExecutionProjectionDetailSecondaryActions,
    type RowCommandRequest,
} from "@/features/execution-projections/components/execution-projection-detail-actions"
import { ExecutionProjectionDiffPanel } from "@/features/execution-projections/components/execution-projection-diff-panel"
import { ExecutionProjectionOverviewSummary } from "@/features/execution-projections/components/execution-projection-overview-summary"
import { ExecutionProjectionVersionLinks } from "@/features/execution-projections/components/execution-projection-version-links"
import { WhitelistContentGrid } from "@/features/execution-projections/components/whitelist-content-grid"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    ExecutionProjectionRow,
    ExecutionProjectionView,
} from "@/features/execution-projections/types"

type ReplaceParams = (patch: Record<string, string | null | undefined>) => void

/** 对象中心半屏 / 主区：详情头部、三轨状态、概览/内容/历史/版本/差异页签。 */
export function ExecutionProjectionDetailSheet({
    open,
    onOpenChange,
    detail,
    isPending,
    isError,
    error,
    onRetry,
    rows,
    replaceParams,
    commandPending,
    onRequestRowCommand,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    detail: ExecutionProjectionView | undefined
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    rows: ExecutionProjectionRow[]
    replaceParams: ReplaceParams
    commandPending: boolean
    onRequestRowCommand: (action: RowCommandRequest) => void
}) {
    const [objectTab, setObjectTab] = React.useState("overview")

    return (
        <QuickPreviewSheet
            id="execution-projections-detail-sheet"
            open={open}
            onOpenChange={(next) => {
                if (!next) onOpenChange(false)
            }}
            size="detail"
            title={
                detail
                    ? `执行信息 · ${detail.identity.salesOrderNo}`
                    : "执行信息对象"
            }
            description={
                detail
                    ? `${detail.identity.projectionNo} · ${detail.identity.targetMallName}`
                    : "加载中…"
            }
            identity={
                detail ? (
                    <span className="num">{detail.identity.projectionId}</span>
                ) : null
            }
        >
            {isPending ? (
                <div className="h-48 animate-pulse rounded-lg bg-muted" />
            ) : isError ? (
                <BusinessFailureState
                    error={error}
                    action={
                        <Button
                            id="execution-projections-detail-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onRetry}
                        >
                            重试
                        </Button>
                    }
                />
            ) : !detail ? (
                <BusinessEmptyState
                    kind="no-data"
                    title="暂无执行信息"
                    description="未找到对应记录，请刷新后重试或返回列表查看。"
                />
            ) : (
                <div className="flex flex-col gap-4">
                    <DocumentHeader
                        density="compact"
                        title={detail.identity.salesOrderNo}
                        documentNumber={detail.identity.projectionNo}
                        version={`数据 v${detail.selectedRevision.revisionNo} · ERP v${detail.selectedRevision.salesOrderRevisionNo}`}
                        primaryStatus={{
                            label: detail.tracks.projectionDelivery.label,
                            tone: detail.tracks.projectionDelivery.tone,
                        }}
                        meta={
                            <span className="text-muted-foreground">
                                {detail.identity.targetMallName}
                            </span>
                        }
                        statuses={[
                            {
                                id: "sales-fact",
                                label: "销售记录",
                                status: {
                                    label: detail.tracks.salesFact.label,
                                    tone: detail.tracks.salesFact.tone,
                                },
                            },
                            {
                                id: "delivery",
                                label: "信息发送",
                                status: {
                                    label: detail.tracks.projectionDelivery
                                        .label,
                                    tone: detail.tracks.projectionDelivery.tone,
                                },
                            },
                            {
                                id: "mall",
                                label: "商城确认",
                                status: {
                                    label: detail.tracks.mallConfirm.label,
                                    tone: detail.tracks.mallConfirm.tone,
                                },
                            },
                        ]}
                        primaryAction={
                            detail.allowedActions.includes("QUERY_RESULT") ? (
                                <Button
                                    id={`execution-projections-detail-${toAutomationIdSegment(detail.identity.projectionId)}-query`}
                                    type="button"
                                    size="sm"
                                    disabled={commandPending}
                                    onClick={() => {
                                        const row = rows.find(
                                            (r) =>
                                                r.projectionId ===
                                                detail.identity.projectionId,
                                        )
                                        if (!row) return
                                        onRequestRowCommand({
                                            kind: "QUERY_RESULT",
                                            row,
                                            objectVersion: detail.objectVersion,
                                        })
                                    }}
                                >
                                    查询结果
                                </Button>
                            ) : undefined
                        }
                        secondaryActions={
                            <ExecutionProjectionDetailSecondaryActions
                                detail={detail}
                                rows={rows}
                                commandPending={commandPending}
                                onRequestRowCommand={onRequestRowCommand}
                            />
                        }
                    />

                    <Alert>
                        <TriangleAlertIcon aria-hidden="true" />
                        <AlertTitle>只读提示</AlertTitle>
                        <AlertDescription>
                            {detail.boundaryNotice}
                        </AlertDescription>
                    </Alert>

                    <StatusTrackSummary
                        aria-label="详情三轨状态"
                        variant="table"
                        tracks={[
                            {
                                id: "sales-fact",
                                label: "销售记录",
                                status: {
                                    label: detail.tracks.salesFact.label,
                                    tone: detail.tracks.salesFact.tone,
                                    description:
                                        detail.tracks.salesFact.description,
                                },
                            },
                            {
                                id: "projection-delivery",
                                label: "信息发送",
                                status: {
                                    label: detail.tracks.projectionDelivery
                                        .label,
                                    tone: detail.tracks.projectionDelivery.tone,
                                    description:
                                        detail.tracks.projectionDelivery
                                            .description,
                                },
                            },
                            {
                                id: "mall-confirm",
                                label: "商城确认",
                                status: {
                                    label: detail.tracks.mallConfirm.label,
                                    tone: detail.tracks.mallConfirm.tone,
                                    description:
                                        detail.tracks.mallConfirm.description,
                                },
                            },
                        ]}
                    />

                    <Tabs value={objectTab} onValueChange={setObjectTab}>
                        <TabsList>
                            <TabsTrigger
                                id="execution-projections-detail-tab-overview"
                                value="overview"
                            >
                                概览
                            </TabsTrigger>
                            <TabsTrigger
                                id="execution-projections-detail-tab-content"
                                value="content"
                            >
                                执行内容
                            </TabsTrigger>
                            <TabsTrigger
                                id="execution-projections-detail-tab-history"
                                value="history"
                            >
                                发送历史
                            </TabsTrigger>
                            <TabsTrigger
                                id="execution-projections-detail-tab-versions"
                                value="versions"
                            >
                                版本对应
                            </TabsTrigger>
                            <TabsTrigger
                                id="execution-projections-detail-tab-diff"
                                value="diff"
                            >
                                差异与错误
                            </TabsTrigger>
                        </TabsList>
                    </Tabs>

                    {objectTab === "overview" ? (
                        <ExecutionProjectionOverviewSummary detail={detail} />
                    ) : null}

                    {objectTab === "content" ? (
                        <DocumentSection
                            title="执行内容"
                            description="字段以系统数据修订为准。不含成交金额、配赠、税率、开票、应收、玩法规则。"
                        >
                            <WhitelistContentGrid
                                content={detail.selectedRevision.content}
                                revisionNo={detail.selectedRevision.revisionNo}
                            />
                        </DocumentSection>
                    ) : null}

                    {objectTab === "history" ? (
                        <DocumentSection title="发送历史">
                            <DeliveryHistoryTable
                                deliveries={detail.deliveries}
                            />
                        </DocumentSection>
                    ) : null}

                    {objectTab === "versions" ? (
                        <ExecutionProjectionVersionLinks
                            detail={detail}
                            replaceParams={replaceParams}
                        />
                    ) : null}

                    {objectTab === "diff" ? (
                        <ExecutionProjectionDiffPanel detail={detail} />
                    ) : null}
                </div>
            )}
        </QuickPreviewSheet>
    )
}
