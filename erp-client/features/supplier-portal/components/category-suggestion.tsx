"use client"

import { useState } from "react"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { usePortalCategorySuggestion } from "../hooks/queries"
import type { PortalDictionary } from "../types"
import { PortalError } from "./surface"

type CategorySelection = { id: string; version: number }
type CategorySuggestionProps = {
    originalPath: string
    productKind: string
    candidates: PortalDictionary[]
    disabled?: boolean
    onApply: (selection: CategorySelection) => void
}

/** 分类建议只经主动查询和人工确认回填当前分类，保留供应商原始路径。 */
export function PortalCategorySuggestion(props: CategorySuggestionProps) {
    return (
        <CategorySuggestionLookup
            key={JSON.stringify([props.originalPath, props.productKind])}
            {...props}
        />
    )
}

function CategorySuggestionLookup({
    originalPath,
    productKind,
    candidates,
    disabled = false,
    onApply,
}: CategorySuggestionProps) {
    const params = {
        original_category_path: originalPath.trim(),
        product_kind: productKind,
    }
    const lookupKey = JSON.stringify(params)
    const [requested, setRequested] = useState(false)
    const [confirmedKey, setConfirmedKey] = useState<string | null>(null)
    const [appliedKey, setAppliedKey] = useState<string | null>(null)
    const query = usePortalCategorySuggestion(params, false)
    const suggestion =
        requested && !query.isFetching && query.isSuccess ? query.data : null
    const category = suggestion?.category
    const matchingInput =
        suggestion?.original_category_path === params.original_category_path &&
        suggestion?.product_kind === productKind
    const candidate = category
        ? candidates.find(
              (item) =>
                  item.id === category.id &&
                  item.version === category.version &&
                  item.product_kind === productKind &&
                  category.product_kind === productKind,
          )
        : undefined
    const requiresRecheck = suggestion?.status === "recheck_required"
    const suggestionKey = suggestion
        ? JSON.stringify([
              lookupKey,
              suggestion.mapping_id,
              suggestion.version,
              category?.id,
              category?.version,
              category?.path,
              suggestion.status,
          ])
        : null
    const confirmed = suggestionKey !== null && confirmedKey === suggestionKey
    const applied = suggestionKey !== null && appliedKey === suggestionKey
    const canApply =
        !disabled &&
        !query.isFetching &&
        !query.isError &&
        matchingInput &&
        !!candidate &&
        suggestion?.requires_confirmation === true &&
        (suggestion.status === "confirmation_required" || requiresRecheck) &&
        (!requiresRecheck || confirmed)
    const prefix = "supplier-portal-category-suggestion"
    const mappingPrefix = suggestion
        ? `${prefix}-${toAutomationIdSegment(suggestion.mapping_id)}`
        : prefix
    const lookup = () => {
        if (disabled || !params.original_category_path || !productKind) return
        setRequested(true)
        setConfirmedKey(null)
        setAppliedKey(null)
        void query.refetch()
    }

    return (
        <section className="space-y-3 rounded-lg border bg-muted/20 p-4">
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="space-y-1">
                    <h3 className="text-sm font-medium">此前核对的分类建议</h3>
                    <p className="text-sm text-muted-foreground">
                        按本供应商的原始完整路径和商品类型查询。查到建议后仍需人工确认，原始路径会保留。
                    </p>
                </div>
                <Button
                    id={`${prefix}-query`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={
                        disabled ||
                        !params.original_category_path ||
                        !productKind ||
                        query.isFetching
                    }
                    onClick={lookup}
                >
                    {query.isFetching && requested
                        ? "正在查询…"
                        : "查询此前核对建议"}
                </Button>
            </div>
            {!params.original_category_path && (
                <p className="text-sm text-muted-foreground">
                    请先填写供应商原始分类完整路径。
                </p>
            )}
            {requested && query.isError && (
                <PortalError
                    error={query.error}
                    retry={disabled || query.isFetching ? undefined : lookup}
                    id={`${prefix}-retry`}
                />
            )}
            {requested &&
                query.isSuccess &&
                !query.isFetching &&
                !suggestion && (
                    <p role="status" className="text-sm text-muted-foreground">
                        未查到此前确认的对应分类，请选择当前分类或保留原始路径，交由采购核对。
                    </p>
                )}
            {suggestion && (
                <div className="space-y-3">
                    <Badge variant={requiresRecheck ? "warning" : "info"}>
                        {requiresRecheck ? "需重新核对" : "待人工确认采用"}
                    </Badge>
                    <dl className="grid gap-x-4 gap-y-2 text-sm sm:grid-cols-[auto_1fr]">
                        <dt className="text-muted-foreground">原始分类路径</dt>
                        <dd className="min-w-0 break-words">
                            {suggestion.original_category_path}
                        </dd>
                        <dt className="text-muted-foreground">先前确认路径</dt>
                        <dd className="min-w-0 break-words">
                            {suggestion.confirmed_category_path}
                        </dd>
                        <dt className="text-muted-foreground">当前分类路径</dt>
                        <dd className="min-w-0 break-words">
                            {category?.path ||
                                category?.name ||
                                "当前已无可选分类"}
                        </dd>
                    </dl>
                    {!matchingInput && (
                        <p role="alert" className="text-sm text-warning">
                            建议与当前原始路径或商品类型不一致，请重新查询后核对。
                        </p>
                    )}
                    {!category ? (
                        <p role="status" className="text-sm text-warning">
                            先前确认的分类当前不可选，请重新选择分类并核对。
                        </p>
                    ) : !candidate ? (
                        <p role="status" className="text-sm text-warning">
                            当前候选中未找到相同分类、版本和商品类型，请刷新分类资料后重新核对。
                        </p>
                    ) : null}
                    {requiresRecheck && (
                        <div className="space-y-2">
                            <p className="text-sm text-warning">
                                先前确认资料已需要重新核对。请比较原始路径、先前确认路径和当前分类，再确认本次匹配。
                            </p>
                            <label
                                htmlFor={`${mappingPrefix}-confirm-recheck`}
                                className="flex items-start gap-2 text-sm"
                            >
                                <NativeCheckbox
                                    id={`${mappingPrefix}-confirm-recheck`}
                                    checked={confirmed}
                                    disabled={
                                        disabled || !candidate || !matchingInput
                                    }
                                    onCheckedChange={(checked) =>
                                        setConfirmedKey(
                                            checked ? suggestionKey : null,
                                        )
                                    }
                                />
                                <span>
                                    我已重新核对原始路径和当前分类，确认本次采用当前分类。
                                </span>
                            </label>
                        </div>
                    )}
                    <Button
                        id={`${mappingPrefix}-apply`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={!canApply}
                        onClick={() => {
                            if (!canApply || !candidate) return
                            onApply({
                                id: candidate.id,
                                version: candidate.version,
                            })
                            setAppliedKey(suggestionKey)
                        }}
                    >
                        {requiresRecheck
                            ? "采用核对后的当前分类"
                            : "确认采用建议分类"}
                    </Button>
                    {applied && (
                        <p role="status" className="text-sm text-success">
                            已选择当前分类，供应商原始分类路径仍保留。保存或提交时将再次核对当前资料。
                        </p>
                    )}
                </div>
            )}
        </section>
    )
}
