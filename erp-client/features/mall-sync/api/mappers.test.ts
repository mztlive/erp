import { describe, it, expect } from 'vitest'

import type {
    BackendJob,
    BackendMappingTask,
    BackendReconItem,
    BackendReconJob,
    BackendSnapshot,
} from '@/features/mall-sync/api/backend-dtos'
import {
    instantToIso,
    mapJobStatus,
    mapJobType,
    mapMappingStatus,
    mapMappingType,
    mapOwnerRole,
    mapSnapshotStatus,
    shortHash,
    toJobRow,
    toMappingTask,
    toSnapshotRow,
} from '@/features/mall-sync/api/mappers'
import {
    buildMetrics,
    DIFF_TYPE_LABEL,
    mapDiffStatus,
    mapDiffType,
    mapReconJobStatus,
    toDifference,
} from '@/features/mall-sync/api/recon-mappers'

const baseJob: BackendJob = {
    id: 'job-000000000001',
    source_system_id: 'src-1',
    job_type: 'incremental',
    trigger_source: 'MANUAL',
    triggered_by: '张三',
    started_at: 1_755_123_600,
    finished_at: 1_755_123_900,
    status: 'success',
    page_count: 3,
    item_count: 41,
    error_count: 0,
    version: 1,
    created_at: 1_755_123_600,
}

const baseSnapshot: BackendSnapshot = {
    id: 'snap-1',
    source_system_id: 'src-1',
    external_order_no: 'SO-2026-001',
    source_updated_at: 1_755_123_600,
    content_hash: 'abcdef1234567890',
    source_status_code: 'paid',
    observed_at: 1_755_123_660,
    mapping_status: 'difference',
    applied_sales_order_revision_id: 'rev-1',
    sync_job_id: 'job-000000000001',
    version: 1,
    created_at: 1_755_123_600,
}

function baseMappingTask(): BackendMappingTask {
    return {
        id: 'mt-1',
        source_snapshot_id: 'snap-1',
        mapping_type: 'customer',
        status: 'pending',
        owner_role: 'SALES',
        owner_routing_state: 'CONFIGURED',
        work_item: {
            work_item_id: 'wi-1',
            task_version: 'tv-1',
            work_item_type: 'BUSINESS_EXCEPTION',
            business_object_type: 'MASTER_MAPPING_TASK',
            business_object_id: 'mt-1',
            subject_version: 'sv-1',
            status: 'OPEN',
            assignment_mode: 'POOL',
            owner_user_id: 'user-1',
            allowed_actions: ['START_PROCESSING'],
        },
        source_evidence: [
            {
                field: 'external_order_no',
                label: '来源单号',
                value: 'SO-2026-001',
                sensitive: false,
            },
            {
                field: 'payer_mobile',
                label: '付款人手机',
                value: '13800000000',
                sensitive: true,
            },
        ],
        candidate_targets: [
            {
                object_type: 'CUSTOMER',
                object_id: 'c-1',
                stable_no: 'C-001',
                label: '客户甲',
                current_revision_id: 'cr-1',
                eligibility: 'ELIGIBLE',
                reason: '名称与信用代码一致',
            },
        ],
        current_targets: [],
        impact_summary: '客户身份未归属',
        resolution_history: [
            {
                action: 'REQUEST_SOURCE_FIX',
                result: '已记录',
                handled_by: '李四',
                handled_at: 1_755_123_000,
                evidence_reference: 'ev-1',
            },
        ],
        allowed_actions: ['CONFIRM_TARGET'],
        action_blockers: [],
        reapply_operation: {
            operation_id: 'op-1',
            status: 'UNKNOWN',
            last_updated_at: 1_755_124_000,
        },
        lock_version: 2,
        version: 1,
        created_at: 1_755_123_600,
    }
}

describe('instantToIso', () => {
    it('converts seconds timestamps to ISO strings', () => {
        expect(instantToIso(1_786_695_600)).toBe('2026-08-14T08:20:00.000Z')
    })

    it('returns undefined for null, undefined and non-finite values', () => {
        expect(instantToIso(null)).toBeUndefined()
        expect(instantToIso(undefined)).toBeUndefined()
        expect(instantToIso(Number.NaN)).toBeUndefined()
    })
})

