import { vi } from 'vitest'

import type {
    AccessGovernancePolicyView,
    AccessListView,
    AuditEventRow,
    FieldPolicyRow,
    RoleRow,
    ScopeRow,
    UserRow,
} from '../types'
import { useAccessColumns } from './use-access-columns'

// Base UI 弹层依赖 ResizeObserver / scrollIntoView，jsdom 未实现；桩掉避免渲染报错。
if (typeof globalThis.ResizeObserver === 'undefined') {
    class ResizeObserverStub {
        observe() {}
        unobserve() {}
        disconnect() {}
    }
    globalThis.ResizeObserver =
        ResizeObserverStub as unknown as typeof ResizeObserver
}
if (!Element.prototype.scrollIntoView) {
    Element.prototype.scrollIntoView = () => {}
}

export function makeGovernancePolicies(): AccessGovernancePolicyView {
    return {
        userRoleTimePolicy: {
            state: 'MISSING',
            allowedActions: ['EMERGENCY_REVOKE_USER_ROLE'],
            blockerCode: 'USER_ROLE_TIME_POLICY_MISSING',
        },
        fieldPolicyGranularity: {
            state: 'MISSING',
            editable: false,
            blockerCode: 'FIELD_POLICY_GRANULARITY_MISSING',
        },
        auditAccessPolicy: {
            state: 'MISSING',
            fallbackFrom: '2026-08-14T08:00:00.000Z',
            fallbackTo: '2026-08-14T10:00:00.000Z',
            configurationExportAllowed: false,
            auditExportAllowed: false,
            blockerCode: 'AUDIT_ACCESS_POLICY_MISSING',
        },
    }
}

export function makeRoleRow(overrides?: Partial<RoleRow>): RoleRow {
    return {
        id: 'role-1',
        roleCode: 'role_code_1',
        name: '管理员',
        status: 'enabled',
        statusLabel: '启用',
        statusTone: 'success',
        permissionSummary: '共 12 项 · 系统审计 7 · 角色管理 5',
        permissionCount: 12,
        permissionGroups: [
            { name: '系统审计', count: 7 },
            { name: '角色管理', count: 5 },
        ],
        allPermissions: false,
        boundAccountCount: 3,
        dataScopeSummary: '公司级',
        fieldPolicySummary: '—',
        riskFlags: [],
        permissionVersion: 'pv-live',
        organizationLabel: '总部',
        ...overrides,
    }
}

export function makeUserRow(overrides?: Partial<UserRow>): UserRow {
    return {
        id: 'user-1',
        userId: 'u1',
        displayName: '王小明',
        accountName: 'wangxm',
        roleIds: ['role-1'],
        accountStatus: 'enabled',
        statusLabel: '启用',
        statusTone: 'success',
        activeRoles: '管理员',
        dataScopeSummary: '公司级',
        riskFlags: [],
        permissionVersion: 'pv-live',
        organizationLabel: '总部',
        roleAssignmentId: 'ura-1',
        ...overrides,
    }
}

export function makeScopeRow(overrides?: Partial<ScopeRow>): ScopeRow {
    return {
        id: 'scope-1',
        subjectType: 'ROLE',
        subjectId: 'role-1',
        subjectLabel: '管理员',
        scopeType: 'ORGANIZATION',
        scopeTypeLabel: '组织',
        scopeTargets: '总部',
        permissionVersion: 'pv-live',
        riskFlags: [],
        ...overrides,
    }
}

export function makeFieldRow(overrides?: Partial<FieldPolicyRow>): FieldPolicyRow {
    return {
        id: 'fp-1',
        policyTargetId: 'salary',
        targetLabel: '薪资字段',
        accessCapabilities: ['MASKED', 'VISIBLE'],
        capabilitySummary: '打码 · 可见',
        subjectLabel: '全员',
        permissionVersion: 'pv-live',
        editable: true,
        ...overrides,
    }
}

export function makeAuditRow(overrides?: Partial<AuditEventRow>): AuditEventRow {
    return {
        auditEventId: 'ae-1',
        recordedAt: '2026-08-14T10:00:00.000Z',
        actorId: 'u1',
        actorLabel: '王小明',
        actorRole: '管理员',
        actionType: 'QUERY_AUDIT',
        actionLabel: '查询审计',
        objectType: 'audit_event',
        objectTypeLabel: '审计事件',
        objectId: 'ae-1',
        objectLabel: '审计事件 ae-1',
        requestId: 'req-1',
        traceId: 'trace-1',
        result: 'SUCCESS',
        resultLabel: '成功',
        resultTone: 'success',
        changedFieldNames: ['salary'],
        changedFieldDisplay: 'salary · 已变更',
        ...overrides,
    }
}

export function makeListView(overrides?: Partial<AccessListView>): AccessListView {
    return {
        view: 'roles',
        permissionVersion: 'pv-live',
        watermark: 'w19-test',
        calculatedAt: '2026-08-14T10:00:00.000Z',
        metrics: {
            roleCount: 1,
            userCount: 1,
            scopeCount: 1,
            fieldPolicyCount: 1,
            auditEventCount: 1,
        },
        governancePolicies: makeGovernancePolicies(),
        roles: [makeRoleRow()],
        users: [makeUserRow()],
        scopes: [makeScopeRow()],
        fieldPolicies: [makeFieldRow()],
        auditEvents: [makeAuditRow()],
        allowedActions: ['VIEW_EFFECTIVE_ACCESS', 'EMERGENCY_REVOKE_USER_ROLE'],
        actionBlockers: [],
        workItemSupport: 'DISABLED',
        ...overrides,
    }
}

type ColumnsInput = Parameters<typeof useAccessColumns>[0]

export function makeColumnsInput(overrides?: Partial<ColumnsInput>): ColumnsInput {
    return {
        data: makeListView(),
        policies: makeGovernancePolicies(),
        router: { push: vi.fn() },
        rowFocusRef: { current: new Map() },
        openExplain: vi.fn(),
        openEvent: vi.fn(),
        startChange: vi.fn().mockResolvedValue(undefined),
        setRoleAssignment: vi.fn(),
        setDeletingRole: vi.fn(),
        ...overrides,
    }
}
