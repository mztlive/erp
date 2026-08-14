"use client"

import Link from "next/link"
import { ExternalLinkIcon, TriangleAlertIcon } from "lucide-react"

import { DocumentSection } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { w29Href } from "@/features/execution-projections/lib/url-state"
import type { ExecutionProjectionView } from "@/features/execution-projections/types"
import { RECONCILIATION_LABEL } from "@/features/execution-projections/types"

export function ExecutionProjectionDiffPanel({
    detail,
}: {
    detail: ExecutionProjectionView
}) {
    return (
        <DocumentSection title="差异与错误">
            {detail.reconciliationStatus === "VERSION_MISMATCH" ? (
                <Alert variant="warning" className="mb-3">
                    <TriangleAlertIcon aria-hidden="true" />
                    <AlertTitle>版本对账差异</AlertTitle>
                    <AlertDescription>
                        {RECONCILIATION_LABEL.VERSION_MISMATCH}
                        。请前往接口错误中心核对；本页不提供覆盖任一侧记录。
                        <div className="mt-2">
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link
                                        href={w29Href(
                                            detail.deliveries[0]?.workItemId,
                                            detail.deliveries[0]?.errorTaskId,
                                        )}
                                    />
                                }
                            >
                                打开接口错误差异任务
                            </Button>
                        </div>
                    </AlertDescription>
                </Alert>
            ) : (
                <p className="text-sm text-muted-foreground">
                    当前无版本对账差异。
                </p>
            )}
            {detail.deliveries[0]?.errorSummary ? (
                <div className="rounded-xl border p-3 text-sm">
                    <div className="font-medium">失败摘要</div>
                    <p className="mt-1 text-muted-foreground">
                        {detail.deliveries[0].errorCode
                            ? `${detail.deliveries[0].errorCode} · `
                            : ""}
                        {detail.deliveries[0].errorSummary}
                    </p>
                    {detail.deliveries[0].workItemId ? (
                        <div className="mt-2 flex flex-wrap items-center gap-2">
                            <Badge variant="secondary">关联错误任务</Badge>
                            <Button
                                type="button"
                                size="xs"
                                variant="outline"
                                render={
                                    <Link
                                        href={w29Href(
                                            detail.deliveries[0].workItemId,
                                            detail.deliveries[0].errorTaskId,
                                        )}
                                    />
                                }
                            >
                                <ExternalLinkIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                在接口错误中心处理
                            </Button>
                        </div>
                    ) : null}
                </div>
            ) : null}
            <p className="mt-3 text-xs text-muted-foreground">
                本页不建立处理责任，也不支持转交或完成处理任务。
            </p>
        </DocumentSection>
    )
}
