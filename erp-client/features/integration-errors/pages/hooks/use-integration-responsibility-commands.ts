import { getErrorMessage } from "@/lib/api/errors"
import type { WorkItemResponsibilityCommand } from "@/features/work-items"
import type {
    IntegrationFormalResult,
    IntegrationResolutionItemView,
} from "../../types"
import type { CommandIdentityStore } from "../lib/command-identity"

export function useIntegrationResponsibilityCommands({
    item,
    comment,
    replacementTaskId,
    responsibilityMutation,
    commandIdentities,
    setActionError,
    afterResult,
}: {
    item: IntegrationResolutionItemView | undefined
    comment: string
    replacementTaskId: string
    responsibilityMutation: {
        mutateAsync: (input: WorkItemResponsibilityCommand) => Promise<unknown>
        isPending: boolean
    }
    commandIdentities: CommandIdentityStore
    setActionError: (error: string | null) => void
    afterResult: (result: IntegrationFormalResult) => void
}) {
    async function handleClose(kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED") {
        if (!item?.workItem) return
        if (kind === "CLOSE_DUPLICATE" && !replacementTaskId) {
            setActionError("请先选择替代任务")
            throw new Error("请先选择替代任务")
        }
        const identity = commandIdentities.get(kind, item.workItem.workItemId)
        try {
            await responsibilityMutation.mutateAsync({
                kind: "CLOSE",
                workItemId: item.workItem.workItemId,
                expectedTaskVersion: item.workItem.taskVersion,
                reasonCode:
                    kind === "CLOSE_DUPLICATE" ? "DUPLICATE" : "MISROUTED",
                replacementWorkItemId:
                    kind === "CLOSE_DUPLICATE" ? replacementTaskId : undefined,
                comment: comment || undefined,
                idempotencyKey: identity.idempotencyKey,
            })
            commandIdentities.delete(identity.key)
            afterResult({
                status: "succeeded",
                title:
                    kind === "CLOSE_DUPLICATE"
                        ? "已关闭重复任务"
                        : "已关闭误派",
                description: "仅关闭当前处理任务；未写入业务解决结论。",
                workItemStatus: "CLOSED",
                stayOnItem: false,
                terminal: true,
                replacementWorkItemId:
                    kind === "CLOSE_DUPLICATE" ? replacementTaskId : undefined,
            })
        } catch (e) {
            setActionError(getErrorMessage(e, "关闭失败"))
            throw e
        }
    }

    return { handleClose }
}
