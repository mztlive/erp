"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import type { BackfillCommandAction } from "@/features/history-backfill/hooks/use-backfill-command"
import type { HistoryBackfillJobCore } from "@/features/history-backfill/types"
import {
    ENVIRONMENT_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
} from "@/features/history-backfill/types"

export function JobCommandDialogs({
    job,
    pending,
    startOpen,
    onStartOpenChange,
    resumeOpen,
    onResumeOpenChange,
    confirmReportOpen,
    onConfirmReportOpenChange,
    reattributeOpen,
    onReattributeOpenChange,
    reattributeItemId,
    onRun,
}: {
    job: HistoryBackfillJobCore
    pending: boolean
    startOpen: boolean
    onStartOpenChange: (open: boolean) => void
    resumeOpen: boolean
    onResumeOpenChange: (open: boolean) => void
    confirmReportOpen: boolean
    onConfirmReportOpenChange: (open: boolean) => void
    reattributeOpen: boolean
    onReattributeOpenChange: (open: boolean) => void
    reattributeItemId: string | null
    onRun: (
        action: BackfillCommandAction,
        itemIds?: string[],
    ) => Promise<void>
}) {
    return (
        <>
            <FormalActionConfirmDialog
                open={startOpen}
                onOpenChange={onStartOpenChange}
                actionLabel="开始回填"
                title="确认开始历史回填"
                description="将锁定回填范围并创建后台任务，只补充缺失记录；回填起点前的支付不计入；范围创建后不可修改。"
                fromStatus={{
                    label: PROCESSING_STATUS_LABEL[job.processingStatus],
                    tone: PROCESSING_STATUS_TONE[job.processingStatus],
                }}
                toStatus={{ label: "运行中", tone: "info" }}
                lockedFields={[
                    `范围起点 = 必须覆盖起点 = ${formatDay(job.requiredHistoryStart)}`,
                    `截止时点 = ${formatDay(job.rangeEnd)}`,
                    `商城 ${job.mallName} · ${ENVIRONMENT_LABEL[job.environment]}`,
                ]}
                effects={[
                    "后台执行五类关键记录回填",
                    "与实时记录按业务记录键去重",
                    "成本按实际、时点标准、未覆盖三种口径评估",
                ]}
                irreversibleEffects={[
                    "已成功写入的业务记录不因失败或续跑回滚",
                    "范围冻结后不可修改",
                ]}
                pending={pending}
                onConfirm={() => onRun("START")}
            />

            <FormalActionConfirmDialog
                open={resumeOpen}
                onOpenChange={onResumeOpenChange}
                actionLabel="续跑原任务"
                title="确认续跑失败/中断任务"
                description="沿原任务、原范围与原任务标识续跑，不新建重叠业务批次。"
                fromStatus={{
                    label: PROCESSING_STATUS_LABEL[job.processingStatus],
                    tone: PROCESSING_STATUS_TONE[job.processingStatus],
                }}
                toStatus={{ label: "运行中", tone: "info" }}
                lockedFields={[
                    `任务 ${job.jobNo}`,
                    `范围起点 ${formatDay(job.rangeStart)} 至 截止时点 ${formatDay(job.rangeEnd)}`,
                    "沿用原任务提交记录",
                    `已成功 ${job.progress.insertedCount} · 待处理剩余项`,
                ]}
                effects={["逐项仍使用相同业务记录键", "已成功记录保持不变"]}
                irreversibleEffects={["不删除已入库记录"]}
                pending={pending}
                onConfirm={() => onRun("RESUME")}
            />

            <FormalActionConfirmDialog
                open={confirmReportOpen}
                onOpenChange={onConfirmReportOpenChange}
                actionLabel="确认报告"
                title="确认技术报告并解锁后续流程"
                description="仅更新报告确认状态；不改写已入库记录或处理状态。"
                fromStatus={{
                    label: REPORT_REVIEW_STATUS_LABEL[job.reportReviewStatus],
                    tone: REPORT_REVIEW_STATUS_TONE[job.reportReviewStatus],
                }}
                toStatus={{ label: "已确认", tone: "success" }}
                effects={[
                    "技术报告标记为已确认",
                    "覆盖完整时解锁后续流程",
                    "不改写已入库记录",
                ]}
                irreversibleEffects={["报告确认状态进入处理审计"]}
                pending={pending}
                onConfirm={() => onRun("CONFIRM_REPORT")}
            />

            <FormalActionConfirmDialog
                open={reattributeOpen}
                onOpenChange={onReattributeOpenChange}
                actionLabel="重新归集"
                title="确认逐项重新归集"
                description="引用原业务记录重新归集并追加成本评估；不复制业务记录、不改写原消费。"
                fromStatus={{ label: "待归集", tone: "warning" }}
                toStatus={{ label: "已提交重新归集", tone: "success" }}
                effects={["按原业务记录键重新归集", "追加成本评估"]}
                irreversibleEffects={["归集结果进入处理审计"]}
                pending={pending}
                onConfirm={() =>
                    onRun(
                        "REATTRIBUTE",
                        reattributeItemId ? [reattributeItemId] : undefined,
                    )
                }
            />
        </>
    )
}
