"use client"

import Link from "next/link"

import { DocumentSection, DocumentSummary } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import type { MallConsumptionOrderView } from "@/features/mall-consumption-orders/types"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"

/** 动作阻断的中文动作名（审计 tab；命令码不允许上屏） */
const BLOCKED_ACTION_LABEL: Record<string, string> = {
    EDIT_MALL_ORDER: "修改商城订单",
    RETRY_SUPPLIER: "重试供应商下单",
}

export function AuditSection({ view }: { view: MallConsumptionOrderView }) {
    return (
        <DocumentSection title="审计与禁止动作">
            <DocumentSummary
                columns="two"
                items={[
                    {
                        id: "f-15241",
                        label: "记录更新时间",
                        value: (
                            <span className="num">
                                {formatDateTime(
                                    view.freshness.factWatermark,
                                    "default",
                                )}
                            </span>
                        ),
                    },
                    {
                        id: "f-92756",
                        label: "归集更新",
                        value: (
                            <span className="num">
                                {formatDateTime(
                                    view.freshness.attributionUpdatedAt,
                                    "default",
                                )}
                            </span>
                        ),
                    },
                    {
                        id: "f-24032",
                        label: "供应商更新",
                        value: (
                            <span className="num">
                                {formatDateTime(
                                    view.freshness.supplierUpdatedAt,
                                    "default",
                                )}
                            </span>
                        ),
                    },
                    {
                        id: "f-18033",
                        label: "成本评估",
                        value: (
                            <span className="num">
                                {formatDateTime(
                                    view.freshness.costAssessedAt,
                                    "default",
                                )}
                            </span>
                        ),
                    },
                ]}
            />
            <div className="mt-4 space-y-2">
                <p className="text-sm font-medium">动作阻断</p>
                {view.actionBlockers.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        无额外阻断
                    </p>
                ) : (
                    <ul className="space-y-2">
                        {view.actionBlockers.map((b) => (
                            <li
                                key={`${b.action}-${b.code}`}
                                className="rounded-lg bg-muted/40 p-3 text-sm"
                            >
                                {BLOCKED_ACTION_LABEL[b.action] ? (
                                    <span className="font-medium">
                                        {BLOCKED_ACTION_LABEL[b.action]}
                                    </span>
                                ) : null}
                                <div className="text-muted-foreground">
                                    {b.message}
                                </div>
                            </li>
                        ))}
                    </ul>
                )}
            </div>
            <Alert variant="default" className="mt-4">
                <AlertTitle>原始记录内容不在本页展示</AlertTitle>
                <AlertDescription>
                    如需排障，请前往接口错误中心查看脱敏摘要。
                    {view.workItemIds[0] ? (
                        <div className="mt-2">
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/governance/integration-errors?resolveWorkItemId=${view.workItemIds[0]}&queueContextId=queue:W29:mine:all`}
                                    />
                                }
                            >
                                {openWorkspaceLabel("W29")}
                            </Button>
                        </div>
                    ) : null}
                </AlertDescription>
            </Alert>
        </DocumentSection>
    )
}
