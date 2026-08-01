import type {
  DemoRole,
  DifferenceType,
  SettlementSection,
  SettlementView,
} from "@/features/supplier-settlements/types"
import { SECTIONS } from "@/features/supplier-settlements/types"

export type SettlementsUrlState = {
  view: SettlementView
  supplierId?: string
  periodFrom?: string
  periodTo?: string
  status?: string
  differenceType?: DifferenceType
  q?: string
  page: number
  preview?: string
  statementId?: string
  section: SettlementSection
  role: DemoRole
  demoFlag?: "no-permission" | "no-scope" | "policy-missing"
}

const VIEW_SET = new Set([
  "pending",
  "prepared_by_me",
  "review_by_me",
  "confirmed",
])
const ROLE_SET = new Set([
  "finance_prep",
  "finance_review",
  "procurement",
  "manager",
])
const DIFF_SET = new Set([
  "MISSING_ORDER",
  "DUPLICATE",
  "AMOUNT",
  "REFUND",
  "STATUS",
])

export function parseSettlementsSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): SettlementsUrlState {
  const viewRaw = searchParams.get("view") ?? "pending"
  const view: SettlementView = VIEW_SET.has(viewRaw)
    ? (viewRaw as SettlementView)
    : "pending"

  const supplierId =
    searchParams.get("supplier") ??
    searchParams.get("supplierId") ??
    undefined
  const periodFrom =
    searchParams.get("periodFrom") ?? searchParams.get("period") ?? undefined
  const periodTo = searchParams.get("periodTo") ?? undefined
  const status = searchParams.get("status") ?? undefined
  const diffRaw = searchParams.get("differenceType") ?? undefined
  const differenceType =
    diffRaw && DIFF_SET.has(diffRaw)
      ? (diffRaw as DifferenceType)
      : undefined
  const q = searchParams.get("q") ?? undefined
  const preview = searchParams.get("preview") ?? undefined
  const statementId =
    searchParams.get("statementId") ?? searchParams.get("id") ?? undefined

  const sectionRaw = searchParams.get("section")
  const section: SettlementSection =
    sectionRaw && (SECTIONS as string[]).includes(sectionRaw)
      ? (sectionRaw as SettlementSection)
      : "overview"

  const pageRaw = Number(searchParams.get("page") ?? "1")
  const page =
    Number.isFinite(pageRaw) && pageRaw >= 1 ? Math.floor(pageRaw) : 1

  const roleRaw = searchParams.get("role") ?? searchParams.get("demoRole")
  const role: DemoRole =
    roleRaw && ROLE_SET.has(roleRaw) ? (roleRaw as DemoRole) : "finance_prep"

  const flagRaw = searchParams.get("demoFlag")
  const demoFlag =
    flagRaw === "no-permission" ||
    flagRaw === "no-scope" ||
    flagRaw === "policy-missing"
      ? flagRaw
      : undefined

  return {
    view,
    supplierId,
    periodFrom,
    periodTo,
    status,
    differenceType,
    q,
    page,
    preview,
    statementId,
    section,
    role,
    demoFlag,
  }
}

export function buildSettlementsSearchParams(
  state: SettlementsUrlState
): string {
  const params = new URLSearchParams()
  if (state.view !== "pending") params.set("view", state.view)
  if (state.supplierId) params.set("supplier", state.supplierId)
  if (state.periodFrom) params.set("periodFrom", state.periodFrom)
  if (state.periodTo) params.set("periodTo", state.periodTo)
  if (state.status) params.set("status", state.status)
  if (state.differenceType) params.set("differenceType", state.differenceType)
  if (state.q?.trim()) params.set("q", state.q.trim())
  if (state.page > 1) params.set("page", String(state.page))
  if (state.preview) params.set("preview", state.preview)
  if (state.statementId) {
    params.set("statementId", state.statementId)
    if (state.section !== "overview") params.set("section", state.section)
  }
  if (state.role !== "finance_prep") params.set("role", state.role)
  if (state.demoFlag) params.set("demoFlag", state.demoFlag)
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}
