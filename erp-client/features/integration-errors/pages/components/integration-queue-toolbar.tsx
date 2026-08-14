import * as React from "react"
import { SearchIcon } from "lucide-react"
import {
    ListToolbar,
    OptionCombobox,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"
import { cn } from "@/lib/utils"

import type { IntegrationUrlState } from "../../lib/url-state"
import {
    ENV_LABEL,
    ERROR_CLASS_LABEL,
    MODE_LABEL,
    OWNER_LABEL,
    VIEW_LABEL,
    type IntegrationView,
} from "../../types"

export function IntegrationQueueToolbar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    searchInputRef,
    autoNext,
    hasQueueFilters,
    patchUrl,
    onClearFilters,
}: {
    urlState: IntegrationUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    searchInputRef: React.Ref<HTMLInputElement>
    autoNext: boolean
    hasQueueFilters: boolean
    patchUrl: (patch: Record<string, string | null | undefined>) => void
    onClearFilters: () => void
}) {
    return (
        <div
            className={cn(
                surfacePanelClassName,
                "sticky top-0 z-10 space-y-2.5 px-3 py-2.5",
            )}
        >
            <div className="flex flex-wrap items-center gap-2">
                <OptionCombobox
                    value={urlState.view}
                    onValueChange={(v) =>
                        patchUrl({
                            view: (v as IntegrationView | null) ?? "mine",
                            taskId: null,
                            differenceId: null,
                        })
                    }
                    options={(Object.keys(VIEW_LABEL) as IntegrationView[]).map(
                        (v) => ({ value: v, label: VIEW_LABEL[v] }),
                    )}
                    allowClear={false}
                    size="sm"
                    aria-label="队列视图"
                    inputClassName="w-[9.5rem]"
                />
            </div>
            <ListToolbar
                aria-label="队列筛选"
                search={
                    <form
                        onSubmit={(e) => {
                            e.preventDefault()
                            patchUrl({
                                q: searchDraft.trim() || null,
                                taskId: null,
                                differenceId: null,
                            })
                        }}
                    >
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden="true" />
                            </InputGroupAddon>
                            <InputGroupInput
                                ref={searchInputRef}
                                value={searchDraft}
                                onChange={(e) =>
                                    onSearchDraftChange(e.target.value)
                                }
                                placeholder="任务号 / 业务单号 / 事件摘要"
                                aria-label="搜索"
                            />
                        </InputGroup>
                    </form>
                }
                filters={
                    <>
                        <OptionCombobox
                            value={urlState.mode}
                            onValueChange={(v) =>
                                patchUrl({
                                    mode: v ?? "all",
                                    taskId: null,
                                    differenceId: null,
                                })
                            }
                            options={(
                                Object.keys(MODE_LABEL) as (keyof typeof MODE_LABEL)[]
                            ).map((m) => ({
                                value: m,
                                label: MODE_LABEL[m],
                            }))}
                            inputClassName="w-[8rem]"
                            size="sm"
                            aria-label="模式"
                            allowClear={false}
                        />
                        <OptionCombobox
                            value={urlState.environment}
                            onValueChange={(v) =>
                                patchUrl({
                                    environment: v ?? "production",
                                    taskId: null,
                                    differenceId: null,
                                })
                            }
                            options={(
                                Object.keys(ENV_LABEL) as (keyof typeof ENV_LABEL)[]
                            ).map((e) => ({
                                value: e,
                                label: ENV_LABEL[e],
                            }))}
                            inputClassName="w-[7rem]"
                            size="sm"
                            aria-label="环境"
                            allowClear={false}
                        />
                        <OptionCombobox
                            value={urlState.errorClass ?? "all"}
                            onValueChange={(v) =>
                                patchUrl({
                                    errorClass: !v || v === "all" ? null : v,
                                    taskId: null,
                                    differenceId: null,
                                })
                            }
                            options={[
                                { value: "all", label: "全部类别" },
                                ...Object.entries(ERROR_CLASS_LABEL).map(
                                    ([k, label]) => ({
                                        value: k,
                                        label,
                                    }),
                                ),
                            ]}
                            inputClassName="w-[10rem]"
                            size="sm"
                            aria-label="错误类别"
                            placeholder="错误类别"
                            allowClear={false}
                        />
                    </>
                }
                secondary={
                    <OptionCombobox
                        value={urlState.owner}
                        onValueChange={(v) =>
                            patchUrl({
                                owner: v ?? "me",
                                taskId: null,
                                differenceId: null,
                            })
                        }
                        options={(
                            Object.keys(OWNER_LABEL) as (keyof typeof OWNER_LABEL)[]
                        ).map((o) => ({
                            value: o,
                            label: OWNER_LABEL[o],
                        }))}
                        inputClassName="w-[8rem]"
                        size="sm"
                        aria-label="责任人"
                        allowClear={false}
                    />
                }
                actions={
                    <>
                        {hasQueueFilters ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        ) : null}
                        <div className="flex items-center gap-2">
                            <Label
                                htmlFor="w29-auto-next"
                                className="text-xs text-muted-foreground"
                            >
                                自动下一项
                            </Label>
                            <Switch
                                id="w29-auto-next"
                                checked={autoNext}
                                onCheckedChange={(on) => {
                                    patchUrl({
                                        autoNext: on ? "1" : "0",
                                    })
                                }}
                            />
                        </div>
                    </>
                }
            />
        </div>
    )
}
