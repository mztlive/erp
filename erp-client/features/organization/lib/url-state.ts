import { createUrlStateCodec } from "@/lib/url-state"
import type {
    DataScopeType,
    DataScopeUrlState,
    OrganizationUrlState,
    OrgUnitKind,
} from "@/features/organization/types"

const KINDS: Array<OrganizationUrlState["kind"]> = [
    "all",
    "department",
    "team",
]
const STATUSES: Array<OrganizationUrlState["status"]> = [
    "all",
    "enabled",
    "disabled",
]
const SUBJECT_TYPES: Array<DataScopeUrlState["subjectType"]> = [
    "all",
    "role",
    "user",
]
const SCOPE_TYPES: Array<DataScopeUrlState["scopeType"]> = [
    "all",
    "company",
    "organization",
    "team",
    "self_owned",
    "collaborative",
]

const organizationCodec = createUrlStateCodec<OrganizationUrlState>([
    { key: "unitId", type: "string", trim: true },
    { key: "q", name: "q", type: "string", trim: true, aliases: ["search"] },
    { key: "kind", type: "enum", values: KINDS, defaultValue: "all" },
    { key: "status", type: "enum", values: STATUSES, defaultValue: "all" },
])

const dataScopeCodec = createUrlStateCodec<DataScopeUrlState>([
    { key: "q", name: "q", type: "string", trim: true, aliases: ["search"] },
    { key: "resource", type: "string", trim: true },
    { key: "action", type: "string", trim: true },
    {
        key: "subjectType",
        type: "enum",
        values: SUBJECT_TYPES,
        defaultValue: "all",
    },
    { key: "subjectId", type: "string", trim: true },
    {
        key: "scopeType",
        type: "enum",
        values: SCOPE_TYPES,
        defaultValue: "all",
    },
])

const ORGANIZATION_KEYS = ["unitId", "q", "search", "kind", "status"] as const
const DATA_SCOPE_KEYS = [
    "q",
    "search",
    "resource",
    "action",
    "subjectType",
    "subjectId",
    "scopeType",
] as const

export function parseOrganizationSearchParams(
    searchParams: URLSearchParams | { get(name: string): string | null },
): OrganizationUrlState {
    return organizationCodec.parse(searchParams)
}

export function buildOrganizationSearchParams(
    state: OrganizationUrlState,
): string {
    return organizationCodec.build(state)
}

export function mergeOrganizationSearchParams(
    searchParams: { toString(): string },
    state: OrganizationUrlState,
): string {
    return mergeManaged(searchParams, ORGANIZATION_KEYS, organizationCodec.buildParams(state))
}

export function parseDataScopeSearchParams(
    searchParams: URLSearchParams | { get(name: string): string | null },
): DataScopeUrlState {
    return dataScopeCodec.parse(searchParams)
}

export function buildDataScopeSearchParams(state: DataScopeUrlState): string {
    return dataScopeCodec.build(state)
}

export function mergeDataScopeSearchParams(
    searchParams: { toString(): string },
    state: DataScopeUrlState,
): string {
    return mergeManaged(searchParams, DATA_SCOPE_KEYS, dataScopeCodec.buildParams(state))
}

function mergeManaged(
    searchParams: { toString(): string },
    keys: readonly string[],
    managed: URLSearchParams,
): string {
    const merged = new URLSearchParams(searchParams.toString())
    for (const key of keys) merged.delete(key)
    for (const [key, value] of managed) merged.append(key, value)
    const query = merged.toString()
    return query ? `?${query}` : ""
}

export function isOrgKind(value: string): value is OrgUnitKind {
    return value === "department" || value === "team"
}

export function isDataScopeType(value: string): value is DataScopeType {
    return SCOPE_TYPES.includes(value as DataScopeUrlState["scopeType"]) &&
        value !== "all"
}
