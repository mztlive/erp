"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import {
    incrementalSchema,
    pullSchema,
} from "@/features/mall-sync/lib/presentation"
import {
    useRetryJobMutation,
    useTriggerIncrementalMutation,
    useTriggerSingleOrderMutation,
} from "@/features/mall-sync/hooks/queries"
import type { PatchUrl } from "@/features/mall-sync/pages/hooks/use-mall-sync-url-state"
import type { MallSyncPageData } from "@/features/mall-sync/pages/hooks/use-mall-sync-page-data"
import type { MallSyncActionFeedback } from "@/features/mall-sync/pages/hooks/use-mall-sync-action-feedback"
import { useCommandIdentities } from "@/features/mall-sync/pages/hooks/use-command-identities"

export function useMallSyncManualSyncActions(
    data: MallSyncPageData,
    feedback: MallSyncActionFeedback,
    patchUrl: PatchUrl,
) {
    const { pageQuery, stage } = data
    const { setResult, setActionError } = feedback

    const { commandIdentity, clearIdentity } = useCommandIdentities()

    const triggerInc = useTriggerIncrementalMutation()
    const triggerSo = useTriggerSingleOrderMutation()
    const retryJob = useRetryJobMutation()

    const [pullOpen, setPullOpen] = React.useState(false)
    const [incrementalOpen, setIncrementalOpen] = React.useState(false)
    const [retryConfirmOpen, setRetryConfirmOpen] = React.useState(false)

    const pullForm = useAppForm({
        defaultValues: { externalOrderNo: "", reason: "" },
        validators: { onChange: pullSchema },
        onSubmit: async ({ value }) => {
            const identity = commandIdentity(
                "single-order",
                value.externalOrderNo.trim(),
            )
            const res = await triggerSo.mutateAsync({
                externalOrderNo: value.externalOrderNo,
                reason: value.reason,
                stage,
                idempotencyKey: identity.idempotencyKey,
            })
            if (res.status === "succeeded") {
                clearIdentity(identity.key)
                setResult({
                    status: "succeeded",
                    title: "按单补拉已受理",
                    description: res.message,
                    reference: res.jobNo,
                })
                setPullOpen(false)
                patchUrl({ view: "jobs", jobId: res.jobId })
            } else {
                setActionError(res.message)
            }
        },
    })

    const incrementalForm = useAppForm({
        defaultValues: { reason: "" },
        validators: { onChange: incrementalSchema },
        onSubmit: async ({ value }) => {
            const identity = commandIdentity("incremental", "manual")
            const res = await triggerInc.mutateAsync({
                reason: value.reason,
                stage,
                idempotencyKey: identity.idempotencyKey,
            })
            if (res.status === "succeeded") {
                clearIdentity(identity.key)
                setResult({
                    status: "succeeded",
                    title: "立即增量已受理",
                    description: res.message,
                    reference: res.jobNo,
                })
                setIncrementalOpen(false)
                patchUrl({ view: "jobs", jobId: res.jobId })
            } else {
                setActionError(res.message)
            }
        },
    })

    async function handleRetryJob() {
        if (!data.data?.selectedJob) return
        const identity = commandIdentity("retry-job", data.data.selectedJob.jobId)
        const res = await retryJob.mutateAsync({
            jobId: data.data.selectedJob.jobId,
            reason: "重试未成功部分的分页",
            stage,
            idempotencyKey: identity.idempotencyKey,
        })
        if (res.status === "succeeded") {
            clearIdentity(identity.key)
        }
        setRetryConfirmOpen(false)
        if (res.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "重试已创建",
                description: res.message,
                reference: res.jobNo,
            })
            void pageQuery.refetch()
        } else {
            setActionError(res.message)
        }
    }

    return {
        pullOpen,
        setPullOpen,
        incrementalOpen,
        setIncrementalOpen,
        retryConfirmOpen,
        setRetryConfirmOpen,
        pullForm,
        incrementalForm,
        retryPending: retryJob.isPending,
        handleRetryJob,
    }
}
