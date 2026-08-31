import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { TriangleAlertIcon } from "lucide-react"

import { HistoryBackfillFact } from "@/features/history-backfill/components/history-backfill-fact"
import { formatHistoryBackfillDay } from "@/features/history-backfill/lib/format"
import { HistoryBackfillResultBanner } from "@/features/history-backfill/components/history-backfill-result-banner"
import type {
    CreateBackfillContext,
    HistoryBackfillCommandResult,
} from "@/features/history-backfill/types"
import { ENVIRONMENT_LABEL } from "@/features/history-backfill/types"

export function CreateBackfillSheet({
    open,
    onOpenChange,
    context,
    pending,
    result,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    context?: CreateBackfillContext
    pending: boolean
    result: HistoryBackfillCommandResult | null
    onSubmit: () => Promise<void>
}) {
    const blocked = !context?.canCreateDraft
    return (
        <Sheet open={open} onOpenChange={onOpenChange}>
            <SheetContent
                side="right"
                size="detail"
                className="overflow-y-auto"
                closeButtonId="operations-history-backfill-create-close"
            >
                <SheetHeader>
                    <SheetTitle>创建回填任务</SheetTitle>
                    <SheetDescription>
                        回填起点固定为系统登记的必需历史起点，不可晚于该日期。回填范围覆盖起点至当前日期。
                    </SheetDescription>
                </SheetHeader>

                {!context ? (
                    <p className="text-sm text-muted-foreground">
                        正在加载创建上下文…
                    </p>
                ) : (
                    <div className="mt-4 space-y-4">
                        <div className="grid gap-3 sm:grid-cols-2">
                            <HistoryBackfillFact
                                label="商城"
                                value={context.mallName}
                            />
                            <HistoryBackfillFact
                                label="环境"
                                value={ENVIRONMENT_LABEL[context.environment]}
                            />
                            <HistoryBackfillFact
                                label="必需历史起点"
                                value={formatHistoryBackfillDay(
                                    context.requiredHistoryStart,
                                )}
                                mono
                            />
                            <HistoryBackfillFact
                                label="消费回流启用日 / 回填终点"
                                value={formatHistoryBackfillDay(
                                    context.rangeEnd,
                                )}
                                mono
                            />
                            <HistoryBackfillFact
                                label="来源可提供起点"
                                value={formatHistoryBackfillDay(
                                    context.sourceCoverageStart,
                                )}
                                mono
                            />
                            <HistoryBackfillFact
                                label="预计记录数"
                                value={context.estimatedFactCount.toLocaleString(
                                    "zh-CN",
                                )}
                            />
                            <HistoryBackfillFact
                                label="来源覆盖"
                                value={
                                    context.coverageComplete
                                        ? "完整"
                                        : "不足 · 阻断"
                                }
                            />
                        </div>

                        {blocked ? (
                            <Alert variant="destructive">
                                <TriangleAlertIcon />
                                <AlertTitle>当前无法创建回填任务</AlertTitle>
                                <AlertDescription>
                                    {context.blockReasons.join("；") ||
                                        "前置条件尚未满足"}
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        <Alert>
                            <TriangleAlertIcon />
                            <AlertTitle>截止时点前支付只补台账</AlertTitle>
                            <AlertDescription>
                                履约链固定为历史手工口径，不创建供应商订单。截止时点当天发生的记录
                                不在回填范围内。
                            </AlertDescription>
                        </Alert>

                        {context.coverageGaps.length > 0 ? (
                            <Alert variant="destructive">
                                <AlertTitle>覆盖缺口 · 禁止开始回填</AlertTitle>
                                <AlertDescription>
                                    <ul className="mt-1 list-disc space-y-1 pl-4">
                                        {context.coverageGaps.map((gap) => (
                                            <li key={`${gap.from}-${gap.to}`}>
                                                {formatHistoryBackfillDay(
                                                    gap.from,
                                                )}{" "}
                                                →{" "}
                                                {formatHistoryBackfillDay(
                                                    gap.to,
                                                )}{" "}
                                                · {gap.reasonLabel}
                                            </li>
                                        ))}
                                    </ul>
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        {context.blockReasons.length > 0 ? (
                            <Alert variant="destructive">
                                <AlertTitle>创建阻断</AlertTitle>
                                <AlertDescription>
                                    <ul className="mt-1 list-disc space-y-1 pl-4">
                                        {context.blockReasons.map((reason) => (
                                            <li key={reason}>{reason}</li>
                                        ))}
                                    </ul>
                                </AlertDescription>
                            </Alert>
                        ) : null}

                        {context.hasOverlappingFormalJob ? (
                            <Alert variant="destructive">
                                <AlertTitle>禁止重叠业务批次</AlertTitle>
                                <AlertDescription>
                                    已存在回填任务 {context.overlappingJobNo}
                                    。修复只能续跑原任务，不能新建覆盖同一范围的批次。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </div>
                )}

                <SheetFooter className="mt-6">
                    {result && result.status !== "COMMITTED" ? (
                        <div className="w-full">
                            <HistoryBackfillResultBanner result={result} />
                        </div>
                    ) : null}
                    <div className="flex w-full justify-end gap-2">
                        <Button
                            id="operations-history-backfill-create-cancel"
                            type="button"
                            variant="secondary"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <Button
                            id="operations-history-backfill-create-submit"
                            type="button"
                            disabled={blocked || pending || !context}
                            onClick={() => void onSubmit()}
                        >
                            {pending ? "提交中…" : "创建任务草稿"}
                        </Button>
                    </div>
                </SheetFooter>
            </SheetContent>
        </Sheet>
    )
}
