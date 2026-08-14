"use client"

import Link from "next/link"
import { ExternalLinkIcon, ShieldAlertIcon } from "lucide-react"

import { MaintenanceBanner } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { MallSyncPageView, MallSyncViewName } from "@/features/mall-sync/types"
import { DIRECTION_LABEL, STAGE_LABEL } from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"
import { workspaceLabel } from "@/lib/ui-text"

type MallSyncOwnershipBannerProps = {
    ownership: MallSyncPageView["context"]["ownership"] | undefined
    sealed: boolean
    view: MallSyncViewName
    onEnterHistory: () => void
}

export function MallSyncOwnershipBanner({
    ownership,
    sealed,
    view,
    onEnterHistory,
}: MallSyncOwnershipBannerProps) {
    return (
        <>
            {/* OwnershipBanner — 始终可见 */}
            {ownership ? (
                <MaintenanceBanner
                    tone={sealed ? "info" : "info"}
                    icon={sealed ? ShieldAlertIcon : undefined}
                    title={
                        sealed
                            ? `第一期已封存 · ${DIRECTION_LABEL[ownership.syncDirection]}`
                            : `当前主责：${STAGE_LABEL[ownership.stage]} · 方向 ${DIRECTION_LABEL[ownership.syncDirection]}`
                    }
                    description={
                        <div className="space-y-1 text-sm">
                            <p>
                                <span className="font-medium">商城边界：</span>
                                {ownership.mallWriteBoundary}
                            </p>
                            <p>
                                <span className="font-medium">ERP 边界：</span>
                                {ownership.erpWriteBoundary}
                            </p>
                            {ownership.sealedAt ? (
                                <p>
                                    封存时间{" "}
                                    {formatDateTime(
                                        ownership.sealedAt,
                                        "default",
                                    )}
                                    {ownership.finalWatermark
                                        ? ` · 最终同步点 ${formatDateTime(ownership.finalWatermark, "default")}`
                                        : ""}
                                </p>
                            ) : null}
                            <p className="text-muted-foreground">
                                无「编辑来源数据」「向商城回写商业修改」「手工标记同步成功」入口。
                            </p>
                        </div>
                    }
                />
            ) : null}

            {sealed && (
                <div className="flex flex-wrap gap-2 text-sm">
                    <Button
                        variant="link"
                        size="sm"
                        render={<Link href="/commerce/execution-projections" />}
                    >
                        {workspaceLabel("W23")}
                        <ExternalLinkIcon className="size-3.5" />
                    </Button>
                    <Button
                        variant="link"
                        size="sm"
                        render={<Link href="/governance/integration-errors" />}
                    >
                        {workspaceLabel("W29")}
                        <ExternalLinkIcon className="size-3.5" />
                    </Button>
                    {sealed && view !== "history" ? (
                        <Button
                            type="button"
                            variant="secondary"
                            size="sm"
                            onClick={onEnterHistory}
                        >
                            进入历史只读
                        </Button>
                    ) : null}
                </div>
            )}
        </>
    )
}
