import { z } from "zod"

import type { MallSyncViewName } from "@/features/mall-sync/types"

const VIEWS: MallSyncViewName[] = [
    "overview",
    "jobs",
    "snapshots",
    "mapping",
    "reconciliation",
    "history",
]

/** 每个视图可携带的对象定位参数；切视图时清理其它视图的残留参数 */
const VIEW_OBJECT_PARAMS: Record<MallSyncViewName, readonly string[]> = {
    overview: [],
    jobs: ["jobId"],
    snapshots: ["snapshotId"],
    mapping: ["mappingTaskId", "workItemId", "currentWorkItemId"],
    reconciliation: ["differenceId"],
    history: [],
}

const ALL_OBJECT_PARAMS = [
    "jobId",
    "snapshotId",
    "mappingTaskId",
    "workItemId",
    "currentWorkItemId",
    "differenceId",
] as const

function parseView(raw: string | null): MallSyncViewName {
    if (raw && (VIEWS as string[]).includes(raw)) return raw as MallSyncViewName
    return "overview"
}

type SessionLease = {
    workItemId: string
    subjectVersion: string
}

const confirmSchema = z.object({
    evidenceNote: z.string().trim().min(4, "请填写至少 4 个字的确认依据"),
})

const deferSchema = z.object({
    reasonCode: z.enum([
        "WAITING_SOURCE",
        "NEED_CLARIFICATION",
        "WAITING_MASTER_DATA",
        "OTHER",
    ]),
    note: z.string(),
})

const pullSchema = z.object({
    externalOrderNo: z.string().trim().min(1, "请填写商城销售单号"),
    reason: z.string().trim().min(4, "请填写至少 4 个字的理由"),
})

const incrementalSchema = z.object({
    reason: z.string().trim().min(4, "请填写至少 4 个字的理由"),
})

export {
    ALL_OBJECT_PARAMS,
    confirmSchema,
    deferSchema,
    incrementalSchema,
    parseView,
    pullSchema,
    VIEW_OBJECT_PARAMS,
    VIEWS,
}
export type { SessionLease }
