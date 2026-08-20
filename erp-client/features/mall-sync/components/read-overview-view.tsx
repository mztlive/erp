"use client"

import {
    BusinessStatusBadge,
    surfacePanelClassName,
} from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import type {
    MallSyncJobRow,
    MallSyncPageView,
} from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"
import type { PatchUrl } from "@/features/mall-sync/components/mall-sync-read-views"

type MallSyncOverviewViewProps = {
    context: MallSyncPageView["context"] | undefined
    ownership: MallSyncPageView["context"]["ownership"] | undefined
    jobs: MallSyncJobRow[]
    patchUrl: PatchUrl
}

export function MallSyncOverviewView({
    context,
    ownership,
    jobs,
    patchUrl,
}: MallSyncOverviewViewProps) {
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>运行摘要</CardTitle>
                    <CardDescription>
                        同步进度仅证明来源数据已捕获，不证明映射或应收已成功。
                    </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2 text-sm">
                    <div className="flex justify-between gap-2">
                        <span className="text-muted-foreground">
                            当前同步进度
                        </span>
                        <span className="num text-xs">
                            {context?.freshness.currentWatermark
                                ? formatDateTime(
                                      context.freshness.currentWatermark,
                                      "default",
                                  )
                                : "—"}
                        </span>
                    </div>
                    <div className="flex justify-between gap-2">
                        <span className="text-muted-foreground">最近成功</span>
                        <span>
                            {formatDateTime(
                                context?.freshness.latestSuccessfulJobAt,
                                "default",
                            )}
                        </span>
                    </div>
                    <div className="flex justify-between gap-2">
                        <span className="text-muted-foreground">
                            来源数据更新时间
                        </span>
                        <span>
                            {formatDateTime(
                                context?.freshness.sourceSafeTime,
                                "default",
                            )}
                        </span>
                    </div>
                    <div className="flex justify-between gap-2">
                        <span className="text-muted-foreground">主责数量</span>
                        <span>
                            商城 {ownership?.mallOwnedOrderCount ?? "—"} · ERP{" "}
                            {ownership?.erpOwnedOrderCount ?? "—"}
                        </span>
                    </div>
                    <Separator />
                    <p className="text-muted-foreground">
                        同步失败不阻塞商城销售/制卡/绑定/激活/消费；差异在 ERP
                        侧处理，无「手工补建销售单」入口。
                    </p>
                </CardContent>
            </Card>
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>最近同步任务</CardTitle>
                </CardHeader>
                <CardContent className="space-y-2">
                    {jobs.slice(0, 4).map((job) => (
                        <button
                            key={job.jobId}
                            type="button"
                            className="flex w-full items-center justify-between rounded-lg border px-3 py-2 text-left text-sm hover:bg-accent/50"
                            onClick={() =>
                                patchUrl({
                                    view: "jobs",
                                    jobId: job.jobId,
                                })
                            }
                        >
                            <span className="font-medium">{job.jobNo}</span>
                            <BusinessStatusBadge
                                context="list"
                                label={job.statusLabel}
                                tone={job.statusTone}
                            />
                        </button>
                    ))}
                    {jobs.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            暂无同步任务。
                        </p>
                    ) : null}
                </CardContent>
            </Card>
        </div>
    )
}
