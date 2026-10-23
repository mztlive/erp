"use client"

import { useState } from "react"
import { PlusIcon, UserRoundIcon, XIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"
import type { DocumentType, EligibleAssignee } from "../types"
import { AssigneeCombobox } from "./assignee-combobox"

export function PersonPicker({
    id,
    documentType,
    person,
    people,
    disabled,
    onChoose,
    compact = false,
}: {
    id: string
    documentType: DocumentType
    person?: EligibleAssignee
    people: readonly EligibleAssignee[]
    disabled: boolean
    onChoose: (person: EligibleAssignee) => void
    compact?: boolean
}) {
    const [open, setOpen] = useState(false)
    return (
        <Popover open={open && !disabled} onOpenChange={setOpen}>
            <PopoverTrigger
                id={`${id}-trigger`}
                disabled={disabled}
                render={
                    <button
                        type="button"
                        aria-label={
                            person ? `更换审批人${person.name}` : "添加审批人"
                        }
                        title={
                            person
                                ? `更换审批人${person.name}`
                                : people.length >= 50
                                  ? "每个节点最多 50 名审批人"
                                  : "添加审批人"
                        }
                        className={cn(
                            "rounded-md outline-none hover:bg-accent/40 focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-60",
                            compact
                                ? "inline-flex size-6 shrink-0 items-center justify-center"
                                : "flex min-w-0 flex-1 items-center gap-2 px-3 py-2 text-left text-sm",
                        )}
                    />
                }
                aria-label={person ? `更换审批人${person.name}` : "添加审批人"}
            >
                {person ? (
                    <UserRoundIcon className="size-3.5 shrink-0" />
                ) : (
                    <PlusIcon className="size-3.5 shrink-0" />
                )}
                <span className={compact ? "sr-only" : "min-w-0 break-words"}>
                    {person?.name || (person ? "待确认人员" : "添加审批人")}
                </span>
            </PopoverTrigger>
            <PopoverContent
                className="nodrag nopan nowheel"
                align="start"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="flex items-center justify-between gap-2">
                    <PopoverTitle className="text-sm">
                        {person ? "更换审批人" : "添加审批人"}
                    </PopoverTitle>
                    <Button
                        id={`${id}-close`}
                        type="button"
                        size="icon-xs"
                        variant="ghost"
                        aria-label="关闭人员选择"
                        onClick={() => setOpen(false)}
                    >
                        <XIcon />
                    </Button>
                </div>
                {open && !disabled && (
                    <AssigneeCombobox
                        id={`${id}-select`}
                        documentType={documentType}
                        value={person?.user_id ?? ""}
                        selectedName={person?.name ?? ""}
                        excludedIds={people
                            .filter((item) => item.user_id !== person?.user_id)
                            .map((item) => item.user_id)}
                        allowClear={false}
                        onChange={(next) => {
                            if (
                                !next ||
                                people.some(
                                    (item) =>
                                        item.user_id === next.user_id &&
                                        item.user_id !== person?.user_id,
                                )
                            )
                                return
                            setOpen(false)
                            onChoose(next)
                        }}
                    />
                )}
            </PopoverContent>
        </Popover>
    )
}