describe('shortHash', () => {
    it('truncates long hashes with an ellipsis', () => {
        expect(shortHash('abcdef1234567890')).toBe('abcdef12…')
    })

    it('keeps short hashes and renders an em dash for empty input', () => {
        expect(shortHash('abc')).toBe('abc')
        expect(shortHash(null)).toBe('—')
        expect(shortHash('')).toBe('—')
    })
})

describe('enum label mappers', () => {
    it('maps job types with an INCREMENTAL fallback', () => {
        expect(mapJobType('baseline')).toBe('BASELINE')
        expect(mapJobType('single_order_backfill')).toBe('SINGLE_ORDER')
        expect(mapJobType('monthly_reconciliation')).toBe('RECONCILIATION')
        expect(mapJobType('incremental')).toBe('INCREMENTAL')
    })

    it('maps job statuses and falls back to FAILED with the raw value', () => {
        expect(mapJobStatus('success')).toMatchObject({
            status: 'SUCCEEDED',
            statusLabel: '成功',
        })
        expect(mapJobStatus('partial_failure')).toMatchObject({
            status: 'PARTIAL_FAILED',
            statusLabel: '部分失败',
        })
        expect(mapJobStatus('running')).toMatchObject({ status: 'RUNNING' })
        expect(mapJobStatus('failed')).toMatchObject({ status: 'FAILED' })
    })

    it('maps snapshot mapping statuses', () => {
        expect(mapSnapshotStatus('pending')).toMatchObject({
            mappingStatus: 'PENDING_MAPPING',
            mappingStatusLabel: '待映射',
        })
        expect(mapSnapshotStatus('difference')).toMatchObject({
            mappingStatus: 'DIFF',
            mappingStatusLabel: '差异',
        })
        expect(mapSnapshotStatus('applied')).toMatchObject({
            mappingStatus: 'APPLIED',
        })
        expect(mapSnapshotStatus('no_change')).toMatchObject({
            mappingStatus: 'UNCHANGED',
        })
    })

    it('maps mapping types and statuses', () => {
        expect(mapMappingType('voucher_category')).toBe('VOUCHER_CATEGORY')
        expect(mapMappingType('unique_line_item')).toBe('UNIQUE_LINE')
        expect(mapMappingStatus('unresolvable')).toMatchObject({
            mappingTaskStatus: 'UNRESOLVABLE',
            mappingTaskStatusLabel: '无法处理',
        })
        expect(mapMappingStatus('closed')).toMatchObject({
            mappingTaskStatus: 'CLOSED',
            mappingTaskStatusLabel: '关闭',
        })
    })

    it('normalizes owner roles across code and Chinese forms', () => {
        expect(mapOwnerRole('ROLE-SALES')).toBe('SALES')
        expect(mapOwnerRole('销售')).toBe('SALES')
        expect(mapOwnerRole('OPS')).toBe('OPERATIONS')
        expect(mapOwnerRole('财务')).toBe('FINANCE')
        expect(mapOwnerRole('unknown')).toBeUndefined()
    })
})

describe('toJobRow', () => {
    it('maps a succeeded manual job', () => {
        const row = toJobRow(baseJob)
        expect(row.jobId).toBe('job-000000000001')
        expect(row.jobNo).toBe('JOB-00000000')
        expect(row.jobTypeLabel).toBe('增量拉取')
        expect(row.status).toBe('SUCCEEDED')
        expect(row.triggeredBy).toBe('张三')
        expect(row.watermarkAdvanced).toBe(true)
        expect(row.allowedActions).toEqual([])
    })

    it('marks scheduled jobs as system-triggered and failed jobs as retryable', () => {
        const scheduled = toJobRow({
            ...baseJob,
            trigger_source: 'SCHEDULED',
        })
        expect(scheduled.triggeredBy).toBe('系统调度')

        const failed = toJobRow({ ...baseJob, status: 'failed' })
        expect(failed.allowedActions).toEqual(['RETRY_FAILED_JOB'])
        expect(failed.status).toBe('FAILED')
        expect(failed.watermarkAdvanced).toBe(false)
    })
})

