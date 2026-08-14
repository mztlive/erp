import { surfacePanelClassName, WorkTaskItem } from "@/components/business"
import { cn } from "@/lib/utils"

import type { QueueWorkItemView } from "../types"

export type TaskListPanelProps = Readonly<{
    items: readonly QueueWorkItemView[]
    selectedWorkItemId: string
    onSelect: (item: QueueWorkItemView) => void
}>

export function TaskListPanel({
    items,
    selectedWorkItemId,
    onSelect,
}: TaskListPanelProps) {
    return (
        <section
            className={cn(
                surfacePanelClassName,
                "max-h-[calc(100vh-18rem)] space-y-2 overflow-auto p-3",
            )}
            aria-label="任务队列"
        >
            {items.map((item) => (
                <button
                    key={item.workItemId}
                    type="button"
                    className="block w-full text-left"
                    aria-current={
                        item.workItemId === selectedWorkItemId || undefined
                    }
                    onClick={() => onSelect(item)}
                >
                    <WorkTaskItem
                        density="compact"
                        taskType={item.workItemTypeLabel}
                        businessObject={item.businessObject}
                        counterparty={item.counterparty}
                        enteredAt={item.enteredAt}
                        enteredDateTime={item.enteredDateTime}
                        dueAt={item.dueLabel}
                        dueDateTime={item.dueDateTime}
                        responsibleParty={item.responsibilityLabel}
                        reason={item.reason}
                        impact={item.impact}
                        status={item.statusPresentation}
                        className={cn(
                            item.workItemId === selectedWorkItemId &&
                                "border-primary bg-primary/5",
                        )}
                    />
                </button>
            ))}
        </section>
    )
}
