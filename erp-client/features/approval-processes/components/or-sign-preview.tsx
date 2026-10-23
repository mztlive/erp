import { UserRoundIcon } from "lucide-react"
import type { EditorNode } from "../types"
import { nodePeople } from "./flow-canvas-model"

/** 只读流程图沿用或签分流、人员分支、汇合的阅读顺序。 */
export function OrSignPreview({ node }: { node: EditorNode }) {
    return (
        <div
            className="overflow-x-auto border-t border-info-border bg-info-soft/40 p-4"
            aria-label="或签分支：任一人通过后进入下一步"
        >
            <div className="flex min-w-max flex-col items-center py-3">
                <div className="flex size-12 rotate-45 items-center justify-center rounded-md border border-info-border bg-info-soft text-info-soft-foreground">
                    <span className="-rotate-45 text-xs font-medium">或签</span>
                </div>
                <div className="mt-3 h-5 w-px bg-info-border" />
                <div className="flex px-3">
                    {nodePeople(node).map((person) => (
                        <div
                            key={person.user_id}
                            className="relative flex w-36 flex-col items-center px-2 before:absolute before:inset-x-0 before:top-0 before:border-t before:border-info-border after:absolute after:inset-x-0 after:bottom-0 after:border-b after:border-info-border first:before:left-1/2 first:after:left-1/2 last:before:right-1/2 last:after:right-1/2"
                        >
                            <div className="h-4 w-px bg-info-border" />
                            <span className="flex w-full items-center gap-2 rounded-md border border-info-border bg-info-soft px-3 py-2 text-xs text-info-soft-foreground">
                                <UserRoundIcon className="size-3.5 shrink-0" />
                                <span className="min-w-0 break-words">
                                    {person.name || "待确认人员"}
                                </span>
                            </span>
                            <div className="min-h-4 w-px flex-1 bg-info-border" />
                        </div>
                    ))}
                </div>
                <div className="h-4 w-px bg-info-border" />
                <span className="size-1.5 rounded-full bg-info" />
            </div>
        </div>
    )
}