describe('toSnapshotRow', () => {
    it('maps snapshot fields, shortens the hash and flags diffs', () => {
        const row = toSnapshotRow(
            baseSnapshot,
            new Map([['job-000000000001', 'JOB-00000000']]),
        )
        expect(row.snapshotId).toBe('snap-1')
        expect(row.externalOrderNo).toBe('SO-2026-001')
        expect(row.contentHashShort).toBe('abcdef12…')
        expect(row.mappingStatus).toBe('DIFF')
        expect(row.conflictFlags).toEqual(['MAPPING_DIFF'])
        expect(row.syncJobNo).toBe('JOB-00000000')
        expect(row.whitelistFields.map((f) => f.field)).toEqual([
            'external_order_no',
            'source_status_code',
        ])
    })

    it('falls back to the id prefix when the job number is unknown', () => {
        const row = toSnapshotRow(baseSnapshot, new Map())
        expect(row.syncJobNo).toBe('job-00000000')
    })
})

describe('toMappingTask', () => {
    it('maps a configured task with routing and work item details', () => {
        const view = toMappingTask(
            baseMappingTask(),
            new Map([['snap-1', baseSnapshot]]),
        )
        expect(view.mappingTaskId).toBe('mt-1')
        expect(view.mappingType).toBe('CUSTOMER')
        expect(view.mappingTypeLabel).toBe('客户映射')
        expect(view.externalOrderNo).toBe('SO-2026-001')
        expect(view.ownerRoutingState).toBe('CONFIGURED')
        if (view.ownerRoutingState === 'CONFIGURED') {
            expect(view.ownerRole).toBe('SALES')
            expect(view.ownerRoleLabel).toBe('销售')
            expect(view.workItem.statusLabel).toBe('处理中')
            expect(view.workItem.allowedActions).toEqual([
                'START_PROCESSING',
            ])
            expect(view.ownerUserId).toBe('user-1')
        }
        expect(view.reapplyOperation?.status).toBe('UNKNOWN')
        expect(view.reapplyOperation?.statusLabel).toBe('结果未知')
        expect(view.resolutionHistory).toHaveLength(1)
        expect(view.hasConflict).toBe(false)
    })

    it('falls back to the snapshot order no when evidence lacks it', () => {
        const task = baseMappingTask()
        task.source_evidence = []
        const view = toMappingTask(
            task,
            new Map([['snap-1', baseSnapshot]]),
        )
        expect(view.externalOrderNo).toBe('SO-2026-001')
    })

    it('keeps the em dash fallback when evidence and snapshot are both unavailable', () => {
        const task = baseMappingTask()
        task.source_evidence = []
        const view = toMappingTask(task, new Map())
        expect(view.externalOrderNo).toBe('—')
    })

    it('degrades to MISSING routing and clears candidates/actions when routing is not configured', () => {
        const task = baseMappingTask()
        task.owner_routing_state = 'MISSING'
        const view = toMappingTask(task, new Map())
        expect(view.ownerRoutingState).toBe('MISSING')
        expect(view.allowedActions).toEqual([])
        expect(view.candidateTargets).toEqual([])
    })

    it('degrades to MISSING routing when the work item does not match the task', () => {
        const task = baseMappingTask()
        task.work_item!.business_object_id = 'mt-other'
        const view = toMappingTask(task, new Map())
        expect(view.ownerRoutingState).toBe('MISSING')
    })

    it('detects conflicts from action blockers', () => {
        const task = baseMappingTask()
        task.action_blockers = [
            { action: 'CONFIRM_TARGET', code: 'VERSION_CONFLICT', message: 'x' },
        ]
        const view = toMappingTask(task, new Map())
        expect(view.hasConflict).toBe(true)
    })
})

