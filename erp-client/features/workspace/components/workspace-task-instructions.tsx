import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceDescriptionText } from "./workspace-description-text"

/** 任务办理、审批和说明弹层共用同一套指引、影响与受阻信息排版。 */
export function WorkspaceTaskInstructions({
    item,
}: {
    item: WorkspaceWorkItem
}) {
    const guide = item.nextActionHint.trim()
    const impact = item.impactSummary.trim()
    const blockers = [
        ...new Set(
            item.actionBlockers
                .map(({ message }) => message.trim())
                .filter(Boolean),
        ),
    ]
    if (!guide && !impact && blockers.length === 0) return null

    return (
        <div className="min-w-0 space-y-4">
            {guide && (
                <section className="min-w-0 space-y-2 rounded-lg bg-muted/40 p-3">
                    <h4 className="text-xs font-medium text-muted-foreground">
                        {item.status !== "OPEN"
                            ? "办理参考"
                            : item.approval
                              ? "审批指引"
                              : "办理指引"}
                    </h4>
                    <WorkspaceDescriptionText text={guide} />
                </section>
            )}
            {impact && (
                <section className="min-w-0 space-y-2">
                    <h4 className="text-xs font-medium text-muted-foreground">
                        影响说明
                    </h4>
                    <WorkspaceDescriptionText text={impact} />
                </section>
            )}
            {blockers.length > 0 && (
                <Alert variant="warning">
                    <AlertTitle>办理限制</AlertTitle>
                    <AlertDescription className="space-y-3">
                        {blockers.map((message) => (
                            <WorkspaceDescriptionText
                                key={message}
                                text={message}
                            />
                        ))}
                    </AlertDescription>
                </Alert>
            )}
        </div>
    )
}
