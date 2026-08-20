import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { surfacePanelClassName } from "@/components/business"
import { HistoryBackfillFact as Fact } from "@/features/history-backfill/components/history-backfill-fact"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import type { HistoryBackfillJobCore } from "@/features/history-backfill/types"
import { PIPELINE_STAGE_LABEL } from "@/features/history-backfill/types"

export function OverviewSection({ job }: { job: HistoryBackfillJobCore }) {
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>任务身份与范围</CardTitle>
                    <CardDescription>
                        范围起点固定等于必须覆盖起点
                    </CardDescription>
                </CardHeader>
                <CardContent className="grid gap-3 sm:grid-cols-2">
                    <Fact label="切换编号" value={job.cutoverId} mono />
                    <Fact
                        label="必须覆盖起点"
                        value={formatDay(job.requiredHistoryStart)}
                        mono
                    />
                    <Fact
                        label="范围起点"
                        value={formatDay(job.rangeStart)}
                        mono
                    />
                    <Fact
                        label="截止时点"
                        value={formatDay(job.rangeEnd)}
                        mono
                    />
                    <Fact
                        label="覆盖完整"
                        value={job.coverageComplete ? "是" : "否"}
                    />
                    <Fact
                        label="阶段"
                        value={PIPELINE_STAGE_LABEL[job.pipelineStage]}
                    />
                </CardContent>
            </Card>
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <CardTitle>结果记录</CardTitle>
                    <CardDescription>
                        统计由系统统一计算；明细可按页浏览。
                    </CardDescription>
                </CardHeader>
                <CardContent className="grid gap-3 sm:grid-cols-2">
                    <Fact
                        label="来源记录数"
                        value={job.progress.totalCount.toLocaleString("zh-CN")}
                    />
                    <Fact
                        label="已处理"
                        value={job.progress.processedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="新增"
                        value={job.progress.insertedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="去重"
                        value={job.progress.deduplicatedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="待归集"
                        value={job.progress.unattributedCount.toLocaleString(
                            "zh-CN",
                        )}
                    />
                    <Fact
                        label="失败"
                        value={job.progress.failedCount.toLocaleString("zh-CN")}
                    />
                </CardContent>
            </Card>
        </div>
    )
}