describe('reconciliation mappers', () => {
    it('maps difference types and provides Chinese labels', () => {
        expect(mapDiffType('mall_missing')).toBe('MALL_MISSING')
        expect(mapDiffType('erp_missing')).toBe('ERP_MISSING')
        expect(mapDiffType('status_difference')).toBe('STATUS')
        expect(mapDiffType('content_fingerprint_difference')).toBe(
            'FINGERPRINT',
        )
        expect(mapDiffType('duplicate_identity')).toBe('DUPLICATE')
        expect(DIFF_TYPE_LABEL.MALL_MISSING).toBe('商城缺失')
        expect(DIFF_TYPE_LABEL.FINGERPRINT).toBe('内容不一致')
    })

    it('maps difference statuses with tones', () => {
        expect(mapDiffStatus('pending')).toMatchObject({
            status: 'OPEN',
            statusTone: 'warning',
        })
        expect(mapDiffStatus('backfilling')).toMatchObject({
            status: 'PULLING',
            statusLabel: '补拉中',
        })
        expect(mapDiffStatus('resolved')).toMatchObject({
            status: 'RESOLVED',
        })
        expect(mapDiffStatus('confirmed_no_difference')).toMatchObject({
            status: 'CONFIRMED',
            statusLabel: '确认无误',
        })
    })

    it('maps recon job statuses', () => {
        expect(mapReconJobStatus('running')).toMatchObject({
            status: 'RUNNING',
            statusLabel: '运行中',
        })
        expect(mapReconJobStatus('completed')).toMatchObject({
            status: 'SUCCEEDED',
            statusLabel: '完成',
        })
        expect(mapReconJobStatus('has_difference')).toMatchObject({
            status: 'DIFFERENCE',
            statusLabel: '有差异',
        })
        expect(mapReconJobStatus('failed')).toMatchObject({
            status: 'FAILED',
            statusLabel: '失败',
        })
    })

    it('maps a difference item and prefers the resolution text', () => {
        const item: BackendReconItem = {
            id: 'd-1',
            reconciliation_job_id: 'rj-1',
            external_order_no: 'SO-2026-002',
            source_status_code: 'paid',
            source_updated_at: 1_755_123_600,
            difference_type: 'content_fingerprint_difference',
            status: 'pending',
            resolution: '已核对来源内容',
            version: 1,
            created_at: 1_755_123_600,
        }
        const view = toDifference(item)
        expect(view.differenceId).toBe('d-1')
        expect(view.differenceType).toBe('FINGERPRINT')
        expect(view.status).toBe('OPEN')
        expect(view.impactSummary).toBe('已核对来源内容')
    })
})

describe('buildMetrics', () => {
    it('counts pending mappings, failed jobs and recon differences', () => {
        const jobs = [
            { status: 'FAILED' },
            { status: 'PARTIAL_FAILED' },
            { status: 'SUCCEEDED' },
        ] as Parameters<typeof buildMetrics>[0]
        const tasks = [
            { mappingTaskStatus: 'PENDING' },
            { mappingTaskStatus: 'PENDING' },
            { mappingTaskStatus: 'RESOLVED' },
        ] as Parameters<typeof buildMetrics>[1]
        const recon = {
            differenceCount: 7,
        } as Parameters<typeof buildMetrics>[2]

        const metrics = buildMetrics(jobs, tasks, recon, 300)
        expect(metrics.find((m) => m.key === 'failed')?.count).toBe(2)
        expect(metrics.find((m) => m.key === 'pending')?.count).toBe(2)
        expect(metrics.find((m) => m.key === 'recon')?.count).toBe(7)
        expect(metrics.find((m) => m.key === 'lag')?.value).toBe('5 分')
    })

    it('renders an em dash lag when no lag is known', () => {
        const metrics = buildMetrics([], [], null, undefined)
        expect(metrics.find((m) => m.key === 'lag')?.value).toBe('—')
        expect(metrics.find((m) => m.key === 'recon')?.count).toBe(0)
    })
})

describe('recon job status contract', () => {
    it('covers the BackendReconJob status union', () => {
        const job: BackendReconJob = {
            id: 'rj-1',
            source_system_id: 'src-1',
            job_no: 'RJ-1',
            source_list_as_of: 1_755_123_600,
            source_count: 10,
            erp_count: 9,
            difference_count: 1,
            status: 'has_difference',
            started_at: 1_755_123_600,
            version: 1,
            created_at: 1_755_123_600,
        }
        expect(mapReconJobStatus(job.status).status).toBe('DIFFERENCE')
    })
})
