import * as React from "react"

import { OptionCombobox } from "@/components/business"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    ListWorkspaceInlineFilter,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import { Label } from "@/components/ui/label"
import { Switch } from "@/components/ui/switch"

import type { IntegrationUrlState } from "../../lib/url-state"
import { ResponsibleUserFilter } from "@/features/entity-selectors/components/responsible-user-filter"
import {
    ENV_LABEL,
    MODE_LABEL,
    VIEW_LABEL,
    type IntegrationView,
} from "../../types"

type IntegrationFilterKey =
    | "q"
    | "mode"
    | "environment"
    | "handlerUserIds"
    | "operatorUserIds"

type IntegrationAppliedChip = Readonly<{
    key: IntegrationFilterKey
    label: string
}>

export function IntegrationQueueToolbar({
    urlState,
    searchDraft,
    onSearchDraftChange,
    searchInputRef,
    autoNext,
    patchUrl,
    onClearFilters,
    resultCount,
    loading,
    failed,
}: {
    urlState: IntegrationUrlState
    searchDraft: string
    onSearchDraftChange: (value: string) => void
    searchInputRef: React.Ref<HTMLInputElement>
    autoNext: boolean
    patchUrl: (patch: Record<string, string | null | undefined>) => void
    onClearFilters: () => void
    resultCount?: number
    loading?: boolean
    failed?: boolean
}) {
    const [modeDraft, setModeDraft] = React.useState(urlState.mode)
    const [environmentDraft, setEnvironmentDraft] = React.useState(
        urlState.environment,
    )
    React.useEffect(() => {
        setModeDraft(urlState.mode)
    }, [urlState.mode])
    React.useEffect(() => {
        setEnvironmentDraft(urlState.environment)
    }, [urlState.environment])
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            mode: modeDraft,
            environment: environmentDraft,
            errorClass: null,
            taskId: null,
            differenceId: null,
        })
    }, [environmentDraft, modeDraft, patchUrl, searchDraft])

    const removeFilter = React.useCallback(
        (key: IntegrationFilterKey) => {
            if (key === "q") onSearchDraftChange("")
            if (key === "mode") setModeDraft("all")
            if (key === "environment") setEnvironmentDraft("production")
            const cleared =
                key === "mode"
                    ? "all"
                    : key === "environment"
                      ? "production"
                      : null
            patchUrl({
                [key]: cleared,
                taskId: null,
                differenceId: null,
            })
        },
        [onSearchDraftChange, patchUrl],
    )

    const appliedChips = React.useMemo<
        readonly IntegrationAppliedChip[]
    >(() => {
        const chips: IntegrationAppliedChip[] = []
        if (urlState.q) chips.push({ key: "q", label: `搜索：${urlState.q}` })
        if (urlState.handlerUserIds) {
            chips.push({
                key: "handlerUserIds",
                label: `当前处理人：${urlState.handlerUserIds}`,
            })
        }
        if (urlState.operatorUserIds) {
            chips.push({
                key: "operatorUserIds",
                label: `历史处理人：${urlState.operatorUserIds}`,
            })
        }
        if (urlState.mode !== "all") {
            chips.push({
                key: "mode",
                label: `模式：${MODE_LABEL[urlState.mode]}`,
            })
        }
        if (urlState.environment !== "production") {
            chips.push({
                key: "environment",
                label: `环境：${ENV_LABEL[urlState.environment]}`,
            })
        }
        return chips
    }, [
        urlState.environment,
        urlState.handlerUserIds,
        urlState.mode,
        urlState.operatorUserIds,
        urlState.q,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== (urlState.q ?? "") ||
        modeDraft !== urlState.mode ||
        environmentDraft !== urlState.environment

    return (
        <div className="sticky top-0 z-10 space-y-3 bg-card py-2">
            <div className="flex flex-wrap items-center gap-2">
                <Label
                    htmlFor="integration-queue-toolbar-view"
                    className="text-xs text-muted-foreground"
                >
                    队列视图
                </Label>
                <OptionCombobox
                    id="integration-queue-toolbar-view"
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
                <ResponsibleUserFilter
                    id="integration-queue-toolbar-handler"
                    label="当前处理人"
                    value={urlState.handlerUserIds ?? ""}
                    options={[]}
                    onChange={(value) =>
                        patchUrl({
                            handlerUserIds: value || null,
                            taskId: null,
                            differenceId: null,
                        })
                    }
                />
                <ResponsibleUserFilter
                    id="integration-queue-toolbar-operator"
                    label="历史处理人"
                    value={urlState.operatorUserIds ?? ""}
                    options={[]}
                    onChange={(value) =>
                        patchUrl({
                            operatorUserIds: value || null,
                            taskId: null,
                            differenceId: null,
                        })
                    }
                />
            </div>
            <ListWorkspaceFilterBar
                density="compact"
                idPrefix="integration-queue-toolbar"
                formAriaLabel="接口错误队列查询"
                onSubmit={applyFilters}
                search={
                    <ListSearchField
                        id="integration-queue-toolbar-search"
                        searchInputRef={searchInputRef}
                        value={searchDraft}
                        onChange={onSearchDraftChange}
                        placeholder="任务号 / 业务单号 / 事件摘要 / 错误类别"
                        aria-label="搜索"
                    />
                }
                commonFilters={
                    <>
                        <ListWorkspaceInlineFilter
                            htmlFor="integration-queue-toolbar-mode"
                            label="模式"
                        >
                            <OptionCombobox
                                id="integration-queue-toolbar-mode"
                                value={modeDraft}
                                onValueChange={(v) =>
                                    setModeDraft(
                                        (v as typeof modeDraft | null) ?? "all",
                                    )
                                }
                                options={(
                                    Object.keys(
                                        MODE_LABEL,
                                    ) as (keyof typeof MODE_LABEL)[]
                                ).map((m) => ({
                                    value: m,
                                    label: MODE_LABEL[m],
                                }))}
                                inputClassName="w-[8rem]"
                                aria-label="模式"
                                allowClear={false}
                            />
                        </ListWorkspaceInlineFilter>
                        <ListWorkspaceInlineFilter
                            htmlFor="integration-queue-toolbar-environment"
                            label="环境"
                        >
                            <OptionCombobox
                                id="integration-queue-toolbar-environment"
                                value={environmentDraft}
                                onValueChange={(v) =>
                                    setEnvironmentDraft(
                                        (v as typeof environmentDraft | null) ??
                                            "production",
                                    )
                                }
                                options={(
                                    Object.keys(
                                        ENV_LABEL,
                                    ) as (keyof typeof ENV_LABEL)[]
                                ).map((e) => ({
                                    value: e,
                                    label: ENV_LABEL[e],
                                }))}
                                inputClassName="w-[7rem]"
                                aria-label="环境"
                                allowClear={false}
                            />
                        </ListWorkspaceInlineFilter>
                    </>
                }
                resultStatus={listWorkspaceFilterStatusText({
                    loading,
                    failed,
                    resultCount,
                    noun: "项任务",
                    loadingLabel: "正在加载队列…",
                })}
                chips={appliedChips}
                onClearChip={(key) => removeFilter(key as IntegrationFilterKey)}
                onClearAll={onClearFilters}
                clearButtonId="integration-queue-toolbar-clear-filters"
                hasPendingChanges={hasPendingChanges}
                pendingHint="条件已修改，待查询"
                actions={
                    <div className="flex items-center gap-2">
                        <Label
                            htmlFor="integration-queue-toolbar-auto-next"
                            className="text-xs text-muted-foreground"
                        >
                            自动下一项
                        </Label>
                        <Switch
                            id="integration-queue-toolbar-auto-next"
                            checked={autoNext}
                            onCheckedChange={(on) => {
                                patchUrl({
                                    autoNext: on ? "1" : "0",
                                })
                            }}
                        />
                    </div>
                }
            />
        </div>
    )
}
