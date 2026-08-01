import { mockDelay } from "@/features/workspace-kit/delay"
import { MOCK_SENSITIVE_REVEALS } from "@/features/master-data/data"
import {
  buildW14ListResult,
  createW14Object,
  disableW14Object,
  getW14Center,
  queryW14Idempotency,
  queryW14Selector,
  reviseW14Object,
} from "@/features/master-data/session"
import type {
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  MasterDataCenterView,
  MasterDataListQuery,
  MasterDataListResult,
  MasterDataMutationResult,
  MasterDataResource,
  SelectorQueryResult,
  SelectorQueryScene,
} from "@/features/master-data/types"
import { filterMasterDataRows } from "@/features/master-data/filter"

export async function fetchMasterDataList(
  query: MasterDataListQuery
): Promise<MasterDataListResult> {
  await mockDelay(90)
  const full = buildW14ListResult(query.resource)
  const rows = filterMasterDataRows(full.rows, query)
  return {
    ...full,
    rows,
    totalCount: rows.length,
  }
}

export async function fetchMasterDataCenter(
  resource: MasterDataResource,
  stableId: string
): Promise<MasterDataCenterView | null> {
  await mockDelay(100)
  return getW14Center(resource, stableId)
}

export async function createMasterDataObject(
  input: CreateMasterDataInput
): Promise<MasterDataMutationResult> {
  await mockDelay(120)
  return createW14Object(input)
}

export async function createMasterDataRevision(
  input: CreateRevisionInput
): Promise<MasterDataMutationResult> {
  await mockDelay(120)
  return reviseW14Object(input)
}

export async function disableMasterDataObject(
  input: DisableMasterDataInput
): Promise<MasterDataMutationResult> {
  await mockDelay(120)
  return disableW14Object(input)
}

export async function queryMasterDataIdempotency(
  idempotencyKey: string
): Promise<MasterDataMutationResult | null> {
  await mockDelay(60)
  return queryW14Idempotency(idempotencyKey)
}

export async function revealMasterDataSensitive(
  revealToken: string
): Promise<string> {
  await mockDelay(80)
  const value = MOCK_SENSITIVE_REVEALS[revealToken]
  if (!value) {
    throw new Error("无权查看或权限已失效")
  }
  return value
}

export async function fetchSelectorCandidates(
  scene: SelectorQueryScene
): Promise<SelectorQueryResult> {
  await mockDelay(70)
  return queryW14Selector(scene)
}

/** Export uses current filter snapshot; permission re-check is mock copy. */
export function buildMasterDataExportCsv(
  rows: readonly {
    stableNo: string
    name: string
    revisionNo: number
    lifecycleStatusLabel: string
    revisionTimingLabel: string
    effectiveFrom: string
    effectiveTo?: string
    primaryBlocker?: string
  }[],
  filterSnapshotLabel: string,
  permissionVersion: string
): string {
  const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
  const header = [
    "稳定编号",
    "名称",
    "版本",
    "启停生命周期",
    "修订时序",
    "生效起",
    "生效止",
    "主要阻塞",
  ]
    .map(quote)
    .join(",")
  const body = rows
    .map((row) =>
      [
        row.stableNo,
        row.name,
        `v${row.revisionNo}`,
        row.lifecycleStatusLabel,
        row.revisionTimingLabel,
        row.effectiveFrom,
        row.effectiveTo ?? "长期",
        row.primaryBlocker ?? "",
      ]
        .map((v) => quote(v))
        .join(",")
    )
    .join("\n")
  const meta = [
    `# filterSnapshot=${filterSnapshotLabel}`,
    `# permissionVersion=${permissionVersion}`,
    `# note=下载时按权限版本重新鉴权；不含无权敏感字段明文`,
  ].join("\n")
  return `${meta}\n${header}\n${body}`
}

export function downloadCsv(content: string, fileName: string) {
  const url = URL.createObjectURL(
    new Blob(["\uFEFF", content], { type: "text/csv;charset=utf-8" })
  )
  const anchor = document.createElement("a")
  anchor.href = url
  anchor.download = fileName.endsWith(".csv") ? fileName : `${fileName}.csv`
  anchor.click()
  URL.revokeObjectURL(url)
}
