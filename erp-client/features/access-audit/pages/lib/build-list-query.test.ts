import { describe, expect, it } from "vitest"

import { buildAccessListQuery } from "./build-list-query"

const baseInput = {
    view: "roles" as const,
    q: "运营",
    subjectType: "USER",
    subjectId: "u1",
    from: "2026-01-01",
    to: "2026-01-02",
    actorId: "a1",
    action: "user_role.assign",
    objectId: "o1",
    result: "SUCCESS",
    traceId: "tr_1",
}

describe("buildAccessListQuery", () => {
    it("keeps an empty q as undefined and forwards raw filters unchanged", () => {
        const query = buildAccessListQuery({ ...baseInput, q: "" })
        expect(query.q).toBeUndefined()
        expect(query.action).toBe("user_role.assign")
        expect(query.actorId).toBe("a1")
        expect(query.eventId).toBeUndefined()
    })

    it("forwards a non-empty q as-is without trimming", () => {
        const query = buildAccessListQuery({ ...baseInput, q: " 运营 " })
        expect(query.q).toBe(" 运营 ")
    })

    it("keeps detail params out of the list query", () => {
        for (const view of ["roles", "users", "audit"] as const) {
            const query = buildAccessListQuery({ ...baseInput, view })
            expect(query.subjectType).toBeUndefined()
            expect(query.subjectId).toBeUndefined()
            expect(query.eventId).toBeUndefined()
        }
    })
})
