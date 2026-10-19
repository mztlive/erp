import { ArrowRightIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DraftForm } from "../hooks/use-draft-form"
import { totalOf, type DraftValues } from "../lib/draft"
import type { PaymentCandidate } from "../types"
import { RequestDraftGroup } from "./request-draft-group"
import { RequestDraftGroupHeader } from "./request-draft-group-header"

export function RequestDraftWorkspace({
    form,
    values,
    candidates,
    editing,
    fixedOrder = false,
    activeKey,
    issues,
    onSelect,
}: {
    form: DraftForm
    values: DraftValues
    candidates: PaymentCandidate[]
    editing: boolean
    fixedOrder?: boolean
    activeKey: string | undefined
    issues: Map<string, string[]>
    onSelect: (key: string) => void
}) {
    const { groups, amounts } = values
    const selected =
        groups.find((group) => group.key === activeKey) ?? groups[0]
    const activeIndex = groups.findIndex((group) => group.key === selected?.key)
    const nextIncomplete = [
        ...groups.slice(activeIndex + 1),
        ...groups.slice(0, activeIndex),
    ].find((group) => issues.has(group.key))
    if (!selected)
        return (
            <p role="alert" className="p-6 text-sm">
                {fixedOrder
                    ? "当前采购单暂无可申请付款的来源，请核对采购单是否已生效、余额是否已付清或已被其他申请占用，以及是否存在待处理的采购依据变更。"
                    : "当前采购来源不可申请，请返回上一步重新选择。"}
            </p>
        )
    return (
        <div className="flex min-h-0 flex-1 flex-col">
            {!fixedOrder ? (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-2 border-b px-6 py-3 text-sm">
                    <p>
                        共 <strong>{groups.length}</strong> 份申请{" "}
                        <span className="text-muted-foreground">
                            · {values.selectedIds.length} 张采购单
                        </span>
                    </p>
                    <p role="status" className="text-xs text-muted-foreground">
                        已填完整 {groups.length - issues.size} 份
                        {issues.size > 0 && (
                            <span className="ml-3 text-warning-soft-foreground">
                                {issues.size} 份待补充
                            </span>
                        )}
                    </p>
                </div>
            ) : null}
            <div className="flex min-h-0 flex-1 flex-col lg:flex-row">
                {!fixedOrder ? (
                    <nav
                        aria-label="付款申请列表"
                        className="max-h-44 shrink-0 overflow-y-auto border-b bg-muted/30 p-3 lg:max-h-none lg:w-[30%] lg:border-r lg:border-b-0"
                    >
                        <div className="space-y-1.5">
                            {groups.map((group, index) => (
                                <RequestDraftGroupHeader
                                    key={group.key}
                                    id={`ppr-group-${toAutomationIdSegment(group.key)}`}
                                    group={group}
                                    index={index}
                                    first={candidates.find(
                                        (row) =>
                                            row.payable_account_id ===
                                            group.ids[0],
                                    )}
                                    active={selected.key === group.key}
                                    incomplete={issues.has(group.key)}
                                    amount={totalOf(
                                        group.ids.map(
                                            (id) => amounts[id] ?? "",
                                        ),
                                    )}
                                    onSelect={() => onSelect(group.key)}
                                />
                            ))}
                        </div>
                    </nav>
                ) : null}
                <div
                    id="ppr-create-editor"
                    className="min-h-0 min-w-0 flex-1 overflow-y-auto p-5 sm:p-6"
                >
                    {groups.map((group, index) => (
                        <RequestDraftGroup
                            key={group.key}
                            form={form}
                            group={group}
                            index={index}
                            candidates={candidates}
                            editing={editing}
                            fixedOrder={fixedOrder}
                            open={selected.key === group.key}
                            onSplit={onSelect}
                            issues={issues.get(group.key) ?? []}
                        />
                    ))}
                    {nextIncomplete && (
                        <div className="mt-6 flex justify-end border-t pt-4">
                            <Button
                                id="ppr-create-next-incomplete"
                                type="button"
                                variant="outline"
                                onClick={() => onSelect(nextIncomplete.key)}
                            >
                                下一份待补充{" "}
                                <ArrowRightIcon aria-hidden="true" />
                            </Button>
                        </div>
                    )}
                </div>
            </div>
        </div>
    )
}
