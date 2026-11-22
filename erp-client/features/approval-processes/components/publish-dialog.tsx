"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

import { documentTypeLabel, rejectionPolicyCopy, versionLabel } from "../labels"
import { toEditorNode } from "../draft-nodes"
import { CopyRuleSummary } from "./copy-rule-editor"
import { DefinitionFlowchart } from "./definition-flowchart"
import { usePublishDefinitionMutation } from "../queries"
import type { DefinitionDetailView } from "../types"
import { buildLockRequest } from "../write-payload"
import { useDefinitionLockCommand } from "./use-definition-lock-command"

/**
 * 发布确认。展示最终线性路径和固定驳回说明；409 时关闭提交态并保留输入。
 */
export function PublishDialog({
    detail,
    lockVersion,
    open,
    onOpenChange,
    onConflict,
    onPublished,
    id = "governance-approval-processes-publish-dialog",
}: {
    detail: DefinitionDetailView
    lockVersion: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onConflict: () => void
    onPublished: () => void
    id?: string
}) {
    const publish = usePublishDefinitionMutation()
    const { error, submit } = useDefinitionLockCommand({
        prefix: "publish",
        open,
        lockVersion,
        run: (commandKey) =>
            publish.mutateAsync({
                definitionId: detail.definition_id,
                request: buildLockRequest(lockVersion, commandKey),
            }),
        onOpenChange,
        onDone: onPublished,
        onConflict,
    })

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && publish.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId={`${id}-close`}
                showCloseButton={!publish.isPending}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>发布审批流程</DialogTitle>
                    <DialogDescription>
                        {documentTypeLabel(
                            detail.document_type,
                            detail.document_type_label,
                        )}
                        · {versionLabel(detail.definition_version)}
                    </DialogDescription>
                </DialogHeader>
                <div className="flex flex-col gap-3 text-body-compact">
                    <div data-testid="publish-path-preview">
                        <DefinitionFlowchart
                            id={`${id}-flow`}
                            nodes={[...detail.nodes]
                                .sort(
                                    (a, b) => a.display_order - b.display_order,
                                )
                                .map(toEditorNode)}
                            compact
                        />
                    </div>
                    <CopyRuleSummary detail={detail} />
                    <p
                        className="text-xs text-muted-foreground"
                        data-testid="publish-reject-copy"
                    >
                        {rejectionPolicyCopy()}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        发布后，新发起的单据将按此版本审批；已创建或审批中的单据仍按原流程继续执行。
                    </p>
                </div>
                {error ? (
                    <Alert variant="destructive">
                        <AlertTitle>未能发布</AlertTitle>
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                ) : null}
                <DialogFooter>
                    <Button
                        id={`${id}-cancel`}
                        type="button"
                        variant="outline"
                        disabled={publish.isPending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <LoadingButton
                        loading={publish.isPending}
                        id={`${id}-confirm`}
                        type="button"
                        disabled={publish.isPending}
                        onClick={() => void submit()}
                    >
                        {publish.isPending ? "正在发布…" : "确认发布"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
