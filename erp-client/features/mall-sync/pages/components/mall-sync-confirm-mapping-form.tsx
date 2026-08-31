"use client"

import { PauseIcon } from "lucide-react"

import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import { Button } from "@/components/ui/button"
import type { MappingTaskView } from "@/features/mall-sync/types"
import type { MallSyncConfirmFormApi } from "@/features/mall-sync/pages/hooks/use-mall-sync-page"
import { toAutomationIdSegment } from "@/lib/automation-id"

type MallSyncConfirmMappingFormProps = {
    mappingTask: MappingTaskView | undefined
    form: MallSyncConfirmFormApi
    selectedCandidateId: string | null
    canConfirmMapping: boolean
    responsibilityStatus: ResponsibilityStatus
    onOpenSourceFix: () => void
}

export function MallSyncConfirmMappingForm({
    mappingTask,
    form,
    selectedCandidateId,
    canConfirmMapping,
    responsibilityStatus,
    onOpenSourceFix,
}: MallSyncConfirmMappingFormProps) {
    const mappingTaskSegment = mappingTask
        ? toAutomationIdSegment(mappingTask.mappingTaskId)
        : "none"
    if (
        mappingTask?.ownerRoutingState !== "CONFIGURED" ||
        mappingTask.mappingTaskStatus !== "PENDING"
    ) {
        return null
    }
    return (
        <form
            className="space-y-2"
            onSubmit={(e) => {
                e.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField
                name="evidenceNote"
                children={(field) => (
                    <field.TextareaField
                        id={`mall-sync-mapping-${mappingTaskSegment}-evidence`}
                        label="确认依据"
                        required
                        placeholder="说明选择该 ERP 对象的业务依据"
                    />
                )}
            />
            <div className="flex flex-wrap gap-2">
                <form.AppForm>
                    <form.SubmitButton
                        id={`mall-sync-mapping-${mappingTaskSegment}-confirm`}
                        label="确认映射"
                        disabled={!canConfirmMapping}
                    />
                </form.AppForm>
                <Button
                    id={`mall-sync-mapping-${mappingTaskSegment}-source-fix`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={
                        responsibilityStatus !== "assigned_to_me" ||
                        !mappingTask.allowedActions.includes(
                            "REQUEST_SOURCE_FIX",
                        )
                    }
                    onClick={onOpenSourceFix}
                >
                    <PauseIcon className="size-4" />
                    请求来源修复
                </Button>
            </div>
            {!selectedCandidateId ? (
                <p className="text-xs text-muted-foreground">
                    请先选择左侧 ERP 候选后即可确认。
                </p>
            ) : mappingTask.hasConflict ? (
                <p className="text-xs text-muted-foreground">
                    冲突未解决前确认禁用。
                </p>
            ) : responsibilityStatus !== "assigned_to_me" ? (
                <p className="text-xs text-muted-foreground">
                    当前责任人不匹配，不能确认。
                </p>
            ) : null}
        </form>
    )
}
