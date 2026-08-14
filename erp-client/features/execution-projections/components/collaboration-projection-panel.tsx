"use client"

import * as React from "react"

import {
    BusinessStatusBadge,
    StatusTrackSummary,
    surfaceInsetClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import type { SalesOrderCollaborationSummary } from "@/features/execution-projections/types"
import {
    DELIVERY_STATUS_LABEL,
    RECONCILIATION_LABEL,
} from "@/features/execution-projections/types"

/**
 * 协同子区中部：三轨进度 + 当前推送摘要 + 推给商城的内容预览。
 * children 渲染进同一 grid（消费情况区块横跨整行，见 W05 布局契约）。
 */
export function CollaborationProjectionPanel({
    data,
    children,
}: {
    data: SalesOrderCollaborationSummary
    children?: React.ReactNode
}) {
    const tracks = data.tracks
    const preview = data.whitelistPreview

    return (
        <>
            {tracks ? (
                <StatusTrackSummary
                    aria-label="与商城对接进度"
                    variant="table"
                    tracks={[
                        {
                            id: "sales-fact",
                            label: "销售生效",
                            status: {
                                label: tracks.salesFact.label,
                                tone: tracks.salesFact.tone,
                                description: tracks.salesFact.description,
                            },
                        },
                        {
                            id: "projection-delivery",
                            label: "信息发出",
                            status: {
                                label: tracks.projectionDelivery.label,
                                tone: tracks.projectionDelivery.tone,
                                description:
                                    tracks.projectionDelivery.description,
                            },
                        },
                        {
                            id: "mall-confirm",
                            label: "商城确认",
                            status: {
                                label: tracks.mallConfirm.label,
                                tone: tracks.mallConfirm.tone,
                                description: tracks.mallConfirm.description,
                            },
                        },
                    ]}
                />
            ) : null}

            <div className="mt-4 grid gap-3 sm:grid-cols-2">
                <section className={`${surfaceInsetClassName} space-y-2 p-3`}>
                    <div>
                        <h3 className="text-sm font-medium">当前推送</h3>
                        <p className="mt-1 text-xs text-muted-foreground">
                            {data.projectionNo}
                            {data.projectionRevisionNo != null
                                ? ` · 推送 v${data.projectionRevisionNo}`
                                : ""}
                            {data.salesOrderRevisionNo != null
                                ? ` · 对应销售 v${data.salesOrderRevisionNo}`
                                : ""}
                        </p>
                    </div>
                    <div className="space-y-2 text-sm">
                        <div className="flex flex-wrap items-center gap-2">
                            <span className="text-muted-foreground">
                                目标商城
                            </span>
                            <span>{data.targetMallName ?? "—"}</span>
                        </div>
                        {data.delivery ? (
                            <div className="flex flex-wrap items-center gap-2">
                                <span className="text-muted-foreground">
                                    接收状态
                                </span>
                                <BusinessStatusBadge
                                    context="detail"
                                    label={
                                        data.delivery.statusLabel ??
                                        DELIVERY_STATUS_LABEL[
                                            data.delivery.status
                                        ]
                                    }
                                    tone={data.delivery.statusTone}
                                />
                            </div>
                        ) : null}
                        {data.currentAckedRevisionNo != null ? (
                            <div className="text-muted-foreground">
                                商城已确认版本{" "}
                                <span className="num text-foreground">
                                    v{data.currentAckedRevisionNo}
                                </span>
                            </div>
                        ) : (
                            <div className="text-muted-foreground">
                                商城尚未确认
                            </div>
                        )}
                        {data.reconciliationStatus === "VERSION_MISMATCH" ? (
                            <Badge variant="warning">
                                {RECONCILIATION_LABEL.VERSION_MISMATCH}
                            </Badge>
                        ) : null}
                        {data.delivery?.errorSummary ? (
                            <p className="text-xs text-destructive">
                                {data.delivery.errorSummary}
                            </p>
                        ) : null}
                        <p className="text-xs text-muted-foreground">
                            共 {data.historyCount}{" "}
                            次推送记录；历史会写明对应哪一版销售单。
                        </p>
                    </div>
                </section>

                <section className={`${surfaceInsetClassName} space-y-2 p-3`}>
                    <div>
                        <h3 className="text-sm font-medium">推给商城的内容</h3>
                        <p className="mt-1 text-xs text-muted-foreground">
                            只含卡券基础信息，不含金额、税率、开票和玩法。
                        </p>
                    </div>
                    {preview ? (
                        <dl className="grid grid-cols-2 gap-2 text-sm">
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    卡券类目
                                </dt>
                                <dd>{preview.voucherCategoryErpName}</dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    面额
                                </dt>
                                <dd className="num">{preview.faceValue}</dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    数量
                                </dt>
                                <dd className="num">{preview.cardCount}</dd>
                            </div>
                            <div>
                                <dt className="text-xs text-muted-foreground">
                                    卡形态
                                </dt>
                                <dd>{preview.cardForm}</dd>
                            </div>
                            <div className="col-span-2">
                                <dt className="text-xs text-muted-foreground">
                                    履约期限
                                </dt>
                                <dd className="num">
                                    {preview.voucherExpiryAt}
                                </dd>
                            </div>
                        </dl>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            暂无摘要
                        </p>
                    )}
                </section>

                {children}
            </div>
        </>
    )
}
