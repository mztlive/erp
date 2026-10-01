"use client"

import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import type { CancelAllBackgroundJobsResult } from "../api"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"

type CancelDialogProps = {
    open: boolean
    error: string | null
    pending: boolean
    onOpenChange: (open: boolean) => void
    onConfirm: () => Promise<void>
}

/** 单项取消确认；关闭、版本选择与命令提交由工作台控制。 */
export function BackgroundJobCancelDialog({
    open,
    error,
    pending,
    onOpenChange,
    onConfirm,
}: CancelDialogProps) {
    return (
        <AlertDialog open={open} onOpenChange={onOpenChange}>
            <AlertDialogContent size="sm">
                <AlertDialogHeader>
                    <AlertDialogTitle>取消任务</AlertDialogTitle>
                    <AlertDialogDescription>
                        取消后尚未开始的部分不再执行，已经完成的部分不受影响。
                    </AlertDialogDescription>
                </AlertDialogHeader>
                {error ? (
                    <p className="text-sm text-destructive" role="alert">
                        {error}
                    </p>
                ) : null}
                <AlertDialogFooter>
                    <AlertDialogCancel id={`${ID_PREFIX}-cancel-back`}>
                        返回
                    </AlertDialogCancel>
                    <AlertDialogAction
                        id={`${ID_PREFIX}-cancel-confirm`}
                        disabled={pending}
                        loading={pending}
                        onClick={() => {
                            void onConfirm()
                        }}
                    >
                        {pending ? "取消中…" : "确认取消"}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}

/** 全部停止的确认与结果反馈，保持原有的关闭和重试方式。 */
export function BackgroundJobsStopAllDialog({
    open,
    error,
    pending,
    onOpenChange,
    onConfirm,
    result,
}: CancelDialogProps & { result: CancelAllBackgroundJobsResult | null }) {
    return (
        <AlertDialog open={open} onOpenChange={onOpenChange}>
            <AlertDialogContent size="sm">
                <AlertDialogHeader>
                    <AlertDialogTitle>停止并取消所有任务</AlertDialogTitle>
                    <AlertDialogDescription>
                        将停止并取消当前全部未完成的后台任务，包括他人创建的任务。已经完成的部分不受影响，没有进行中的任务时不做任何处理。
                    </AlertDialogDescription>
                </AlertDialogHeader>
                {result ? (
                    <p className="text-sm text-muted-foreground" role="status">
                        已取消 {result.cancelled_count} 个任务
                        {result.skipped_count > 0
                            ? `，跳过 ${result.skipped_count} 个已结束任务`
                            : ""}
                        {result.failed_count > 0
                            ? `，${result.failed_count} 个取消失败，可重试`
                            : ""}
                        。
                    </p>
                ) : null}
                {error ? (
                    <p className="text-sm text-destructive" role="alert">
                        {error}
                    </p>
                ) : null}
                <AlertDialogFooter>
                    {result ? (
                        <AlertDialogCancel id={`${ID_PREFIX}-stop-all-close`}>
                            关闭
                        </AlertDialogCancel>
                    ) : (
                        <>
                            <AlertDialogCancel
                                id={`${ID_PREFIX}-stop-all-back`}
                            >
                                返回
                            </AlertDialogCancel>
                            <AlertDialogAction
                                id={`${ID_PREFIX}-stop-all-confirm`}
                                disabled={pending}
                                loading={pending}
                                onClick={() => {
                                    void onConfirm()
                                }}
                            >
                                {pending ? "停止中…" : "确认停止"}
                            </AlertDialogAction>
                        </>
                    )}
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}
