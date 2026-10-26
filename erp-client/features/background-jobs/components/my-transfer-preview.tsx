"use client"

import { useQuery } from "@tanstack/react-query"
import { BusinessFailureState, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { apiGet } from "@/lib/api"
import type { BackgroundJobView } from "../api"
import {
    backgroundJobIdentity,
    isJobActive,
    JOB_STATUS_LABELS,
} from "../labels"
import { DataTransferTaskResult } from "./data-transfer-task-result"

/** 本人任务通过账号或门户接口读取，不依赖全局任务管理权限。 */
export function MyTransferPreview({
    jobId,
    portal,
    onClose,
}: {
    jobId: string | null
    portal: boolean
    onClose: () => void
}) {
    const query = useQuery({
        queryKey: [
            portal ? "supplier-portal" : "account",
            "data-transfers",
            jobId,
        ],
        queryFn: () =>
            apiGet<BackgroundJobView>(
                `${portal ? "/supplier-portal" : "/account"}/data-transfers/${encodeURIComponent(jobId ?? "")}`,
                undefined,
                portal ? { session: "supplier-portal" } : undefined,
            ),
        enabled: Boolean(jobId),
        refetchInterval: (current) => {
            const job = current.state.data
            return job && isJobActive(job.status, job.finished_at)
                ? 2000
                : false
        },
    })
    const job = query.data
    const identity = job ? backgroundJobIdentity(job) : null
    return (
        <QuickPreviewSheet
            idPrefix="my-transfer-preview"
            size="preview"
            open={Boolean(jobId)}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            title={identity?.title ?? "文件任务"}
            identity={
                identity ? (
                    <span className="num break-all">
                        任务号：{identity.number}
                    </span>
                ) : null
            }
            summary={
                job ? (
                    <StatusBadge
                        tone={
                            job.status === "failed"
                                ? "destructive"
                                : job.status === "succeeded"
                                  ? "success"
                                  : "info"
                        }
                        label={JOB_STATUS_LABELS[job.status]}
                    />
                ) : null
            }
            footer={
                <Button
                    id="my-transfer-preview-footer-close"
                    variant="outline"
                    onClick={onClose}
                >
                    关闭
                </Button>
            }
        >
            {query.isError ? (
                <BusinessFailureState
                    error={query.error}
                    action={
                        <Button
                            id="my-transfer-preview-retry"
                            variant="outline"
                            onClick={() => void query.refetch()}
                        >
                            重新读取任务
                        </Button>
                    }
                />
            ) : job ? (
                <div className="space-y-4 text-sm">
                    <DataTransferTaskResult
                        key={job.id}
                        job={job}
                        portal={portal}
                    />
                    {job.error_summary && (
                        <p role="alert" className="text-destructive">
                            {job.error_summary}
                        </p>
                    )}
                </div>
            ) : (
                <p className="text-muted-foreground">正在读取任务…</p>
            )}
        </QuickPreviewSheet>
    )
}
