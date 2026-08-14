import { describe, expect, it } from "vitest"

import { buildAccessListQuery } from "./build-list-query"

const baseInput = {
    view: "roles" as const,
    q: "运营",
    status: "enabled",
    org: "org_1",
    risk: "HIGH_PRIVILEGE",
    subjectType: "USER",
    subjectId: "u1",
    from: "2026-01-01",
    to: "2026-01-02",
    actorId: "a1",
    action: "QUERY_AUDIT",
    objectType: "audit",
    objectId: "o1",
    result: "SUCCESS",
    traceId: "tr_1",
}

describe("buildAccessListQuery", () => {
    it("keeps an empty q as undefined and forwards raw filters unchanged", () => {
        const query = buildAccessListQuery({ ...baseInput, q: "" })
        expect(query.q).toBeUndefined()
        expect(query.status).toBe("enabled")
        expect(query.actorId).toBe("a1")
        expect(query.eventId).toBeUndefined()
    })

    it("forwards a non-empty q as-is without trimming", () => {
        const query = buildAccessListQuery({ ...baseInput, q: " 运营 " })
        expect(query.q).toBe(" 运营 ")
    })

    it("passes subject params only for the scopes view", () => {
        const scopes = buildAccessListQuery({ ...baseInput, view: "scopes" })
        expect(scopes.subjectType).toBe("USER")
        expect(scopes.subjectId).toBe("u1")

        const roles = buildAccessListQuery({ ...baseInput, view: "roles" })
        expect(roles.subjectType).toBeUndefined()
        expect(roles.subjectId).toBeUndefined()
    })
})
