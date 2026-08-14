import type { OwnerComboboxItem } from "@/components/business/entity-comboboxes"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"

import type {
    TaskActionFormApi,
    TaskActionKind,
} from "../hooks/use-task-action"
import type { QueueWorkItemView } from "../types"

export type TaskActionPanelProps = Readonly<{
    action: TaskActionKind
    form: TaskActionFormApi
    teamOptions: readonly OwnerComboboxItem[]
    items: readonly QueueWorkItemView[]
    selected: QueueWorkItemView
    isPending: boolean
    onCancel: () => void
    onSubmit: (
        kind: TaskActionKind,
        reason: string,
        targetUserId: string,
        reasonCode: "DUPLICATE" | "MISROUTED",
        replacementWorkItemId: string,
    ) => void
}>

export function TaskActionPanel({
    action,
    form,
    teamOptions,
    items,
    selected,
    isPending,
    onCancel,
    onSubmit,
}: TaskActionPanelProps) {
    return (
        <Card>
            <CardHeader>
                <CardTitle>
                    {action === "RELEASE_TO_TEAM"
                        ? "退回团队"
                        : action === "REASSIGN"
                          ? "转交任务"
                          : "关闭无效任务"}
                </CardTitle>
                <CardDescription>
                    此操作只更新当前任务责任，不形成业务结论。
                </CardDescription>
            </CardHeader>
            <CardContent>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {action === "REASSIGN" ? (
                        <form.AppField
                            name="targetUserId"
                            children={(field) => (
                                <field.SelectField
                                    label="转交给"
                                    options={teamOptions.map((user) => ({
                                        value: user.userId,
                                        label: user.displayName,
                                    }))}
                                />
                            )}
                        />
                    ) : null}
                    {action === "CLOSE" ? (
                        <form.AppField
                            name="reasonCode"
                            children={(field) => (
                                <field.SelectField
                                    label="关闭原因"
                                    options={[
                                        {
                                            value: "DUPLICATE",
                                            label: "重复任务",
                                        },
                                        {
                                            value: "MISROUTED",
                                            label: "任务误派",
                                        },
                                    ]}
                                />
                            )}
                        />
                    ) : null}
                    {action === "CLOSE" ? (
                        <form.Subscribe
                            selector={(state) => state.values.reasonCode}
                        >
                            {(reasonCode) =>
                                reasonCode === "DUPLICATE" ? (
                                    <form.AppField
                                        name="replacementWorkItemId"
                                        children={(field) => (
                                            <field.SelectField
                                                label="有效替代任务"
                                                options={items
                                                    .filter(
                                                        (item) =>
                                                            item.workItemId !==
                                                                selected.workItemId &&
                                                            item.status ===
                                                                "OPEN" &&
                                                            item.workItemType ===
                                                                selected.workItemType &&
                                                            item.businessObjectType ===
                                                                selected.businessObjectType,
                                                    )
                                                    .map((item) => ({
                                                        value: item.workItemId,
                                                        label: item.businessObjectLabel,
                                                    }))}
                                            />
                                        )}
                                    />
                                ) : null
                            }
                        </form.Subscribe>
                    ) : null}
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField label="原因" rows={3} />
                        )}
                    />
                    <form.Subscribe
                        selector={(state) =>
                            [state.canSubmit, state.values] as const
                        }
                    >
                        {([canSubmit, values]) => (
                            <div className="flex gap-2">
                                <Button
                                    type="button"
                                    variant="outline"
                                    onClick={onCancel}
                                >
                                    取消
                                </Button>
                                <Button
                                    type="button"
                                    disabled={
                                        !canSubmit ||
                                        isPending ||
                                        (action === "REASSIGN" &&
                                            !values.targetUserId)
                                    }
                                    onClick={() =>
                                        onSubmit(
                                            action,
                                            values.reason,
                                            values.targetUserId,
                                            values.reasonCode,
                                            values.replacementWorkItemId,
                                        )
                                    }
                                >
                                    确认操作
                                </Button>
                            </div>
                        )}
                    </form.Subscribe>
                </form>
            </CardContent>
        </Card>
    )
}
