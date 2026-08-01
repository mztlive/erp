/** Map Chinese scope labels ↔ URL scope slugs (W02 contract). */

const DEFAULT_SLUG_BY_LABEL: Record<string, string> = {
  我的待办: "mine",
  待领取: "role_pool",
  团队: "team",
  待我处理: "mine",
  已暂挂: "hold",
  待复核: "review",
  有差异: "diff",
  已通过: "passed",
  待映射: "unmap",
  供给变更: "supply",
  成本异常: "cost",
}

export function scopeLabelToSlug(
  label: string,
  scopeLabels: readonly string[]
): string {
  if (DEFAULT_SLUG_BY_LABEL[label]) return DEFAULT_SLUG_BY_LABEL[label]
  const index = scopeLabels.indexOf(label)
  return index >= 0 ? `s${index}` : "mine"
}

export function scopeSlugToLabel(
  slug: string | null | undefined,
  scopeLabels: readonly string[]
): string {
  const defaultLabel = scopeLabels[0] ?? "我的待办"
  if (!slug) return defaultLabel
  for (const label of scopeLabels) {
    if (scopeLabelToSlug(label, scopeLabels) === slug) return label
  }
  return defaultLabel
}

export function buildQueueSearchParams(options: {
  scopeLabel: string
  scopeLabels: readonly string[]
  currentWorkItemId?: string | null
  queueContextId?: string | null
}): string {
  const params = new URLSearchParams()
  params.set("scope", scopeLabelToSlug(options.scopeLabel, options.scopeLabels))
  if (options.currentWorkItemId) {
    params.set("currentWorkItemId", options.currentWorkItemId)
  }
  if (options.queueContextId) {
    params.set("queueContextId", options.queueContextId)
  }
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}
